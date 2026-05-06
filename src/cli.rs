//! `cli` — the clap-derive CLI root.
//!
//! Declares the [`Cli`] parser, the [`Command`] subcommand enum, and the
//! [`parse_and_dispatch`] helper that `lcrc::run` calls. The four subcommand
//! modules live in `src/cli/`; this file owns the top-level parser and the
//! dispatch boundary.
//!
//! Process-exit policy: clap's default `parse()` and `get_matches()` both
//! call `std::process::exit` internally. To preserve the
//! "single `process::exit` call site lives in `src/main.rs`" invariant, this
//! module uses `try_get_matches`, captures any `clap::Error` (including the
//! `--help` and `--version` non-error paths), routes the rendered output
//! through [`crate::output`], and returns a typed [`crate::error::Error`].

pub mod scan;
pub mod show;
pub mod verify;

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};

/// Top-level CLI parser.
///
/// `command` is `Option<Command>` so `lcrc` with no subcommand is not a clap
/// parse error — the no-args path renders help to stdout and exits 0,
/// preserving `tests/cli_exit_codes.rs::ok_path_exits_0`.
#[derive(Debug, Parser)]
#[command(
    name = "lcrc",
    about = "Local-only LLM coding-runtime comparison harness",
    long_about = None,
)]
pub struct Cli {
    /// The chosen subcommand, if any.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Subcommand surface. Each variant carries a unit args struct that grows
/// flags in later stories owned by their respective epics.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run a measurement scan against installed models.
    Scan(ScanArgs),
    /// Show the cached leaderboard.
    Show(ShowArgs),
    /// Re-measure cached cells to detect drift.
    Verify(VerifyArgs),
}

/// `scan` subcommand arguments. Empty in this story; flags like `--depth`,
/// `--model`, `--quiet`, `--report-path` land in their owner stories.
#[derive(Debug, Args)]
pub struct ScanArgs {}

/// `show` subcommand arguments. Empty in this story; populated in Epic 4.
#[derive(Debug, Args)]
pub struct ShowArgs {}

/// `verify` subcommand arguments. Empty in this story; populated in Epic 5.
#[derive(Debug, Args)]
pub struct VerifyArgs {}

/// Parse `std::env::args` and dispatch to the chosen subcommand.
///
/// `--help` and `--version` surface as `Err(clap::Error)` whose `kind()` is
/// `DisplayHelp`/`DisplayVersion`; both are routed to stdout via
/// [`crate::output::result`] and return `Ok(())`. Real usage errors render
/// to stderr via [`crate::output::diag`] and map to
/// [`crate::error::Error::Config`] (→ exit code 10).
///
/// # Errors
///
/// Returns [`crate::error::Error::Config`] for clap parse errors and
/// propagates any error returned by a subcommand body.
pub fn parse_and_dispatch() -> Result<(), crate::error::Error> {
    let cmd = Cli::command().long_version(crate::version::long_version_static());
    match cmd.try_get_matches() {
        Ok(matches) => {
            let cli = Cli::from_arg_matches(&matches).map_err(|e| {
                // Route the diagnostic to stderr before mapping so the user
                // sees what failed instead of a bare exit-code 10. clap's
                // own usage errors take this same path via `handle_clap_error`.
                let rendered = e.render().to_string();
                crate::output::diag(rendered.trim_end());
                crate::error::Error::Config(format!("invalid command-line arguments: {e}"))
            })?;
            dispatch(&cli)
        }
        Err(e) => handle_clap_error(&e),
    }
}

fn handle_clap_error(e: &clap::Error) -> Result<(), crate::error::Error> {
    let rendered = e.render().to_string();
    let trimmed = rendered.trim_end();
    match e.kind() {
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
            crate::output::result(trimmed);
            Ok(())
        }
        other => {
            crate::output::diag(trimmed);
            Err(crate::error::Error::Config(format!(
                "invalid command-line arguments: {other}"
            )))
        }
    }
}

fn dispatch(cli: &Cli) -> Result<(), crate::error::Error> {
    // Subscriber install is idempotent at the user-visible level: a second
    // `try_init` call across a process is `Err(TryInitError)` and harmless.
    let _ = crate::util::tracing::init();
    match &cli.command {
        Some(Command::Scan(_)) => scan::run(),
        Some(Command::Show(_)) => show::run(),
        Some(Command::Verify(_)) => verify::run(),
        None => {
            render_root_help();
            Ok(())
        }
    }
}

fn render_root_help() {
    let mut cmd = Cli::command().long_version(crate::version::long_version_static());
    let rendered = cmd.render_long_help().to_string();
    crate::output::result(rendered.trim_end());
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{Cli, Command};
    use clap::Parser;

    #[test]
    fn no_args_parses_to_none_command() {
        let cli = Cli::try_parse_from(["lcrc"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn scan_parses_to_scan_variant() {
        let cli = Cli::try_parse_from(["lcrc", "scan"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Scan(_))));
    }

    #[test]
    fn show_parses_to_show_variant() {
        let cli = Cli::try_parse_from(["lcrc", "show"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Show(_))));
    }

    #[test]
    fn verify_parses_to_verify_variant() {
        let cli = Cli::try_parse_from(["lcrc", "verify"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Verify(_))));
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        let res = Cli::try_parse_from(["lcrc", "bogus"]);
        assert!(res.is_err());
    }
}
