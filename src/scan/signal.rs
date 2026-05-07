//! SIGINT / Ctrl-C detection for the scan lifecycle.
//!
//! Exposes a single `wait_for_sigint()` future that resolves once
//! `tokio::signal::ctrl_c()` fires. The scan orchestrator races this
//! against the measurement future via `tokio::select!`.

/// Resolves once the process receives SIGINT (Ctrl-C).
///
/// Designed to be `tokio::select!`-ed against the scan future.
/// The select arm that returns `Err(crate::error::Error::AbortedBySignal)`
/// handles exit-code 3 at the `cli/scan.rs` call site.
///
/// `.unwrap_or_default()` converts `Err(io::Error)` (e.g. when signal
/// handler is not supported) to `()` — if `ctrl_c` setup fails, we treat it
/// as if the signal already fired (conservative fail-safe).
pub async fn wait_for_sigint() {
    tokio::signal::ctrl_c().await.unwrap_or_default();
}
