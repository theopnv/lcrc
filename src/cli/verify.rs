//! Module exists so `lcrc verify --help` works — clap-derive emits the
//! per-subcommand help from the `Subcommand` enum's `#[command(about = ...)]`.

/// Entry point for `lcrc verify`.
///
/// # Errors
///
/// Currently infallible.
pub fn run() -> Result<(), crate::error::Error> {
    crate::output::diag("`lcrc verify` is not yet implemented in this build.");
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
