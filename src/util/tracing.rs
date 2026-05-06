//! `util::tracing` — install the global `tracing-subscriber`.
//!
//! Architecture spec: §"Tracing / Logging" + §"stdout / stderr Discipline"
//! (FR46). Events go to **stderr** because stdout is dedicated to results
//! that downstream tools (e.g. `lcrc show --format json | jq`) consume.
//!
//! Only `src/output.rs` and the subscriber installed here may write to
//! stderr or stdout. Any module that needs to emit a user-visible
//! diagnostic from this story onward should use `tracing::info!` /
//! `tracing::warn!`; the subscriber routes the event to stderr.

use std::io::{self, IsTerminal};

use tracing_subscriber::{
    EnvFilter,
    fmt::Layer,
    layer::SubscriberExt,
    util::{SubscriberInitExt, TryInitError},
};

/// Build and install the global tracing subscriber.
///
/// - Writer: `std::io::stderr` (FR46 stderr discipline).
/// - Level: read from `RUST_LOG` env var; default `INFO`.
/// - Format: module-pathed target rendered on every event; ANSI when
///   stderr is a TTY.
///
/// Returns `Err(TryInitError)` on a second call within the same process so
/// callers can detect double-init without panicking.
///
/// # Errors
///
/// Returns [`TryInitError`] if a global subscriber has already been installed.
pub fn init() -> Result<(), TryInitError> {
    // `EnvFilter::new("info")` is infallible for a literal directive (the
    // parser only fails on malformed user input from `RUST_LOG`); the
    // closure body cannot itself error, so `unwrap_or_else` is safe and
    // avoids the `expect_used` lint.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = Layer::new()
        .with_writer(io::stderr)
        .with_target(true)
        .with_ansi(io::stderr().is_terminal());
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .try_init()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::init;

    #[test]
    fn init_then_double_init_returns_err() {
        // First call may succeed or fail depending on test execution order
        // within the same process (cargo runs all tests in one binary).
        // Either way, a *subsequent* call must not panic and must return
        // `Err`, which is the contract this test pins.
        let _first = init();
        let second = init();
        assert!(
            second.is_err(),
            "second init must return Err, got {second:?}"
        );
    }
}
