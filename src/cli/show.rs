//! Stub for `lcrc show`; the real implementation lands in Epic 4.
//! This file exists so `lcrc show --help` works (clap-derive emits the
//! per-subcommand help from the `Subcommand` enum's `#[command(about = ...)]`).

/// Stub entry point — prints a "not implemented" diagnostic and exits 0.
///
/// # Errors
///
/// Currently infallible; the `Result` shape preserves the contract that the
/// real subcommand body (Epic 4) returns.
pub fn run() -> Result<(), crate::error::Error> {
    crate::output::diag("`lcrc show` is not yet implemented in this build (Epic 4 implements it).");
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::run;

    #[test]
    fn run_returns_ok() {
        assert!(run().is_ok());
    }
}
