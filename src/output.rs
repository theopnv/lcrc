//! `output` — the **only** module in the crate that writes to stdout/stderr.
//!
//! Stdout is reserved for results so pipelines like
//! `lcrc show --format json | jq …` work; stderr carries progress and
//! diagnostics. Structured/async logging belongs to `tracing` and stays out
//! of this module.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fmt::Display;

/// Write `s` followed by a newline to stdout — the **results** band.
///
/// Use this for any output a user would pipe into another tool (`jq`, `awk`, …).
pub fn result(s: &str) {
    println!("{s}");
}

/// Write `item`'s `Display` rendering followed by a newline to stdout — the **results** band.
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
/// Use this for human-readable error messages and warnings.
pub fn diag(s: &str) {
    eprintln!("{s}");
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::result_line;

    #[test]
    fn result_line_accepts_displayable_types() {
        result_line(&"hello");
        result_line(&42_i32);
        result_line(&String::from("owned"));
    }
}
