//! Integration tests for the public `lcrc::cache::cell` API.
//!
//! Verify AC1 (`write_cell` / `lookup_cell` round-trip via the public
//! boundary), AC2 (NFR-R2 atomicity — a transaction aborted by a panic
//! leaves no partial row visible after reopen), AC3 / AC4 (NFR-P5 lookup
//! budget at 10 000 cells), and AC5 (`DuplicateCell` on PK collision).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use lcrc::cache::CacheError;
use lcrc::cache::cell::{Cache, Cell, CellKey};
use tempfile::TempDir;

fn fresh_cache() -> (TempDir, Cache) {
    let dir = TempDir::new().unwrap();
    let cache = Cache::open(&dir.path().join("lcrc.db")).unwrap();
    (dir, cache)
}

fn cell_at(seed: u32) -> Cell {
    Cell {
        key: CellKey {
            machine_fingerprint: "M1Pro-32GB-14gpu".into(),
            model_sha: "0".repeat(64),
            backend_build: "llama.cpp-0.1.0+abcdef0".into(),
            params_hash: "1".repeat(64),
            task_id: format!("synthetic:task-{seed:06}"),
            harness_version: "mini-swe-agent-0.1.0".into(),
            task_subset_version: "swe-bench-pro-0.1.0".into(),
        },
        container_image_id: "sha256:deadbeef".into(),
        lcrc_version: "0.0.1".into(),
        depth_tier: "quick".into(),
        scan_timestamp: "2026-05-07T00:00:00Z".into(),
        pass: true,
        duration_seconds: Some(12.5),
        tokens_per_sec: Some(34.7),
        ttft_seconds: Some(0.15),
        peak_rss_bytes: Some(1_073_741_824),
        power_watts: None,
        thermal_state: Some("nominal".into()),
        badges: Vec::new(),
    }
}

fn populate_10k(cache: &mut Cache) {
    for seed in 0..10_000_u32 {
        cache.write_cell(&cell_at(seed)).unwrap();
    }
}

#[test]
fn roundtrip_single_cell_via_public_api() {
    let (_dir, mut cache) = fresh_cache();
    let cell = cell_at(0);
    cache.write_cell(&cell).unwrap();
    let got = cache.lookup_cell(&cell.key).unwrap();
    assert_eq!(got, Some(cell));
}

#[test]
fn lookup_missing_key_returns_none_via_public_api() {
    let (_dir, cache) = fresh_cache();
    let key = cell_at(13).key;
    assert_eq!(cache.lookup_cell(&key).unwrap(), None);
}

#[test]
fn transaction_rollback_on_panic_leaves_no_partial_row() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("lcrc.db");
    let cell = cell_at(42);
    let badges_json = serde_json::to_string(&cell.badges).unwrap();

    // `Connection::transaction()` requires `&mut self`; combined with
    // `catch_unwind`'s `UnwindSafe` bounds on the closure, that borrow
    // refuses to type-check. `unchecked_transaction` is rusqlite's
    // documented `&self` escape hatch — atomicity semantics are identical;
    // the "unchecked" qualifier refers to nested-tx misuse only.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let conn = lcrc::cache::migrations::open(&path).unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        tx.execute(
            "INSERT INTO cells (
                machine_fingerprint, model_sha, backend_build, params_hash,
                task_id, harness_version, task_subset_version,
                container_image_id, lcrc_version, depth_tier, scan_timestamp,
                pass, duration_seconds, tokens_per_sec, ttft_seconds,
                peak_rss_bytes, power_watts, thermal_state, badges
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17, ?18, ?19
             )",
            rusqlite::params![
                cell.key.machine_fingerprint,
                cell.key.model_sha,
                cell.key.backend_build,
                cell.key.params_hash,
                cell.key.task_id,
                cell.key.harness_version,
                cell.key.task_subset_version,
                cell.container_image_id,
                cell.lcrc_version,
                cell.depth_tier,
                cell.scan_timestamp,
                i64::from(cell.pass),
                cell.duration_seconds,
                cell.tokens_per_sec,
                cell.ttft_seconds,
                cell.peak_rss_bytes,
                cell.power_watts,
                cell.thermal_state,
                badges_json,
            ],
        )
        .unwrap();
        // Drop `tx` without commit by panicking — the `Drop` impl rolls
        // back the partially-applied INSERT.
        panic!("simulated mid-transaction abort");
    }));

    let cache = Cache::open(&path).unwrap();
    assert_eq!(cache.lookup_cell(&cell.key).unwrap(), None);
}

#[test]
fn lookup_existing_key_at_10k_cells_under_100ms_nfr_p5() {
    let (_dir, mut cache) = fresh_cache();
    populate_10k(&mut cache);
    let target_key = cell_at(7_777).key;
    let start = Instant::now();
    let result = cache.lookup_cell(&target_key).unwrap();
    let elapsed = start.elapsed();
    assert!(result.is_some(), "expected hit for seed 7_777");
    assert!(
        elapsed < Duration::from_millis(100),
        "lookup took {elapsed:?}, exceeds NFR-P5 100ms budget"
    );
}

#[test]
fn lookup_missing_key_at_10k_cells_under_100ms_nfr_p5() {
    let (_dir, mut cache) = fresh_cache();
    populate_10k(&mut cache);
    let mut missing_key = cell_at(0).key;
    missing_key.task_id = "synthetic:task-999999".into();
    let start = Instant::now();
    let result = cache.lookup_cell(&missing_key).unwrap();
    let elapsed = start.elapsed();
    assert!(result.is_none(), "expected miss for unrelated key");
    assert!(
        elapsed < Duration::from_millis(100),
        "lookup took {elapsed:?}, exceeds NFR-P5 100ms budget"
    );
}

#[test]
fn duplicate_pk_returns_duplicate_cell_via_public_api() {
    let (_dir, mut cache) = fresh_cache();
    let cell = cell_at(99);
    cache.write_cell(&cell).unwrap();
    let err = cache.write_cell(&cell).unwrap_err();
    match err {
        CacheError::DuplicateCell { key } => assert_eq!(*key, cell.key),
        other => panic!("expected DuplicateCell, got {other:?}"),
    }
}
