//! Integration tests for the process-exit contract.
//!
//! [`exit_code_enum_full_contract`] re-imports [`lcrc::exit_code::ExitCode`]
//! from the library crate and asserts every variant's discriminant — this
//! catches accidental loss of `pub` visibility on the enum, which the
//! in-module test in `src/exit_code.rs` cannot.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use lcrc::exit_code::ExitCode;

#[test]
fn ok_path_exits_0() {
    Command::cargo_bin("lcrc")
        .unwrap()
        .assert()
        .code(ExitCode::Ok.as_i32());
}

#[test]
fn exit_code_enum_full_contract() {
    assert_eq!(ExitCode::Ok.as_i32(), 0);
    assert_eq!(ExitCode::CanaryFailed.as_i32(), 1);
    assert_eq!(ExitCode::SandboxViolation.as_i32(), 2);
    assert_eq!(ExitCode::AbortedBySignal.as_i32(), 3);
    assert_eq!(ExitCode::CacheEmpty.as_i32(), 4);
    assert_eq!(ExitCode::DriftDetected.as_i32(), 5);
    assert_eq!(ExitCode::ConfigError.as_i32(), 10);
    assert_eq!(ExitCode::PreflightFailed.as_i32(), 11);
    assert_eq!(ExitCode::ConcurrentScan.as_i32(), 12);
}

#[test]
fn scan_exits_11_with_setup_instructions_when_no_runtime() {
    if std::path::Path::new("/var/run/docker.sock").exists() {
        eprintln!("skipping: /var/run/docker.sock exists on this machine");
        return;
    }
    if std::env::var("DOCKER_HOST").is_ok() || std::env::var("LCRC_RUNTIME_DOCKER_HOST").is_ok() {
        eprintln!("skipping: DOCKER_HOST or LCRC_RUNTIME_DOCKER_HOST set");
        return;
    }
    let output = Command::cargo_bin("lcrc")
        .unwrap()
        .arg("scan")
        .env_remove("DOCKER_HOST")
        .env_remove("LCRC_RUNTIME_DOCKER_HOST")
        .assert()
        .code(ExitCode::PreflightFailed.as_i32())
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(
        stderr.contains("brew install podman"),
        "stderr missing `brew install podman`: {stderr}"
    );
    assert!(
        stderr.contains("podman machine init"),
        "stderr missing `podman machine init`: {stderr}"
    );
    assert!(
        stderr.contains("podman machine start"),
        "stderr missing `podman machine start`: {stderr}"
    );
    assert!(
        output.stdout.is_empty(),
        "expected empty stdout, got: {:?}",
        output.stdout
    );
}

#[test]
fn scan_exits_11_on_unsupported_runtime_for_network_isolation() {
    let Ok(socket) = std::env::var("LCRC_TEST_UNSUPPORTED_RUNTIME_SOCKET") else {
        eprintln!(
            "skipping: set LCRC_TEST_UNSUPPORTED_RUNTIME_SOCKET to a non-Podman Docker socket"
        );
        return;
    };

    let output = Command::cargo_bin("lcrc")
        .unwrap()
        .arg("scan")
        .env("LCRC_RUNTIME_DOCKER_HOST", &socket)
        .assert()
        .code(ExitCode::PreflightFailed.as_i32())
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(
        stderr.contains("structural port-pin unavailable"),
        "stderr missing 'structural port-pin unavailable': {stderr}"
    );
}
