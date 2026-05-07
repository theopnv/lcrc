//! Module exists so `lcrc scan --help` works — clap-derive emits the
//! per-subcommand help from the `Subcommand` enum's `#[command(about = ...)]`.

/// Entry point for `lcrc scan`.
///
/// # Errors
///
/// Returns [`crate::error::Error::Preflight`] when the container-runtime
/// preflight detects no reachable Docker-Engine-API-compatible socket,
/// when `LCRC_DEV_MODEL_PATH` is unset, or when sandbox setup fails.
/// Returns [`crate::error::Error::AbortedBySignal`] on Ctrl-C.
pub fn run() -> Result<(), crate::error::Error> {
    // multi_thread required: spawn_blocking (for rusqlite sync calls)
    // deadlocks silently under current_thread.
    let runtime = tokio::runtime::Builder::new_multi_thread()
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
                crate::scan::orchestrator::run(probe).await
            }
            Err(err) => Err(crate::error::Error::Preflight(err.to_string())),
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
