//! Embed the short git commit into `LCRC_BUILD_COMMIT` for `lcrc::version::render`.
//!
//! Failure modes (no `git` on PATH, not a git checkout, tarball install) all
//! fall through to the literal string `"unknown"` so source distributions and
//! `cargo install` from non-git sources still build.

use std::process::Command;

fn main() {
    let commit = git_short_head().unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=LCRC_BUILD_COMMIT={commit}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    println!("cargo:rerun-if-changed=build.rs");
}

fn git_short_head() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
