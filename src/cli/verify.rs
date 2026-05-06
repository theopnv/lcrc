//! Stub for `lcrc verify`; the real implementation lands in Epic 5.
//! This file exists so `lcrc verify --help` works (clap-derive emits the
//! per-subcommand help from the `Subcommand` enum's `#[command(about = ...)]`).

/// Stub entry point — prints a "not implemented" diagnostic and exits 0.
///
/// # Errors
///
/// Currently infallible; the `Result` shape preserves the contract that the
/// real subcommand body (Epic 5) returns.
pub fn run() -> Result<(), crate::error::Error> {
    crate::output::diag(
        "`lcrc verify` is not yet implemented in this build (Epic 5 implements it).",
    );
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
