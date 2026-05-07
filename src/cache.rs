//! Cache module root. Four submodules split the cache concerns:
//!
//! - [`cell`] owns the public [`Cache`](crate::cache::cell::Cache) wrapper
//!   around an open `SQLite` `Connection`, the
//!   [`Cell`](crate::cache::cell::Cell) and
//!   [`CellKey`](crate::cache::cell::CellKey) value types, and the atomic
//!   single-cell `write_cell` / `lookup_cell` primitives.
//! - [`key`] owns canonical derivation of the four cache-key components
//!   (`model_sha`, `params_hash`, `machine_fingerprint`, `backend_build`).
//! - [`schema`] owns the SQL DDL string constants for each schema version.
//! - [`migrations`] owns `PRAGMA user_version` discipline and the `open`/init
//!   entry point that creates the file, enables WAL, and applies pending
//!   migrations transactionally.
//!
//! Errors raised by SQLite-touching submodules surface as the shared
//! [`CacheError`] enum defined here at the module root, so future submodules
//! (`query`) can grow new variants without `From` ladders.

use std::path::PathBuf;

use thiserror::Error;

pub mod cell;
pub mod key;
pub mod migrations;
pub mod schema;

/// Errors raised by cache submodules that interact with the `SQLite` database.
// `module_name_repetitions` fires because the type is `CacheError` inside
// `cache`; the name is part of the public surface (re-exported as
// `cache::CacheError`) and renaming it (e.g. to `Error`) would collide with
// `crate::error::Error`.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Error)]
pub enum CacheError {
    /// Failure inside `rusqlite::Connection::open` (file open / create).
    #[error("failed to open cache database '{}': {source}", path.display())]
    Open {
        /// Database path that failed to open.
        path: PathBuf,
        /// Underlying rusqlite error.
        #[source]
        source: rusqlite::Error,
    },

    /// Failure executing a `PRAGMA` statement (WAL enable, `user_version`
    /// read, or `user_version` write outside a migration step).
    #[error("PRAGMA execution failed: {source}")]
    Pragma {
        /// Underlying rusqlite error.
        #[source]
        source: rusqlite::Error,
    },

    /// Failure applying the migration that brings `user_version` to
    /// `version`. Wraps either `execute_batch(script)`, the version-bump
    /// `PRAGMA user_version = N` write, or the surrounding transaction
    /// commit.
    #[error("migration to schema version {version} failed: {source}")]
    MigrationFailed {
        /// Target schema version that the failing step was trying to reach.
        version: u32,
        /// Underlying rusqlite error.
        #[source]
        source: rusqlite::Error,
    },

    /// On-disk `PRAGMA user_version` is greater than this lcrc build's
    /// `SCHEMA_VERSION`. The Display text contains the literal substring
    /// `"upgrade lcrc"`; the consumer story (CLI wiring) relies on this for
    /// a stable user-visible message.
    #[error(
        "cache schema version {found} is newer than this lcrc build supports \
         (this build is at v{expected}); upgrade lcrc to read this cache"
    )]
    FutureSchema {
        /// `user_version` value read from disk.
        found: u32,
        /// Highest schema version this lcrc build knows how to apply.
        expected: u32,
    },

    /// `INSERT` failed because the seven-dimension composite primary key is
    /// already present in the `cells` table. Carries the colliding
    /// [`CellKey`](crate::cache::cell::CellKey) so the Display message is
    /// fully self-describing without having to dump the source error chain.
    ///
    /// The cache layer surfaces this loudly rather than performing an
    /// `UPSERT`: the lookup-before-measure invariant plus the single-writer
    /// scan lock guarantee that a same-PK write at this layer indicates an
    /// upstream caller bug.
    #[error(
        "cache already contains a cell with this primary key \
         (machine_fingerprint={machine_fingerprint}, model_sha={model_sha}, \
         backend_build={backend_build}, params_hash={params_hash}, \
         task_id={task_id}, harness_version={harness_version}, \
         task_subset_version={task_subset_version})",
        machine_fingerprint = key.machine_fingerprint,
        model_sha = key.model_sha,
        backend_build = key.backend_build,
        params_hash = key.params_hash,
        task_id = key.task_id,
        harness_version = key.harness_version,
        task_subset_version = key.task_subset_version,
    )]
    DuplicateCell {
        /// Composite primary key whose `INSERT` collided with an existing row.
        ///
        /// Boxed because [`crate::cache::cell::CellKey`] is 168 bytes (seven
        /// owned `String`s). Inlining it would push the enum past clippy's
        /// 128-byte `result_large_err` budget and cascade onto every other
        /// `Result<_, CacheError>` in the cache module.
        key: Box<crate::cache::cell::CellKey>,
    },
}
