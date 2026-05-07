//! Per-task isolation envelope.
//!
//! The `sandbox` module owns the per-task isolation envelope. Submodules:
//! - `runtime` — preflight detection of a reachable Docker-Engine-API-compatible
//!   socket.

pub mod runtime;

/// Errors crossing the [`crate::sandbox`] module boundary.
///
/// One variant per concrete failure mode the sandbox layer surfaces.
/// Adding a variant is a public-API change; downstream code that
/// `match`-es on this enum must be updated in the same change.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// Preflight probe of the container runtime socket precedence chain
    /// failed to reach any compatible runtime.
    #[error("preflight failed: {0}")]
    Preflight(#[from] runtime::PreflightError),
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
}
