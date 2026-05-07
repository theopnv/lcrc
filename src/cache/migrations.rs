//! `PRAGMA user_version` migration discipline and the `open`/init entry
//! point.
//!
//! [`open`] is the consumer-facing entry point: it opens (or creates) the
//! `SQLite` file, enables WAL journal mode, and applies any pending migrations
//! transactionally. NFR-R3 (cache durable across patch upgrades) is the
//! binding requirement satisfied here — re-opening a cache whose
//! `user_version` matches `SCHEMA_VERSION` is a no-op.
//!
//! The function is **synchronous** by design. `rusqlite` is sync C bindings
//! (the locked `SQLite` driver); the consumer layer wraps `open` and other
//! cache calls in `tokio::task::spawn_blocking`. Keeping the primitive sync
//! avoids forcing a tokio runtime per integration test.

use std::path::Path;

use rusqlite::Connection;

use crate::cache::CacheError;
use crate::cache::schema::CELLS_DDL_V1;

/// Ordered list of migration scripts. Index `[N]` holds the script that
/// brings the cache from `user_version = N` to `user_version = N + 1`.
///
/// Adding a v2 migration in a future story appends `CELLS_DDL_V2` here;
/// [`SCHEMA_VERSION`] tracks the slice length automatically, so both
/// transitions stay structurally tied.
const MIGRATIONS: &[&str] = &[CELLS_DDL_V1];

/// The schema version this lcrc build supports. Equal to `MIGRATIONS.len()`.
/// Used by [`open`] to decide whether to migrate, no-op, or refuse a
/// future-schema cache.
// `MIGRATIONS.len()` is bounded by hand-edits to a const slice; truncation
// from `usize` to `u32` is structurally impossible.
#[allow(clippy::cast_possible_truncation)]
pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

/// Open or create the cache database at `path`, enable WAL journal mode,
/// and apply any pending migrations.
///
/// The returned [`Connection`] is owned by the caller; rusqlite drops it
/// (closing the `SQLite` handle) at end-of-scope.
///
/// The caller is responsible for ensuring `path.parent()` exists. The CLI
/// layer wires `tokio::fs::create_dir_all` upstream of this call; calling
/// `open` against a path whose parent directory is missing returns
/// `Err(CacheError::Open { source: ... })`. This keeps `open` free of any
/// `std::fs` calls so it does not bridge sync I/O into the async consumer.
///
/// # Errors
///
/// - [`CacheError::Open`] — `Connection::open` failed (parent dir missing,
///   permission denied, file exists but is not a `SQLite` database, etc.).
/// - [`CacheError::Pragma`] — enabling WAL or reading `PRAGMA user_version`
///   failed.
/// - [`CacheError::MigrationFailed`] — a migration script or its
///   transaction commit failed.
/// - [`CacheError::FutureSchema`] — on-disk `user_version` exceeds
///   [`SCHEMA_VERSION`].
pub fn open(path: &Path) -> Result<Connection, CacheError> {
    let mut conn = Connection::open(path).map_err(|source| CacheError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    // Refuse a future-schema cache before mutating any state. Setting
    // `PRAGMA journal_mode = WAL` materialises the `-wal` / `-shm`
    // sidecars; deferring it until after the version gate keeps the
    // file untouched on the FutureSchema path.
    let current = read_user_version(&conn)?;
    if current > SCHEMA_VERSION {
        return Err(CacheError::FutureSchema {
            found: current,
            expected: SCHEMA_VERSION,
        });
    }
    enable_wal(&conn)?;
    apply_migrations(&mut conn)?;
    Ok(conn)
}

/// Set the journal mode to WAL and verify `SQLite` accepted it.
///
/// `PRAGMA journal_mode = WAL;` returns the now-active mode as a single
/// row. On file-backed DBs it is `"wal"`; on `:memory:` it falls back to
/// `"memory"`; on a read-only filesystem to `"delete"`. Only `"wal"`
/// satisfies AC1.
fn enable_wal(conn: &Connection) -> Result<(), CacheError> {
    let mode: String = conn
        .query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))
        .map_err(|source| CacheError::Pragma { source })?;
    if !mode.eq_ignore_ascii_case("wal") {
        // `ExecuteReturnedResults` is the closest pre-existing rusqlite
        // variant for "PRAGMA returned a row but the mode it reports is not
        // what we asked for"; promoting this to a dedicated CacheError
        // variant is YAGNI until a real call site needs to distinguish.
        return Err(CacheError::Pragma {
            source: rusqlite::Error::ExecuteReturnedResults,
        });
    }
    Ok(())
}

/// Apply every migration whose target version exceeds the current on-disk
/// `user_version`. Each step runs in its own transaction so a crash mid-step
/// rolls back the DDL and the `user_version` bump together — never partial.
fn apply_migrations(conn: &mut Connection) -> Result<(), CacheError> {
    let current = read_user_version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(CacheError::FutureSchema {
            found: current,
            expected: SCHEMA_VERSION,
        });
    }
    for version in current..SCHEMA_VERSION {
        let target = version + 1;
        let script = MIGRATIONS[version as usize];
        let tx = conn
            .transaction()
            .map_err(|source| CacheError::MigrationFailed {
                version: target,
                source,
            })?;
        tx.execute_batch(script)
            .map_err(|source| CacheError::MigrationFailed {
                version: target,
                source,
            })?;
        // `PRAGMA user_version` cannot use bound parameters (`SQLite` refuses
        // `?` placeholders in PRAGMA values); format the integer in
        // directly. `target` is a u32 we own, never user input.
        tx.execute_batch(&format!("PRAGMA user_version = {target};"))
            .map_err(|source| CacheError::MigrationFailed {
                version: target,
                source,
            })?;
        tx.commit().map_err(|source| CacheError::MigrationFailed {
            version: target,
            source,
        })?;
    }
    Ok(())
}

/// Read the on-disk `PRAGMA user_version` as a `u32`. The default value on
/// a fresh database is `0`.
fn read_user_version(conn: &Connection) -> Result<u32, CacheError> {
    conn.query_row("PRAGMA user_version;", [], |row| row.get::<_, u32>(0))
        .map_err(|source| CacheError::Pragma { source })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{CacheError, MIGRATIONS, SCHEMA_VERSION, apply_migrations, read_user_version};
    use rusqlite::Connection;

    fn in_memory() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn apply_migrations_on_empty_db_bumps_user_version_to_schema_version() {
        let mut conn = in_memory();
        assert_eq!(read_user_version(&conn).unwrap(), 0);
        apply_migrations(&mut conn).unwrap();
        assert_eq!(read_user_version(&conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn apply_migrations_idempotent_when_user_version_equals_schema_version() {
        let mut conn = in_memory();
        apply_migrations(&mut conn).unwrap();
        let after_first = read_user_version(&conn).unwrap();
        // Second call must be a no-op: zero migration steps run because the
        // `current..SCHEMA_VERSION` range is empty.
        apply_migrations(&mut conn).unwrap();
        let after_second = read_user_version(&conn).unwrap();
        assert_eq!(after_first, SCHEMA_VERSION);
        assert_eq!(after_second, SCHEMA_VERSION);
    }

    #[test]
    fn apply_migrations_returns_future_schema_when_user_version_above_schema_version() {
        let mut conn = in_memory();
        let synthetic = SCHEMA_VERSION + 7;
        conn.execute_batch(&format!("PRAGMA user_version = {synthetic};"))
            .unwrap();
        let err = apply_migrations(&mut conn).unwrap_err();
        match err {
            CacheError::FutureSchema { found, expected } => {
                assert_eq!(found, synthetic);
                assert_eq!(expected, SCHEMA_VERSION);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn future_schema_display_locks_upgrade_lcrc_substring() {
        let err = CacheError::FutureSchema {
            found: 99,
            expected: 1,
        };
        let rendered = err.to_string();
        assert!(
            rendered.contains("upgrade lcrc"),
            "Display rendering {rendered:?} missing the 'upgrade lcrc' contract substring"
        );
    }

    #[test]
    fn cells_table_columns_match_architecture_spec() {
        let mut conn = in_memory();
        apply_migrations(&mut conn).unwrap();

        let mut stmt = conn.prepare("PRAGMA table_info(cells);").unwrap();
        let rows: Vec<(String, String, bool)> = stmt
            .query_map([], |row| {
                let name: String = row.get(1)?;
                let ty: String = row.get(2)?;
                let notnull: i64 = row.get(3)?;
                Ok((name, ty, notnull != 0))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();

        let expected: Vec<(&str, &str, bool)> = vec![
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

        assert_eq!(rows.len(), expected.len(), "column count mismatch");
        for (i, (got, want)) in rows.iter().zip(expected.iter()).enumerate() {
            assert_eq!(
                (got.0.as_str(), got.1.as_str(), got.2),
                (want.0, want.1, want.2),
                "column #{i} mismatch: got {got:?}, want {want:?}"
            );
        }
    }

    #[test]
    fn cells_table_primary_key_is_seven_dimension() {
        let mut conn = in_memory();
        apply_migrations(&mut conn).unwrap();

        let mut stmt = conn.prepare("PRAGMA table_info(cells);").unwrap();
        let pk_pairs: Vec<(String, i64)> = stmt
            .query_map([], |row| {
                let name: String = row.get(1)?;
                let pk: i64 = row.get(5)?;
                Ok((name, pk))
            })
            .unwrap()
            .map(Result::unwrap)
            .filter(|(_, pk)| *pk > 0)
            .collect();

        let mut sorted = pk_pairs.clone();
        sorted.sort_by_key(|(_, pk)| *pk);

        let expected: Vec<(&str, i64)> = vec![
            ("machine_fingerprint", 1),
            ("model_sha", 2),
            ("backend_build", 3),
            ("params_hash", 4),
            ("task_id", 5),
            ("harness_version", 6),
            ("task_subset_version", 7),
        ];

        assert_eq!(sorted.len(), 7, "expected 7 PK columns, got {sorted:?}");
        for (i, (got, want)) in sorted.iter().zip(expected.iter()).enumerate() {
            assert_eq!(
                (got.0.as_str(), got.1),
                (want.0, want.1),
                "PK position #{i} mismatch: got {got:?}, want {want:?}"
            );
        }
    }

    #[test]
    fn schema_version_equals_migrations_len() {
        // Structural pin: `SCHEMA_VERSION` is derived from `MIGRATIONS.len()`,
        // so a maintainer who adds a migration without touching the const
        // still gets the correct version. This test would only fail if
        // someone replaced the derived const with a hard-coded number.
        assert_eq!(SCHEMA_VERSION as usize, MIGRATIONS.len());
    }
}
