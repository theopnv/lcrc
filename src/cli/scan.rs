//! Stub for `lcrc scan`; the real implementation lands in Stories 1.5–1.13.
//! This file exists so `lcrc scan --help` works (clap-derive emits the
//! per-subcommand help from the `Subcommand` enum's `#[command(about = ...)]`).

/// Stub entry point — prints a "not implemented" diagnostic and exits 0.
///
/// # Errors
///
/// Currently infallible; the `Result` shape preserves the contract that the
/// real subcommand body (Stories 1.5–1.13) returns.
pub fn run() -> Result<(), crate::error::Error> {
    crate::output::diag(
        "`lcrc scan` is not yet implemented in this build (Epic 1 stories 1.5–1.13 wire it incrementally).",
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
