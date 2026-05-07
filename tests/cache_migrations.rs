//! Integration tests for the public `lcrc::cache::migrations::open` API.
//! Verify AC1 (file creation + WAL), AC2 (cells schema via public API), AC3
//! (NFR-R3 patch durability — second open is a no-op), and AC5
//! (`FutureSchema` error + Display contract) at the library boundary.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lcrc::cache::CacheError;
use lcrc::cache::migrations::{SCHEMA_VERSION, open};
use tempfile::TempDir;

#[test]
fn creates_db_file_on_first_open() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("lcrc.db");
    assert!(!path.exists());
    let _conn = open(&path).unwrap();
    assert!(path.exists());
}

#[test]
fn enables_wal_journal_mode() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("lcrc.db");
    let conn = open(&path).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
        .unwrap();
    assert!(
        mode.eq_ignore_ascii_case("wal"),
        "journal_mode = {mode:?}, expected 'wal'"
    );
}

#[test]
fn cells_table_matches_architecture_spec_via_public_api() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("lcrc.db");
    let conn = open(&path).unwrap();

    let mut stmt = conn.prepare("PRAGMA table_info(cells);").unwrap();
    let rows: Vec<(String, String, bool, i64)> = stmt
        .query_map([], |row| {
            let name: String = row.get(1)?;
            let ty: String = row.get(2)?;
            let notnull: i64 = row.get(3)?;
            let pk: i64 = row.get(5)?;
            Ok((name, ty, notnull != 0, pk))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();

    let expected_columns: Vec<(&str, &str, bool)> = vec![
        ("machine_fingerprint", "TEXT", true),
        ("model_sha", "TEXT", true),
        ("backend_build", "TEXT", true),
        ("params_hash", "TEXT", true),
        ("task_id", "TEXT", true),
        ("harness_version", "TEXT", true),
        ("task_subset_version", "TEXT", true),
        ("container_image_id", "TEXT", true),
        ("lcrc_version", "TEXT", true),
        ("depth_tier", "TEXT", true),
        ("scan_timestamp", "TEXT", true),
        ("pass", "INTEGER", true),
        ("duration_seconds", "REAL", false),
        ("tokens_per_sec", "REAL", false),
        ("ttft_seconds", "REAL", false),
        ("peak_rss_bytes", "INTEGER", false),
        ("power_watts", "REAL", false),
        ("thermal_state", "TEXT", false),
        ("badges", "TEXT", false),
    ];

    assert_eq!(rows.len(), expected_columns.len(), "column count mismatch");
    for (i, (got, want)) in rows.iter().zip(expected_columns.iter()).enumerate() {
        assert_eq!(
            (got.0.as_str(), got.1.as_str(), got.2),
            (want.0, want.1, want.2),
            "column #{i} mismatch"
        );
    }

    let mut pk_pairs: Vec<(&str, i64)> = rows
        .iter()
        .filter(|r| r.3 > 0)
        .map(|r| (r.0.as_str(), r.3))
        .collect();
    pk_pairs.sort_by_key(|(_, pk)| *pk);
    let expected_pk: Vec<(&str, i64)> = vec![
        ("machine_fingerprint", 1),
        ("model_sha", 2),
        ("backend_build", 3),
        ("params_hash", 4),
        ("task_id", 5),
        ("harness_version", 6),
        ("task_subset_version", 7),
    ];
    assert_eq!(pk_pairs, expected_pk);
}

#[test]
fn reopen_after_first_migration_is_no_op_nfr_r3() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("lcrc.db");

    let first = open(&path).unwrap();
    let v1: u32 = first
        .query_row("PRAGMA user_version;", [], |row| row.get(0))
        .unwrap();
    let cols1: i64 = first
        .query_row(
            "SELECT count(*) FROM pragma_table_info('cells');",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(first);

    let second = open(&path).unwrap();
    let v2: u32 = second
        .query_row("PRAGMA user_version;", [], |row| row.get(0))
        .unwrap();
    let cols2: i64 = second
        .query_row(
            "SELECT count(*) FROM pragma_table_info('cells');",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(v1, SCHEMA_VERSION);
    assert_eq!(v2, SCHEMA_VERSION);
    assert_eq!(cols1, cols2);
    assert_eq!(cols1, 19);
}

#[test]
fn future_schema_version_returns_future_schema_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("lcrc.db");

    let conn = open(&path).unwrap();
    let synthetic = SCHEMA_VERSION + 1;
    conn.execute_batch(&format!("PRAGMA user_version = {synthetic};"))
        .unwrap();
    drop(conn);

    let err = open(&path).unwrap_err();
    match err {
        CacheError::FutureSchema { found, expected } => {
            assert_eq!(found, synthetic);
            assert_eq!(expected, SCHEMA_VERSION);
        }
        other => panic!("expected FutureSchema, got {other:?}"),
    }
}

#[test]
fn future_schema_error_display_contains_upgrade_lcrc_advice() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("lcrc.db");

    let conn = open(&path).unwrap();
    let synthetic = SCHEMA_VERSION + 1;
    conn.execute_batch(&format!("PRAGMA user_version = {synthetic};"))
        .unwrap();
    drop(conn);

    let err = open(&path).unwrap_err();
    let rendered = format!("{err}");
    assert!(
        rendered.contains("upgrade lcrc"),
        "Display rendering {rendered:?} missing the 'upgrade lcrc' contract substring"
    );
}
