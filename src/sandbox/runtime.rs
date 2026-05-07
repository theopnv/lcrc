//! Container-runtime preflight detection.
//!
//! Probes the precedence chain:
//!   1. `LCRC_RUNTIME_DOCKER_HOST` env var
//!   2. `DOCKER_HOST` env var
//!   3. `/var/run/docker.sock` (Docker Desktop / Colima / `OrbStack` default)
//!   4. Podman default per-uid socket (`$XDG_RUNTIME_DIR/podman/podman.sock`
//!      with macOS fallback to `~/.local/share/containers/podman/...`)
//!
//! The first layer whose socket accepts a `bollard` `/_ping` round-trip
//! wins. If every layer fails the function returns
//! [`PreflightError::NoRuntimeReachable`] carrying the per-layer failure
//! reasons in precedence order.
//!
//! No `--unsafe-no-sandbox` fallback exists — the sandbox is structural
//! or the scan refuses to run.

use std::path::PathBuf;

/// Identifies which layer of the precedence chain a probe attempt
/// targeted. Variant order matches probe order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrecedenceLayer {
    /// `LCRC_RUNTIME_DOCKER_HOST` env var (lcrc-specific override).
    LcrcRuntimeDockerHost,
    /// `DOCKER_HOST` env var (Docker convention; read explicitly to keep the
    /// precedence-chain fully under our control).
    DockerHost,
    /// Hardcoded `/var/run/docker.sock` path.
    DefaultDockerSock,
    /// Podman default per-uid socket (XDG-resolved at probe time).
    PodmanDefaultSock,
}

impl PrecedenceLayer {
    /// Stable display name used in [`tracing`] structured fields and
    /// the `NoRuntimeReachable` Display rendering.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::LcrcRuntimeDockerHost => "lcrc_runtime_docker_host",
            Self::DockerHost => "docker_host",
            Self::DefaultDockerSock => "default_docker_sock",
            Self::PodmanDefaultSock => "podman_default_sock",
        }
    }
}

/// Concrete failure mode for a single layer of the precedence chain.
/// Variants are ordered by where the probe stopped: env-resolution
/// failure, then filesystem check, then connect, then ping.
#[derive(Debug, thiserror::Error)]
pub enum ProbeFailure {
    /// The env-var-backed layer (`LCRC_RUNTIME_DOCKER_HOST` or `DOCKER_HOST`)
    /// is unset. Path-based layers cannot produce this variant.
    #[error("env var unset")]
    EnvVarUnset,
    /// Resolved socket path does not exist (or is not a Unix socket).
    #[error("socket file missing")]
    SocketFileMissing,
    /// Connect to the Unix socket failed at the OS layer (permission
    /// denied, ECONNREFUSED, EHOSTUNREACH).
    #[error("connect failed: {source}")]
    ConnectFailed {
        /// Underlying I/O error returned by the bollard / tokio connector.
        #[source]
        source: std::io::Error,
    },
    /// Connect succeeded but the `/_ping` request did not return a
    /// 2xx response — the listener is not a Docker-Engine-API server.
    #[error("ping failed: {source}")]
    PingFailed {
        /// Underlying bollard error (HTTP status, parse failure, …).
        #[source]
        source: bollard::errors::Error,
    },
}

/// Per-layer record of a probe attempt. One [`ProbeAttempt`] is recorded
/// per layer in the precedence chain on total failure.
/// [`PreflightError::NoRuntimeReachable`] carries one [`ProbeAttempt`]
/// per layer in precedence order.
#[derive(Debug)]
pub struct ProbeAttempt {
    /// Which layer of the precedence chain this attempt targeted.
    pub source: PrecedenceLayer,
    /// Resolved candidate socket path. For env-var-backed layers that are
    /// unset this is an empty `PathBuf`.
    pub socket_path: PathBuf,
    /// Concrete failure reason.
    pub failure: ProbeFailure,
}

/// Result of a successful preflight probe.
#[derive(Debug, Clone)]
pub struct RuntimeProbe {
    /// Absolute socket path that responded successfully to `/_ping`.
    pub socket_path: PathBuf,
    /// Layer of the precedence chain that produced [`Self::socket_path`].
    pub source: PrecedenceLayer,
}

/// Errors returned by [`detect`].
#[derive(Debug, thiserror::Error)]
pub enum PreflightError {
    /// Every layer of the precedence chain failed. Carries one
    /// [`ProbeAttempt`] per layer in precedence order so the
    /// diagnostic can replay the chain top-to-bottom.
    #[error("{}", crate::sandbox::runtime::format_no_runtime_reachable(attempts))]
    NoRuntimeReachable {
        /// Per-layer failure records, in precedence order. Always exactly
        /// four entries (one per [`PrecedenceLayer`] variant).
        attempts: Vec<ProbeAttempt>,
    },
}

/// Formats the error message for [`PreflightError::NoRuntimeReachable`].
///
/// Produces a multi-line string with a layer-by-layer breakdown and
/// hardcoded Podman setup instructions.
pub(crate) fn format_no_runtime_reachable(attempts: &[ProbeAttempt]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(512);
    let _ = writeln!(s, "no container runtime reachable. lcrc tried (in order):");
    for (i, a) in attempts.iter().enumerate() {
        let label = match a.source {
            PrecedenceLayer::LcrcRuntimeDockerHost => "LCRC_RUNTIME_DOCKER_HOST".to_string(),
            PrecedenceLayer::DockerHost => "DOCKER_HOST".to_string(),
            PrecedenceLayer::DefaultDockerSock | PrecedenceLayer::PodmanDefaultSock => {
                a.socket_path.display().to_string()
            }
        };
        let _ = writeln!(s, "  {}. {}: {}", i + 1, label, a.failure);
    }
    s.push('\n');
    let _ = writeln!(s, "To install Podman (the recommended runtime):");
    let _ = writeln!(s, "  brew install podman");
    let _ = writeln!(s, "  podman machine init");
    let _ = writeln!(s, "  podman machine start");
    s.push('\n');
    let _ = writeln!(s, "Then re-run `lcrc scan`.");
    s
}

/// Abstraction over the environment so tests can drive the precedence
/// chain deterministically without mutating process-global state.
/// Production callers pass [`SystemEnv`]; tests pass an in-memory map.
pub trait EnvSource: Send + Sync {
    /// Returns `Some(value)` if the named env var is set and non-empty,
    /// `None` otherwise. Empty-string values are treated as unset
    /// (matches POSIX shell `${VAR:?}` semantics — an empty `DOCKER_HOST`
    /// is a misconfiguration, not a deliberate path).
    fn get(&self, name: &str) -> Option<String>;
}

/// Production [`EnvSource`] backed by [`std::env::var`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemEnv;

impl EnvSource for SystemEnv {
    fn get(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    }
}

/// Internal: a single layer's pre-resolution result.
#[derive(Debug)]
pub(crate) struct Candidate {
    /// Which layer of the precedence chain this candidate represents.
    pub source: PrecedenceLayer,
    /// Resolved socket path, or `None` when an env-var-backed layer is unset.
    pub path: Option<PathBuf>,
}

/// Strips a `unix://` scheme prefix if present, leaving the raw path.
/// `DOCKER_HOST` values are conventionally `unix:///path`; we accept both
/// the raw path and the `unix://` URL form.
fn strip_unix_prefix(s: String) -> String {
    match s.strip_prefix("unix://") {
        Some(rest) => rest.to_string(),
        None => s,
    }
}

/// Resolves the Podman default per-uid socket path using XDG then OS fallbacks.
///
/// `XDG_RUNTIME_DIR` and `HOME` are read directly here because they are
/// platform-discovery variables, not user-tunable `LCRC_*` config. The
/// architecture prohibition on direct `std::env::var` applies only to the
/// `LCRC_*` namespace; infrastructure-discovery vars are exempt.
pub(crate) fn podman_default_socket_path() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(xdg).join("podman").join("podman.sock");
    }
    if cfg!(target_os = "macos")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(".local/share/containers/podman/machine/qemu/podman.sock");
    }
    let uid = nix::unistd::Uid::current().as_raw();
    PathBuf::from(format!("/run/user/{uid}/podman/podman.sock"))
}

/// Builds the fixed four-layer candidate list, resolving env vars at call time.
pub(crate) fn resolve_candidates(env: &dyn EnvSource) -> [Candidate; 4] {
    [
        Candidate {
            source: PrecedenceLayer::LcrcRuntimeDockerHost,
            path: env
                .get("LCRC_RUNTIME_DOCKER_HOST")
                .map(strip_unix_prefix)
                .map(PathBuf::from),
        },
        Candidate {
            source: PrecedenceLayer::DockerHost,
            path: env
                .get("DOCKER_HOST")
                .map(strip_unix_prefix)
                .map(PathBuf::from),
        },
        Candidate {
            source: PrecedenceLayer::DefaultDockerSock,
            path: Some(PathBuf::from("/var/run/docker.sock")),
        },
        Candidate {
            source: PrecedenceLayer::PodmanDefaultSock,
            path: Some(podman_default_socket_path()),
        },
    ]
}

/// Converts a `bollard::errors::Error` into a `std::io::Error`.
///
/// `ConnectFailed` carries `std::io::Error` so the diagnostic is
/// consistent regardless of whether the failure happened at the OS
/// layer or inside bollard's connector.
fn io_error_from_bollard(e: &bollard::errors::Error) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

/// Probes a single candidate layer. Returns `Ok(())` if the socket
/// accepts a `/_ping` round-trip, or a [`ProbeFailure`] describing why.
pub(crate) async fn probe_one(candidate: &Candidate) -> Result<(), ProbeFailure> {
    let Some(path) = candidate.path.as_ref() else {
        return Err(ProbeFailure::EnvVarUnset);
    };

    if !tokio::fs::try_exists(path).await.unwrap_or(false) {
        return Err(ProbeFailure::SocketFileMissing);
    }

    let addr = path.to_str().ok_or_else(|| ProbeFailure::ConnectFailed {
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "non-utf8 socket path"),
    })?;

    let docker = bollard::Docker::connect_with_unix(addr, 5, bollard::API_DEFAULT_VERSION)
        .map_err(|e| ProbeFailure::ConnectFailed {
            source: io_error_from_bollard(&e),
        })?;

    docker
        .ping()
        .await
        .map_err(|source| ProbeFailure::PingFailed { source })?;

    Ok(())
}

/// Probe the four-layer precedence chain in order. Returns the first
/// layer whose Unix socket accepts a Docker-Engine-API `/_ping`
/// round-trip, or [`PreflightError::NoRuntimeReachable`] if every
/// layer fails.
///
/// `env` is the source for `LCRC_RUNTIME_DOCKER_HOST` and `DOCKER_HOST`
/// — production callers pass [`SystemEnv`]; tests inject an in-memory
/// implementation to exercise the precedence chain deterministically.
///
/// # Errors
///
/// [`PreflightError::NoRuntimeReachable`] when every layer's probe
/// fails. The error carries one [`ProbeAttempt`] per layer in
/// precedence order.
pub async fn detect(env: &dyn EnvSource) -> Result<RuntimeProbe, PreflightError> {
    let candidates = resolve_candidates(env);
    let mut attempts = Vec::with_capacity(4);
    for cand in candidates {
        match probe_one(&cand).await {
            Ok(()) => {
                let path = cand.path.unwrap_or_default();
                return Ok(RuntimeProbe {
                    socket_path: path,
                    source: cand.source,
                });
            }
            Err(failure) => {
                attempts.push(ProbeAttempt {
                    source: cand.source,
                    socket_path: cand.path.unwrap_or_default(),
                    failure,
                });
            }
        }
    }
    Err(PreflightError::NoRuntimeReachable { attempts })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Test-only [`EnvSource`] backed by an in-memory map.
    #[derive(Debug, Default)]
    struct MapEnv(Mutex<HashMap<String, String>>);

    impl MapEnv {
        fn with(pairs: &[(&str, &str)]) -> Self {
            let mut m = HashMap::new();
            for (k, v) in pairs {
                m.insert((*k).to_string(), (*v).to_string());
            }
            Self(Mutex::new(m))
        }
    }

    impl EnvSource for MapEnv {
        fn get(&self, name: &str) -> Option<String> {
            self.0
                .lock()
                .unwrap()
                .get(name)
                .cloned()
                .filter(|v| !v.is_empty())
        }
    }

    #[test]
    fn precedence_layer_name_is_stable_snake_case() {
        assert_eq!(
            PrecedenceLayer::LcrcRuntimeDockerHost.name(),
            "lcrc_runtime_docker_host"
        );
        assert_eq!(PrecedenceLayer::DockerHost.name(), "docker_host");
        assert_eq!(
            PrecedenceLayer::DefaultDockerSock.name(),
            "default_docker_sock"
        );
        assert_eq!(
            PrecedenceLayer::PodmanDefaultSock.name(),
            "podman_default_sock"
        );
    }

    #[test]
    fn strip_unix_prefix_strips_scheme_when_present() {
        assert_eq!(
            strip_unix_prefix("/var/run/docker.sock".into()),
            "/var/run/docker.sock"
        );
        assert_eq!(
            strip_unix_prefix("unix:///var/run/docker.sock".into()),
            "/var/run/docker.sock"
        );
        assert_eq!(strip_unix_prefix(String::new()), "");
    }

    #[test]
    fn resolve_candidates_uses_precedence_order_and_drops_empty_env() {
        let env = MapEnv::with(&[
            ("LCRC_RUNTIME_DOCKER_HOST", "/tmp/lcrc.sock"),
            ("DOCKER_HOST", ""),
        ]);
        let cands = resolve_candidates(&env);
        assert_eq!(cands[0].source, PrecedenceLayer::LcrcRuntimeDockerHost);
        assert_eq!(
            cands[0].path.as_deref(),
            Some(std::path::Path::new("/tmp/lcrc.sock"))
        );
        assert_eq!(cands[1].source, PrecedenceLayer::DockerHost);
        assert_eq!(cands[1].path, None);
        assert_eq!(cands[2].source, PrecedenceLayer::DefaultDockerSock);
        assert_eq!(
            cands[2].path.as_deref(),
            Some(std::path::Path::new("/var/run/docker.sock"))
        );
        assert_eq!(cands[3].source, PrecedenceLayer::PodmanDefaultSock);
        assert!(cands[3].path.is_some());
    }

    #[test]
    fn resolve_candidates_strips_unix_prefix_from_docker_host() {
        let env = MapEnv::with(&[("DOCKER_HOST", "unix:///var/run/colima.sock")]);
        let cands = resolve_candidates(&env);
        assert_eq!(
            cands[1].path.as_deref(),
            Some(std::path::Path::new("/var/run/colima.sock"))
        );
    }

    #[test]
    fn format_no_runtime_reachable_contains_setup_instructions() {
        let attempts = vec![
            ProbeAttempt {
                source: PrecedenceLayer::LcrcRuntimeDockerHost,
                socket_path: std::path::PathBuf::new(),
                failure: ProbeFailure::EnvVarUnset,
            },
            ProbeAttempt {
                source: PrecedenceLayer::DockerHost,
                socket_path: std::path::PathBuf::new(),
                failure: ProbeFailure::EnvVarUnset,
            },
            ProbeAttempt {
                source: PrecedenceLayer::DefaultDockerSock,
                socket_path: std::path::PathBuf::from("/var/run/docker.sock"),
                failure: ProbeFailure::SocketFileMissing,
            },
            ProbeAttempt {
                source: PrecedenceLayer::PodmanDefaultSock,
                socket_path: std::path::PathBuf::from("/some/podman.sock"),
                failure: ProbeFailure::SocketFileMissing,
            },
        ];
        let s = format_no_runtime_reachable(&attempts);
        assert!(s.contains("brew install podman"));
        assert!(s.contains("podman machine init"));
        assert!(s.contains("podman machine start"));
        assert!(s.contains("LCRC_RUNTIME_DOCKER_HOST"));
        assert!(s.contains("DOCKER_HOST"));
        assert!(s.contains("/var/run/docker.sock"));
        assert!(s.contains("/some/podman.sock"));
        assert!(s.contains("env var unset"));
        assert!(s.contains("socket file missing"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn detect_returns_no_runtime_reachable_when_chain_empty() {
        let env = MapEnv::with(&[]);
        if std::path::Path::new("/var/run/docker.sock").exists() {
            eprintln!("skipping: /var/run/docker.sock exists on this machine");
            return;
        }
        if podman_default_socket_path().exists() {
            eprintln!("skipping: podman socket exists on this machine");
            return;
        }
        let result = detect(&env).await;
        match result {
            Err(PreflightError::NoRuntimeReachable { attempts }) => {
                assert_eq!(attempts.len(), 4);
                assert_eq!(attempts[0].source, PrecedenceLayer::LcrcRuntimeDockerHost);
                assert!(matches!(attempts[0].failure, ProbeFailure::EnvVarUnset));
                assert_eq!(attempts[1].source, PrecedenceLayer::DockerHost);
                assert!(matches!(attempts[1].failure, ProbeFailure::EnvVarUnset));
                assert_eq!(attempts[2].source, PrecedenceLayer::DefaultDockerSock);
                assert!(matches!(
                    attempts[2].failure,
                    ProbeFailure::SocketFileMissing
                ));
                assert_eq!(attempts[3].source, PrecedenceLayer::PodmanDefaultSock);
                assert!(matches!(
                    attempts[3].failure,
                    ProbeFailure::SocketFileMissing | ProbeFailure::ConnectFailed { .. }
                ));
            }
            other => panic!("expected NoRuntimeReachable, got {other:?}"),
        }
    }
}
