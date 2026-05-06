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
