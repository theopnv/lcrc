//! `lcrc` — local-only LLM coding-runtime comparison harness (library crate).
//!
//! This crate root declares the single-source-of-truth modules that lock the
//! CLI's process-exit contract (FR45) and stdout/stderr discipline (FR46) per
//! AR-28. The companion binary in `src/main.rs` is the **only** call site
//! permitted to invoke `std::process::exit`; everything else flows up here as
//! a [`Result<(), error::Error>`].
//!
//! Story 1.4 will replace the no-op [`run`] entry point with the real CLI
//! orchestration (clap parsing, tracing subscriber, subcommand dispatch).

#![cfg_attr(not(test), forbid(unsafe_code))]

pub mod error;
pub mod exit_code;
pub mod output;

/// No-op orchestrator entry point.
///
/// Story 1.4 fills this in with clap parsing + subcommand dispatch. For
/// Story 1.3 it always returns `Ok(())` so the integration test can prove
/// that a no-args invocation of the binary exits with `ExitCode::Ok`.
///
/// # Errors
///
/// Returns an [`error::Error`] when the orchestrator fails. The current
/// no-op body never errors; future bodies will.
pub fn run() -> Result<(), error::Error> {
    Ok(())
}
