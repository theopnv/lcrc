//! `lcrc` — local-only LLM coding-runtime comparison harness (library crate).

#![cfg_attr(not(test), forbid(unsafe_code))]

pub mod cache;
pub mod cli;
pub mod error;
pub mod exit_code;
pub mod machine;
pub mod output;
pub mod util;
pub mod version;

/// Parse the CLI and dispatch to the matched subcommand.
///
/// # Errors
///
/// Errors from clap parse-failure or subcommand execution propagate to
/// `main.rs` for exit-code mapping.
pub fn run() -> Result<(), error::Error> {
    cli::parse_and_dispatch()
}
