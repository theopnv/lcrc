//! End-to-end integration tests for `lcrc scan`.
//!
//! All tests are gated on `LCRC_INTEGRATION_TEST_SCAN=1` and
//! `LCRC_DEV_MODEL_PATH=<gguf>`. Without these env vars the tests skip
//! immediately via the `gate()` helper. A container runtime reachable via the
//! normal socket precedence chain is also required.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use lcrc::exit_code::ExitCode;

/// Returns the model path when the integration environment is configured,
/// or `None` to skip.
fn gate() -> Option<std::path::PathBuf> {
    if std::env::var("LCRC_INTEGRATION_TEST_SCAN").is_err() {
        eprintln!(
            "skipping: set LCRC_INTEGRATION_TEST_SCAN=1 \
             LCRC_DEV_MODEL_PATH=<gguf> to run"
        );
        return None;
    }
    let path = std::env::var("LCRC_DEV_MODEL_PATH").ok()?;
    Some(std::path::PathBuf::from(path))
}

/// AC1 + AC2 + AC3: first run → cache miss → cell written; second run → cache hit.
#[test]
fn scan_cache_miss_then_hit() {
    let Some(model_path) = gate() else {
        return;
    };

    let cache_dir = tempfile::TempDir::new().unwrap();

    // First run: cache miss — full pipeline executes, cell written.
    Command::cargo_bin("lcrc")
        .unwrap()
        .arg("scan")
        .env("LCRC_DEV_MODEL_PATH", &model_path)
        .env("XDG_DATA_HOME", cache_dir.path())
        .assert()
        .code(ExitCode::Ok.as_i32());

    let db_path = cache_dir.path().join("lcrc").join("lcrc.db");
    assert!(db_path.exists(), "lcrc.db must exist after first scan");

    // Second run: cache hit — no measurement, still exits 0.
    Command::cargo_bin("lcrc")
        .unwrap()
        .arg("scan")
        .env("LCRC_DEV_MODEL_PATH", &model_path)
        .env("XDG_DATA_HOME", cache_dir.path())
        .assert()
        .code(ExitCode::Ok.as_i32());
}

/// AC5: no container runtime reachable → exit 11.
#[test]
fn scan_exit_11_on_no_runtime() {
    if std::path::Path::new("/var/run/docker.sock").exists() {
        eprintln!("skipping: /var/run/docker.sock exists on this machine");
        return;
    }
    if std::env::var("DOCKER_HOST").is_ok() || std::env::var("LCRC_RUNTIME_DOCKER_HOST").is_ok() {
        eprintln!("skipping: DOCKER_HOST or LCRC_RUNTIME_DOCKER_HOST set in env");
        return;
    }

    Command::cargo_bin("lcrc")
        .unwrap()
        .arg("scan")
        .env_remove("DOCKER_HOST")
        .env_remove("LCRC_RUNTIME_DOCKER_HOST")
        .assert()
        .code(ExitCode::PreflightFailed.as_i32());
}
