//! [`Cell`] value type plus the [`Cache`] wrapper around an open `SQLite`
//! [`Connection`].
//!
//! [`Cache::write_cell`] performs an atomic single-transaction `INSERT` of one
//! [`Cell`]; [`Cache::lookup_cell`] performs a single-row `SELECT` keyed on
//! the seven-dimension composite primary key. Atomicity (no half-written
//! cells) and the <100 ms lookup budget at 10 000 rows are the binding
//! requirements satisfied here. Concurrent reads alongside a single writer
//! are handled at the engine layer by `SQLite` WAL mode (enabled by
//! [`crate::cache::migrations::open`]).

use std::path::Path;

use rusqlite::Connection;

use crate::cache::CacheError;

/// Composite primary key of a cache cell — the seven dimensions whose
/// equality identifies a row in the `cells` table.
///
/// Field values are produced by the canonical helpers in
/// [`crate::cache::key`]; this type carries the already-derived strings as a
/// value type so that the [`Cache::lookup_cell`] API does not require
/// fabricating placeholder values for the twelve attribute columns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CellKey {
    /// Machine fingerprint string (e.g. `"M1Pro-32GB-14gpu"`).
    pub machine_fingerprint: String,
    /// 64-char lowercase-hex SHA-256 of the model file.
    pub model_sha: String,
    /// Canonical `"<name>-<semver>+<commit_short>"` backend identifier.
    pub backend_build: String,
    /// 64-char lowercase-hex SHA-256 of the canonical params JSON.
    pub params_hash: String,
    /// Stable task identifier (e.g. `"swe-bench-pro:django-1234"`).
    pub task_id: String,
    /// Vendored mini-swe-agent harness version string.
    pub harness_version: String,
    /// Vendored task-subset (SWE-Bench Pro) version string.
    pub task_subset_version: String,
}

/// A complete cache cell: composite PK + measurement attributes.
///
/// `Eq` and `Hash` are deliberately not derived: `Cell` carries `Option<f64>`
/// fields where `f64: !Eq` (NaN is unequal to itself). `PartialEq` is
/// sufficient for the round-trip equality assertions exercised by the
/// integration tests.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// Composite primary key (seven dimensions).
    pub key: CellKey,

    /// Container image identifier (digest) used to run the task.
    pub container_image_id: String,
    /// `lcrc` semver string at scan time (e.g. `"0.1.0"`).
    pub lcrc_version: String,
    /// Depth tier that produced this cell: `"quick"` / `"standard"` / `"full"`.
    pub depth_tier: String,
    /// RFC 3339 timestamp string captured at scan time.
    pub scan_timestamp: String,
    /// Pass flag (`true` = task passed). Persisted as `INTEGER 0/1`.
    pub pass: bool,

    /// Total task wall-clock seconds; `None` if the perf collector failed.
    pub duration_seconds: Option<f64>,
    /// Tokens-per-second decode throughput; `None` if unmeasured.
    pub tokens_per_sec: Option<f64>,
    /// Time-to-first-token seconds; `None` if unmeasured.
    pub ttft_seconds: Option<f64>,
    /// Peak resident-set size in bytes; `None` if unmeasured.
    pub peak_rss_bytes: Option<i64>,
    /// Power draw in watts; `None` until the launchd helper lands.
    pub power_watts: Option<f64>,
    /// Thermal-state string (e.g. `"nominal"`); `None` if unread.
    pub thermal_state: Option<String>,
    /// Badges attached to this cell. Empty `Vec` is canonical "no badges".
    pub badges: Vec<String>,
}

/// Owned wrapper around an open `SQLite` [`Connection`] backing the cache.
///
/// Single-owner by design: cloning the wrapper would yield two handles to
/// one underlying [`Connection`], which contradicts the v1 single-writer
/// model. Concurrent readers should call [`Cache::open`] against the same
/// path; `SQLite` WAL handles engine-level concurrency.
#[derive(Debug)]
pub struct Cache {
    conn: Connection,
}

impl Cache {
    /// Open or create the cache database at `path`, enable WAL mode, and
    /// apply any pending migrations (delegates to
    /// [`crate::cache::migrations::open`]).
    ///
    /// # Errors
    ///
    /// Propagates every error variant of
    /// [`crate::cache::migrations::open`]: [`CacheError::Open`],
    /// [`CacheError::Pragma`], [`CacheError::MigrationFailed`],
    /// [`CacheError::FutureSchema`].
    pub fn open(path: &Path) -> Result<Self, CacheError> {
        let conn = crate::cache::migrations::open(path)?;
        Ok(Self { conn })
    }

    /// Atomically insert a single cell within one `SQLite` transaction.
    /// On primary-key collision, returns [`CacheError::DuplicateCell`]
    /// carrying the colliding [`CellKey`]; all other rusqlite failures (the
    /// `badges` JSON encode, statement execution, or the surrounding
    /// transaction commit) surface as [`CacheError::Pragma`].
    ///
    /// Atomicity is guaranteed by `BEGIN; INSERT; COMMIT;` bracketing — a
    /// crash between `BEGIN` and `COMMIT` rolls back via rusqlite's
    /// `Transaction` `Drop` impl, leaving no partial row visible to a
    /// subsequent [`Cache::open`].
    ///
    /// # Errors
    ///
    /// - [`CacheError::DuplicateCell`] — the seven-dimension composite PK
    ///   already exists in the `cells` table.
    /// - [`CacheError::Pragma`] — any other rusqlite failure during the
    ///   `INSERT`, `badges` JSON encoding, or transaction commit.
    pub fn write_cell(&mut self, cell: &Cell) -> Result<(), CacheError> {
        let badges_json = serde_json::to_string(&cell.badges).map_err(|source| {
            // `Vec<String>` always serialises cleanly; this branch exists so
            // that the workspace `unwrap_used` ban is honoured even on the
            // unreachable path.
            CacheError::Pragma {
                source: rusqlite::Error::ToSqlConversionFailure(Box::new(source)),
            }
        })?;

        let tx = self
            .conn
            .transaction()
            .map_err(|source| CacheError::Pragma { source })?;

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
        .map_err(|source| {
            // Discriminate the duplicate-PK case via the *extended* error
            // code. The broad `ConstraintViolation` code (19) also covers
            // UNIQUE / CHECK / NOT NULL / FOREIGN KEY violations; only
            // `SQLITE_CONSTRAINT_PRIMARYKEY` (1555) is the canonical PK
            // collision signal.
            if let rusqlite::Error::SqliteFailure(ref ffi_err, _) = source
                && ffi_err.code == rusqlite::ErrorCode::ConstraintViolation
                && ffi_err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
            {
                return CacheError::DuplicateCell {
                    key: Box::new(cell.key.clone()),
                };
            }
            CacheError::Pragma { source }
        })?;

        tx.commit()
            .map_err(|source| CacheError::Pragma { source })?;
        Ok(())
    }

    /// Look up a single cell by its composite primary key. Returns
    /// `Ok(None)` if no row matches.
    ///
    /// Reads do not require a transaction — `SQLite`'s WAL mode gives
    /// lock-free consistent reads alongside a single writer.
    ///
    /// # Errors
    ///
    /// [`CacheError::Pragma`] for any rusqlite failure (statement
    /// preparation, parameter binding, row decode, or the `badges` JSON
    /// parse).
    pub fn lookup_cell(&self, key: &CellKey) -> Result<Option<Cell>, CacheError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT machine_fingerprint, model_sha, backend_build, params_hash,
                        task_id, harness_version, task_subset_version,
                        container_image_id, lcrc_version, depth_tier, scan_timestamp,
                        pass, duration_seconds, tokens_per_sec, ttft_seconds,
                        peak_rss_bytes, power_watts, thermal_state, badges
                   FROM cells
                  WHERE machine_fingerprint = ?1 AND model_sha = ?2
                    AND backend_build = ?3 AND params_hash = ?4
                    AND task_id = ?5 AND harness_version = ?6
                    AND task_subset_version = ?7",
            )
            .map_err(|source| CacheError::Pragma { source })?;

        let row_result = stmt.query_row(
            rusqlite::params![
                key.machine_fingerprint,
                key.model_sha,
                key.backend_build,
                key.params_hash,
                key.task_id,
                key.harness_version,
                key.task_subset_version,
            ],
            decode_cell_row,
        );

        match row_result {
            Ok(cell) => Ok(Some(cell)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(source) => Err(CacheError::Pragma { source }),
        }
    }
}

/// Decode the 19-column row returned by the `lookup_cell` `SELECT` into a
/// [`Cell`]. Lives outside the `lookup_cell` body so the SQL preparation and
/// the column-by-column decode stay independently readable.
fn decode_cell_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Cell> {
    let machine_fingerprint: String = row.get(0)?;
    let model_sha: String = row.get(1)?;
    let backend_build: String = row.get(2)?;
    let params_hash: String = row.get(3)?;
    let task_id: String = row.get(4)?;
    let harness_version: String = row.get(5)?;
    let task_subset_version: String = row.get(6)?;

    let container_image_id: String = row.get(7)?;
    let lcrc_version: String = row.get(8)?;
    let depth_tier: String = row.get(9)?;
    let scan_timestamp: String = row.get(10)?;

    // Defensive non-zero check: trust producers to write 0 / 1, but accept
    // any non-zero integer as `true` so future schema relaxations do not
    // silently mis-decode.
    let pass_int: i64 = row.get(11)?;
    let pass = pass_int != 0;

    let duration_seconds: Option<f64> = row.get(12)?;
    let tokens_per_sec: Option<f64> = row.get(13)?;
    let ttft_seconds: Option<f64> = row.get(14)?;
    let peak_rss_bytes: Option<i64> = row.get(15)?;
    let power_watts: Option<f64> = row.get(16)?;
    let thermal_state: Option<String> = row.get(17)?;

    let badges_raw: Option<String> = row.get(18)?;
    let badges = match badges_raw {
        Some(s) => serde_json::from_str::<Vec<String>>(&s).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(
                18,
                rusqlite::types::Type::Text,
                Box::new(err),
            )
        })?,
        None => Vec::new(),
    };

    Ok(Cell {
        key: CellKey {
            machine_fingerprint,
            model_sha,
            backend_build,
            params_hash,
            task_id,
            harness_version,
            task_subset_version,
        },
        container_image_id,
        lcrc_version,
        depth_tier,
        scan_timestamp,
        pass,
        duration_seconds,
        tokens_per_sec,
        ttft_seconds,
        peak_rss_bytes,
        power_watts,
        thermal_state,
        badges,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{Cache, Cell, CellKey};
    use crate::cache::CacheError;
    use std::collections::HashSet;
    use tempfile::TempDir;

    fn fresh_cache() -> (TempDir, Cache) {
        let dir = TempDir::new().unwrap();
        let cache = Cache::open(&dir.path().join("lcrc.db")).unwrap();
        (dir, cache)
    }

    fn synthetic_cell(seed: u32) -> Cell {
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

    #[test]
    fn write_then_lookup_roundtrips_all_columns() {
        let (_dir, mut cache) = fresh_cache();
        let cell = synthetic_cell(0);
        cache.write_cell(&cell).unwrap();
        let got = cache.lookup_cell(&cell.key).unwrap();
        assert_eq!(got, Some(cell));
    }

    #[test]
    fn lookup_missing_key_returns_none() {
        let (_dir, cache) = fresh_cache();
        let key = synthetic_cell(7).key;
        assert_eq!(cache.lookup_cell(&key).unwrap(), None);
    }

    #[test]
    fn write_then_write_same_pk_returns_duplicate_cell() {
        let (_dir, mut cache) = fresh_cache();
        let cell = synthetic_cell(3);
        cache.write_cell(&cell).unwrap();
        let err = cache.write_cell(&cell).unwrap_err();
        match err {
            CacheError::DuplicateCell { key } => assert_eq!(*key, cell.key),
            other => panic!("expected DuplicateCell, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_cell_display_lists_all_seven_pk_dimensions() {
        let err = CacheError::DuplicateCell {
            key: Box::new(synthetic_cell(0).key),
        };
        let rendered = err.to_string();
        for col in [
            "machine_fingerprint",
            "model_sha",
            "backend_build",
            "params_hash",
            "task_id",
            "harness_version",
            "task_subset_version",
        ] {
            assert!(
                rendered.contains(col),
                "DuplicateCell Display rendering {rendered:?} missing PK column substring {col:?}"
            );
        }
    }

    #[test]
    fn cell_with_all_optional_perf_fields_some_roundtrips() {
        let (_dir, mut cache) = fresh_cache();
        let mut cell = synthetic_cell(1);
        cell.duration_seconds = Some(99.0);
        cell.tokens_per_sec = Some(123.4);
        cell.ttft_seconds = Some(0.5);
        cell.peak_rss_bytes = Some(2_147_483_648);
        cell.power_watts = Some(42.0);
        cell.thermal_state = Some("throttled".into());
        cache.write_cell(&cell).unwrap();
        let got = cache.lookup_cell(&cell.key).unwrap().unwrap();
        assert_eq!(got, cell);
    }

    #[test]
    fn cell_with_all_optional_perf_fields_none_roundtrips() {
        let (_dir, mut cache) = fresh_cache();
        let mut cell = synthetic_cell(2);
        cell.duration_seconds = None;
        cell.tokens_per_sec = None;
        cell.ttft_seconds = None;
        cell.peak_rss_bytes = None;
        cell.power_watts = None;
        cell.thermal_state = None;
        cache.write_cell(&cell).unwrap();
        let got = cache.lookup_cell(&cell.key).unwrap().unwrap();
        assert_eq!(got.duration_seconds, None);
        assert_eq!(got.tokens_per_sec, None);
        assert_eq!(got.ttft_seconds, None);
        assert_eq!(got.peak_rss_bytes, None);
        assert_eq!(got.power_watts, None);
        assert_eq!(got.thermal_state, None);
    }

    #[test]
    fn cell_with_empty_badges_roundtrips_as_empty_vec() {
        let (_dir, mut cache) = fresh_cache();
        let mut cell = synthetic_cell(4);
        cell.badges = Vec::new();
        cache.write_cell(&cell).unwrap();
        let got = cache.lookup_cell(&cell.key).unwrap().unwrap();
        assert_eq!(got.badges, Vec::<String>::new());
    }

    #[test]
    fn cell_with_multiple_badges_roundtrips_with_order_preserved() {
        let (_dir, mut cache) = fresh_cache();
        let mut cell = synthetic_cell(5);
        cell.badges = vec!["ctx-limited".into(), "thermal-throttled".into()];
        cache.write_cell(&cell).unwrap();
        let got = cache.lookup_cell(&cell.key).unwrap().unwrap();
        assert_eq!(got.badges, vec!["ctx-limited", "thermal-throttled"]);
    }

    #[test]
    fn pass_true_and_pass_false_roundtrip() {
        let (_dir, mut cache) = fresh_cache();
        let mut t = synthetic_cell(6);
        t.pass = true;
        let mut f = synthetic_cell(7);
        f.pass = false;
        cache.write_cell(&t).unwrap();
        cache.write_cell(&f).unwrap();
        let got_t = cache.lookup_cell(&t.key).unwrap().unwrap();
        let got_f = cache.lookup_cell(&f.key).unwrap().unwrap();
        assert!(got_t.pass);
        assert!(!got_f.pass);
    }

    #[test]
    fn cellkey_partial_eq_eq_hash_consistency() {
        let k1 = synthetic_cell(0).key;
        let k2 = synthetic_cell(0).key;
        assert_eq!(k1, k2);
        let mut set: HashSet<CellKey> = HashSet::new();
        set.insert(k1.clone());
        assert!(set.contains(&k2));
    }
}
