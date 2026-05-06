//! `lcrc` — local-only LLM coding-runtime comparison harness (library crate).

#![cfg_attr(not(test), forbid(unsafe_code))]

pub mod error;
pub mod exit_code;
pub mod output;

/// No-op orchestrator entry point.
///
/// # Errors
///
/// Returns an [`error::Error`] when the orchestrator fails.
pub fn run() -> Result<(), error::Error> {
    Ok(())
}
