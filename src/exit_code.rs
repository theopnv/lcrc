//! `ExitCode` — single source of truth for the CLI's process-exit contract.
//!
//! The numeric discriminants are part of the public CLI contract: shell
//! callers test `$?` against these values, so they are semver-stable. The
//! 6→10 gap is intentional and must not be renumbered to make the set
//! contiguous.

use std::fmt;

/// Process exit codes for the `lcrc` CLI.
///
/// Adding, removing, or renumbering a variant is a breaking change to the
/// CLI surface.
#[repr(i32)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExitCode {
    /// Success.
    Ok = 0,
    /// A canary task failed to produce the expected baseline result.
    CanaryFailed = 1,
    /// The container sandbox observed a forbidden syscall or network egress.
    SandboxViolation = 2,
    /// Process was interrupted by SIGINT/SIGTERM.
    AbortedBySignal = 3,
    /// `lcrc show` was invoked but the cache contains no rows.
    CacheEmpty = 4,
    /// `lcrc verify` re-measured a sampled cell and observed numerical drift.
    DriftDetected = 5,
    /// User-supplied configuration failed validation.
    ConfigError = 10,
    /// Pre-flight checks (container runtime, model files, …) refused to proceed.
    PreflightFailed = 11,
    /// Another `lcrc scan` is already running against the same cache.
    ConcurrentScan = 12,
}

impl ExitCode {
    /// Returns the numeric exit code as the `i32` accepted by `std::process::exit`.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            ExitCode::Ok => "ok",
            ExitCode::CanaryFailed => "canary_failed",
            ExitCode::SandboxViolation => "sandbox_violation",
            ExitCode::AbortedBySignal => "aborted_by_signal",
            ExitCode::CacheEmpty => "cache_empty",
            ExitCode::DriftDetected => "drift_detected",
            ExitCode::ConfigError => "config_error",
            ExitCode::PreflightFailed => "preflight_failed",
            ExitCode::ConcurrentScan => "concurrent_scan",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::ExitCode;

    #[test]
    fn as_i32_matches_contract() {
        assert_eq!(ExitCode::Ok.as_i32(), 0);
        assert_eq!(ExitCode::CanaryFailed.as_i32(), 1);
        assert_eq!(ExitCode::SandboxViolation.as_i32(), 2);
        assert_eq!(ExitCode::AbortedBySignal.as_i32(), 3);
        assert_eq!(ExitCode::CacheEmpty.as_i32(), 4);
        assert_eq!(ExitCode::DriftDetected.as_i32(), 5);
        assert_eq!(ExitCode::ConfigError.as_i32(), 10);
        assert_eq!(ExitCode::PreflightFailed.as_i32(), 11);
        assert_eq!(ExitCode::ConcurrentScan.as_i32(), 12);
    }

    #[test]
    fn display_renders_snake_case() {
        assert_eq!(ExitCode::Ok.to_string(), "ok");
        assert_eq!(ExitCode::CanaryFailed.to_string(), "canary_failed");
        assert_eq!(ExitCode::SandboxViolation.to_string(), "sandbox_violation");
        assert_eq!(ExitCode::AbortedBySignal.to_string(), "aborted_by_signal");
        assert_eq!(ExitCode::CacheEmpty.to_string(), "cache_empty");
        assert_eq!(ExitCode::DriftDetected.to_string(), "drift_detected");
        assert_eq!(ExitCode::ConfigError.to_string(), "config_error");
        assert_eq!(ExitCode::PreflightFailed.to_string(), "preflight_failed");
        assert_eq!(ExitCode::ConcurrentScan.to_string(), "concurrent_scan");
    }

    /// Adding or removing an `ExitCode` without updating this match is a
    /// compile error — substitutes for a manual `#[non_exhaustive]` audit.
    #[test]
    fn variant_set_is_exhaustive() {
        for code in [
            ExitCode::Ok,
            ExitCode::CanaryFailed,
            ExitCode::SandboxViolation,
            ExitCode::AbortedBySignal,
            ExitCode::CacheEmpty,
            ExitCode::DriftDetected,
            ExitCode::ConfigError,
            ExitCode::PreflightFailed,
            ExitCode::ConcurrentScan,
        ] {
            let _name: &'static str = match code {
                ExitCode::Ok => "ok",
                ExitCode::CanaryFailed => "canary_failed",
                ExitCode::SandboxViolation => "sandbox_violation",
                ExitCode::AbortedBySignal => "aborted_by_signal",
                ExitCode::CacheEmpty => "cache_empty",
                ExitCode::DriftDetected => "drift_detected",
                ExitCode::ConfigError => "config_error",
                ExitCode::PreflightFailed => "preflight_failed",
                ExitCode::ConcurrentScan => "concurrent_scan",
            };
        }
    }
}
