//! `error` — top-level `Error` type and its mapping to [`ExitCode`].
//!
//! Module boundaries return `thiserror`-derived typed errors that `From`-into
//! a variant of [`Error`]; intra-module application code uses
//! `anyhow::Result` with `.context()`. The single match site that converts
//! an [`Error`] to an [`ExitCode`] lives in `src/main.rs`.

use thiserror::Error;

use crate::exit_code::ExitCode;

/// Top-level error type for the `lcrc` binary.
///
/// Every variant maps to exactly one [`ExitCode`] via [`Error::exit_code`].
/// The mapping is enforced by an exhaustive `match` (no `_` arm) so adding
/// a variant elsewhere is a compile error until the dev maps it.
#[derive(Debug, Error)]
pub enum Error {
    /// Pre-flight check failed (container runtime missing, model file
    /// unreadable, …). Maps to [`ExitCode::PreflightFailed`].
    #[error("preflight failed: {0}")]
    Preflight(String),

    /// User-supplied configuration failed validation. Maps to
    /// [`ExitCode::ConfigError`].
    #[error("config error: {0}")]
    Config(String),

    /// Process was interrupted by SIGINT/SIGTERM. Maps to
    /// [`ExitCode::AbortedBySignal`].
    #[error("aborted by signal")]
    AbortedBySignal,

    /// Another `lcrc scan` is already running. The payload is the PID of
    /// the holder. Maps to [`ExitCode::ConcurrentScan`].
    #[error("concurrent scan in progress (holding pid {0})")]
    ConcurrentScan(u32),

    /// Catch-all for `anyhow::Result` propagation from intra-module code.
    /// Maps to [`ExitCode::PreflightFailed`].
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Error {
    /// Returns the [`ExitCode`] this error should produce when surrendered
    /// at `main.rs`'s top-level match.
    ///
    /// The match is exhaustive by design: a future PR that adds a new
    /// `Error` variant must also add an arm here, or the crate will not
    /// compile. This is the structural guarantee that no variant can
    /// silently fall back to an unrelated code.
    //
    // `Preflight` and `Other` deliberately map to the same `ExitCode` but
    // are kept as separate arms (not merged with `|`) so each variant has a
    // dedicated mapping site and changing the catch-all later is a
    // one-line edit. Hence `#[allow(clippy::match_same_arms)]`.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Error::Preflight(_) => ExitCode::PreflightFailed,
            Error::Config(_) => ExitCode::ConfigError,
            Error::AbortedBySignal => ExitCode::AbortedBySignal,
            Error::ConcurrentScan(_) => ExitCode::ConcurrentScan,
            Error::Other(_) => ExitCode::PreflightFailed,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::Error;
    use crate::exit_code::ExitCode;

    #[test]
    fn preflight_maps_to_preflight_failed() {
        let err = Error::Preflight("docker socket missing".into());
        assert_eq!(err.exit_code(), ExitCode::PreflightFailed);
    }

    #[test]
    fn config_maps_to_config_error() {
        let err = Error::Config("invalid TOML on line 7".into());
        assert_eq!(err.exit_code(), ExitCode::ConfigError);
    }

    #[test]
    fn aborted_by_signal_maps_to_aborted_by_signal() {
        let err = Error::AbortedBySignal;
        assert_eq!(err.exit_code(), ExitCode::AbortedBySignal);
    }

    #[test]
    fn concurrent_scan_maps_to_concurrent_scan() {
        let err = Error::ConcurrentScan(4242);
        assert_eq!(err.exit_code(), ExitCode::ConcurrentScan);
    }

    #[test]
    fn other_falls_back_to_preflight_failed() {
        let err: Error = anyhow::anyhow!("ad-hoc failure").into();
        assert_eq!(err.exit_code(), ExitCode::PreflightFailed);
    }
}
