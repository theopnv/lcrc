//! `lcrc` — binary entry point.
//!
//! Sole call site for [`std::process::exit`]: every other module returns
//! `Result` so errors flow up here to be rendered and mapped to an exit code
//! exactly once.

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
