//! `ExitCode` — single source of truth for the FR45 process-exit contract.
//!
//! This enum locks the CLI's exit-code surface from v0.1.0. The numeric
//! discriminants are part of the public contract: scripts that test
//! `if [ $? -eq 11 ]` after invoking `lcrc` rely on these values being
//! semver-stable. The 6→10 gap is intentional (FR45) and must not be
//! renumbered.
//!
//! Per AR-28, no module outside `src/main.rs` may call `std::process::exit`,
//! and no other location in the crate may carry a bare numeric exit code.

use std::fmt;

/// Process exit codes for the `lcrc` CLI (FR45).
///
/// The numeric discriminants are the public contract; do not renumber them
/// to make the set contiguous. Adding or removing a variant is a breaking
/// change to the CLI surface.
#[repr(i32)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExitCode {
    /// Success. Wired in every story.
    Ok = 0,
    /// A canary task failed to produce the expected baseline result. Trigger path: Epic 2.
    CanaryFailed = 1,
    /// The container sandbox observed a forbidden syscall or network egress. Trigger path: Epic 2.
    SandboxViolation = 2,
    /// Process was interrupted by SIGINT/SIGTERM. Trigger path: Epic 1 (FR27, Story 2.15).
    AbortedBySignal = 3,
    /// `lcrc show` was invoked but the cache contains no rows. Trigger path: Epic 4.
    CacheEmpty = 4,
    /// `lcrc verify` re-measured a sampled cell and observed numerical drift. Trigger path: Epic 5.
    DriftDetected = 5,
    /// User-supplied configuration failed validation. Trigger path: Epic 6.
    ConfigError = 10,
    /// Pre-flight checks (container runtime, model files, …) refused to proceed. Trigger path: Epic 1 (`FR17a`, Story 1.9).
    PreflightFailed = 11,
    /// Another `lcrc scan` is already running against the same cache. Trigger path: Epic 6 (FR52).
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
    fn as_i32_matches_fr45_contract() {
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

    /// Exhaustive match guards the variant set: adding or removing an
    /// `ExitCode` without updating this match is a compile error, which
    /// substitutes for a manual `#[non_exhaustive]` audit on the FR45
    /// contract.
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
