//! Module exists so `lcrc scan --help` works — clap-derive emits the
//! per-subcommand help from the `Subcommand` enum's `#[command(about = ...)]`.
//! The `run` body wires preflight; the rest of the scan pipeline lands in a
//! future story.

/// Entry point for `lcrc scan`.
///
/// Runs the container-runtime preflight check. On success, prints a
/// placeholder message (the full scan pipeline is wired in a future story).
/// On failure, prints the setup-instructions block to stderr and returns
/// [`crate::error::Error::Preflight`].
///
/// # Errors
///
/// Returns [`crate::error::Error::Preflight`] when the container-runtime
/// preflight detects no reachable Docker-Engine-API-compatible socket,
/// or when the async runtime fails to initialize.
pub fn run() -> Result<(), crate::error::Error> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| crate::error::Error::Preflight(format!("tokio runtime init: {e}")))?;
    runtime.block_on(async {
        match crate::sandbox::runtime::detect(&crate::sandbox::runtime::SystemEnv).await {
            Ok(probe) => {
                tracing::info!(
                    target: "lcrc::sandbox::runtime",
                    socket_path = %probe.socket_path.display(),
                    source = probe.source.name(),
                    "detected container runtime",
                );
                crate::output::diag("`lcrc scan` is not yet implemented in this build.");
                Ok(())
            }
            Err(err) => {
                crate::output::diag(&err.to_string());
                Err(crate::error::Error::Preflight(err.to_string()))
            }
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::run;
    use crate::error::Error;

    #[test]
    fn run_returns_preflight_error_when_no_runtime() {
        if std::path::Path::new("/var/run/docker.sock").exists() {
            eprintln!("skipping: /var/run/docker.sock exists on this machine");
            return;
        }
        if std::env::var("DOCKER_HOST").is_ok() || std::env::var("LCRC_RUNTIME_DOCKER_HOST").is_ok()
        {
            eprintln!("skipping: DOCKER_HOST or LCRC_RUNTIME_DOCKER_HOST set in env");
            return;
        }
        let result = run();
        match result {
            Err(Error::Preflight(msg)) => {
                assert!(
                    msg.contains("brew install podman"),
                    "expected setup instructions in error message, got: {msg}"
                );
            }
            other => panic!("expected Err(Preflight), got {other:?}"),
        }
    }
}
