//! `lcrc` — local-only LLM coding-runtime comparison harness (binary entry point).
//!
//! This is the **only** module in the crate that calls
//! [`std::process::exit`]. All errors flow up here as [`lcrc::error::Error`],
//! get rendered to the user via [`lcrc::output::diag`], and are then
//! converted to an [`lcrc::exit_code::ExitCode`] before the process is
//! surrendered. Future work that needs different exit semantics extends the
//! `Error` enum and its `From` impls; it does **not** add a second
//! `process::exit` call.
//!
//! The CLI parsing, tracing subscriber, and real `run()` body land in
//! Story 1.4. This binary's body deliberately stays plumbing-only.

fn main() {
    let code = match lcrc::run() {
        Ok(()) => lcrc::exit_code::ExitCode::Ok,
        Err(e) => {
            lcrc::output::diag(&format!("error: {e}"));
            e.exit_code()
        }
    };
    std::process::exit(code.as_i32());
}
