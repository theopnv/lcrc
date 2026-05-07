//! Cache module root. Three submodules split the cache concerns:
//!
//! - [`key`] owns canonical derivation of the four cache-key components
//!   (`model_sha`, `params_hash`, `machine_fingerprint`, `backend_build`).
//! - [`schema`] owns the SQL DDL string constants for each schema version.
//! - [`migrations`] owns `PRAGMA user_version` discipline and the `open`/init
//!   entry point that creates the file, enables WAL, and applies pending
//!   migrations transactionally.
//!
//! Errors raised by SQLite-touching submodules surface as the shared
//! [`CacheError`] enum defined here at the module root, so future submodules
//! (`cell`, `query`) can grow new variants without `From` ladders.

use std::path::PathBuf;

use thiserror::Error;

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
}
