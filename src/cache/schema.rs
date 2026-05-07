//! SQL DDL string constants for each cache schema version.
//!
//! Each `CELLS_DDL_V<N>` constant is a single SQL string that brings the
//! cache from `user_version = N-1` to `user_version = N`. The
//! [`crate::cache::migrations`] module composes them in slice order; this
//! module's only job is to host the SQL literals so they are reviewable
//! independently of the migration framework.

/// v1 `cells` table — see the architecture spec at
/// `_bmad-output/planning-artifacts/architecture.md` § Cell schema. Keep
/// column order identical to the spec for the AC2 column-by-column
/// verification.
///
/// `CREATE TABLE IF NOT EXISTS` is defence in depth: `apply_migrations`
/// already gates by `user_version`, but `IF NOT EXISTS` ensures a re-run on
/// an already-migrated DB cannot cascade a `table already exists` error if
/// the on-disk `user_version` ever drifts from reality.
pub const CELLS_DDL_V1: &str = "\
CREATE TABLE IF NOT EXISTS cells (
    machine_fingerprint  TEXT NOT NULL,
    model_sha            TEXT NOT NULL,
    backend_build        TEXT NOT NULL,
    params_hash          TEXT NOT NULL,
    task_id              TEXT NOT NULL,
    harness_version      TEXT NOT NULL,
    task_subset_version  TEXT NOT NULL,
    container_image_id   TEXT NOT NULL,
    lcrc_version         TEXT NOT NULL,
    depth_tier           TEXT NOT NULL,
    scan_timestamp       TEXT NOT NULL,
    pass                 INTEGER NOT NULL,
    duration_seconds     REAL,
    tokens_per_sec       REAL,
    ttft_seconds         REAL,
    peak_rss_bytes       INTEGER,
    power_watts          REAL,
    thermal_state        TEXT,
    badges               TEXT,
    PRIMARY KEY (machine_fingerprint, model_sha, backend_build,
                 params_hash, task_id, harness_version, task_subset_version)
);
";
