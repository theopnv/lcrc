//! Integration tests for the HTML report renderer.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lcrc::cache::cell::{Cell, CellKey};

fn synthetic_cell(pass: bool) -> Cell {
    Cell {
        key: CellKey {
            machine_fingerprint: "M1Pro-32GB-14gpu".into(),
            model_sha: "abcdef01".repeat(8),
            backend_build: "llama.cpp-b3791+a1b2c3d".into(),
            params_hash: "1".repeat(64),
            task_id: "swe-bench-pro:canary".into(),
            harness_version: "mini-swe-agent-0.1.0".into(),
            task_subset_version: "0.0.1-canary-only".into(),
        },
        container_image_id: "sha256:deadbeef".into(),
        lcrc_version: "0.0.1".into(),
        depth_tier: "quick".into(),
        scan_timestamp: "2026-05-07T00:00:00.000Z".into(),
        pass,
        duration_seconds: Some(12.5),
        tokens_per_sec: None,
        ttft_seconds: None,
        peak_rss_bytes: None,
        power_watts: None,
        thermal_state: None,
        badges: vec![],
    }
}

#[test]
fn report_snapshot_pass_cell() {
    let cell = synthetic_cell(true);
    let html = lcrc::report::render_string(&cell).unwrap();
    insta::assert_snapshot!(html);
}

#[test]
fn report_snapshot_fail_cell() {
    let cell = synthetic_cell(false);
    let html = lcrc::report::render_string(&cell).unwrap();
    insta::assert_snapshot!(html);
}

#[tokio::test(flavor = "current_thread")]
async fn report_atomic_write_creates_latest_html() {
    use tempfile::TempDir;
    let dir = TempDir::new().unwrap();
    let cell = synthetic_cell(true);
    lcrc::report::render_html(&cell, dir.path()).await.unwrap();
    let path = dir.path().join("latest.html");
    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("PASS"),
        "expected PASS in report: {content}"
    );
    assert!(
        content.contains("swe-bench-pro:canary"),
        "expected task_id: {content}"
    );
    assert!(
        content.contains("2026-05-07"),
        "expected timestamp: {content}"
    );
    assert!(!dir.path().join("latest.html.tmp").exists());
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn report_html_has_correct_permissions() {
    use std::os::unix::fs::PermissionsExt as _;
    use tempfile::TempDir;
    let dir = TempDir::new().unwrap();
    let cell = synthetic_cell(true);
    lcrc::report::render_html(&cell, dir.path()).await.unwrap();
    let meta = std::fs::metadata(dir.path().join("latest.html")).unwrap();
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(mode, 0o644, "expected 0o644, got {mode:o}");
}
