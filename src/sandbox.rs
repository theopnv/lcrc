//! Per-task isolation envelope.
//!
//! The `sandbox` module owns the per-task isolation envelope. Submodules:
//! - `runtime` — preflight detection of a reachable Docker-Engine-API-compatible socket (Story 1.9)
//! - `image` — container image pull and digest verification (this story)
//! - `network` — per-scan internal Docker network with iptables port-pinning (this story)
//! - `container` — ephemeral container lifecycle; the ONLY caller of bollard container APIs (this story)

pub mod container;
pub mod image;
pub mod network;
pub mod runtime;

/// Outcome of a single task execution inside the per-task container.
#[derive(Debug, Clone)]
pub struct TaskOutcome {
    /// `true` if the container exited 0, `false` for any non-zero exit code.
    pub pass: bool,
    /// Wall-clock time from container-start to container-exit, in seconds.
    pub duration_seconds: f64,
}

/// Per-scan execution context: holds the bollard Docker client and owns the
/// lifecycle of the per-scan custom network.
///
/// Construct with [`Sandbox::new`]; call [`Sandbox::run_task`] for each
/// cell; call [`Sandbox::cleanup`] when the scan completes.
pub struct Sandbox {
    docker: bollard::Docker,
    scan_id: String,
    network_name: String,
    #[allow(dead_code)]
    llama_port: u16,
}

impl Sandbox {
    /// Create a new scan-scoped sandbox context.
    ///
    /// Connects to the Docker-Engine-API socket from the supplied preflight
    /// probe, creates the per-scan internal Docker network with iptables
    /// port-pinning, and verifies the rules are in effect via a negative
    /// probe before returning.
    ///
    /// # Errors
    ///
    /// [`SandboxError::UnsupportedRuntime`] if the detected runtime cannot
    /// install structural iptables/nftables port-pin rules.
    /// [`SandboxError::NetworkSetup`] if the custom network cannot be created.
    pub async fn new(probe: &runtime::RuntimeProbe, llama_port: u16) -> Result<Self, SandboxError> {
        let socket_path = probe
            .socket_path
            .to_str()
            .ok_or_else(|| SandboxError::NetworkSetup("non-UTF8 socket path".into()))?;
        let docker =
            bollard::Docker::connect_with_unix(socket_path, 5, bollard::API_DEFAULT_VERSION)
                .map_err(|e| SandboxError::NetworkSetup(format!("bollard connect: {e}")))?;

        let pid = nix::unistd::Pid::this().as_raw();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let scan_id = format!("{pid}-{ts}");

        let network_name = network::create_scan_network(&docker, &scan_id, llama_port).await?;

        Ok(Self {
            docker,
            scan_id,
            network_name,
            llama_port,
        })
    }

    /// Run one task inside an ephemeral container.
    ///
    /// Pulls the container image if not local (verifying digest), creates
    /// the container with the workspace bind-mount and the per-scan network,
    /// starts it, waits for it to exit, force-removes it, and returns the
    /// outcome. The container is removed in all exit paths including errors.
    ///
    /// # Errors
    ///
    /// [`SandboxError::ImagePull`] on pull or digest-verification failure.
    /// [`SandboxError::ContainerCreate`] on container creation or start failure.
    pub async fn run_task(
        &self,
        image_digest: &str,
        workspace_path: &std::path::Path,
    ) -> Result<TaskOutcome, SandboxError> {
        image::ensure_image(&self.docker, image_digest).await?;
        container::run_container(
            &self.docker,
            image_digest,
            workspace_path,
            &self.network_name,
            &self.scan_id,
        )
        .await
    }

    /// Remove the per-scan Docker network.
    ///
    /// Call this once after the last [`Sandbox::run_task`] for the scan.
    /// Best-effort: logs errors via [`tracing::warn!`] but does not propagate.
    pub async fn cleanup(&self) {
        network::remove_scan_network(&self.docker, &self.network_name).await;
    }
}

/// Errors crossing the [`crate::sandbox`] module boundary.
///
/// One variant per concrete failure mode the sandbox layer surfaces.
/// Adding a variant is a public-API change; downstream code that
/// `match`-es on this enum must be updated in the same change.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// Preflight probe of the container runtime socket precedence chain
    /// failed to reach any compatible runtime.
    #[error("preflight failed: {0}")]
    Preflight(#[from] runtime::PreflightError),

    /// Image pull from GHCR failed, or the pulled image's digest does not match
    /// [`crate::constants::CONTAINER_IMAGE_DIGEST`].
    #[error("image pull failed: {0}")]
    ImagePull(String),

    /// Custom Docker network creation or iptables/nftables rule installation failed.
    #[error("network setup failed: {0}")]
    NetworkSetup(String),

    /// The detected container runtime does not support structural iptables
    /// port-pin rule injection. lcrc cannot run safely on this runtime.
    #[error("unsupported runtime: {0}")]
    UnsupportedRuntime(String),

    /// Ephemeral container creation, start, or removal via bollard failed.
    #[error("container error: {0}")]
    ContainerCreate(String),
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::SandboxError;
    use super::runtime::{PrecedenceLayer, PreflightError, ProbeAttempt, ProbeFailure};
    use std::path::PathBuf;

    #[test]
    fn display_passes_preflight_error_through_with_single_prefix() {
        let attempts = vec![ProbeAttempt {
            source: PrecedenceLayer::DefaultDockerSock,
            socket_path: PathBuf::from("/var/run/docker.sock"),
            failure: ProbeFailure::SocketFileMissing,
        }];
        let err = SandboxError::Preflight(PreflightError::NoRuntimeReachable { attempts });
        let rendered = err.to_string();
        assert!(rendered.starts_with("preflight failed: "));
        assert_eq!(rendered.matches("preflight failed:").count(), 1);
    }

    #[test]
    fn image_pull_error_display_starts_with_prefix() {
        let err = SandboxError::ImagePull("digest mismatch".into());
        assert!(err.to_string().starts_with("image pull failed: "));
    }

    #[test]
    fn network_setup_error_display_starts_with_prefix() {
        let err = SandboxError::NetworkSetup("create_network: connection refused".into());
        assert!(err.to_string().starts_with("network setup failed: "));
    }

    #[test]
    fn unsupported_runtime_error_display_starts_with_prefix() {
        let err = SandboxError::UnsupportedRuntime("structural port-pin unavailable".into());
        assert!(err.to_string().starts_with("unsupported runtime: "));
    }

    #[test]
    fn container_create_error_display_starts_with_prefix() {
        let err = SandboxError::ContainerCreate("create: connection refused".into());
        assert!(err.to_string().starts_with("container error: "));
    }
}
