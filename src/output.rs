//! `output` — the **only** module in the crate that writes to stdout/stderr.
//!
//! Per FR46 and AR-28, every user-facing write goes through one of the four
//! functions in this module. Stdout is reserved for results so that
//! pipelines like `lcrc show --format json | jq …` work; stderr carries
//! progress and diagnostics. Structured/async logging is the responsibility
//! of `tracing` (subscriber installed in Story 1.4) and is kept out of this
//! module entirely — `output` is for direct user-facing writes only.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fmt::Display;

/// Write `s` followed by a newline to stdout — the **results** band.
///
/// Use this for any output a user would pipe into another tool (`jq`, `awk`, …).
pub fn result(s: &str) {
    println!("{s}");
}

/// Write `item`'s `Display` rendering followed by a newline to stdout — the **results** band.
///
/// Convenience wrapper around [`result`] so callers do not pre-format with
/// `format!` for trivial single-value emits.
pub fn result_line<T: Display>(item: &T) {
    println!("{item}");
}

/// Write `s` followed by a newline to stderr — the **progress** band.
///
/// Use this for spinners, ETA updates, and other non-result chatter that the
/// user wants to see but tools downstream of a pipe should not.
pub fn progress(s: &str) {
    eprintln!("{s}");
}

/// Write `s` followed by a newline to stderr — the **diagnostics** band.
///
/// Use this for human-readable error messages and warnings (FR47/FR51).
/// The single permitted call site for top-level error rendering is
/// `src/main.rs`; intra-module code should propagate `Result` instead of
/// emitting through this channel directly.
pub fn diag(s: &str) {
    eprintln!("{s}");
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::result_line;

    /// Sanity smoke test that `result_line` accepts both `&str` and integer
    /// types via `Display`. The real exit-code/output-discipline assertions
    /// live in `tests/cli_exit_codes.rs`.
    #[test]
    fn result_line_accepts_displayable_types() {
        // We are not capturing stdout here; the call simply must compile and
        // not panic. End-to-end output capture lives in the integration test.
        result_line(&"hello");
        result_line(&42_i32);
        result_line(&String::from("owned"));
    }
}
