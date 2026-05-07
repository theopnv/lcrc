# Story 1.7: SQLite schema + migrations framework

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a developer,
I want a SQLite cache file with `PRAGMA user_version` migration discipline and the full `cells` table schema per the architecture spec,
so that cells can be persisted with the architecture-locked PK and the cache survives lcrc patch upgrades (NFR-R3).

## Acceptance Criteria

**AC1.** **Given** lcrc is invoked for the first time
**When** the cache initializes
**Then** it creates the database file at the supplied path with WAL mode enabled (`PRAGMA journal_mode=WAL`).

**AC2.** **Given** the cache file
**When** I inspect the schema
**Then** the `cells` table matches the architecture spec — 7-dimension PK (`machine_fingerprint, model_sha, backend_build, params_hash, task_id, harness_version, task_subset_version`) plus all metadata columns (`container_image_id`, `lcrc_version`, `depth_tier`, `scan_timestamp`, `pass`, `duration_seconds`, `tokens_per_sec`, `ttft_seconds`, `peak_rss_bytes`, `power_watts` (nullable), `thermal_state`, `badges`).

**AC3.** **Given** a cache populated by lcrc `0.1.0`
**When** lcrc `0.1.1` opens it
**Then** it reads cleanly without re-running any migration (NFR-R3 patch durability). Verifiable in a single `cargo test` run by opening, closing, and re-opening the same file: the second open MUST be a no-op (`user_version` unchanged, schema unchanged, returns `Ok`).

**AC4.** **Given** a cache with `PRAGMA user_version = N` and lcrc expects `N+1`
**When** lcrc opens it
**Then** the matching numbered migration script runs and bumps `user_version` to `N+1`. In v1 the only such case is `N = 0` (fresh DB) → `N+1 = 1` (the `CELLS_DDL_V1` script applied, `user_version = 1` afterward).

**AC5.** **Given** a cache with `PRAGMA user_version` newer than this lcrc build supports
**When** lcrc opens it
**Then** `migrations::open(path)` returns `Err(CacheError::FutureSchema { found, expected })` whose `Display` text contains the literal substring `"upgrade lcrc"`. The CLI-exit half of AC5 ("the CLI exits with a clear stderr message") is satisfied at the *library boundary* in this story; CLI wiring lands in Story 1.12 (`Err(CacheError::FutureSchema)` will route through `Error::Preflight` → `ExitCode::PreflightFailed = 11`).

## Tasks / Subtasks

- [x] **T1. Update `src/cache.rs` — declare submodules + introduce `CacheError`** (AC: 1, 2, 4, 5)
  - [x] T1.1 Replace the existing `pub mod key;` line with two new module declarations preserving alphabetical order: `pub mod key; pub mod migrations; pub mod schema;`. Update the file-level `//!` doc to note that `migrations` owns `PRAGMA user_version` discipline + the open/init entry point and `schema` owns the SQL DDL constants.
  - [x] T1.2 Add a `pub enum CacheError` typed-error enum to `src/cache.rs` via `thiserror::Error`. Variants — exactly four in this story:
    - `Open { path: PathBuf, source: rusqlite::Error }` — `Connection::open` failure. Display: `"failed to open cache database '{path}': {source}"` (use `path.display()` in the format template, same pattern as `KeyError::ModelShaIo` at `src/cache/key.rs:65`).
    - `Pragma { source: rusqlite::Error }` — failure executing `PRAGMA journal_mode=WAL`, reading `PRAGMA user_version`, or writing it. Display: `"PRAGMA execution failed: {source}"`.
    - `MigrationFailed { version: u32, source: rusqlite::Error }` — `execute_batch(script)` or transaction commit failure for the migration that bumps `user_version` to `{version}`. Display: `"migration to schema version {version} failed: {source}"`.
    - `FutureSchema { found: u32, expected: u32 }` — `user_version` on disk exceeds `SCHEMA_VERSION`. Display: `"cache schema version {found} is newer than this lcrc build supports (this build is at v{expected}); upgrade lcrc to read this cache"`. The `"upgrade lcrc"` substring is the AC5 contract.
  - [x] T1.3 **Do not** add `From<CacheError> for crate::error::Error`. Same rule Story 1.5 (FingerprintError) and Story 1.6 (KeyError) followed: the boundary mapping decision (which `Error` variant — likely `Error::Preflight` for `Open` / `Pragma` / `MigrationFailed`, and a future-typed mapping for `FutureSchema`) belongs to the consumer story (Story 1.12). Pre-adding the `From` creates dead API surface and forces a mapping decision before the call site exists.
  - [x] T1.4 Apply `#[allow(clippy::module_name_repetitions)]` on `CacheError` with a `// CacheError is the public name reused across submodules; renaming it (e.g. to Error) collides with `crate::error::Error`.` comment. Same rationale Story 1.6 used at `src/cache/key.rs:58-60`.

- [x] **T2. Author `src/cache/schema.rs` — SQL DDL constants** (AC: 2)
  - [x] T2.1 File-level `//!` doc: this module owns the SQL DDL strings; each schema version's CREATE statements are constants here. The migrations module composes them in order.
  - [x] T2.2 Define `pub const CELLS_DDL_V1: &str = "..."` containing the exact DDL from `_bmad-output/planning-artifacts/architecture.md` "Cell schema (`cells` table)" (line 254). The string must declare the table with `CREATE TABLE IF NOT EXISTS cells (...)`. Columns and PK below — match the architecture's column order, types, and nullability exactly:

    | Column | Type | NOT NULL? |
    |---|---|---|
    | `machine_fingerprint` | `TEXT` | NOT NULL (PK) |
    | `model_sha` | `TEXT` | NOT NULL (PK) |
    | `backend_build` | `TEXT` | NOT NULL (PK) |
    | `params_hash` | `TEXT` | NOT NULL (PK) |
    | `task_id` | `TEXT` | NOT NULL (PK) |
    | `harness_version` | `TEXT` | NOT NULL (PK) |
    | `task_subset_version` | `TEXT` | NOT NULL (PK) |
    | `container_image_id` | `TEXT` | NOT NULL |
    | `lcrc_version` | `TEXT` | NOT NULL |
    | `depth_tier` | `TEXT` | NOT NULL |
    | `scan_timestamp` | `TEXT` | NOT NULL |
    | `pass` | `INTEGER` | NOT NULL |
    | `duration_seconds` | `REAL` | NULL |
    | `tokens_per_sec` | `REAL` | NULL |
    | `ttft_seconds` | `REAL` | NULL |
    | `peak_rss_bytes` | `INTEGER` | NULL |
    | `power_watts` | `REAL` | NULL |
    | `thermal_state` | `TEXT` | NULL |
    | `badges` | `TEXT` | NULL |
    | `PRIMARY KEY` | `(machine_fingerprint, model_sha, backend_build, params_hash, task_id, harness_version, task_subset_version)` | — |

    Use `IF NOT EXISTS` so a re-run on an already-migrated DB is structurally safe (defence in depth — `apply_migrations` already gates by `user_version`, but this prevents a corrupt user_version from cascading into a `table already exists` error). `///` doc on the constant explains: "v1 cells table — see the architecture spec at `_bmad-output/planning-artifacts/architecture.md` § Cell schema. Keep column order identical to the spec for AC2 verifiability."
  - [x] T2.3 **Do not** add a separate `CREATE INDEX` constant for any non-PK lookups (e.g. `(model_sha, depth_tier)` for `lcrc show` filters). Indexes for read-side queries land in Story 1.8 (`cell.rs` / `query.rs`) once their access patterns are concrete; pre-indexing is API speculation and changes physical layout without a measured win.
  - [x] T2.4 **Do not** declare any other tables (`scans`, `runs`, `cells_history`, etc.). v1's schema is a single `cells` table per architecture line 252-282. Future tables land in their owner stories with their own migration scripts.
  - [x] T2.5 **Do not** parameterize the DDL with placeholders (e.g. table-name templating). The string is a constant SQL literal; future migrations are appended as additional `pub const CELLS_DDL_V2: &str = "ALTER TABLE cells ADD COLUMN ...;"` — see § "Resolved decisions" below.

- [x] **T3. Author `src/cache/migrations.rs` — open/init + migration framework** (AC: 1, 3, 4, 5)
  - [x] T3.1 File-level `//!` doc: this module owns `PRAGMA user_version` discipline. `open(path)` is the consumer-facing entry point; it opens (or creates) the file, enables WAL journal mode, and applies any pending migrations transactionally. NFR-R3 (cache durable across patch upgrades) is the binding requirement.
  - [x] T3.2 Imports: `use std::path::{Path, PathBuf};`, `use rusqlite::Connection;`, `use crate::cache::CacheError;`, `use crate::cache::schema::CELLS_DDL_V1;`. Do not `use rusqlite::*` glob — the locked patterns reject globbing.
  - [x] T3.3 Declare the migration table — `const MIGRATIONS: &[&str] = &[CELLS_DDL_V1];`. Index `[N]` is "the migration that brings `user_version` from `N` to `N+1`". Adding a v2 migration in a future story appends `CELLS_DDL_V2` to this slice; `SCHEMA_VERSION` updates automatically because it is derived from `MIGRATIONS.len()`.
  - [x] T3.4 Declare the schema-version pin — `pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;`. Use `#[allow(clippy::cast_possible_truncation)]` with a `// MIGRATIONS.len() is bounded by hand-edits to a const slice; truncation is structurally impossible.` comment. `<[T]>::len()` is `const` since Rust 1.55, well below MSRV 1.95 (Cargo.toml line 5). `///` doc: "The schema version this lcrc build supports. Equal to `MIGRATIONS.len()`. Used by `Cache::open` to decide whether to migrate, no-op, or refuse a future-schema cache."
  - [x] T3.5 Implement `pub fn open(path: &Path) -> Result<Connection, CacheError>`:
    ```rust
    let mut conn = Connection::open(path).map_err(|source| CacheError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    enable_wal(&conn)?;
    apply_migrations(&mut conn)?;
    Ok(conn)
    ```
    - **Synchronous on purpose.** `rusqlite` is the locked SQLite binding (Cargo.toml line 45) and is sync; the architecture's pattern (architecture.md line 697) wraps sync rusqlite calls in `tokio::task::spawn_blocking` at the *consumer* layer (Story 1.8 / 1.12), not at the primitive. Story 1.7 must NOT introduce `async`, NOT take a tokio runtime dependency, NOT bridge sync/async internally.
    - **Caller owns parent-directory creation.** `Connection::open(path)` creates the file but does NOT `mkdir -p` its parent. A `///` doc note must say: "The caller is responsible for ensuring `path.parent()` exists (Story 1.12 wires `tokio::fs::create_dir_all` at the CLI layer). Calling `open` against a path whose parent directory is missing returns `Err(CacheError::Open { source: ... })`." This keeps the function free of `std::fs` calls (AR-3).
    - `///` doc `# Errors` section: `CacheError::Open` (file open failure), `CacheError::Pragma` (WAL or user_version PRAGMA failure), `CacheError::MigrationFailed` (DDL execution or transaction commit failure), `CacheError::FutureSchema` (`user_version > SCHEMA_VERSION`).
  - [x] T3.6 Implement private helper `fn enable_wal(conn: &Connection) -> Result<(), CacheError>`:
    ```rust
    let mode: String = conn
        .query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))
        .map_err(|source| CacheError::Pragma { source })?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(CacheError::Pragma { source: rusqlite::Error::ExecuteReturnedResults });
    }
    Ok(())
    ```
    - SQLite's `PRAGMA journal_mode = WAL;` returns the *now-active* journal mode as a single-column row. On file-backed DBs this is always `"wal"` (lowercase); on `:memory:` and read-only paths it falls back to `"memory"` / `"delete"`. We accept `"wal"` only.
    - The `Pragma { source: ExecuteReturnedResults }` synthetic error is the closest pre-existing `rusqlite::Error` variant for "WAL was not enabled despite the PRAGMA returning a row" — it preserves the typed-error chain without inventing a new variant. Future maintenance can introduce a more specific `CacheError::WalNotEnabled` if a real call site needs to distinguish; YAGNI for v1.
  - [x] T3.7 Implement private helper `fn apply_migrations(conn: &mut Connection) -> Result<(), CacheError>`:
    ```rust
    let current = read_user_version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(CacheError::FutureSchema { found: current, expected: SCHEMA_VERSION });
    }
    for version in current..SCHEMA_VERSION {
        let target = version + 1;
        let script = MIGRATIONS[version as usize];
        let tx = conn.transaction().map_err(|source| CacheError::MigrationFailed { version: target, source })?;
        tx.execute_batch(script).map_err(|source| CacheError::MigrationFailed { version: target, source })?;
        // PRAGMA user_version cannot use bound parameters; format the integer in directly.
        // Safe because `target` is a u32 we control, never user input.
        tx.execute_batch(&format!("PRAGMA user_version = {target};"))
            .map_err(|source| CacheError::MigrationFailed { version: target, source })?;
        tx.commit().map_err(|source| CacheError::MigrationFailed { version: target, source })?;
    }
    Ok(())
    ```
    - **Atomic per migration step**: `BEGIN; <DDL>; PRAGMA user_version = <target>; COMMIT;`. SQLite supports transactional DDL (CREATE TABLE inside a transaction commits or rolls back atomically), and `user_version` is stored in the database header which is itself transactional. A crash between `execute_batch(script)` and `execute_batch(PRAGMA user_version)` rolls back, leaving the cache at the prior `user_version` and prior schema — never partial.
    - **`PRAGMA user_version = N` cannot be parameterized.** SQLite refuses bound parameters in PRAGMA values; the only path is to format the integer into the SQL literal. `target` is a `u32` we own, not user input — no SQL injection vector.
  - [x] T3.8 Implement private helper `fn read_user_version(conn: &Connection) -> Result<u32, CacheError>`:
    ```rust
    conn.query_row("PRAGMA user_version;", [], |row| row.get::<_, u32>(0))
        .map_err(|source| CacheError::Pragma { source })
    ```
    Use `query_row` (not `pragma_query_value`) to keep the API surface narrow — `query_row` is the universal rusqlite read pattern and reads as one line.
  - [x] T3.9 **Do not** set `PRAGMA synchronous = NORMAL` (or `FULL` / `OFF`) in this story. WAL mode + the SQLite default `synchronous` setting is correct for v1; a tuning pass belongs in Epic 6 (config & polish) or as a `bmad-quick-dev` follow-up if profiling flags it.
  - [x] T3.10 **Do not** set `PRAGMA foreign_keys = ON` in this story. The v1 schema is a single table with no foreign-key relationships.
  - [x] T3.11 **Do not** add a `pub fn close(conn: Connection)` or similar lifecycle helper. `Connection` is RAII-dropped by rusqlite; the test surface and Story 1.8's consumer use the standard drop pattern.

- [x] **T4. In-module unit tests in `src/cache/migrations.rs`** (AC: 1, 4, 5)
  - [x] T4.1 Standard test-module attribute set: `#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] mod tests { ... }` (matches Stories 1.3 / 1.4 / 1.5 / 1.6 pattern). Tests use `Connection::open_in_memory()` for the migration-logic tests because in-memory SQLite is faster, deterministic, and exercises the same `apply_migrations` code path as file-backed DBs (the only difference is WAL mode, which `:memory:` falls back to `"memory"` — exercised separately by the file-backed integration tests in T5).
  - [x] T4.2 `apply_migrations_on_empty_db_bumps_user_version_to_schema_version` — open in-memory DB, call `apply_migrations(&mut conn)?`, assert `read_user_version(&conn)? == SCHEMA_VERSION`. Verifies AC4 fundamentals (the only `N → N+1` step in v1 is `0 → 1`).
  - [x] T4.3 `apply_migrations_idempotent_when_user_version_equals_schema_version` — open in-memory DB, call `apply_migrations` twice; both calls succeed; `user_version` stays at `SCHEMA_VERSION`. Verifies AC3 idempotency at the unit level.
  - [x] T4.4 `apply_migrations_returns_future_schema_when_user_version_above_schema_version` — open in-memory DB, manually set `user_version = SCHEMA_VERSION + 7` via `conn.execute_batch("PRAGMA user_version = ...;")?`, call `apply_migrations`, assert `Err(CacheError::FutureSchema { found: SCHEMA_VERSION + 7, expected: SCHEMA_VERSION })`. Verifies AC5 at the unit level.
  - [x] T4.5 `future_schema_display_locks_upgrade_lcrc_substring` — construct `CacheError::FutureSchema { found: 99, expected: 1 }`; assert `.to_string().contains("upgrade lcrc")`. AC5 Display contract pin (same Display-substring lesson as Story 1.5 § AC3 and Story 1.6 § ModelShaIo Display).
  - [x] T4.6 `cells_table_columns_match_architecture_spec` — open in-memory DB, `apply_migrations`, then `conn.prepare("PRAGMA table_info(cells);")`, iterate rows, collect `(name, type, notnull)` tuples, assert against the expected vector below. AC2's column-by-column verification at the unit level.
    - Expected (in PRAGMA table_info row order, which is column declaration order — SQLite guarantees this):
      ```
      ("machine_fingerprint",  "TEXT",    true),
      ("model_sha",            "TEXT",    true),
      ("backend_build",        "TEXT",    true),
      ("params_hash",          "TEXT",    true),
      ("task_id",              "TEXT",    true),
      ("harness_version",      "TEXT",    true),
      ("task_subset_version",  "TEXT",    true),
      ("container_image_id",   "TEXT",    true),
      ("lcrc_version",         "TEXT",    true),
      ("depth_tier",           "TEXT",    true),
      ("scan_timestamp",       "TEXT",    true),
      ("pass",                 "INTEGER", true),
      ("duration_seconds",     "REAL",    false),
      ("tokens_per_sec",       "REAL",    false),
      ("ttft_seconds",         "REAL",    false),
      ("peak_rss_bytes",       "INTEGER", false),
      ("power_watts",          "REAL",    false),
      ("thermal_state",        "TEXT",    false),
      ("badges",               "TEXT",    false),
      ```
  - [x] T4.7 `cells_table_primary_key_is_seven_dimension` — `PRAGMA table_info(cells);` includes a `pk` column with values `0` (not in PK) or `1, 2, 3, ...` (PK position). Assert exactly seven columns have `pk > 0` and that their (name, pk-position) pairs match `[("machine_fingerprint", 1), ("model_sha", 2), ("backend_build", 3), ("params_hash", 4), ("task_id", 5), ("harness_version", 6), ("task_subset_version", 7)]`. AC2's PK-shape verification.
  - [x] T4.8 `schema_version_equals_migrations_len` — assert `SCHEMA_VERSION as usize == MIGRATIONS.len()`. Cheap structural test; guards against a future maintainer typo'ing `SCHEMA_VERSION` away from the derived definition.
  - [x] T4.9 **Do not** add a benchmark or perf test. NFR-P5 (cache lookup <100 ms for 10K cells) is verified by Story 1.8's `tests/cache_roundtrip.rs`, not by this story.

- [x] **T5. Author `tests/cache_migrations.rs` — integration tests for the public API** (AC: 1, 2, 3, 5)
  - [x] T5.1 New file `tests/cache_migrations.rs`. Standard integration-test crate (separate compilation unit; sees `lcrc::*` only via the public API, no `pub(crate)` access). Standard exemption attribute: `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` at file top (matches `tests/cli_exit_codes.rs`, `tests/cli_help_version.rs`, `tests/machine_fingerprint.rs`). Plain `#[test]` (not `#[tokio::test]` — `migrations::open` is sync).
  - [x] T5.2 `creates_db_file_on_first_open` — uses `tempfile::TempDir::new()?` as the parent dir; `let path = dir.path().join("lcrc.db");` (parent already exists, satisfies the doc-noted contract); `assert!(!path.exists());` before; `let _conn = lcrc::cache::migrations::open(&path)?;`; `assert!(path.exists());` after. AC1's "creates the database file" half.
  - [x] T5.3 `enables_wal_journal_mode` — open the DB, then `let mode: String = conn.query_row("PRAGMA journal_mode;", [], |row| row.get(0))?;`; `assert_eq!(mode.to_lowercase(), "wal");`. AC1's "with WAL mode enabled" half.
  - [x] T5.4 `cells_table_matches_architecture_spec_via_public_api` — same column/PK assertions as T4.6 + T4.7, but reached through the public `lcrc::cache::migrations::open` entry point instead of the in-module `apply_migrations`. AC2's end-to-end verification.
  - [x] T5.5 `reopen_after_first_migration_is_no_op_NFR_R3` — open the DB once, drop the connection (`drop(conn)` or scope-exit), then reopen the same path; assert the second open returns `Ok(_)`; assert `user_version` is still `SCHEMA_VERSION`; assert the `cells` table still exists with the same column count. AC3's NFR-R3 patch durability check. The two-open flow in a single test simulates "lcrc 0.1.0 wrote, then lcrc 0.1.1 read" — both opens use the same `SCHEMA_VERSION` constant because both are this lcrc build, so the second open's `apply_migrations` loop runs zero iterations (the AC3 invariant).
  - [x] T5.6 `future_schema_version_returns_future_schema_error` — open the DB, set `user_version` to `SCHEMA_VERSION + 1` via raw `conn.execute_batch("PRAGMA user_version = ...;")?`, drop the connection, reopen — assert `Err(lcrc::cache::CacheError::FutureSchema { found, expected })` with `found == SCHEMA_VERSION + 1` and `expected == SCHEMA_VERSION`. AC5's end-to-end verification at the public API.
  - [x] T5.7 `future_schema_error_display_contains_upgrade_lcrc_advice` — same construction as T5.6 but assert `format!("{err}").contains("upgrade lcrc")`. AC5's user-visible message contract.
  - [x] T5.8 **Do not** spawn the `lcrc` binary in this test (no `assert_cmd::Command::cargo_bin("lcrc")`). The CLI wiring of the cache lives in Story 1.12; testing it here would conflate this story's primitive surface with the integration surface. The exit-code half of AC5 is owed by Story 1.12.
  - [x] T5.9 **Do not** add an `assert!(path.parent().unwrap().exists())` boilerplate check in T5.2/T5.3. `TempDir::new` guarantees the parent exists; the test's contract is "given a valid parent dir, open creates the file". The "missing parent dir → CacheError::Open" path is library-contract-only and not in the AC set.

- [x] **T6. Local CI mirror** (AC: 1, 2, 3, 4, 5)
  - [x] T6.1 Run `cargo build` — confirms the module compiles. No new dep adds; `Cargo.lock` should be unchanged (rusqlite, tempfile, thiserror are all already locked). If `Cargo.lock` does change, investigate before pushing — that signals an unintended dep introduction.
  - [x] T6.2 Run `cargo fmt` — apply rustfmt; commit any reformatted lines.
  - [x] T6.3 Run `cargo clippy --all-targets --all-features -- -D warnings` locally. Specifically watch for:
    - `clippy::cast_possible_truncation` on `MIGRATIONS.len() as u32` — suppress with the documented `#[allow]` + comment per T3.4.
    - `clippy::module_name_repetitions` on `CacheError` — suppress per T1.4.
    - `clippy::missing_errors_doc` on `pub fn open` — `# Errors` rustdoc section per T3.5.
    - `clippy::missing_docs` on every `pub` item (`open`, `SCHEMA_VERSION`, `CacheError`, every variant + field, `CELLS_DDL_V1`).
    - `clippy::needless_pass_by_value` should NOT fire — all helper params are `&Connection` / `&mut Connection`.
  - [x] T6.4 Run `cargo test` — confirms all in-module tests in `src/cache/migrations.rs::tests` pass AND the new `tests/cache_migrations.rs` integration tests pass AND every existing test in the suite (`tests/cli_exit_codes.rs`, `tests/cli_help_version.rs`, `tests/machine_fingerprint.rs`, plus the in-module suites for `cache::key`, `error`, `exit_code`, `machine`, `output`, `version`, etc.) still passes.
  - [x] T6.5 Manual scope-discipline grep: `git grep -nE 'rusqlite::|PRAGMA|user_version' src/ tests/ | grep -v '^src/cache/migrations.rs:' | grep -v '^src/cache/schema.rs:' | grep -v '^src/cache.rs:' | grep -v '^tests/cache_migrations.rs:'`. Must produce zero matches — the rusqlite + PRAGMA surface is contained inside the new modules. Same single-source-of-truth grep contract Story 1.6 used for `model_sha|params_hash|backend_build`.

## Dev Notes

### Scope discipline (read this first)

This story authors **two new files** and **updates one existing file** plus **one new test file**:

- **New (Rust source):** `src/cache/schema.rs` (DDL constants), `src/cache/migrations.rs` (open/init + migration framework + tests)
- **Updated:** `src/cache.rs` (add `pub mod schema; pub mod migrations;` declarations + `pub enum CacheError`)
- **New (tests):** `tests/cache_migrations.rs` (integration-level public-API verification)

This story does **not**:

- Wire `migrations::open` into any CLI command (`lcrc scan`, `lcrc show`, `lcrc verify`). The CLI integration is Story 1.12 (end-to-end one-cell scan) and Stories 4.1+ (`lcrc show` against the cache). Pre-wiring violates the tracer-bullet vertical-slice principle (`MEMORY.md → feedback_tracer_bullet_epics.md`) and inflates this story past its single concern.
- Author `src/cache/cell.rs` or `src/cache/query.rs`. Story 1.8 (cache cell write/read API) owns those; they will use `lcrc::cache::migrations::open` to obtain a `Connection`, then add `Cache::write_cell` / `Cache::lookup_cell` methods.
- Introduce a `pub struct Cache { conn: Connection }` wrapper. Story 1.8 (the first consumer that needs methods on the connection) decides whether `Cache` is a wrapper struct or whether the `Connection` is passed around bare. Pre-defining the struct is API speculation.
- Wrap rusqlite in `tokio::task::spawn_blocking`. The architecture's pattern (architecture.md line 697) places the async wrapping at the consumer layer (Story 1.8 / 1.12), not at the primitive layer. `migrations::open` stays sync.
- Create `src/constants.rs`. Architecture line 889 names "schema version" as one of the things `constants.rs` will hold, but in v1 the schema version is *structurally derived* from `MIGRATIONS.len()` (T3.4) — pinning it as a separate constant in another file would let the two drift. The container-image-digest constant (the other resident of `constants.rs` per architecture line 889) lands in Story 1.10 / 1.14; that's the right time to create the file.
- Add config-side path resolution (`{paths.cache_dir}/lcrc.db`). The TOML config schema lives in Story 6.1 (`src/config/schema.rs`); for now `migrations::open` accepts any `&Path` and tests pass tempdir paths. Story 1.12 wires `cli/scan.rs` to compose `config.paths.cache_dir.join("lcrc.db")` and pass it.
- Add tracing/logging events. Same rule Story 1.5 / 1.6 followed: this story's primitives are silent on success; the consumer story wires `tracing::info!("opened cache at {path}, schema v{version}")` once it owns the call site.
- Add `From<CacheError> for crate::error::Error`. Story 1.12 (the consumer) decides the boundary mapping; pre-adding it creates dead API surface.
- Touch `src/main.rs`, `src/cli.rs`, `src/cli/*.rs`, `src/error.rs`, `src/exit_code.rs`, `src/output.rs`, `src/util.rs`, `src/util/tracing.rs`, `src/version.rs`, `src/machine.rs`, `src/machine/apple_silicon.rs`, `src/cache/key.rs`, `tests/cli_exit_codes.rs`, `tests/cli_help_version.rs`, `tests/machine_fingerprint.rs`, `Cargo.toml`, `Cargo.lock`, `build.rs`, or `.github/workflows/*`. None of those need to change for Story 1.7.
- Author or update `tasks/swe-bench-pro/manifest.json` or any vendored task data. Container concerns are owned by Story 1.10 / 1.14.
- Add CREATE INDEX statements. Read-side index decisions land in Story 1.8 / 1.12 once query patterns are concrete.

### Architecture compliance (binding constraints)

- **Single source of truth: `src/cache/schema.rs` for DDL constants and `src/cache/migrations.rs` for the open/init + migration logic** [Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" line 898-899]: `schema.rs` owns the SQL DDL strings; `migrations.rs` owns `PRAGMA user_version` discipline. No other module embeds raw SQL or executes `PRAGMA` directly. After this story merges, the rusqlite + PRAGMA surface is contained inside `src/cache/{migrations,schema,cache}.rs` and `tests/cache_migrations.rs`; the T6.5 grep guards this contract.
- **No `unsafe` anywhere** [Source: AR `unsafe_code = "forbid"` in Cargo.toml line 78 + `lib.rs:3`]: `rusqlite` ships with internal `unsafe` for FFI to libsqlite3 — that is its problem, not ours; the host crate stays `forbid(unsafe_code)`.
- **All async file I/O via `tokio::fs` / `tokio::process`, never `std::fs` / `std::process`** [Source: `_bmad-output/planning-artifacts/architecture.md` line 687]: this story's `open` function is *synchronous* by design — it does not perform `std::fs::create_dir_all` or any other `std::fs` call. Parent-directory creation is pushed to the caller (Story 1.12 wires `tokio::fs::create_dir_all(parent).await` at the CLI layer). Inside `open`, the only filesystem touch is `rusqlite::Connection::open(path)`, which uses libsqlite3's own `open(2)` call via FFI — not `std::fs`. AR-3's intent (no sync I/O bridged into async contexts) is upheld because `migrations::open` will itself be wrapped in `spawn_blocking` by the consumer.
- **No `std::process` anywhere** [Source: AR-3]: N/A in this story — no subprocess execution.
- **Workspace lints — `unwrap_used`, `expect_used`, `panic = "deny"`** [Source: Cargo.toml lines 83-85]: All `?` propagation against `CacheError`. The two test surfaces (`#[cfg(test)] mod tests` in `migrations.rs`, `tests/cache_migrations.rs`) carry the documented `#[allow(...)]` exemption pattern. Production code uses zero `unwrap` / `expect` / `panic`.
- **`missing_docs = "warn"`** [Source: Cargo.toml line 79]: Every `pub` item gets a `///` doc — `CELLS_DDL_V1` (in `schema.rs`), `open`, `SCHEMA_VERSION`, `CacheError`, every `CacheError` variant, every variant field. `pub fn open` returns `Result`, so it also needs a `# Errors` rustdoc section (clippy `missing_errors_doc`).
- **MSRV 1.95** [Source: Cargo.toml line 5]: `<[T]>::len` is `const` since Rust 1.55 (T3.4). `Connection::transaction`, `execute_batch`, `query_row` are stable in rusqlite 0.32. `tempfile::TempDir::new()` is stable in tempfile 3.x. No nightly-only features.
- **Crate is binary + library** [Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" line 874-876 + Story 1.3 T1.2]: `cache::migrations` and `cache::schema` are library-only; `tests/cache_migrations.rs` consumes them as `lcrc::cache::migrations::open` and `lcrc::cache::CacheError`. `cargo test` exercises the library path.
- **Tracing / logging discipline** [Source: AR `tracing` discipline + architecture.md § "Tracing / Logging" line 770]: This story emits **no** tracing events. `migrations::open` is silent on success; on `CacheError`, the function returns `Err`; the consumer (Story 1.12) decides whether to `tracing::warn!` before propagating.
- **Atomic-write discipline** [Source: AR atomic writes + architecture.md § "Atomic-Write Discipline" line 692]: Each migration step (`<DDL>; PRAGMA user_version = N+1;`) runs in a single SQLite transaction (`BEGIN; ...; COMMIT;`). A crash mid-step rolls back; the cache stays at the prior `(user_version, schema)` pair. This is the migration-side application of NFR-R2 (atomicity of cell writes) — the same invariant Story 1.8 will apply to `write_cell`.
- **No glob imports** [Source: implicit per existing code style; verified by grepping `src/` for `use rusqlite::*`]: Always name the imported items (`use rusqlite::Connection;`) — the locked codebase uses no `*` imports.
- **`Cargo.lock` is committed; CI cache keys on it** [Source: Story 1.2 § Architecture compliance]. This story adds **no** new dependencies. `Cargo.lock` should be unchanged after `cargo build`. If it changes, investigate before pushing — most likely an accidental glob-import or a `tempfile` re-resolve, neither of which should be persisted.

### Resolved decisions (don't re-litigate)

These are choices the dev agent might be tempted to revisit. Each is locked here with rationale.

- **`migrations::open(path)` is SYNC, not `async`.** Why: `rusqlite` (Cargo.toml line 45) is the locked SQLite binding and is sync C bindings; bridging it to async at the primitive layer wastes a tokio runtime per open and complicates the test surface (every test would need `#[tokio::test]`). The architecture's pattern (architecture.md line 697 illustrates `pub async fn write_cell`) places `spawn_blocking` at the *consumer* layer — Story 1.8 (`Cache::write_cell`) and Story 1.12 (CLI wiring). For Story 1.7, sync is correct.
- **`Connection::open(path)` does NOT create parent directories; the caller does.** Why: keeping `migrations::open` sync requires `std::fs::create_dir_all` if we wanted to mkdir-p inside, which violates AR-3. Pushing dir creation to the caller (Story 1.12 wires `tokio::fs::create_dir_all(parent).await` first) keeps both rules satisfied. Tests use `tempfile::TempDir`, which guarantees the parent exists.
- **`SCHEMA_VERSION` is derived from `MIGRATIONS.len()`, not declared independently.** Why: a separately-declared `pub const SCHEMA_VERSION: u32 = 1;` lets a future maintainer add a migration without bumping the version (or bump it without adding the migration). Deriving makes both transitions atomic. `<[T]>::len` is const since Rust 1.55 (well below MSRV 1.95).
- **`MIGRATIONS` is a `&[&str]` indexed by "from-version", not a `&[(u32, &str)]` keyed by target version or a `HashMap<u32, &str>`.** Why: we always migrate one-step-at-a-time in a contiguous version sequence; the slice index *is* the from-version. A keyed map would invite version-skipping (e.g. `MIGRATIONS.insert(3, ...)` skipping 2), which is a footgun for forward-compatibility. The slice shape forces dense, contiguous version numbers.
- **DDL strings live in `src/cache/schema.rs`, not inlined in `migrations.rs`.** Why: architecture line 898-899 maps `schema.rs` → "SQL DDL constants" and `migrations.rs` → "PRAGMA user_version + migration scripts". The split keeps the SQL reviewable independent of the migration framework code, and lets future migrations (`CELLS_DDL_V2`) land in `schema.rs` without thrash in `migrations.rs`.
- **`CREATE TABLE IF NOT EXISTS cells` (not bare `CREATE TABLE`).** Why: defence in depth. `apply_migrations` already gates by `user_version`, but if a future bug, manual edit, or filesystem corruption leaves the table present without the corresponding `user_version` bump, a bare `CREATE TABLE` would error. `IF NOT EXISTS` makes the migration step structurally safe to re-run, costing nothing on a fresh DB.
- **`CacheError` lives at the cache module root (`src/cache.rs`), not in `src/cache/migrations.rs`.** Why: Story 1.8 will reuse the same enum for `DuplicateCell` (architecture line 571) and other cell-write errors. Defining `CacheError` at the module root lets all cache submodules (`migrations`, `cell`, `query`) share one error type without `From` ladders. This is the structural symmetry to Story 1.6's `KeyError` — but `KeyError` is scoped to `key.rs` because key-derivation errors are unrelated to the SQLite errors that other cache submodules will produce.
- **`CacheError` has FOUR variants in this story** (`Open`, `Pragma`, `MigrationFailed`, `FutureSchema`); future variants land in their owner stories. Why: same Story 1.5 / 1.6 rule — pre-adding `DuplicateCell` etc. creates dead surface area until Story 1.8 ships the `write_cell` call site.
- **`CacheError::FutureSchema` has `{ found, expected }` named fields, not positional.** Why: named fields self-document the meaning at the construction site (`CacheError::FutureSchema { found: 99, expected: 1 }` reads better than `CacheError::FutureSchema(99, 1)` where the order is implicit) and at the Display-template site (`"{found}"` / `"{expected}"`).
- **`CacheError::FutureSchema.Display` text contains the literal substring `"upgrade lcrc"`.** Why: AC5 binds the user-visible message to "the CLI exits with a clear stderr message". The substring check (T4.5 + T5.7) pins the message stability against future Display-template edits — same Display-substring lesson as Story 1.5 § AC3 (`"unsupported hardware"`) and Story 1.6 § ModelShaIo (`"failed to read model file"`).
- **No `From<CacheError> for crate::error::Error` impl.** Why: same Story 1.5 / 1.6 rule — boundary mapping is the consumer story's call. Story 1.12 will likely map `Open` / `Pragma` / `MigrationFailed` to `Error::Preflight` and `FutureSchema` to either `Error::Preflight` (clean) or a new `Error::CacheFutureSchema` variant if the exit-code semantics warrant it. That decision depends on the wiring context.
- **`PRAGMA user_version = N` is set via formatted SQL (`format!("PRAGMA user_version = {target};")`), not via rusqlite parameter binding.** Why: SQLite refuses bound parameters in PRAGMA values — this is a SQLite limitation, not a rusqlite one. `target` is a `u32` we control (never user input), so format-string interpolation introduces no SQL injection vector.
- **WAL mode is enabled via `query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))`** — not via `pragma_update`. Why: PRAGMA journal_mode behaves like a query (returns the now-active mode as a single-row result), not like a config write — `pragma_update` is intended for value-only PRAGMAs (`user_version`, `application_id`, etc.). `query_row` matches the actual behavior; check the returned mode string against `"wal"` (case-insensitive) and surface `CacheError::Pragma` if SQLite refused.
- **Tests use `Connection::open_in_memory()` for unit tests in `src/cache/migrations.rs`** and `tempfile::TempDir` for integration tests in `tests/cache_migrations.rs`. Why: in-memory DBs are deterministic and fast; they exercise the same `apply_migrations` code path (the only difference is WAL fallback, which `:memory:` reports as `"memory"` instead of `"wal"`). The file-backed tests in T5 cover AC1's WAL-on-real-file requirement and the AC3 reopen flow. Splitting test surfaces by IO boundary (in-memory for migration logic, on-disk for file lifecycle) keeps each test focused on its actual contract.
- **`tests/cache_migrations.rs` does NOT spawn the `lcrc` binary** (no `assert_cmd::Command::cargo_bin("lcrc")`). Why: the CLI wiring of the cache lives in Story 1.12; testing it here would conflate this story's primitive surface with the integration surface. The "CLI exits" half of AC5 is owed by Story 1.12. Story 1.7's contract is the library boundary.
- **No `PRAGMA synchronous` / `PRAGMA foreign_keys` / `PRAGMA temp_store` tuning in this story.** Why: WAL mode plus SQLite defaults is correct for v1's workload (single-writer, occasional concurrent readers). Performance tuning is a Story 1.8 / Epic 6 concern once the actual write/read patterns are measurable.

### Library / framework requirements

| Crate | Version (Cargo.toml line) | Use in this story |
|---|---|---|
| `rusqlite` | `0.32`, features `["bundled"]` (line 45) | `Connection::open`, `Connection::open_in_memory`, `Connection::transaction`, `Connection::execute_batch`, `Connection::query_row` for the migration framework. Already locked. |
| `tempfile` | `3` (line 49) | `TempDir::new` for test fixtures in both unit tests (T4) and integration tests (T5). Already locked. |
| `thiserror` | `2` (line 60) | `#[derive(Error)]` on `CacheError`. Already locked. |
| `std::path::{Path, PathBuf}` (std) | — | `migrations::open` parameter type + `CacheError::Open.path` field. |

**Do not** add: `sqlx` (architecture.md line 697 illustrates `sqlx`-style code but the locked impl is `rusqlite` per Cargo.toml line 45 — sync, bundled, no migration runner needed beyond what we hand-roll), `refinery` / `barrel` / any other migration-library crate (the framework here is ~30 lines of explicit code; a 5-figure-LOC library would be over-engineering for v1's single migration), `tokio` async glue inside `migrations.rs` (sync by design — see § "Resolved decisions"), `regex` / `serde_yaml` / etc. (no parsing or templating needed).

**Do not** widen the `rusqlite` feature set beyond `bundled`. The `bundled` feature compiles libsqlite3 from source, eliminating the host-libsqlite-dependency NFR. Other features (`time`, `serde_json`, `chrono`, `array`, `vtab`) are not needed for v1's schema and would inflate compile time + binary size.

### File structure requirements (this story only)

Files created or updated:

```
src/
  cache.rs                       # UPDATE: declare `pub mod schema; pub mod migrations;`; add `pub enum CacheError { Open, Pragma, MigrationFailed, FutureSchema }`
  cache/
    schema.rs                    # NEW: pub const CELLS_DDL_V1: &str = "...";
    migrations.rs                # NEW: MIGRATIONS slice, SCHEMA_VERSION const, pub fn open + private helpers + in-module tests
tests/
  cache_migrations.rs            # NEW: AC1/AC2/AC3/AC5 integration tests via the public API
```

Files **NOT** created by this story (deferred to listed owner stories — do not pre-stub):

- `src/cache/cell.rs`, `src/cache/query.rs` — Story 1.8 (cache cell write/read API)
- `src/constants.rs` — Story 1.10 / 1.14 (container image digest is the first concrete consumer)
- `src/discovery.rs`, `src/discovery/llama_cpp.rs`, `src/discovery/gguf.rs`, `src/discovery/fit_gate.rs` — Story 2.1 (`Backend` trait + llama.cpp model discovery) and downstream
- `src/sandbox*` — Stories 1.9 / 1.10 / 2.7
- `src/scan*` — Stories 1.10 / 1.11 / 1.12 / 2.6 / 2.13 / 2.15
- `src/backend.rs`, `src/backend/llama_cpp.rs` — Story 2.1
- `src/tasks.rs`, `src/tasks/swe_bench_pro.rs` — Story 2.3
- `src/config.rs`, `src/config/schema.rs`, `src/config/env.rs` — Story 6.1
- `tests/cache_roundtrip.rs` — Story 1.8
- Any other architecture-named module — owned by their respective stories per `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure"

### Testing requirements

This story authors **two test surfaces**:

**1. In-module unit tests** (T4) — verify the migration framework's logic in isolation, in `src/cache/migrations.rs::tests`:

- `apply_migrations_on_empty_db_bumps_user_version_to_schema_version` — AC4 fundamental.
- `apply_migrations_idempotent_when_user_version_equals_schema_version` — AC3 idempotency at the unit level.
- `apply_migrations_returns_future_schema_when_user_version_above_schema_version` — AC5 unit-level.
- `future_schema_display_locks_upgrade_lcrc_substring` — AC5 Display contract.
- `cells_table_columns_match_architecture_spec` — AC2 column-by-column.
- `cells_table_primary_key_is_seven_dimension` — AC2 PK shape.
- `schema_version_equals_migrations_len` — structural pin against drift.

Pattern: `#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] mod tests { ... }` at file end. Tests use `Connection::open_in_memory()` for migration-logic verification — fast, deterministic, exercises the same code path as on-disk for everything except WAL fallback.

**2. Integration tests** (T5) — verify the public-API contract via `lcrc::cache::migrations::open`, in `tests/cache_migrations.rs`:

- `creates_db_file_on_first_open` — AC1 file-creation.
- `enables_wal_journal_mode` — AC1 WAL.
- `cells_table_matches_architecture_spec_via_public_api` — AC2 end-to-end.
- `reopen_after_first_migration_is_no_op_NFR_R3` — AC3 patch durability.
- `future_schema_version_returns_future_schema_error` — AC5 end-to-end.
- `future_schema_error_display_contains_upgrade_lcrc_advice` — AC5 Display.

Pattern: standard integration crate (file-top `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`); plain `#[test]` (NOT `#[tokio::test]` — `migrations::open` is sync); uses `tempfile::TempDir` for filesystem fixtures.

Existing tests (`tests/cli_exit_codes.rs::ok_path_exits_0`, `tests/cli_help_version.rs::*`, `tests/machine_fingerprint.rs::*`, plus all in-module test suites) must continue to pass. This story does not touch any code path those tests exercise; if any goes red after this story's commit, the dev wired something wrong outside the story scope — investigate before relaxing.

The grep T6.5 (rusqlite + PRAGMA single-source-of-truth) is a manual code-review check, not an automated test, paralleling Story 1.6's grep T10.5.

### Anti-patterns to avoid (LLM-developer pitfalls)

- **Do not** make `migrations::open` `async`. `rusqlite` is sync; bridging it to async at the primitive layer is the architecture's documented anti-pattern (architecture.md line 697 places `spawn_blocking` at the consumer layer). A `pub async fn open` would force every test to be `#[tokio::test]` and require a tokio runtime per integration test for no benefit.
- **Do not** use `tokio::fs::*` inside `migrations::open`. The function is sync; `tokio::fs` requires an async context. If you find yourself reaching for it, you've started introducing the wrong layering — push parent-dir creation to the consumer (Story 1.12).
- **Do not** call `std::fs::create_dir_all` (or any other `std::fs` function) inside `migrations::open`. AR-3 forbids `std::fs` in production code; the function's contract is "open the DB at this path; caller ensures parent exists". Tests use `TempDir` whose parent always exists.
- **Do not** swap `rusqlite` for `sqlx`, `sea-orm`, `diesel`, or any other ORM/driver. Cargo.toml line 45 locks `rusqlite`. Architecture.md line 198 explicitly forbids ORMs ("No database ORM (no diesel, no sea-orm). The data model is small and explicit; rusqlite/serde_json is enough."). Architecture.md line 697 illustrates `sqlx`-style code but that's an illustration, not the locked impl.
- **Do not** add a migration library (`refinery`, `barrel`, `embedded-migrations`, `sqlx-migrations`). The framework here is ~30 lines of explicit code; a migration library would be 4-figure LOC for what we already have. AR-4 (locked dependency set) makes adding deps a deliberate decision; this one isn't worth it.
- **Do not** declare `SCHEMA_VERSION` as a separate constant from `MIGRATIONS.len()`. The two will drift (someone adds a migration but forgets to bump the const, or vice versa). Derive: `pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;` makes both transitions atomic.
- **Do not** use `pragma_update(None, "user_version", target)` for setting `user_version`. Test it locally first if you doubt — many `pragma_update` paths in rusqlite work for *some* PRAGMAs but not others. The reliable pattern across rusqlite versions is the formatted SQL: `tx.execute_batch(&format!("PRAGMA user_version = {target};"))`. Same for setting WAL mode (use `query_row("PRAGMA journal_mode = WAL;", ...)`, not `pragma_update`).
- **Do not** parameterize PRAGMA values via `?` placeholders (e.g. `tx.execute("PRAGMA user_version = ?;", params![target])`). SQLite refuses bound parameters in PRAGMA values — this returns a runtime error, not a compile-time one. Use formatted SQL instead.
- **Do not** add `CREATE INDEX` statements alongside `CELLS_DDL_V1` in `schema.rs`. Read-side query patterns (Story 1.8 / 4.1) drive index choices; pre-indexing makes the DDL noisier than the architecture spec and changes physical layout without a measured win.
- **Do not** declare additional tables (`scans`, `runs`, `metadata`, `cells_history`, etc.). v1's schema is a single `cells` table per architecture line 252-282. Future tables land in their owner stories with their own migration scripts.
- **Do not** use `CREATE TABLE` (without `IF NOT EXISTS`). The `IF NOT EXISTS` clause is defence in depth — `apply_migrations` already gates by `user_version`, but if a future bug, manual edit, or filesystem corruption leaves the table present without the version bump, a bare `CREATE TABLE` errors. `IF NOT EXISTS` costs nothing on a fresh DB and prevents a pathological re-run from corrupting the migration state.
- **Do not** run migrations *outside* a transaction. Each `<DDL>; PRAGMA user_version = N+1;` step is atomic per T3.7's pattern; a crash mid-step rolls back. Without the transaction, a crash between the DDL and the PRAGMA leaves the cache in an inconsistent state (table exists but `user_version` not bumped → next open re-runs the DDL → `CREATE TABLE` errors if `IF NOT EXISTS` was forgotten).
- **Do not** add a `pub fn close(conn: Connection)` wrapper. rusqlite's `Connection` is RAII-dropped; explicit close adds nothing. Story 1.8's `Cache::write_cell` will use the standard drop pattern.
- **Do not** introduce a `pub struct Cache { conn: Connection }` wrapper in this story. Story 1.8 (the first consumer that needs to add methods on the connection) decides whether `Cache` is a struct or whether the `Connection` is passed bare. Pre-defining is API speculation; same Story 1.6 anti-pattern about pre-defining `pub struct CacheKey`.
- **Do not** add `From<CacheError> for crate::error::Error`. Same Story 1.5 / 1.6 reasoning: the consumer story (1.12) decides the boundary mapping. Pre-adding it forces the mapping decision before the call site exists.
- **Do not** add `tracing::info!("opened cache at {path}")` or any other tracing event inside `migrations::open`. Same Story 1.5 / 1.6 rule: observability events at the primitive layer couple the module to the tracing scheme prematurely; the wiring story (1.12) decides whether and where to log.
- **Do not** memoize / cache the `Connection`. Each `migrations::open` call is the entry point for a new logical session; caching across calls would couple the primitive to a global lifecycle that doesn't exist in v1.
- **Do not** add `#[cfg(target_os = "macos")]` gates. SQLite is platform-agnostic; the migration framework runs identically on macOS, Linux, and Windows. Gating the module breaks the v1.1 Linux NVIDIA additive port (NFR-C5) for no benefit.
- **Do not** create `src/cache/cell.rs` or `src/cache/query.rs` "while you're in there". Tracer-bullet vertical slices: Story 1.8 owns those files; pre-stubbing violates the slice principle (`MEMORY.md → feedback_tracer_bullet_epics.md`).
- **Do not** re-export `cache::migrations::open`, `cache::CacheError`, or `cache::SCHEMA_VERSION` at the crate root. Callers use the fully-qualified paths `lcrc::cache::migrations::open(...)`, `lcrc::cache::CacheError::FutureSchema { ... }`, `lcrc::cache::migrations::SCHEMA_VERSION`. Re-exports are a v1-API-surface-locking decision; defer to Epic 6's polish story (same rule Story 1.5 / 1.6 applied to `MachineFingerprint` and `KeyError`).
- **Do not** rename `src/cache/migrations.rs` to `src/cache/migration.rs` (singular) or `src/cache/migrate.rs`. The architecture's project-structure tree at architecture.md line 899 names it `migrations.rs` (plural — a framework that hosts a *collection* of migration scripts). Renaming silently breaks every existing reference.
- **Do not** rename `src/cache/schema.rs`. Architecture line 898 names it `schema.rs`.
- **Do not** spawn the `lcrc` binary in `tests/cache_migrations.rs` (no `assert_cmd::Command::cargo_bin("lcrc")`). The CLI wiring of the cache lives in Story 1.12; testing it here would conflate this story's primitive surface with the integration surface. The "CLI exits" half of AC5 is owed by Story 1.12.
- **Do not** add `assert_cmd` or `predicates` imports to `tests/cache_migrations.rs`. They're for the CLI exit-code tests (Story 1.4 / 1.12); cache integration tests use the library API directly.
- **Do not** widen `rusqlite` features beyond `bundled` (e.g. `chrono`, `serde_json`, `array`, `vtab`). v1's schema is plain TEXT/INTEGER/REAL columns; richer types are over-engineering and inflate compile time + binary size.
- **Do not** add `PRAGMA synchronous = NORMAL` / `FULL` / `OFF` tuning in this story. WAL mode + SQLite defaults is correct for v1's workload; performance tuning is a Story 1.8 / Epic 6 concern.
- **Do not** add `PRAGMA foreign_keys = ON` in this story. v1's schema has no FK relationships.
- **Do not** add `PRAGMA temp_store = MEMORY` / `MMAP` / etc. in this story. SQLite defaults are correct.
- **Do not** add `// Story 1.8 will use this` / `// Per architecture §X` comments. Same chore-commit `7a6e029` lesson Story 1.6 carries: the *why* (e.g. `// SQLite supports transactional DDL — DDL inside BEGIN/COMMIT is atomic and rolls back on commit failure.`) goes in the comment; planning-artifact references go in the PR description and are discoverable via `git blame`.

### Previous story intelligence (Story 1.1 → 1.2 → 1.3 → 1.4 → 1.5 → 1.6 → 1.7)

- **Story 1.6 created `src/cache.rs` as a parent module file with `pub mod key;`** [Source: `src/cache.rs`]. This story extends it with `pub mod schema; pub mod migrations;` and the `CacheError` enum — additive, no removal of existing content. Preserve the existing module-doc text; replace it with a broader cache-module summary that mentions all three submodules.
- **Story 1.6 established the per-submodule typed-error pattern** (`KeyError` in `src/cache/key.rs:62`) [Source: `src/cache/key.rs:62`]. Story 1.7 deviates intentionally: `CacheError` lives at the cache module root (`src/cache.rs`), not in `src/cache/migrations.rs`. Reason: `CacheError` will be reused across `migrations`, `cell`, `query` submodules in Story 1.8+, while `KeyError` is unique to key derivation. Same shape, different scope — the choice is by reuse expectation.
- **Story 1.6 left `src/cache.rs` as a thin module-declaration file** (5 lines, `pub mod key;` only) [Source: `src/cache.rs`]. After this story it grows to ~50 lines (additional `pub mod` declarations + `pub enum CacheError` with four variants). Architecture line 896 promises `src/cache.rs` as the eventual home for "Cache struct, public API (FR24-FR31)", but that struct lands in Story 1.8 (the first consumer). Story 1.7 grows `cache.rs` with the error enum + module declarations only.
- **Story 1.5 / 1.6 both added Display-substring tests for typed-error messages** [Source: `src/machine.rs:147-155`, `src/cache/key.rs:tests`]. Apply the same lesson here: `CacheError::FutureSchema.Display` includes `"upgrade lcrc"` and the test asserts the substring. Same reasoning — Display templates rot easily; substring pins guard the user-visible contract.
- **Story 1.6 left `src/cache/key.rs` with `KeyError` carrying `#[allow(clippy::module_name_repetitions)]`** [Source: `src/cache/key.rs:58-61`]. Apply the same lint suppression to `CacheError` in `src/cache.rs` for the same reason: bare `Error` would collide with `crate::error::Error` (visible in `src/error.rs:18`), and pedantic clippy fires on the `Error` suffix inside a module named `cache`. Use the same comment template Story 1.6 used.
- **Story 1.6's review surfaced a deferred `serde_json/preserve_order` static-guard concern** [Source: `_bmad-output/implementation-artifacts/deferred-work.md:21`]. Not in scope for Story 1.7 — the deferred guard is about JSON canonicalization in `params_hash`, not SQLite migrations.
- **Story 1.4's review surfaced two clippy gates that were masked because clippy was permission-blocked in the dev session** [Source: Story 1.5 line 258 → Story 1.6 line 276]. **Run `cargo clippy --all-targets --all-features -- -D warnings` locally** before pushing this story (T6.3) — local mirror is not optional. Specifically watch for:
  - `clippy::cast_possible_truncation` on `MIGRATIONS.len() as u32` — suppress with the documented `#[allow]` + comment per T3.4.
  - `clippy::module_name_repetitions` on `CacheError` — suppress per T1.4.
  - `clippy::missing_errors_doc` on `pub fn open` — `# Errors` rustdoc section per T3.5.
  - `clippy::missing_docs` on every `pub` item.
  - `clippy::needless_pass_by_value` should NOT fire — all helper params are `&Connection` / `&mut Connection`.
- **Story 1.6 added `serde_json` as a new direct dependency** [Source: Story 1.6 § Resolved decisions]. Story 1.7 adds **NO** new dependencies — `rusqlite` (line 45), `tempfile` (line 49), `thiserror` (line 60) are all already locked. `Cargo.lock` should be unchanged after `cargo build`. Verify locally; if `Cargo.lock` does change, investigate before pushing.
- **Per-story branch + PR + squash-merge workflow** [Source: `MEMORY.md → feedback_lcrc_branch_pr_workflow.md`]. The branch `story/1-7-sqlite-schema-migrations-framework` is already checked out per the activation context. Push commits, open PR, wait for green CI, squash-merge with branch deletion via `scripts/bmad-auto.sh` (or the orchestrator's manual equivalent).
- **Tracer-bullet vertical-slice principle was honored in 1.1 / 1.2 / 1.3 / 1.4 / 1.5 / 1.6** [Source: `MEMORY.md → feedback_tracer_bullet_epics.md`]. This story's slice is thin: the SQLite migration framework + its tests, no consumer wiring. Stories 1.8 / 1.12 take the full vertical from CLI → scan → cache → cell write.
- **Apply the chore commit `ee6a89f` lesson** [Source: Story 1.6 line 294]: do not write `// Story 1.8 wires this` / `// Per architecture.md §Cache Architecture` in code comments — the *why* (e.g. `// SQLite supports transactional DDL; DDL + PRAGMA user_version bump in one tx is atomic.`) goes in the comment; the planning artifact reference goes in the PR description and is discoverable via `git blame`.
- **Story 1.5 / 1.6 deferred items in `_bmad-output/implementation-artifacts/deferred-work.md`** are NOT in scope for Story 1.7 — they belong to a `bmad-quick-dev` pass over `src/machine/apple_silicon.rs` / `src/cache/key.rs`, not the new SQLite modules.

### Git intelligence summary

- Recent commits (newest first per repo state at story creation): `ba42e15` (Story 1.6: Cache key helpers in `src/cache/key.rs` — PR #5), `f98d307` (Story 1.5: Machine fingerprint module — PR #4), `3cb7e77` (bmad-auto retry transient GH API failures + friction-report pause — PR #2), `ee6a89f` (chore: strip planning-meta comments from Story 1.4 modules — PR #3), `91b95be` (Story 1.4: clap CLI root + `--version` + `--help` + tracing subscriber — PR #1).
- The `ba42e15` (Story 1.6) commit landed `src/cache.rs` + `src/cache/key.rs` + the `serde_json` dep. **Inspect `src/cache.rs` (5 lines, `pub mod key;` + module doc)** — Story 1.7 extends this file with the additional `pub mod` declarations + `CacheError` enum. Do NOT replace the file; surgically add to it.
- The `f98d307` (Story 1.5) commit landed `src/machine.rs` + `src/machine/apple_silicon.rs` + `tests/machine_fingerprint.rs`. The pattern Story 1.5 / 1.6 established for typed errors (`#[derive(thiserror::Error)]`, named-field Display templates, no `From<…> for crate::error::Error` until a consumer exists) carries forward to `CacheError`.
- The `ee6a89f` chore commit is informative: it stripped `// Per Story 1.4` / `// FR3 placeholder` planning-meta comments from the post-1.4 modules. **Apply the same restraint** in this story — comments explain *why* (constraints, invariants, non-obvious choices), not which planning artifact owns the change.
- Current `src/` (post-1.6) contains 16 files: `main.rs`, `lib.rs`, `error.rs`, `exit_code.rs`, `output.rs`, `cli.rs`, `cli/scan.rs`, `cli/show.rs`, `cli/verify.rs`, `util.rs`, `util/tracing.rs`, `version.rs`, `machine.rs`, `machine/apple_silicon.rs`, `cache.rs`, `cache/key.rs`. After this story: 18 files (+ `cache/schema.rs`, `cache/migrations.rs`).
- `tests/` (post-1.6) contains 3 files: `cli_exit_codes.rs`, `cli_help_version.rs`, `machine_fingerprint.rs`. After this story: 4 files (+ `cache_migrations.rs`).
- Current branch `story/1-7-sqlite-schema-migrations-framework` is checked out per `gitStatus`; working tree status was clean at story-creation time.
- The `actions/checkout@v5` deferred item from Story 1.2 [`_bmad-output/implementation-artifacts/deferred-work.md` line 33] is **not** in scope for this story; soft deadline 2026-06-02 (≈ 4 weeks out as of 2026-05-07).
- The Story 1.6 deferred items in `deferred-work.md` lines 17-23 are **not** in scope for this story.
- No release tags exist; pre-v0.1.0 development. The `Cargo.toml` `version = "0.0.1"` pin (line 3) stays.
- **Cold-cache wall times** [Source: Story 1.3 / 1.6 completion notes]: clippy ~19.6s, full test ~18.3s baseline. Story 1.7 adds the bundled libsqlite3 compile to the test binaries — **expected creep** is significant (bundled SQLite is ~400 KB of C, ~20-40 s additional compile time on a cold cache for the first build that actually exercises rusqlite). All subsequent builds hit the warm cache. If clippy/test wall time on subsequent builds (warm cache) jumps by more than 5×, investigate — that signals an unwanted dep or an unintended widening of `rusqlite` features.
- **`Cargo.lock` is NOT modified by this story** (unlike Story 1.6 which added `serde_json`). All deps used here (`rusqlite`, `tempfile`, `thiserror`, `std::path`) are already locked. CI cache hits the warm `Swatinem/rust-cache@v2` entry from Story 1.6's push (the cache key includes `Cargo.lock` hash; unchanged → warm hit).

### Latest tech information (Rust ecosystem — relevant to this story only)

- **`rusqlite` 0.32 with `bundled`** [Source: rusqlite docs + Cargo.toml line 45]: `bundled` compiles libsqlite3 from source, eliminating any host-libsqlite dependency. Stable API surface: `Connection::open(path)` opens or creates a file-backed DB; `Connection::open_in_memory()` opens a `:memory:` DB; `Connection::transaction()` returns a `Transaction` with `commit()` / `rollback()` (RAII rollback on drop); `Connection::execute_batch(sql)` runs multi-statement SQL (DDL); `Connection::query_row(sql, params, row_fn)` runs a one-row query and maps the row via the closure. PRAGMA values cannot be bound parameters; format integers in directly (safe when the integer is type-checked Rust code, never user input).
- **SQLite WAL mode** [Source: SQLite docs]: `PRAGMA journal_mode = WAL;` returns the now-active mode as a single-row result (`"wal"` on success, `"memory"` on `:memory:` DBs, `"delete"` on read-only filesystems). WAL enables concurrent readers + single writer (NFR-R7), and stores the WAL file at `<dbpath>-wal` plus a shared-memory `<dbpath>-shm` file. Both ancillary files are managed automatically by SQLite; no host-side cleanup needed.
- **SQLite `user_version`** [Source: SQLite docs]: a 32-bit signed integer stored at offset 60 in the database header. Default value on a fresh DB is 0. Set via `PRAGMA user_version = N;` (formatted SQL, no parameter binding). Read via `PRAGMA user_version;` (returns single-row, single-column INTEGER). Header writes are transactional — `BEGIN; <DDL>; PRAGMA user_version = N; COMMIT;` is atomic.
- **SQLite transactional DDL** [Source: SQLite docs]: SQLite supports CREATE TABLE / CREATE INDEX / DROP TABLE inside `BEGIN; ... COMMIT;`. The DDL is rolled back on `ROLLBACK` (or on connection drop without commit). This is the foundation for the migration framework's "atomic per migration step" guarantee.
- **SQLite `PRAGMA table_info(name)`** [Source: SQLite docs]: returns one row per column with the schema (cid, name, type, notnull, dflt_value, pk). The `pk` column is `0` for non-PK columns and `1, 2, 3, ...` for the position of the column in the composite PK (1-indexed). Used in T4.6 / T4.7 for verifying the cells table column layout and PK shape against the architecture spec.
- **`tempfile::TempDir`** [Source: tempfile 3.x docs]: `TempDir::new()` creates a new temp dir under `std::env::temp_dir()`; `.path()` returns `&Path`; the dir + contents are RAII-deleted on drop. Test-side use: `let dir = TempDir::new()?; let path = dir.path().join("lcrc.db");` then pass `&path` to `migrations::open`. The DB file (and its `-wal` / `-shm` companions) are cleaned up on `dir` drop at scope end.
- **`thiserror` 2.0** [Source: thiserror docs via Story 1.5 / 1.6]: `#[derive(Error)]`, `#[error("...")]` for Display templates with named-field interpolation (`{source}`, `{path}`, `{found}`, `{expected}`). `#[source]` for the error-chain pointer (used for `rusqlite::Error` payloads). Already locked in Cargo.toml; no version bump needed.

### Project Structure Notes

The architecture's `src/` directory map [`_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" lines 896-902] places:
- `src/cache.rs` at line 896 (annotation: "Cache struct, public API (FR24-FR31)")
- `src/cache/schema.rs` at line 898 (annotation: "SQL DDL constants")
- `src/cache/migrations.rs` at line 899 (annotation: "PRAGMA user_version + migration scripts (NFR-R3)")
- `src/cache/key.rs` at line 900 (annotation: "canonical key computation (per Patterns)") — landed by Story 1.6
- `src/cache/cell.rs` at line 901 (annotation: "Cell struct, read/write (atomic transactions)") — Story 1.8
- `src/cache/query.rs` at line 902 (annotation: "leaderboard, drift, sample queries") — Story 1.8

Story 1.7 lands `schema.rs` and `migrations.rs`. The "Cache struct, public API" promise of architecture line 896 is **partially** delivered — the `CacheError` enum (the public error surface) lands here; the `Cache` *struct* (the public connection wrapper) lands in Story 1.8. This split keeps Story 1.7 a pure-framework story (no consumer surface decisions about `Cache::write_cell` etc.) and Story 1.8 a pure-API story (additive `Cache` struct + `write_cell` / `lookup_cell` methods).

The architectural-boundaries table at architecture.md line 998 names `src/cache/*` as the sole owner of the SQLite database: "rusqlite/sqlx; schema + migrations + queries". After this story merges, the boundary is enforced *conventionally* via the T6.5 grep contract (no `rusqlite::` outside `src/cache/{migrations,schema}.rs` and `tests/cache_migrations.rs`); *structurally* it's enforced once Story 1.8 ships and the `Cache` struct is the only legitimate connection-wielding API.

The single architectural judgment call in this story is **where `CacheError` lives** — alternatives:
- (a) `src/cache.rs` (module root) — shared across `migrations`, `cell`, `query` submodules. **Locked.**
- (b) `src/cache/migrations.rs` — scoped to migration concerns; `cell.rs` would define `CellError`; `query.rs` would define `QueryError`; a parent `CacheError` would `From`-into all three.
- (c) Per-submodule typed errors with no parent `CacheError`.

Choice **(a)** is locked. Reasoning: cache-side errors are all rusqlite-error wrappers with module-specific context (path, version, operation); the contextual differentiation is enough that variants (rather than separate enums) satisfy "what went wrong". A parent enum lets Story 1.8 / future stories grow `CacheError::DuplicateCell` etc. without `From` ladders or aliasing. The architecture's hint at architecture.md line 543 — "`Err(CacheError::FutureSchema)`" — names this enum at the cache module root, lending support to (a). Compare to Story 1.6's `KeyError`, which stays in `src/cache/key.rs` because key-derivation errors (file I/O, JSON serialization) have no overlap with the SQLite errors that other cache submodules will produce.

The four `pub` entrypoints / values from this story are:
- `lcrc::cache::CacheError` (and its 4 variants) — the public error type
- `lcrc::cache::migrations::open(path)` — the public open/init function
- `lcrc::cache::migrations::SCHEMA_VERSION` — the public schema-version pin
- `lcrc::cache::schema::CELLS_DDL_V1` — the public DDL constant (used by `migrations.rs` internally, but also `pub` for AC2 cross-verification visibility from tests)

`#[cfg(target_os = "macos")]` does **NOT** appear in `src/cache/{schema,migrations}.rs` or `src/cache.rs` extensions. The migration framework is platform-agnostic (SQLite + standard transactions); only `src/machine/apple_silicon.rs` (Story 1.5) carries the `cfg`-gate. The v1.1 Linux NVIDIA additive port (NFR-C5) drops in via new `src/machine/linux_nvidia.rs` and a new `src/backend/cuda.rs` (Stories far in the future) without touching the cache modules.

No conflicts detected between this story's plan and the existing codebase or planning artifacts.

### References

- [Source: `_bmad-output/planning-artifacts/epics.md` § "Story 1.7: SQLite schema + migrations framework"] — the AC source
- [Source: `_bmad-output/planning-artifacts/epics.md` § "Epic 1: Integration spine — one cell, one row, end-to-end"] — epic context (FR24/FR25/FR26 cache surface is in Epic 1's FR coverage)
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Cache Architecture" (lines 242-296)] — the cache decisions: SQLite + `cells` table + atomicity + WAL + PRAGMA user_version migration discipline
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Cell schema (`cells` table)" (lines 252-282)] — the SQL DDL spec; locked column types, PK shape, and nullability
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Cache Key Canonicalization" (lines 720-729)] — Story 1.6 reference; the `cells` table PK columns are produced by `cache::key` helpers landed in Story 1.6
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Curated Dependencies" (lines 116-173)] — `rusqlite` for SQLite (line 138); `bundled` feature locked (line 138)
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Module Organization / file-as-module" (AR-26)] — `src/cache.rs` parent + `src/cache/{migrations,schema}.rs` submodule pattern
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" (lines 896-902)] — `src/cache.rs` (line 896), `src/cache/schema.rs` (line 898), `src/cache/migrations.rs` (line 899) placement
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Architectural Boundaries" (line 998)] — `SQLite database | src/cache/* | rusqlite/sqlx; schema + migrations + queries` — single-owner contract
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Requirements → Structure Mapping" (lines 1038-1045)] — FR24/FR25 (`src/cache/{key,schema,cell}.rs`); FR26 (`src/cache/query.rs::lookup`); FR27 (`src/cache/cell.rs` atomic write); FR31 (per-cell metadata)
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Implementation Sequence" (line 536)] — "SQLite schema + migration framework (cells table, PRAGMA user_version, migration scripts)" is sequence step 2; this story is that step
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Implementation Patterns" / AR-30] — atomic-write discipline applies to the migration step (`<DDL>; PRAGMA user_version = N+1;` in one transaction)
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Atomic-Write Discipline" (line 692)] — single-transaction pattern; same shape Story 1.8's `write_cell` will use
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Tracing / Logging" (line 770)] — no tracing events at the primitive layer
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Enforcement Summary" (lines 820-832)] — "Write cells inside a single SQLite transaction; never partially" (line 826) — applied to migrations as well (this story's atomic-per-step pattern)
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Cross-Cutting NFRs" (line 1074)] — `NFR-R3 (cache durability): src/cache/migrations.rs + tests/cache_migrations.rs` — Story 1.7 lands both
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Implementation Handoff" (line 1281)] — single-source-of-truth modules list
- [Source: `_bmad-output/planning-artifacts/prd.md` § "FR24"] — `(machine_fingerprint, model_sha, backend_build, params)` cache key — the columns the `cells` table must support
- [Source: `_bmad-output/planning-artifacts/prd.md` § "FR25"] — store/retrieve each `(model, task)` cell independently; cells are the unit of caching/measurement/resumability/depth-extension
- [Source: `_bmad-output/planning-artifacts/prd.md` § "FR27"] — partial scan results persisted such that Ctrl-C/OOM/crash mid-scan loses no completed cells; foundational for Story 1.8's atomic write but verified at the SQLite-transaction level here
- [Source: `_bmad-output/planning-artifacts/prd.md` § "FR31"] — per-cell metadata: depth tier, scan timestamp, backend_build, lcrc version, harness/task version, perf metrics — the columns this story's `cells` table provides
- [Source: `_bmad-output/planning-artifacts/prd.md` § "NFR-R2"] — atomicity of cell writes (architecturally extended to migrations: each migration step is atomic)
- [Source: `_bmad-output/planning-artifacts/prd.md` § "NFR-R3"] — cache durability across version upgrades; the binding requirement this story's migration framework satisfies
- [Source: `_bmad-output/planning-artifacts/prd.md` § "NFR-R7"] — concurrency safety; SQLite WAL mode (this story's AC1) provides lock-free concurrent reads alongside a single writer
- [Source: `_bmad-output/planning-artifacts/prd.md` § "NFR-P5"] — cache-key lookup <100 ms for 10K cells; verified by Story 1.8's `tests/cache_roundtrip.rs`, not by this story
- [Source: `_bmad-output/implementation-artifacts/1-6-cache-key-helpers-in-src-cache-key-rs.md`] — `src/cache.rs` parent file pattern; per-module-attribute test exemption pattern; "no `From<…> for crate::error::Error` in primitive-author story" rule; Display-substring contract pin pattern
- [Source: `_bmad-output/implementation-artifacts/1-5-machine-fingerprint-module.md`] — `MachineFingerprint::as_str()` contract (Story 1.6 consumer); typed-error pattern via `thiserror`
- [Source: `_bmad-output/implementation-artifacts/1-4-clap-cli-root-lcrc-version-lcrc-help-tracing-subscriber.md`] — file-as-module pattern; clippy local-mirror lesson
- [Source: `_bmad-output/implementation-artifacts/1-3-output-module-full-exitcode-enum-error-layer.md`] — `Error::Preflight` variant (the future boundary mapping target for `CacheError::{Open,Pragma,MigrationFailed,FutureSchema}` once Story 1.12 wires it)
- [Source: `_bmad-output/implementation-artifacts/1-2-ci-workflow-gates-fmt-clippy-and-tests.md`] — CI gate (macos-14 runner, 8-min budget); `Swatinem/rust-cache@v2` keys on `Cargo.lock` (this story does NOT change `Cargo.lock`)
- [Source: `_bmad-output/implementation-artifacts/1-1-project-scaffold-with-locked-workspace-lints.md`] — workspace lints + dep lockset; `rusqlite` was added here with `bundled` feature
- [Source: `_bmad-output/implementation-artifacts/deferred-work.md`] — Story 1.5 / 1.6 deferred items (out of scope here); Story 1.2 `actions/checkout@v5` deferred item (out of scope, soft deadline 2026-06-02)
- [Source: `src/cache.rs` (Story 1.6)] — current parent-module file; this story extends it
- [Source: `src/cache/key.rs` (Story 1.6)] — `KeyError` typed-error pattern; Display-template style with named-field interpolation
- [Source: `src/error.rs:18`] — `Error` enum (the future boundary mapping target, deferred to Story 1.12)
- [Source: `src/exit_code.rs:30-34`] — `ExitCode::ConfigError = 10` and `ExitCode::PreflightFailed = 11` (the eventual exit-code home of `CacheError`-derived `Error::Preflight`)
- [Source: `Cargo.toml` line 45] — `rusqlite = { version = "0.32", features = ["bundled"] }` — locked
- [Source: `Cargo.toml` line 49] — `tempfile = "3"` — locked, used here for tests
- [Source: `Cargo.toml` line 60] — `thiserror = "2"` — locked, used here for `CacheError`
- [Source: `<claude-auto-memory>/feedback_tracer_bullet_epics.md`] — vertical-slice principle (no pre-stubbing future-story files like `src/cache/cell.rs`)
- [Source: `<claude-auto-memory>/feedback_lcrc_branch_pr_workflow.md`] — branch-then-PR-then-squash workflow
- [Source: `<claude-auto-memory>/CLAUDE.md` → "HIGH-PRECEDENCE RULES" → "Comments explain WHY, never planning meta"] — code comments justify *why* a non-obvious choice was made; do not reference Story / Epic / FR identifiers in comments
- [Source: `<claude-auto-memory>/CLAUDE.md` → "HIGH-PRECEDENCE RULES" → "No absolute or machine-specific paths"] — all paths in code/docs are relative to repo root

## Dev Agent Record

### Agent Model Used

claude-opus-4-7 (1M context) via bmad-dev-story workflow

### Debug Log References

- `cargo build` → finished in 2.90 s; `Cargo.lock` unchanged (no new direct deps; `rusqlite`, `tempfile`, `thiserror` already locked).
- `cargo fmt` → reformatted method-chain indentation in `src/cache/migrations.rs::apply_migrations` (per-step `tx.execute_batch(...).map_err(...)` blocks expanded onto multi-line) and inlined the `super::{...}` use-list in the test module.
- `cargo clippy --all-targets --all-features -- -D warnings` → first pass surfaced six `clippy::doc_markdown` violations on bare "SQLite" in module/function docs (`src/cache.rs:22`, `src/cache/migrations.rs:5`, `:11`, `:42`, `:53`, `:70`) plus one in `tests/cache_migrations.rs:4` ("FutureSchema"); fixed by backticking each occurrence. Second pass: clean.
- `cargo test` → 78 tests passing across 7 suites in 1.20 s (lib + main unittests + 4 integration tests including the new `cache_migrations`).
- T6.5 grep `git grep -nE 'rusqlite::|PRAGMA|user_version' src/ tests/` outside the cache modules → zero matches; single-source-of-truth contract upheld.

### Completion Notes List

- `src/cache.rs` extended additively: kept the existing `pub mod key;`, added `pub mod migrations;` + `pub mod schema;` in alphabetical order, added the `pub enum CacheError` with four variants (`Open`, `Pragma`, `MigrationFailed`, `FutureSchema`) per T1.2; no `From<CacheError> for crate::error::Error` impl (deferred to Story 1.12).
- `src/cache/schema.rs` hosts `pub const CELLS_DDL_V1` exactly mirroring the architecture spec at `_bmad-output/planning-artifacts/architecture.md` § Cell schema (lines 252–282): 7-dimension PK + 12 metadata columns; nullability matches the spec; `IF NOT EXISTS` per T2.2.
- `src/cache/migrations.rs` hosts the migration framework: `MIGRATIONS: &[&str]` slice indexed by from-version, derived `pub const SCHEMA_VERSION = MIGRATIONS.len() as u32`, `pub fn open(path)` synchronous entry point, plus three private helpers (`enable_wal`, `apply_migrations`, `read_user_version`). Each migration step runs in its own `BEGIN; <DDL>; PRAGMA user_version = N; COMMIT;` transaction for atomicity (T3.7).
- In-module unit tests (T4.2 → T4.8) cover empty-DB → `SCHEMA_VERSION` bump, idempotency on re-run, `FutureSchema` on `user_version > SCHEMA_VERSION`, `Display` substring pin (`"upgrade lcrc"`), full 19-column `cells` schema verification, 7-dimension PK shape, and the `SCHEMA_VERSION == MIGRATIONS.len()` structural pin.
- Integration tests in `tests/cache_migrations.rs` (T5.2 → T5.7) verify the public API end-to-end via `lcrc::cache::migrations::open`: file creation, WAL mode on a real file, `cells` table column + PK matrix, NFR-R3 reopen-no-op, and the `FutureSchema` error + `Display` substring at the public boundary.
- Local CI mirror (T6) all green: `cargo build` clean, `cargo fmt` applied, `cargo clippy --all-targets --all-features -- -D warnings` clean, `cargo test` 78/78 passing, single-source-of-truth grep zero leakage. `Cargo.lock` unchanged.

### File List

- `src/cache.rs` — UPDATED: extended to declare `pub mod migrations;` + `pub mod schema;` and host `pub enum CacheError` with four variants.
- `src/cache/schema.rs` — NEW: `pub const CELLS_DDL_V1: &str` matching the architecture spec.
- `src/cache/migrations.rs` — NEW: `MIGRATIONS` slice, `pub const SCHEMA_VERSION`, `pub fn open`, `enable_wal` / `apply_migrations` / `read_user_version` helpers, plus in-module unit tests.
- `tests/cache_migrations.rs` — NEW: integration tests for AC1 / AC2 / AC3 / AC5 via the public `lcrc::cache::migrations::open` API.
- `_bmad-output/implementation-artifacts/sprint-status.yaml` — UPDATED: `1-7-sqlite-schema-migrations-framework` transitioned `ready-for-dev → in-progress → review`; `last_updated` bumped.
- `_bmad-output/implementation-artifacts/1-7-sqlite-schema-migrations-framework.md` — UPDATED: task checkboxes marked complete, Status transitioned to `review`, Dev Agent Record and File List populated, Change Log appended.

## Change Log

- 2026-05-07: Story 1.7 implementation complete (claude-opus-4-7 via bmad-dev-story). Added `src/cache/schema.rs` (`CELLS_DDL_V1`), `src/cache/migrations.rs` (`MIGRATIONS`, `SCHEMA_VERSION`, `open`, `enable_wal`, `apply_migrations`, `read_user_version` + 7 in-module tests), and `tests/cache_migrations.rs` (6 integration tests). Extended `src/cache.rs` with `pub mod migrations;` + `pub mod schema;` declarations and the `pub enum CacheError { Open, Pragma, MigrationFailed, FutureSchema }`. All 78 tests pass; `cargo clippy --all-targets --all-features -- -D warnings` clean; T6.5 single-source-of-truth grep clean; `Cargo.lock` unchanged. Status: `ready-for-dev → in-progress → review`.
- 2026-05-07: Code review pass (claude-opus-4-7 via bmad-code-review). Acceptance Auditor: zero violations against AC1–AC5, all "Resolved decisions" / "Anti-patterns to avoid" / architecture-compliance bullets confirmed in code. Adversarial layers (Blind Hunter + Edge Case Hunter): 2 patches applied inline, 1 item deferred to `deferred-work.md`, 3 nits noted in Review Findings, ~38 findings dismissed (most rejected pre-PRAGMA tuning, secondary indexes, STRICT tables, etc., that the spec explicitly forbids; remainder were truncated-diff false positives). All 78 tests still pass; clippy clean. Status: `review → done`.

### Review Findings

- [x] [Review][Patch] `open()` mutated `journal_mode` (creating `-wal` / `-shm` sidecars) before checking `user_version`, weakening the AC5 "refuse a future-schema cache without touching the file" intent. Fix: hoist a `read_user_version` + early `FutureSchema` return ahead of `enable_wal`. [`src/cache/migrations.rs:60-78`]
- [x] [Review][Patch] WAL mode comparison was inconsistent between source and integration test (`eq_ignore_ascii_case` in `enable_wal`, `to_lowercase()` in `enables_wal_journal_mode`). Aligned the test on `eq_ignore_ascii_case` (no allocation, matches source). [`tests/cache_migrations.rs:26-33`]
- [x] [Review][Defer] `Connection::open` failure when the path points at an existing non-SQLite file (`SQLITE_NOTADB`) surfaces as a generic `CacheError::Open`. Mapping it to a dedicated `CorruptDb` variant is a UX-side decision that belongs to the consumer story (1.12) wiring `Error::Preflight`; deferred. — recorded in `deferred-work.md`
- [x] [Review][Defer] `read_user_version` reads as `u32` directly; a manually-poisoned negative on-disk `user_version` would surface as a generic `CacheError::Pragma` (rusqlite type-conversion error) rather than as `FutureSchema` or a dedicated `CorruptVersion` variant. Pathological case; defer to a future hardening pass once a real corruption-recovery story exists. — recorded in `deferred-work.md`
- [x] [Review][Nit] `SCHEMA_VERSION = MIGRATIONS.len() as u32` cast suppresses `clippy::cast_possible_truncation` blanket-style. A `const _: () = assert!(MIGRATIONS.len() <= u32::MAX as usize);` would let the lint stay on for future migrations. Skipped: documented `#[allow]` is the project's existing pattern, and `MIGRATIONS` is hand-edited so the assert would never fire in practice.
- [x] [Review][Nit] Test `cells_table_matches_architecture_spec_via_public_api` collapses two `Result` layers via `.unwrap().map(Result::unwrap)`; a SQLite row-decode failure would panic without column context. Skipped: the downstream `assert_eq!` carries the user-visible "column #{i} mismatch" message; `unwrap` here only fires on test-infra failure. [`tests/cache_migrations.rs:39-49`]
- [x] [Review][Nit] `enable_wal` synthesizes `rusqlite::Error::ExecuteReturnedResults` when SQLite reports a non-WAL mode — semantically a stretch (the variant is documented as "called execute when query was needed"). Skipped: the spec explicitly accepts this as YAGNI until a real call site needs to distinguish "WAL not enabled" from other PRAGMA failures.
