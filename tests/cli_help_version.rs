//! Integration tests for `lcrc --version`, `lcrc --help`, per-subcommand
//! `--help`, the no-args path, the unknown-subcommand error mapping, and the
//! NFR-P7 cold-latency budget.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use assert_cmd::Command;
use lcrc::exit_code::ExitCode;
use predicates::prelude::predicate;

#[test]
fn version_prints_lcrc_and_build_to_stdout() {
    Command::cargo_bin("lcrc")
        .unwrap()
        .arg("--version")
        .assert()
        .code(0)
        .stdout(predicate::str::starts_with("lcrc "))
        .stdout(predicate::str::contains("(build "))
        .stdout(predicate::str::contains("task source:"))
        .stdout(predicate::str::contains("harness:"))
        .stdout(predicate::str::contains("backend:"))
        .stdout(predicate::str::contains("container:"));
}

#[test]
fn help_lists_three_subcommands_on_stdout() {
    Command::cargo_bin("lcrc")
        .unwrap()
        .arg("--help")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("scan"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("verify"));
}

#[test]
fn per_subcommand_help_works() {
    Command::cargo_bin("lcrc")
        .unwrap()
        .args(["scan", "--help"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Run a measurement scan"));

    Command::cargo_bin("lcrc")
        .unwrap()
        .args(["show", "--help"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Show the cached leaderboard"));

    Command::cargo_bin("lcrc")
        .unwrap()
        .args(["verify", "--help"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Re-measure cached cells"));
}

#[test]
fn version_warm_under_200ms() {
    // AC5 (NFR-P7) is a *cold* (page-cache cleared) latency budget. Tests
    // run after `cargo build`, so the binary's pages are warm — warm wall
    // time is a *lower bound* on cold wall time, not an upper bound.
    // A warm pass therefore does not prove the cold AC; a warm regression
    // *does* prove the cold AC fails. This test gates the easy direction
    // (warm > 200 ms ⇒ cold > 200 ms) and the manual measurement in T8.5
    // (recorded in Completion Notes) covers the cold ground truth.
    //
    // We sample three times and take the min to absorb scheduler jitter on
    // shared CI runners.
    let mut samples = Vec::with_capacity(3);
    for _ in 0..3 {
        let start = Instant::now();
        Command::cargo_bin("lcrc")
            .unwrap()
            .arg("--version")
            .assert()
            .code(0);
        samples.push(start.elapsed());
    }
    let min = samples
        .iter()
        .min()
        .copied()
        .expect("loop runs 3 iterations, samples is non-empty");
    assert!(
        min < Duration::from_millis(200),
        "min warm wall time {min:?} exceeds NFR-P7 budget of 200 ms (samples: {samples:?})"
    );
}

#[test]
fn help_when_no_subcommand_exits_0() {
    Command::cargo_bin("lcrc")
        .unwrap()
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn unknown_subcommand_exits_config_error() {
    Command::cargo_bin("lcrc")
        .unwrap()
        .arg("bogus-subcommand")
        .assert()
        .code(ExitCode::ConfigError.as_i32())
        .stderr(predicate::str::contains("error:"));
}
