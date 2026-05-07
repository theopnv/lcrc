# Story 1.8: Cache cell write/read API with atomic semantics

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a developer,
I want `Cache::write_cell(&Cell) -> Result<(), CacheError>` and `Cache::lookup_cell(&CellKey) -> Result<Option<Cell>, CacheError>` with single-transaction atomic semantics,
so that no half-written cells exist (NFR-R2) and lookups before measurement work correctly (FR26).

## Acceptance Criteria

**AC1.** **Given** a fresh cache (created via `Cache::open(path)` against a `TempDir` path)
**When** I call `cache.write_cell(&cell)` for some `cell: Cell`
**Then** the row is inserted within a single SQLite transaction; a subsequent `cache.lookup_cell(&cell.key)` returns `Ok(Some(roundtripped))` whose every field equals `cell`'s corresponding field (per the equality contract in `Cell`'s `PartialEq` derive — see Resolved decisions for nullability handling).

**AC2.** **Given** a `write_cell()` invocation that aborts mid-transaction (simulated in a test by panicking inside a closure that holds the `Transaction` before `commit()`)
**When** the cache file is reopened with a fresh `Cache::open(path)`
**Then** the cell is NOT present (`lookup_cell(&cell.key)` returns `Ok(None)`). NFR-R2 atomicity verified at the SQLite-transaction layer.

**AC3.** **Given** a cache populated with 10,000 synthetic cells (generated in a test fixture; PK fields varied so all 10K rows are distinct)
**When** I call `cache.lookup_cell(&existing_key)` for an existing key
**Then** the call returns `Ok(Some(cell))` in **< 100 ms** wall-clock (NFR-P5). The 100 ms budget is enforced by `assert!(elapsed < Duration::from_millis(100))` in the integration test, with the Instant captured immediately before and after the single `lookup_cell` call.

**AC4.** **Given** the same 10,000-cell cache from AC3
**When** I call `cache.lookup_cell(&nonexistent_key)` for a key whose composite PK is not present
**Then** the call returns `Ok(None)` in **< 100 ms** wall-clock. Negative-lookup performance pinned to the same NFR-P5 budget — SQLite's PK index makes this an O(log n) probe, so the same budget applies symmetrically.

**AC5.** **Given** a fresh cache and a `cell` already inserted via `write_cell(&cell)`
**When** I call `write_cell(&cell)` a second time with the identical PK
**Then** the second call returns `Err(CacheError::DuplicateCell { key })` whose `key` field equals `cell.key`. The error is mapped from rusqlite's `SqliteFailure(ffi::Error { code: ConstraintViolation, extended_code: SQLITE_CONSTRAINT_PRIMARYKEY, .. }, ..)` — UPSERT (`ON CONFLICT REPLACE` / `INSERT OR IGNORE`) is **not** used, because the FR26 lookup-before-measure invariant + FR52 single-writer scan.lock guarantee that a same-PK write at the cache layer indicates an upstream caller bug; surfacing it loudly is the design.

## Tasks / Subtasks

- [ ] **T1. Update `src/cache.rs` — declare `cell` submodule + extend `CacheError` with `DuplicateCell`** (AC: 1, 5)
  - [ ] T1.1 Add `pub mod cell;` to the existing module-declaration block in `src/cache.rs`. Preserve alphabetical order: `pub mod cell; pub mod key; pub mod migrations; pub mod schema;`.
  - [ ] T1.2 Update the file-level `//!` doc to mention the new submodule. Existing doc lists `key`, `schema`, `migrations`; add a fourth bullet: `[`cell`] owns the public `Cache` wrapper around `Connection` plus the `Cell` / `CellKey` value types and the atomic `write_cell` / `lookup_cell` primitives.`
  - [ ] T1.3 Append a fifth variant to `pub enum CacheError` — exactly one variant in this story:
    - `DuplicateCell { key: crate::cache::cell::CellKey }` — INSERT failed because the composite PK is already present. Display: `"cache already contains a cell with this primary key (machine_fingerprint={machine_fingerprint}, model_sha={model_sha}, backend_build={backend_build}, params_hash={params_hash}, task_id={task_id}, harness_version={harness_version}, task_subset_version={task_subset_version})"`. The Display template MUST include all seven PK columns by name + value so a debug-time encounter is fully self-describing without having to dump the source error chain.
    - The variant carries the *full* `CellKey` (not just the source `rusqlite::Error`) because the AC contract is "report which PK collided" — the rusqlite error itself only carries the constraint name, not the row values. Wrapping the `CellKey` is the canonical "what went wrong + which row" pattern.
  - [ ] T1.4 Do NOT add `From<CacheError> for crate::error::Error`. Same rule Stories 1.5 / 1.6 / 1.7 followed: boundary mapping decisions belong to the consumer story (Story 1.12, the first CLI wiring of cache → exit code). Pre-adding the `From` impl creates dead surface area until the consumer story exists.
  - [ ] T1.5 Do NOT add additional `CacheError` variants in this story (`SerializeBadges`, `DeserializeBadges`, `RowDecode`, `Busy`, etc.). The four variants from Story 1.7 (`Open`, `Pragma`, `MigrationFailed`, `FutureSchema`) plus the new `DuplicateCell` are the complete public error surface this story owns. Other variants land in their owner stories when concrete failure modes need distinct typed handling. The "single rusqlite::Error wrapped under a generic variant" pattern (e.g. wrap row-decode failures into a `Pragma { source }` analog) is fine for this story — see Resolved decisions for the precise mapping table.

- [ ] **T2. Author `src/cache/cell.rs` — `Cell`, `CellKey`, `Cache` wrapper, `write_cell`, `lookup_cell`** (AC: 1, 2, 3, 4, 5)
  - [ ] T2.1 File-level `//!` doc: `Cell` value type + the `Cache` wrapper around an open SQLite [`Connection`]. `write_cell` performs an atomic single-transaction INSERT; `lookup_cell` performs a single-row SELECT keyed on the seven-dimension composite PK. NFR-R2 (atomicity) and NFR-P5 (<100 ms lookup at 10K cells) are the binding requirements satisfied here.
  - [ ] T2.2 Define `pub struct CellKey` — the seven-dimension composite PK as a value type:
    ```rust
    /// Composite primary-key dimensions of a cache cell. All seven fields
    /// participate in the PK; their canonical derivation lives in
    /// [`crate::cache::key`].
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct CellKey {
        /// Machine fingerprint string (e.g. `"M1Pro-32GB-14gpu"`).
        pub machine_fingerprint: String,
        /// 64-char lowercase-hex SHA-256 of the model file (`cache::key::model_sha`).
        pub model_sha: String,
        /// Canonical `"<name>-<semver>+<commit_short>"` (`cache::key::backend_build`).
        pub backend_build: String,
        /// 64-char lowercase-hex SHA-256 of canonical params JSON (`cache::key::params_hash`).
        pub params_hash: String,
        /// Stable task identifier (e.g. `"swe-bench-pro:django-1234"`).
        pub task_id: String,
        /// Vendored mini-swe-agent harness version string.
        pub harness_version: String,
        /// Vendored task subset (SWE-Bench Pro) version string.
        pub task_subset_version: String,
    }
    ```
    - All fields `String` (owned). The values come from `cache::key::*` helpers which return `String`, so no lifetime gymnastics.
    - `Hash` derive added so a future caller can use `CellKey` as a `HashMap` / `HashSet` key without re-deriving — cheap and matches the natural identity semantics. `Eq` is required for `Hash` to be sound, hence the pair.
    - `PartialEq` (not `Eq` until manually verified) is fine because all fields are `String`. Actually all `String` fields satisfy `Eq` — derive both for correctness.
  - [ ] T2.3 Define `pub struct Cell` — the full row value type:
    ```rust
    /// A complete cache cell: composite PK + measurement attributes.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Cell {
        /// Composite PK (seven dimensions).
        pub key: CellKey,

        // Required attributes (NOT NULL columns)
        /// Container image identifier (digest) per FR17b.
        pub container_image_id: String,
        /// `lcrc` semver string at scan time (e.g. `"0.1.0"`).
        pub lcrc_version: String,
        /// Depth tier that produced this cell: `"quick"` | `"standard"` | `"full"`.
        pub depth_tier: String,
        /// ISO 8601 / RFC 3339 timestamp string from `util::time` (when that helper lands).
        pub scan_timestamp: String,
        /// Pass flag (`true` = task passed, `false` = failed). Stored as `INTEGER 0/1`.
        pub pass: bool,

        // Optional perf attributes (nullable columns) — see Resolved decisions
        /// Total task wall-clock seconds; `None` when perf collector failed (NFR-R4 graceful degrade).
        pub duration_seconds: Option<f64>,
        /// Tokens-per-second throughput (decode); `None` when perf collector failed.
        pub tokens_per_sec: Option<f64>,
        /// Time-to-first-token seconds; `None` when perf collector failed.
        pub ttft_seconds: Option<f64>,
        /// Peak resident-set size in bytes; `None` when collector failed.
        pub peak_rss_bytes: Option<i64>,
        /// Power draw in watts; v1: always `None`. v1.1+ launchd helper populates.
        pub power_watts: Option<f64>,
        /// Thermal-state string (e.g. `"nominal"`, `"throttled"`); `None` when unread.
        pub thermal_state: Option<String>,
        /// Badges attached to this cell. Empty `Vec` is canonical "no badges" — see Resolved decisions.
        pub badges: Vec<String>,
    }
    ```
    - `pass: bool` is the Rust-side type; serialization to SQLite's `INTEGER NOT NULL` column maps `false → 0`, `true → 1`. Round-trip on read maps `0 → false`, anything-non-zero → `true` (defensive: SQLite's `INTEGER` is i64, and the schema column is `INTEGER NOT NULL` so we trust producers but do not over-validate readers).
    - All numeric perf fields are `f64` / `i64`. SQLite's `REAL` is 8-byte IEEE 754, `INTEGER` ranges up to `i64::MAX`. Matching widths avoids silent truncation.
    - `peak_rss_bytes: Option<i64>` (signed) because rusqlite's `Column` for `INTEGER` returns `i64` natively. SQLite stores it as a signed 8-byte int regardless. RSS values in bytes fit in `i64` comfortably (max ~9.2 EiB).
    - `badges: Vec<String>` — empty Vec is "no badges". On write, serialize via `serde_json::to_string(&self.badges)` → produces `"[]"` for empty, `"[\"foo\",\"bar\"]"` otherwise. On read, NULL → empty Vec; non-NULL → `serde_json::from_str(&s)`. See Resolved decisions § "Badges nullability convention".
  - [ ] T2.4 Define `pub struct Cache` — the connection wrapper:
    ```rust
    /// Owned wrapper around an open SQLite [`Connection`] backing the cache.
    /// Provides the atomic single-cell `write_cell` / `lookup_cell` API.
    #[derive(Debug)]
    pub struct Cache {
        conn: Connection,
    }
    ```
    - **Owns the `Connection`** (not `Arc<Mutex<Connection>>`, not `Cow`, not a borrow). Rusqlite's `Connection` is the natural "open session" handle; wrapping it `Arc<Mutex<>>` would impose serialization overhead the v1 single-writer model (FR52 scan.lock) does not need. A consumer that needs concurrent access opens a second `Cache` against the same path — SQLite WAL handles concurrent readers + single writer at the engine level.
    - `Debug` derive is fine because `Connection: Debug`; clippy may grumble at `pedantic` level if the derive elides a field — `conn: Connection` is the only field, so it shows up as `Cache { conn: <Connection> }`. Acceptable.
    - **Do NOT derive `Clone`.** `Connection: !Clone`; the natural way to "duplicate" a Cache is to call `Cache::open(path)` again. Cloning the wrapper would give two handles to one `Connection`, which is not what SQLite's WAL concurrency model expects.
  - [ ] T2.5 Implement `pub fn open(path: &Path) -> Result<Self, CacheError>` on `Cache` — the public entry point that delegates to `crate::cache::migrations::open` and then wraps the resulting `Connection`:
    ```rust
    impl Cache {
        /// Open or create the cache database at `path`, enabling WAL and
        /// applying any pending migrations (delegates to
        /// [`crate::cache::migrations::open`]). Returns a [`Cache`] handle.
        ///
        /// # Errors
        /// Propagates every error variant of [`crate::cache::migrations::open`]:
        /// [`CacheError::Open`], [`CacheError::Pragma`],
        /// [`CacheError::MigrationFailed`], [`CacheError::FutureSchema`].
        pub fn open(path: &Path) -> Result<Self, CacheError> {
            let conn = crate::cache::migrations::open(path)?;
            Ok(Self { conn })
        }
    }
    ```
    - `Cache::open` is the v1 public surface. `migrations::open` stays public (Story 1.7 contract) for callers that want a bare `Connection` (tests, future low-level tooling); `Cache::open` is the application-facing wrapper.
    - **Synchronous on purpose.** Same logic as Story 1.7's `migrations::open`: rusqlite is sync; bridging to async at the primitive layer wastes runtime. Story 1.12 (CLI wiring) wraps the synchronous `Cache::open` / `write_cell` / `lookup_cell` calls in `tokio::task::spawn_blocking`. See Resolved decisions for the locked sync/async split.
  - [ ] T2.6 Implement `pub fn write_cell(&self, cell: &Cell) -> Result<(), CacheError>` on `Cache`:
    ```rust
    /// Atomically insert a single cell within one SQLite transaction.
    /// On primary-key collision returns [`CacheError::DuplicateCell`].
    ///
    /// # Errors
    /// - [`CacheError::DuplicateCell`] when the seven-dimension composite PK
    ///   already exists in the `cells` table.
    /// - [`CacheError::Pragma`] for any other rusqlite failure during the
    ///   INSERT, the JSON encoding of `badges`, or the surrounding
    ///   transaction commit. (Generic variant; specialized variants are
    ///   added in their owner stories when concrete handling diverges.)
    pub fn write_cell(&self, cell: &Cell) -> Result<(), CacheError> {
        // Hold a read of `&self.conn` across the closure body. `Connection`
        // is not `Sync`, but `&Connection` borrows are; `transaction()`
        // takes `&mut self` so we need a `&mut Connection` — push that on
        // the caller via `&mut self`. Actually `Connection::transaction`
        // takes `&mut self`. Switch the signature to `&mut self` if the
        // dev-story implementer hits the borrow check. See Resolved
        // decisions § "&self vs &mut self on Cache::write_cell".
        ...
    }
    ```
    - **Implementation outline** (the dev-story implementer fills this in; the contract above is what the unit tests pin):
      1. Encode `cell.badges` to a JSON string via `serde_json::to_string(&cell.badges)`. On encode failure (impossible for `Vec<String>` — strings always JSON-encode cleanly) wrap as `CacheError::Pragma { source: rusqlite::Error::ToSqlConversionFailure(...) }`. See § "Resolved decisions → badges encoding error mapping" for the locked variant choice.
      2. Open a transaction: `let tx = self.conn.transaction().map_err(|source| CacheError::Pragma { source })?;`
      3. Execute the parameterized INSERT with all 19 column values:
         ```rust
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
                 cell.key.machine_fingerprint, cell.key.model_sha, cell.key.backend_build,
                 cell.key.params_hash, cell.key.task_id, cell.key.harness_version,
                 cell.key.task_subset_version,
                 cell.container_image_id, cell.lcrc_version, cell.depth_tier,
                 cell.scan_timestamp,
                 i64::from(cell.pass),                  // bool → 0/1
                 cell.duration_seconds, cell.tokens_per_sec, cell.ttft_seconds,
                 cell.peak_rss_bytes, cell.power_watts, cell.thermal_state,
                 badges_json,
             ],
         ).map_err(|source| /* see step 4 */)?;
         ```
      4. **Map the INSERT error.** Inspect `source` — if it is `rusqlite::Error::SqliteFailure(ffi::Error { code: ErrorCode::ConstraintViolation, extended_code }, _)` AND `extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY` (value `1555`), return `CacheError::DuplicateCell { key: cell.key.clone() }`. Otherwise wrap as `CacheError::Pragma { source }`. Use a `match` (not nested `if let`) for readability; see test T3.5 for the exact discriminator the tests pin.
      5. Commit: `tx.commit().map_err(|source| CacheError::Pragma { source })?;`
      6. Return `Ok(())`.
    - **Atomicity guarantee.** Steps 2–5 are bracketed by `BEGIN; INSERT; COMMIT;`. SQLite atomically commits the INSERT or rolls back if `commit()` fails or the `tx` is dropped without commit (RAII rollback). NFR-R2 satisfied at the SQLite-engine layer.
    - **`&mut self` vs `&self` — the borrow-check resolution.** `Connection::transaction` takes `&mut self`. So `Cache::write_cell` must be `&mut self`. This is the locked signature: `pub fn write_cell(&mut self, cell: &Cell) -> Result<(), CacheError>`. Story 2.6 (multi-model orchestrator) will need to mutate the Cache from inside a per-cell loop; passing `&mut Cache` through the call stack is fine and matches the v1 single-writer model. See Resolved decisions for the alternatives explored and rejected.
  - [ ] T2.7 Implement `pub fn lookup_cell(&self, key: &CellKey) -> Result<Option<Cell>, CacheError>` on `Cache`:
    ```rust
    /// Look up a single cell by its composite PK. Returns `Ok(None)` if no
    /// row matches. Reads do not require a transaction — SQLite's WAL mode
    /// gives lock-free consistent reads.
    ///
    /// # Errors
    /// [`CacheError::Pragma`] for any rusqlite failure (statement
    /// preparation, row decode, or `badges` JSON parse).
    pub fn lookup_cell(&self, key: &CellKey) -> Result<Option<Cell>, CacheError> {
        ...
    }
    ```
    - **`&self` (not `&mut self`)** because reads don't need a transaction. `Connection::query_row` takes `&self` for the SELECT path.
    - **Implementation outline:**
      1. Prepare a parameterized SELECT over all 19 columns:
         ```rust
         let mut stmt = self.conn.prepare(
             "SELECT machine_fingerprint, model_sha, backend_build, params_hash,
                     task_id, harness_version, task_subset_version,
                     container_image_id, lcrc_version, depth_tier, scan_timestamp,
                     pass, duration_seconds, tokens_per_sec, ttft_seconds,
                     peak_rss_bytes, power_watts, thermal_state, badges
                FROM cells
               WHERE machine_fingerprint = ?1 AND model_sha = ?2
                 AND backend_build = ?3 AND params_hash = ?4
                 AND task_id = ?5 AND harness_version = ?6
                 AND task_subset_version = ?7"
         ).map_err(|source| CacheError::Pragma { source })?;
         ```
      2. `stmt.query_row(params!, |row| { ... decode all 19 columns into a Cell ... })`. Use `match` on the result: `Ok(cell)` → `Ok(Some(cell))`; `Err(rusqlite::Error::QueryReturnedNoRows)` → `Ok(None)`; any other `Err` → `Err(CacheError::Pragma { source })`.
      3. **Decoding `pass`**: `let pass_int: i64 = row.get(11)?; let pass = pass_int != 0;`
      4. **Decoding nullable columns**: `row.get::<_, Option<f64>>(12)?` for `duration_seconds` and friends. rusqlite's `FromSql for Option<T>` maps NULL → `None`, non-NULL → `Some(value)`.
      5. **Decoding `badges`**: `let badges_raw: Option<String> = row.get(18)?; let badges = match badges_raw { Some(s) => serde_json::from_str(&s).map_err(|e| /* wrap as Pragma */)?, None => Vec::new() };`. The JSON-decode error wrapper goes through `CacheError::Pragma` per the generic-variant policy in Resolved decisions.
    - **Why `prepare` + `query_row` vs `query_row` directly.** `Connection::query_row(sql, params, f)` is fine for one-shot queries. `prepare`-then-`query_row` has identical correctness; it's a stylistic call. Use whichever the dev finds clearer — both are tested by the same AC. The locked recommendation is `prepare`-then-`query_row` because the rendered SQL is long enough that locking the prepared-statement variable name aids readability.
    - **No additional indexes** beyond the implicit PK index. The composite PK on (machine_fingerprint, model_sha, backend_build, params_hash, task_id, harness_version, task_subset_version) gives SQLite a covering index for the equality SELECT — the lookup is an O(log n) probe, satisfying NFR-P5 (<100 ms at 10K cells) by an order of magnitude. Read-side query patterns for `lcrc show` (Story 4.1) drive the next index decisions.
  - [ ] T2.8 **Do NOT** add a `Cache::write_cells_batch` / bulk-write API. Story 1.8's contract is the single-cell primitive. Bulk writes (batch INSERT, prepared statement reuse) are an Epic 6 perf optimization or land in their own story when actual usage warrants. Pre-adding them is API speculation.
  - [ ] T2.9 **Do NOT** add a `Cache::iter_cells` / range-scan / leaderboard query. `lcrc show`'s leaderboard scan lands in Story 4.1 inside `src/cache/query.rs` (or wherever 4.1's author chooses). This story owns only the single-cell primitives.
  - [ ] T2.10 **Do NOT** add a `Cache::delete_cell` / mutation API beyond INSERT. v1 cache cells are immutable: a re-measurement is either a NEW cell (different `backend_build` or other PK dim → distinct row) or — in `lcrc verify` — a comparison against the cached value (Story 5.1+, READ-only). DELETE / UPDATE land if and when a real consumer needs them.
  - [ ] T2.11 **Do NOT** add a `From<rusqlite::Error> for CacheError` blanket impl. Each `?` propagation in `write_cell` / `lookup_cell` MUST go through an explicit `.map_err(|source| CacheError::Pragma { source })` (or, for the INSERT error, the duplicate-key discriminator). A blanket `From` impl loses the contextual variant choice (`Pragma` vs `MigrationFailed` vs `DuplicateCell`) that the per-call mapping makes explicit.

- [ ] **T3. In-module unit tests in `src/cache/cell.rs`** (AC: 1, 5)
  - [ ] T3.1 Standard test-module attribute set: `#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] mod tests { ... }`. Tests use `Cache::open` against a `tempfile::TempDir`-backed path so they exercise the real file-backed code path including WAL mode (in-memory DBs cannot be passed to `migrations::open` because `migrations::open` takes `&Path`; the unit-vs-integration split is therefore by *what is asserted*, not by `:memory:` vs file).
  - [ ] T3.2 Helper `fn fresh_cache() -> (TempDir, Cache)` at the top of the test module: returns `(dir, Cache::open(&dir.path().join("lcrc.db")).unwrap())`. The `TempDir` is returned alongside so the caller scopes its lifetime to the test (drop = filesystem cleanup).
  - [ ] T3.3 Helper `fn synthetic_cell(seed: u32) -> Cell` that returns a `Cell` with deterministic content varying only by `seed`. Vary at least `task_id` so distinct seeds produce distinct PKs (e.g. `task_id: format!("synthetic:task-{seed:06}")`). All other fields can stay constant per call. Used by T3 and the perf tests in T4.
  - [ ] T3.4 `write_then_lookup_roundtrips_all_columns` — `let (_dir, mut cache) = fresh_cache(); let cell = synthetic_cell(0); cache.write_cell(&cell).unwrap(); let got = cache.lookup_cell(&cell.key).unwrap(); assert_eq!(got, Some(cell));`. AC1's full-column round-trip — the `PartialEq` derive on `Cell` (T2.3) makes this a one-line assertion.
  - [ ] T3.5 `lookup_missing_key_returns_none` — `let (_dir, cache) = fresh_cache(); let key = synthetic_cell(7).key; assert_eq!(cache.lookup_cell(&key).unwrap(), None);`. AC4's nonexistent-key half (perf budget verified separately in T4 at scale).
  - [ ] T3.6 `write_then_write_same_pk_returns_duplicate_cell` — `let (_dir, mut cache) = fresh_cache(); let cell = synthetic_cell(3); cache.write_cell(&cell).unwrap(); let err = cache.write_cell(&cell).unwrap_err(); match err { CacheError::DuplicateCell { key } => assert_eq!(key, cell.key), other => panic!("expected DuplicateCell, got {other:?}") }`. AC5's duplicate-PK contract.
  - [ ] T3.7 `duplicate_cell_display_lists_all_seven_pk_dimensions` — construct a `CacheError::DuplicateCell { key: synthetic_cell(0).key }`, assert that its rendered Display string contains EACH of the seven PK column names as a substring (`"machine_fingerprint"`, `"model_sha"`, `"backend_build"`, `"params_hash"`, `"task_id"`, `"harness_version"`, `"task_subset_version"`). Pins the AC5 self-describing-message contract against future Display-template edits. Same Display-substring lesson as Story 1.5 § AC3 (`"unsupported hardware"`) and Story 1.7 § AC5 (`"upgrade lcrc"`).
  - [ ] T3.8 `cell_with_all_optional_perf_fields_some_roundtrips` — `synthetic_cell` variant where every nullable column is `Some(...)` (`duration_seconds: Some(12.5)`, `tokens_per_sec: Some(34.7)`, etc.); write + lookup; assert exact equality including each `Some`.
  - [ ] T3.9 `cell_with_all_optional_perf_fields_none_roundtrips` — `synthetic_cell` variant where every nullable column is `None`; write + lookup; assert each comes back `None`. NFR-R4 graceful-degrade serialization correctness.
  - [ ] T3.10 `cell_with_empty_badges_roundtrips_as_empty_vec` — `synthetic_cell` with `badges: Vec::new()`; write + lookup; assert `got.unwrap().badges == Vec::<String>::new()`. The "empty badges → JSON `[]` → empty Vec" canonical encoding (see Resolved decisions § "Badges nullability convention").
  - [ ] T3.11 `cell_with_multiple_badges_roundtrips_with_order_preserved` — `synthetic_cell` with `badges: vec!["ctx-limited".into(), "thermal-throttled".into()]`; write + lookup; assert `got.unwrap().badges == vec!["ctx-limited", "thermal-throttled"]`. JSON arrays preserve order; this pins that contract against any future Set-style refactor.
  - [ ] T3.12 `pass_true_and_pass_false_roundtrip` — write two cells (different `task_id` PKs to avoid collision), one with `pass: true`, one with `pass: false`; lookup both; assert each round-trips its `pass` value. Pins the `bool → i64 0/1 → bool` mapping.
  - [ ] T3.13 `cellkey_partial_eq_eq_hash_consistency` — `let k1 = synthetic_cell(0).key; let k2 = synthetic_cell(0).key; assert_eq!(k1, k2); use std::collections::HashSet; let mut set = HashSet::new(); set.insert(k1.clone()); assert!(set.contains(&k2));`. Cheap structural test that the derive set on `CellKey` is internally consistent (PartialEq matches Hash).
  - [ ] T3.14 **Do NOT** add a per-column-NULL parameterized test (one test per nullable column with that single column NULL, the rest Some). The "all None" + "all Some" pair (T3.8 / T3.9) plus the round-trip equality check are sufficient: any per-column NULL handling bug would surface in the all-None test (read decodes NULL incorrectly) or the all-Some test (write encodes Some incorrectly). Per-column tests would multiply by 7 (the number of nullable columns) for negligible additional coverage.
  - [ ] T3.15 **Do NOT** add a perf test in this in-module unit suite. The 10K-cell perf tests (AC3, AC4) live in the integration tests (T4) where they have access to the full `lcrc::cache::cell::Cache` public API and are filed under the perf-budget category. In-module tests must stay fast (cargo test runs them on every commit) — perf assertions belong to slower integration runs.

- [ ] **T4. Author `tests/cache_roundtrip.rs` — public-API integration tests + NFR-P5 perf budget** (AC: 1, 2, 3, 4, 5)
  - [ ] T4.1 New file `tests/cache_roundtrip.rs`. Standard integration-test crate (separate compilation unit; sees `lcrc::*` only via the public API, no `pub(crate)` access). Standard exemption attribute: `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` at file top (matches `tests/cache_migrations.rs:6`, `tests/cli_exit_codes.rs`, etc.). Plain `#[test]` (NOT `#[tokio::test]` — `Cache::open`, `write_cell`, `lookup_cell` are sync).
  - [ ] T4.2 Imports: `use lcrc::cache::cell::{Cache, Cell, CellKey}; use lcrc::cache::CacheError; use std::time::{Duration, Instant}; use tempfile::TempDir;`. No glob imports.
  - [ ] T4.3 Helper `fn cell_at(seed: u32) -> Cell` — same shape as T3.3's `synthetic_cell` but lives in the integration crate (which cannot see the `cell.rs` `tests` module). Vary `task_id` by seed; keep all other fields constant + plausible. Used by every test in this file.
  - [ ] T4.4 Helper `fn fresh_cache() -> (TempDir, Cache)` — same shape as T3.2's helper, lifted into this crate (same justification: the test module of `cell.rs` is not visible to integration crates).
  - [ ] T4.5 `roundtrip_single_cell_via_public_api` — write a cell via `Cache::write_cell`, look it up via `Cache::lookup_cell`, assert exact `PartialEq` equality. AC1's end-to-end verification at the public boundary.
  - [ ] T4.6 `lookup_missing_key_returns_none_via_public_api` — `Cache::lookup_cell(&unrelated_key).unwrap()` returns `None`. AC4's nonexistent-key correctness.
  - [ ] T4.7 `transaction_rollback_on_panic_leaves_no_partial_row` — AC2 verification. The cleanest in-process simulation of "a write that aborts mid-transaction" is to use `std::panic::catch_unwind` around a closure that opens a transaction, does a partial INSERT (or skips the commit), then panics:
    ```rust
    use rusqlite::Connection;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("lcrc.db");
    let cell = cell_at(42);

    let _ = std::panic::catch_unwind(|| {
        let conn = lcrc::cache::migrations::open(&path).unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        // Execute the INSERT but panic before committing.
        tx.execute(
            "INSERT INTO cells (machine_fingerprint, model_sha, backend_build, params_hash,
                task_id, harness_version, task_subset_version, container_image_id,
                lcrc_version, depth_tier, scan_timestamp, pass, duration_seconds,
                tokens_per_sec, ttft_seconds, peak_rss_bytes, power_watts,
                thermal_state, badges) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
            rusqlite::params![/* full cell + JSON badges */],
        ).unwrap();
        // Drop tx without commit by panicking — RAII rolls back.
        panic!("simulated abort");
    });

    // Reopen the cache and confirm the cell did NOT persist.
    let cache = Cache::open(&path).unwrap();
    assert_eq!(cache.lookup_cell(&cell.key).unwrap(), None);
    ```
    - **Why `unchecked_transaction` (not `transaction`)**: `Connection::transaction` takes `&mut self`, so the `Connection` is mutably borrowed across the closure body, and `catch_unwind` requires `RefUnwindSafe + UnwindSafe` bounds the borrow doesn't satisfy. `unchecked_transaction` takes `&self` (shared borrow) and is the documented rusqlite escape hatch for exactly this pattern. The "unchecked" refers to "no compile-time prevention of nested-tx misuse", not to anything atomicity-relevant.
    - **Why the test uses raw `migrations::open` + `unchecked_transaction` instead of `Cache::write_cell`**: the production `write_cell` uses `transaction()` which RAII-rolls-back on panic correctly, but exercising the panic path through it would require panicking inside the rusqlite-managed transaction scope. The raw transaction is the closest equivalent — same atomicity guarantees, more direct test of "INSERT executed but transaction aborted → nothing visible after reopen".
    - The post-panic `Cache::open` (against the same path) is the verification step: a successful open + lookup = no partial state — NFR-R2 atomicity at the SQLite-engine layer.
  - [ ] T4.8 `lookup_existing_key_at_10k_cells_under_100ms_NFR_P5` — AC3 verification:
    ```rust
    let (_dir, mut cache) = fresh_cache();
    for seed in 0..10_000u32 {
        cache.write_cell(&cell_at(seed)).unwrap();
    }
    let target_key = cell_at(7_777).key;  // any seed in the loop range
    let start = Instant::now();
    let result = cache.lookup_cell(&target_key).unwrap();
    let elapsed = start.elapsed();
    assert!(result.is_some(), "expected hit for seed 7_777");
    assert!(elapsed < Duration::from_millis(100), "lookup took {:?}, exceeds NFR-P5 budget", elapsed);
    ```
    - **Cold-cache budget realism**: SQLite PK lookup at 10K rows on a stock SSD-backed `TempDir` typically completes in single-digit milliseconds. The 100 ms budget has ~10× headroom even on the slowest CI runner.
    - **Build-time creep awareness**: this test populates 10K rows via 10K individual `write_cell` calls, each its own transaction. Estimated wall-clock: ~3-8 seconds on M1 Pro, possibly ~10-15 s on the CI runner. Acceptable for an integration test; if the populate phase exceeds 30 s on CI in practice, switch the populate phase to a single explicit transaction that batches all 10K INSERTs (the perf measurement is the LOOKUP, not the populate). See Resolved decisions for the populate-phase budget guidance.
  - [ ] T4.9 `lookup_missing_key_at_10k_cells_under_100ms_NFR_P5` — AC4 verification: same setup as T4.8 (10K populated cells), look up a key whose `task_id` is `"synthetic:task-999999"` (well outside the populated `0..10_000` range), assert `result == None` and `elapsed < Duration::from_millis(100)`.
    - Both T4.8 and T4.9 may share the populate-phase scaffolding via a small helper `fn populate_10k(cache: &mut Cache)` to avoid duplicating the loop. The two tests separate by AC and by what they assert about the result.
  - [ ] T4.10 `duplicate_pk_returns_duplicate_cell_via_public_api` — AC5 end-to-end via the public API: write a cell, attempt to write the same cell again, match `Err(CacheError::DuplicateCell { key })` with `key == cell.key`. Mirrors T3.6 but via the integration crate boundary.
  - [ ] T4.11 **Do NOT** spawn the `lcrc` binary in this test (no `assert_cmd::Command::cargo_bin("lcrc")`). The CLI wiring of `Cache` lives in Story 1.12; testing it here would conflate this story's primitive surface with the integration surface. Same rule Story 1.7 § T5.8 applied to `cache_migrations.rs`.
  - [ ] T4.12 **Do NOT** add a concurrent-writer test (two threads, one or two Cache instances, racing the same PK). The AC5 "(whether concurrent or sequential)" parenthetical is a generality clause — the same invariant holds in both access patterns because the test surface is the SQLite transaction layer. The single-writer model (FR52 scan.lock, Story 6.4) prevents real-world concurrent same-PK writes; testing the SQLite-level edge case here would over-scope a primitive-author story.
  - [ ] T4.13 **Do NOT** assert on the absolute lookup wall-clock value beyond `< 100 ms`. The AC budget is 100 ms; tightening the assertion to `< 10 ms` or similar invites flaky CI on slow runners. The 100 ms ceiling is the spec contract.

- [ ] **T5. Local CI mirror** (AC: 1, 2, 3, 4, 5)
  - [ ] T5.1 Run `cargo build` — confirms the module compiles. `Cargo.lock` should be unchanged: this story uses `rusqlite` (Cargo.toml line 45), `serde_json` (line 31, added by Story 1.6), `tempfile` (line 49), `thiserror` (line 60) — all already locked. If `Cargo.lock` does change, investigate before pushing — that signals an unintended dep introduction.
  - [ ] T5.2 Run `cargo fmt` — apply rustfmt; commit any reformatted lines.
  - [ ] T5.3 Run `cargo clippy --all-targets --all-features -- -D warnings` locally. Specifically watch for:
    - `clippy::missing_errors_doc` on `pub fn open` / `write_cell` / `lookup_cell` — `# Errors` rustdoc section per T2.5 / T2.6 / T2.7.
    - `clippy::missing_docs` on every `pub` item (`Cache`, `Cell`, `CellKey`, every public field of `Cell` / `CellKey`, every method).
    - `clippy::module_name_repetitions` on `Cache` may NOT fire because the type is `Cache` (no `Cell` suffix) inside the `cell` submodule — verify locally.
    - `clippy::struct_excessive_bools` should NOT fire — `Cell` has exactly one `bool` field (`pass`).
    - `clippy::too_many_arguments` should NOT fire — methods take `&Cell` / `&CellKey`, not flat argument lists.
    - `clippy::needless_pass_by_value` should NOT fire — all public method params are `&self` / `&mut self` / `&Cell` / `&CellKey`.
  - [ ] T5.4 Run `cargo test` — confirms all in-module unit tests in `src/cache/cell.rs::tests` pass AND the new `tests/cache_roundtrip.rs` integration tests pass AND every existing test in the suite (in-module suites for `cache::key`, `cache::migrations`, `error`, `exit_code`, `machine`, `output`, `version`, etc., plus `tests/cache_migrations.rs`, `tests/cli_exit_codes.rs`, `tests/cli_help_version.rs`, `tests/machine_fingerprint.rs`) still passes.
  - [ ] T5.5 Manual scope-discipline grep: `git grep -nE 'rusqlite::|PRAGMA|user_version|INSERT INTO cells|SELECT .* FROM cells' src/ tests/ | grep -v '^src/cache/cell.rs:' | grep -v '^src/cache/migrations.rs:' | grep -v '^src/cache/schema.rs:' | grep -v '^src/cache.rs:' | grep -v '^tests/cache_migrations.rs:' | grep -v '^tests/cache_roundtrip.rs:'`. Must produce zero matches — the rusqlite + cells-table SQL surface stays contained inside the cache modules and their tests. Same single-source-of-truth grep contract Stories 1.6 / 1.7 used.
  - [ ] T5.6 Verify the perf-test wall-clock on the local machine: `cargo test --test cache_roundtrip lookup_existing_key_at_10k_cells_under_100ms_NFR_P5 -- --nocapture` — run manually; eyeball that the test wall-clock is well under the 100 ms budget (typically single-digit ms on M1 Pro). If the lookup approaches 100 ms locally, investigate the populate phase (per-row tx overhead) and consider the bulk-populate optimization noted in T4.8.

## Dev Notes

### Scope discipline (read this first)

This story authors **one new file** and **updates one existing file** plus **one new test file**:

- **New (Rust source):** `src/cache/cell.rs` — `Cell`, `CellKey`, `Cache` wrapper, `Cache::open` / `write_cell` / `lookup_cell` + in-module tests
- **Updated:** `src/cache.rs` — add `pub mod cell;` declaration; append `DuplicateCell { key: CellKey }` variant to `CacheError`
- **New (tests):** `tests/cache_roundtrip.rs` — integration-level public-API verification + NFR-P5 perf budget

This story does **not**:

- Wire `Cache::write_cell` or `Cache::lookup_cell` into any CLI command (`lcrc scan`, `lcrc show`, `lcrc verify`). The CLI integration is Story 1.12 (end-to-end one-cell scan) and Stories 4.1+ (`lcrc show` against the cache). Pre-wiring violates the tracer-bullet vertical-slice principle (`MEMORY.md → feedback_tracer_bullet_epics.md`) and inflates this story past its single concern.
- Author `src/cache/query.rs`. The architecture line 902 reserves it for "leaderboard, drift, sample queries" — Story 4.1 (`lcrc show` plain-text leaderboard) is the first consumer that needs range scans / aggregations. Pre-creating the file with only `lookup_cell` inside would either duplicate `cell.rs`'s API or split a single primitive across two files for no reason. `lookup_cell` lives in `cell.rs` alongside `write_cell` because both are single-cell primitives over the same `Cell` value type.
- Define the `Badge` enum in `src/report/badges.rs`. Story 2.4 (initial Badge enum + HTML rendering) owns that file and its enum. Until then, `Cell::badges` is `Vec<String>` — Story 2.4 may either keep it `Vec<String>` (with the enum used at the rendering layer) or convert the column to `Vec<Badge>` via an additive type change.
- Define the `Params` / `BackendInfo` types beyond what already exists in `src/cache/key.rs` (Story 1.6). `CellKey` carries the *derived* PK strings, not the upstream typed inputs.
- Wire `tokio::task::spawn_blocking` around `Cache::*` calls. The architecture's pattern (architecture.md line 697 illustrates `pub async fn write_cell`) places the async wrapping at the *consumer* layer (Story 1.12 wires the CLI; Story 2.6 wires the orchestrator). For Story 1.8, the Cache API stays sync — Story 1.7's locked rationale (rusqlite is sync; sync primitives + spawn_blocking at the consumer) carries forward.
- Add config-side path resolution (`{paths.cache_dir}/lcrc.db`). The TOML config schema lives in Story 6.1; for now `Cache::open` accepts any `&Path` and tests pass tempdir paths. Story 1.12 wires `cli/scan.rs` to compose `config.paths.cache_dir.join("lcrc.db")` and pass it.
- Add tracing/logging events (`tracing::info!("wrote cell ...")`, `tracing::debug!("lookup hit/miss")`). Same rule Stories 1.5 / 1.6 / 1.7 followed: this story's primitives are silent on success; the consumer story (Story 1.12 + Story 2.6) wires the per-cell completion / lookup-hit-rate events once it owns the call site.
- Add `From<CacheError> for crate::error::Error`. Story 1.12 (the consumer) decides the boundary mapping.
- Touch `src/main.rs`, `src/cli.rs`, `src/cli/*.rs`, `src/error.rs`, `src/exit_code.rs`, `src/output.rs`, `src/util.rs`, `src/util/tracing.rs`, `src/version.rs`, `src/machine.rs`, `src/machine/apple_silicon.rs`, `src/cache/key.rs`, `src/cache/schema.rs`, `src/cache/migrations.rs`, `tests/cli_exit_codes.rs`, `tests/cli_help_version.rs`, `tests/machine_fingerprint.rs`, `tests/cache_migrations.rs`, `Cargo.toml`, `Cargo.lock`, `build.rs`, or `.github/workflows/*`. None of those need to change for Story 1.8.
- Author or update `tasks/swe-bench-pro/manifest.json` or any vendored task data. Container concerns are owned by Story 1.10 / 1.14.
- Add `CREATE INDEX` statements to `src/cache/schema.rs`. The cells table's composite PK (`PRIMARY KEY (machine_fingerprint, ..., task_subset_version)`) gives SQLite a covering index on the `lookup_cell` access pattern (full-PK equality probe) — the lookup is O(log n). Read-side query patterns for `lcrc show` (range scans by `model_sha`, depth_tier filters) drive the next index decisions; pre-indexing for hypothetical access patterns is API speculation. Story 4.1 owns the leaderboard-side index call.
- Add a new schema migration (`CELLS_DDL_V2`). v1's schema is complete for the AC set (the cells table from Story 1.7 covers every column `Cell` needs to round-trip). Future schema changes (v1.1 pass@k via `trial_id`, v1.1 `power_watts` populated by the launchd helper) land in their owner stories with their own migration scripts.
- Define `CacheError::SerializeBadges` / `DeserializeBadges` variants. JSON encode/decode failures are surfaced via the generic `CacheError::Pragma` variant with the wrapped rusqlite error chain (encode = `ToSqlConversionFailure`, decode = `FromSqlConversionFailure`). A specialized variant is added if and when a consumer needs typed handling distinct from "any other PRAGMA / SQLite failure" — see Resolved decisions § "Generic Pragma variant scope".

### Architecture compliance (binding constraints)

- **Single source of truth: `src/cache/cell.rs` for the cells-table read/write SQL** [Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" lines 901-902 + § "Architectural Boundaries" line 998]: `cell.rs` owns the INSERT and SELECT SQL strings; `migrations.rs` (Story 1.7) owns the DDL; `schema.rs` (Story 1.7) owns the DDL constants. No other module embeds raw `INSERT INTO cells` / `SELECT ... FROM cells` SQL or executes `rusqlite::Connection` calls. After this story merges, the SQL + rusqlite surface is contained inside `src/cache/{cell,migrations,schema}.rs` + `src/cache.rs` + the matching `tests/cache_*.rs` files; the T5.5 grep guards this contract.
- **No `unsafe` anywhere** [Source: `unsafe_code = "forbid"` in Cargo.toml line 78 + `lib.rs:3`]: rusqlite ships with internal `unsafe` for FFI to libsqlite3 — that is its problem; the host crate stays `forbid(unsafe_code)`.
- **All async file I/O via `tokio::fs` / `tokio::process`, never `std::fs` / `std::process`** [Source: architecture.md line 687]: this story's `Cache` API is *synchronous* by design — no `tokio::fs` calls, no `std::fs` calls. The only filesystem touch happens inside `migrations::open` (Story 1.7) which uses `rusqlite::Connection::open(path)` via FFI — not `std::fs`. AR-3's intent (no sync I/O bridged into async contexts) is upheld because consumers wrap `Cache::*` calls in `spawn_blocking` themselves.
- **No `std::process` anywhere** [Source: AR-3]: N/A in this story — no subprocess execution.
- **Workspace lints — `unwrap_used`, `expect_used`, `panic = "deny"`** [Source: Cargo.toml lines 83-85]: All `?` propagation against `CacheError`. The two test surfaces (`#[cfg(test)] mod tests` in `cell.rs`, `tests/cache_roundtrip.rs`) carry the documented `#[allow(...)]` exemption pattern. Production code uses zero `unwrap` / `expect` / `panic`.
- **`missing_docs = "warn"`** [Source: Cargo.toml line 79]: Every `pub` item gets a `///` doc — `Cache`, `Cell`, `CellKey`, every public field of `Cell` / `CellKey`, every public method. `pub fn open` / `write_cell` / `lookup_cell` return `Result`, so each also needs a `# Errors` rustdoc section (clippy `missing_errors_doc`).
- **MSRV 1.95** [Source: Cargo.toml line 5]: `Connection::transaction`, `Connection::execute`, `Connection::prepare`, `Connection::query_row`, `Connection::unchecked_transaction` are all stable in rusqlite 0.32. `serde_json::to_string` / `serde_json::from_str` for `Vec<String>` are stable in serde_json 1. `std::panic::catch_unwind` is stable since Rust 1.9. `std::time::Instant` / `Duration` are stable. No nightly-only features.
- **Crate is binary + library** [Source: architecture.md § "Complete Project Directory Structure" line 874-876 + Story 1.3 T1.2]: `cache::cell` is library-only; `tests/cache_roundtrip.rs` consumes it as `lcrc::cache::cell::{Cache, Cell, CellKey}` and `lcrc::cache::CacheError`. `cargo test` exercises the library path.
- **Tracing / logging discipline** [Source: AR `tracing` discipline + architecture.md § "Tracing / Logging" line 770]: This story emits **no** tracing events. `Cache::write_cell` and `lookup_cell` are silent on success and on `CacheError`-typed errors; the consumer (Story 1.12 + Story 2.6) decides whether to `tracing::info!("wrote cell {model_sha}/{task_id}")` after a successful write.
- **Atomic-write discipline** [Source: AR atomic writes + architecture.md § "Atomic-Write Discipline" line 692-705]: `write_cell` runs a single SQLite transaction `BEGIN; INSERT; COMMIT;` per the architecture's locked example. A crash mid-transaction RAII-rolls back via `Transaction`'s `Drop` impl; the cell is either fully present or absent. This is the literal NFR-R2 contract.
- **No glob imports** [Source: implicit per existing code style; verified by grepping `src/` for `use rusqlite::*`]: Always name the imported items (`use rusqlite::Connection;`, `use rusqlite::params;`) — the locked codebase uses no `*` imports.
- **`Cargo.lock` is committed; CI cache keys on it** [Source: Story 1.2 § Architecture compliance]. This story adds **no** new dependencies. `Cargo.lock` should be unchanged after `cargo build`. If it changes, investigate before pushing — most likely an accidental glob-import or a `tempfile` re-resolve.
- **Cache key canonicalization** [Source: architecture.md § "Cache Key Canonicalization" line 720-729]: `CellKey` carries the derived PK strings produced by `cache::key::*` helpers (Story 1.6). This story does NOT re-derive `model_sha` / `params_hash` / `machine_fingerprint` / `backend_build` — it accepts them as `String` fields on `CellKey`. Producers of `Cell` / `CellKey` (Story 1.12, Story 2.6) call `cache::key::*` helpers to populate those fields.
- **Single-writer model** [Source: architecture.md § "Cache Architecture" line 287-294 + FR52]: v1 cache is single-writer (one `lcrc scan` at a time, enforced by Story 6.4's `scan.lock`). `Cache::write_cell` does not need application-layer write serialization; SQLite WAL handles the engine layer. Concurrent reads (`lcrc show` during `lcrc scan`, FR53) are lock-free via WAL.

### Resolved decisions (don't re-litigate)

These are choices the dev agent might be tempted to revisit. Each is locked here with rationale.

- **`Cache` is a wrapper struct around an owned `Connection`, not `Arc<Mutex<Connection>>`.** Why: v1's single-writer model (FR52 scan.lock + FR26 lookup-before-measure) does not require application-layer write serialization. Wrapping in `Arc<Mutex<>>` would impose lock contention overhead on every operation for a concurrency model the architecture explicitly does not support. Concurrent readers open their own `Cache::open(path)` — SQLite WAL allows multiple connections against the same file with lock-free reads + serialized writes at the engine layer.
- **`Cache::write_cell` takes `&mut self`; `Cache::lookup_cell` takes `&self`.** Why: rusqlite's `Connection::transaction` requires `&mut self` (the transaction needs exclusive access to the connection's prepared-statement cache during the BEGIN-COMMIT scope). Reads via `Connection::query_row` / `prepare` need only `&self`. The asymmetry is the natural rusqlite shape; matches Rust's reads-vs-writes borrow contract. Story 2.6's orchestrator passes `&mut Cache` through the per-cell loop.
- **`Cache::open` is SYNC, not `async`.** Why: same as Story 1.7's `migrations::open`. rusqlite is sync C bindings; bridging to async at the primitive layer wastes runtime and forces every test to be `#[tokio::test]`. The architecture's `pub async fn write_cell` example (architecture.md line 697) is the eventual *consumer-layer* signature (`spawn_blocking` wrap); the primitive stays sync. Stories 1.12 (CLI wiring) and 2.6 (orchestrator) own the async wrapping.
- **`Cell` and `CellKey` are separate structs; `Cell` contains `key: CellKey` as its first field.** Why: the `lookup_cell(&CellKey)` API needs only the seven PK fields, not the full 19-field row. Splitting them into two types makes the API self-documenting (a `CellKey` is exactly what's needed for a lookup); the caller constructs a `CellKey` from already-derived strings without having to fabricate placeholder values for the 12 attribute columns. The `Cell { key: CellKey, ... }` containment relationship matches PRD FR24/FR25's identity-vs-attribute separation.
- **`CellKey` derives `Clone, Debug, PartialEq, Eq, Hash`.** Why: `Clone` is needed for the `DuplicateCell { key }` variant payload (which moves a copy of the colliding key into the error). `Debug` is needed for `?` derivation and clippy/test failure messages. `PartialEq + Eq + Hash` is the natural PK-as-map-key shape (cheap derive; future code that hashes cells by PK gets it for free).
- **`Cell` derives `Clone, Debug, PartialEq` (NOT `Eq`, NOT `Hash`).** Why: `Cell` contains `Option<f64>` fields (`duration_seconds`, etc.). `f64: !Eq` because of NaN; the derive would refuse `Eq` regardless. `PartialEq` is correct (NaN-vs-NaN comparing unequal is the right behavior for round-trip correctness checks — any `NaN` in a perf field would already be a measurement-side bug). `Hash` is omitted because `Hash` requires `Eq`; not needed for the v1 API surface.
- **`pass: bool` (Rust side) ↔ `INTEGER NOT NULL` (SQLite side); mapped via `i64::from(bool)` on write and `!= 0` on read.** Why: SQLite has no native bool. The `pass` column from architecture.md line 270 is `INTEGER NOT NULL` (0 or 1). Rust's `bool` is the natural API type; the conversion is one line at the boundary. Reading via `!= 0` (not `== 1`) is defensive — accepts any non-zero value as `true`, which is what SQL's `WHERE pass` semantics also do.
- **`badges: Vec<String>` (Rust side) ↔ `TEXT NULL` (SQLite side); empty vec is serialized as the JSON literal `"[]"` (NOT as NULL).** Why: making the round-trip canonical means "what was written is what is read back" without two equivalent on-disk representations. Storing `[]` for empty Vec keeps the column's representation unambiguous: NULL exists for forward-compat with external tools that may write older schemas, but lcrc itself always writes a non-NULL JSON array. On read, NULL → empty Vec (forward-compat) and `"[]"` → empty Vec (the canonical case) are equivalent. See § "Badges nullability convention" below for the locked test pin.
- **Badges nullability convention (test pin).** T3.10 (`cell_with_empty_badges_roundtrips_as_empty_vec`) verifies the canonical encoding (`Vec::new()` → write produces `"[]"` on disk → read produces `Vec::new()` again). A test that manually inserts NULL into the badges column and verifies it round-trips as `Vec::new()` would also be valid; it is NOT in the AC set and is deferred. Production-side `write_cell` MUST always produce a non-NULL JSON encoding.
- **Generic `Pragma` variant scope.** `CacheError::Pragma { source: rusqlite::Error }` is the catch-all for non-duplicate-PK rusqlite failures inside `write_cell` / `lookup_cell` (statement preparation, parameter binding, row decode, JSON encode/decode, transaction commit). A future story can split this into specialized variants (`CacheError::RowDecode`, `CacheError::BadgesJson`) when a consumer needs typed handling distinct from "any other SQLite error". Story 1.8 explicitly does NOT pre-add those — same rule Stories 1.5 / 1.6 / 1.7 followed. The `rusqlite::Error` chain is preserved through the `#[source]` attribute, so any debug log will show the underlying constraint name / type-conversion message.
- **`DuplicateCell { key: CellKey }` (NOT `DuplicateCell { source: rusqlite::Error }`).** Why: the AC contract is "report which PK collided"; the rusqlite error itself only carries the constraint name (`"cells.machine_fingerprint, cells.model_sha, ..."`), not the row values. Wrapping the `CellKey` in the variant payload makes the error self-describing without forcing the caller to hold onto the source `Cell` / `CellKey` separately. The rusqlite source is logged inside the discriminator (`if let SqliteFailure { extended_code: SQLITE_CONSTRAINT_PRIMARYKEY } = source`); the variant's Display does NOT include the rusqlite chain because the row-data half is more useful for triage.
- **Use `rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY` (extended error code 1555) to discriminate the duplicate-PK case.** Why: SQLite's `ConstraintViolation` error code (`19`) is too broad — UNIQUE constraints (none in our v1 schema), CHECK constraints (none), FOREIGN KEY (none), and NOT NULL violations all share it. The extended code `SQLITE_CONSTRAINT_PRIMARYKEY` (1555 = `(19 << 8) | 3`) is the canonical "PK violation" signal. rusqlite re-exports the constant via `rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY`; bind to that constant, not to a hard-coded `1555` literal.
- **No `INSERT OR IGNORE` / `ON CONFLICT REPLACE` (UPSERT).** Why: the AC explicitly forbids UPSERT semantics — "the lookup-before-measure invariant (FR26) plus single-writer enforcement (FR52, scan.lock) means a same-PK write indicates an upstream bug". A silent-replace would mask the bug. The `DuplicateCell` error is the design.
- **Use `Connection::transaction()` (which RAII-rolls-back on drop without commit) over manual `BEGIN`/`COMMIT`/`ROLLBACK` `execute_batch` calls.** Why: rusqlite's `Transaction` type encodes the rollback-on-drop guarantee in the type system. Manual `execute_batch("BEGIN;")` + `execute_batch("ROLLBACK;")` requires explicit error-path handling that is exactly the bug `Transaction` was designed to prevent. The architecture line 697 example uses sqlx's `tx = self.db.begin().await?` shape; rusqlite's `transaction()` is the equivalent.
- **`tests/cache_roundtrip.rs` uses `Connection::unchecked_transaction()` for the panic-rollback test (T4.7), NOT `transaction()`.** Why: `Connection::transaction` takes `&mut self`; combined with `std::panic::catch_unwind`'s `UnwindSafe` requirement, the borrow check refuses. `unchecked_transaction` takes `&self` — the "unchecked" refers to "no compile-time prevention of nested-tx misuse", not to anything atomicity-relevant; the rollback semantics are identical. This is rusqlite's documented escape hatch for exactly this pattern.
- **The 10K-cell perf test populates one row at a time via `Cache::write_cell` (each row its own transaction).** Why: tests the realistic single-cell write path; matches the v1 production flow (one cell per task per scan iteration). Estimated wall-clock: ~3-8 s on M1 Pro; ~10-15 s on macos-14 CI runner. Acceptable for an integration test. If CI runs the populate phase >30 s in practice, switch to a single explicit transaction wrapping all 10K INSERTs (the LOOKUP is the perf measurement, the populate is fixture setup). See T4.8 deferral guidance.
- **Use the seven-column `WHERE machine_fingerprint = ?1 AND ...` SELECT (full PK match).** Why: SQLite uses the composite PK index when the `WHERE` clause supplies all PK columns in equality form. This is an O(log n) probe; NFR-P5 (<100 ms at 10K cells) is met by an order of magnitude. A `SELECT WHERE rowid = ...` shortcut is NOT possible because rusqlite can't compose a rowid from arbitrary PK values without a separate index lookup. The seven-column SELECT is canonical.
- **Do NOT add a `Cache::is_open` / `Cache::path` introspection method.** Why: the `Connection` is owned by `Cache`; if the caller needs the path, they pass it in. v1 has no use case for path introspection (no logging that rebuilds the path string from a Cache instance). API speculation.
- **Do NOT split `cell.rs` and `query.rs`.** Why: the architecture line 901-902 splits "Cell struct, read/write" (cell.rs) from "leaderboard, drift, sample queries" (query.rs). `lookup_cell` is a single-cell read primitive — it lives with the write primitive in `cell.rs`. `query.rs` is reserved for range scans / aggregations / drift comparisons that future stories own. Pre-creating `query.rs` with only `lookup_cell` would either fragment a single primitive across two files or duplicate the `Cell` import surface for no benefit.
- **Re-export policy: `Cache`, `Cell`, `CellKey` are accessed via `lcrc::cache::cell::{Cache, Cell, CellKey}`, not re-exported at `lcrc::cache::*` or `lcrc::*`.** Why: same Story 1.5 / 1.6 / 1.7 rule. Re-exports are a v1-API-surface-locking decision; defer to Epic 6's polish story. `lcrc::cache::CacheError` stays at the cache module root (Story 1.7's locked decision) and continues to be the shared error type.

### Library / framework requirements

| Crate | Version (Cargo.toml line) | Use in this story |
|---|---|---|
| `rusqlite` | `0.32`, features `["bundled"]` (line 45) | `Connection`, `Connection::transaction`, `Connection::unchecked_transaction`, `Connection::execute`, `Connection::prepare`, `Connection::query_row`, `rusqlite::params!` macro, `rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY`, `rusqlite::ErrorCode::ConstraintViolation`. Already locked. |
| `serde_json` | `1` (line 31, locked by Story 1.6 for canonical params encoding) | `serde_json::to_string(&Vec<String>)` and `serde_json::from_str::<Vec<String>>(&str)` for the `badges` column round-trip. Already locked. |
| `tempfile` | `3` (line 49) | `TempDir::new` for test fixtures in both unit tests (T3) and integration tests (T4). Already locked. |
| `thiserror` | `2` (line 60) | Already used by `CacheError`; this story adds the fifth variant via the same `#[derive(Error)]` pattern. Already locked. |
| `std::path::Path` (std) | — | `Cache::open` parameter type. |
| `std::time::{Duration, Instant}` (std) | — | NFR-P5 wall-clock budget assertion in `tests/cache_roundtrip.rs`. |
| `std::panic::catch_unwind` (std) | — | T4.7 transaction-rollback simulation. |

**Do not** add: `sqlx` (architecture.md line 697 illustrates sqlx-style code but the locked impl is `rusqlite`), `refinery` / `barrel` (no migration runner needed beyond Story 1.7's hand-rolled framework), `r2d2` / `deadpool-sqlite` / connection pool (single-writer model + WAL means no pool needed), `tokio` async glue inside `cell.rs` (sync by design), `chrono` / `time` (timestamps come in as already-formatted RFC 3339 strings from the producer; `util::time` lands in a future story).

**Do not** widen the `rusqlite` feature set beyond `bundled`. Same rule Story 1.7 documented: other features (`time`, `serde_json`, `chrono`, `array`, `vtab`) are not needed for v1's schema and would inflate compile time + binary size.

### File structure requirements (this story only)

Files created or updated:

```
src/
  cache.rs                       # UPDATE: add `pub mod cell;`; append `DuplicateCell { key }` variant to CacheError
  cache/
    cell.rs                      # NEW: pub struct CellKey; pub struct Cell; pub struct Cache;
                                 #      impl Cache { open, write_cell, lookup_cell }
                                 #      + in-module unit tests
tests/
  cache_roundtrip.rs             # NEW: AC1/AC2/AC3/AC4/AC5 integration tests via the public API
                                 #      including the NFR-P5 perf budget at 10K cells
```

Files **NOT** created by this story (deferred to listed owner stories — do not pre-stub):

- `src/cache/query.rs` — Story 4.1 (`lcrc show` plain-text leaderboard) is the first consumer that needs range scans / aggregations
- `src/constants.rs` — Story 1.10 / 1.14 (container image digest is the first concrete consumer)
- `src/discovery.rs`, `src/discovery/llama_cpp.rs`, `src/discovery/gguf.rs`, `src/discovery/fit_gate.rs` — Story 2.1 and downstream
- `src/sandbox*` — Stories 1.9 / 1.10 / 2.7
- `src/scan*` — Stories 1.10 / 1.11 / 1.12 / 2.6 / 2.13 / 2.15
- `src/backend.rs`, `src/backend/llama_cpp.rs` — Story 2.1
- `src/tasks.rs`, `src/tasks/swe_bench_pro.rs` — Story 2.3
- `src/config.rs`, `src/config/schema.rs`, `src/config/env.rs` — Story 6.1
- `src/util/time.rs` — landed when first consumer needs RFC 3339 helpers (likely Story 1.12)
- `src/report/badges.rs` (the `Badge` enum) — Story 2.4
- Any other architecture-named module — owned by their respective stories per `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure"

### Testing requirements

This story authors **two test surfaces**:

**1. In-module unit tests** (T3) — verify the `Cache` API contract and value-type round-trip in `src/cache/cell.rs::tests`:

- `write_then_lookup_roundtrips_all_columns` — AC1 full-column round-trip with `PartialEq`.
- `lookup_missing_key_returns_none` — AC4's nonexistent-key correctness (perf budget verified at scale in T4).
- `write_then_write_same_pk_returns_duplicate_cell` — AC5's duplicate-PK contract.
- `duplicate_cell_display_lists_all_seven_pk_dimensions` — AC5 Display contract (substring pin).
- `cell_with_all_optional_perf_fields_some_roundtrips` — nullable columns Some-side correctness.
- `cell_with_all_optional_perf_fields_none_roundtrips` — NFR-R4 graceful-degrade None-side correctness.
- `cell_with_empty_badges_roundtrips_as_empty_vec` — badges canonical encoding (empty Vec → `"[]"` → empty Vec).
- `cell_with_multiple_badges_roundtrips_with_order_preserved` — JSON array ordering pin.
- `pass_true_and_pass_false_roundtrip` — `bool ↔ INTEGER 0/1` mapping.
- `cellkey_partial_eq_eq_hash_consistency` — `CellKey` derive set internal consistency.

Pattern: `#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] mod tests { ... }` at file end. Tests use `Cache::open` against `tempfile::TempDir`-backed paths (no `:memory:` here because `migrations::open` requires `&Path`; the unit-vs-integration split is by *what is asserted*).

**2. Integration tests** (T4) — verify the public-API contract via `lcrc::cache::cell::{Cache, Cell, CellKey}`, in `tests/cache_roundtrip.rs`:

- `roundtrip_single_cell_via_public_api` — AC1 end-to-end at the public boundary.
- `lookup_missing_key_returns_none_via_public_api` — AC4 negative-lookup correctness.
- `transaction_rollback_on_panic_leaves_no_partial_row` — AC2 NFR-R2 atomicity via `catch_unwind` + `unchecked_transaction` panic simulation.
- `lookup_existing_key_at_10k_cells_under_100ms_NFR_P5` — AC3 perf budget.
- `lookup_missing_key_at_10k_cells_under_100ms_NFR_P5` — AC4 perf budget.
- `duplicate_pk_returns_duplicate_cell_via_public_api` — AC5 end-to-end.

Pattern: standard integration crate (file-top `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`); plain `#[test]` (NOT `#[tokio::test]` — `Cache::*` is sync); uses `tempfile::TempDir` for filesystem fixtures.

Existing tests must continue to pass: `tests/cli_exit_codes.rs`, `tests/cli_help_version.rs`, `tests/machine_fingerprint.rs`, `tests/cache_migrations.rs`, plus all in-module test suites. This story does not touch any code path those tests exercise; if any goes red after this story's commit, the dev wired something wrong outside the story scope — investigate before relaxing.

The grep T5.5 (rusqlite + cells-table SQL single-source-of-truth) is a manual code-review check, not an automated test, paralleling Stories 1.6 / 1.7's grep contracts.

### Anti-patterns to avoid (LLM-developer pitfalls)

- **Do not** make `Cache::open` / `write_cell` / `lookup_cell` `async`. rusqlite is sync; the architecture's `pub async fn write_cell` example places `spawn_blocking` at the consumer layer (Story 1.12 / 2.6), not at the primitive. A `pub async fn` would force every test to be `#[tokio::test]` and require a tokio runtime per integration test for no benefit.
- **Do not** wrap the `Connection` in `Arc<Mutex<>>` or `RwLock` inside `Cache`. The v1 single-writer model (FR52 scan.lock) does not require application-layer write serialization. Concurrent readers open their own `Cache::open(path)` against the same file; SQLite WAL handles the engine-level concurrency.
- **Do not** derive `Clone` on `Cache`. `Connection: !Clone`; cloning the wrapper would either fail to compile or require an Arc-wrapping sleight-of-hand that contradicts the locked single-owner model.
- **Do not** use `INSERT OR IGNORE`, `INSERT OR REPLACE`, `INSERT ... ON CONFLICT (pk_cols) DO UPDATE / DO NOTHING`, or any other UPSERT shape. The AC explicitly forbids it. The duplicate-PK error is the design.
- **Do not** discriminate the duplicate-PK case by message-string matching (`if err.to_string().contains("UNIQUE constraint failed: cells.machine_fingerprint")`). That is fragile against rusqlite + libsqlite version bumps. Use the typed extended error code: `rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY`.
- **Do not** discriminate by the broad `rusqlite::ErrorCode::ConstraintViolation` (extended code `19`). Other constraint types (UNIQUE, CHECK, FOREIGN KEY, NOT NULL) share that code; checking only the broad code would mis-classify e.g. a NOT NULL violation as `DuplicateCell`. Use the extended code `SQLITE_CONSTRAINT_PRIMARYKEY` (1555).
- **Do not** hard-code `1555` literally. Use the named constant `rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY`.
- **Do not** call `tx.commit()` and discard the `Result`. Commit failure (e.g. disk-full mid-flush) MUST propagate as an error; ignoring it would leave the caller believing the write succeeded.
- **Do not** add a `From<rusqlite::Error> for CacheError` blanket impl. Each `?` propagation must explicitly choose its variant (`Open` / `Pragma` / `MigrationFailed` / `DuplicateCell`). A blanket impl loses the contextual choice.
- **Do not** use `tokio::fs::*` or `std::fs::*` inside `cell.rs`. The function set is sync; the only filesystem touch is via `migrations::open` (Story 1.7) which uses the rusqlite FFI.
- **Do not** call `std::fs::create_dir_all` inside `Cache::open`. Same rule Story 1.7 documented for `migrations::open`: parent-directory creation is the caller's responsibility (Story 1.12 wires `tokio::fs::create_dir_all` at the CLI layer).
- **Do not** add CREATE INDEX statements anywhere (`schema.rs`, inline in `cell.rs`, in a migration). The composite PK gives SQLite a covering index for `lookup_cell`. Read-side query patterns for `lcrc show` (Story 4.1) drive the next index decisions.
- **Do not** add a CREATE INDEX during `Cache::open` ("for free"). Schema changes go through the migration framework (Story 1.7's `MIGRATIONS` slice + `SCHEMA_VERSION` bump). Inline index creation outside that framework breaks NFR-R3 cache durability across versions.
- **Do not** use `Connection::execute_batch` for the multi-statement INSERT. Use `Connection::execute(sql, params!)` with the parameterized form — `execute_batch` does not support `?` parameter binding (it's for DDL / pre-formatted SQL only).
- **Do not** parameterize the table name (`INSERT INTO ?1 (...) VALUES (...)`). SQLite forbids parameter binding for table/column identifiers. The cells-table name is a constant SQL literal.
- **Do not** add a `Cache::write_cells` / batch INSERT API. v1 contract is the single-cell primitive; bulk writes are an Epic 6 perf optimization or a downstream story when actual usage warrants.
- **Do not** add a `Cache::iter_cells` / range scan / `Cache::all_cells` API. Range queries land in Story 4.1 (`lcrc show` leaderboard). This story owns only single-cell primitives.
- **Do not** add a `Cache::delete_cell` / `Cache::update_cell` API. v1 cache cells are immutable: re-measurement is either a NEW cell (different PK dim) or a `lcrc verify` comparison (Story 5.1+, READ-only).
- **Do not** add a `Cache::close()` lifecycle method. rusqlite's `Connection` is RAII-dropped; `Cache` holds it owned; drop chain handles cleanup.
- **Do not** add a `Cache::path()` / `Cache::is_open()` introspection method. v1 has no consumer.
- **Do not** memoize prepared statements across calls (no `Cache { conn, write_stmt: Statement<'_>, lookup_stmt: Statement<'_> }`). rusqlite's `Statement` borrows from the `Connection`; storing both in the same struct creates a self-referential type that requires unsafe or `Pin`. The simple `Connection.prepare(...)` per call is fast (rusqlite caches prepared statements internally via `Connection::prepare_cached` if needed — but that's an optional per-call optimization, not a struct-shape change).
- **Do not** spawn the `lcrc` binary in `tests/cache_roundtrip.rs` (no `assert_cmd::Command::cargo_bin("lcrc")`). The CLI wiring of the cache lives in Story 1.12; testing it here would conflate this story's primitive surface with the integration surface.
- **Do not** add `assert_cmd` or `predicates` imports to `tests/cache_roundtrip.rs`. Cache integration tests use the library API directly, same as `tests/cache_migrations.rs` (Story 1.7).
- **Do not** add `tracing::info!` / `tracing::debug!` events inside `Cache::*` methods. Same rule Stories 1.5 / 1.6 / 1.7 followed: observability events at the primitive layer couple the module to the tracing scheme prematurely; the wiring story (1.12 / 2.6) decides whether and where to log per-cell completion / lookup hit-rate.
- **Do not** introduce a `Cache::write_cell_async` / sync+async API duplication. One sync surface; the consumer wraps in `spawn_blocking`. Pre-adding async wrappers couples this story to a runtime decision that belongs to the consumer.
- **Do not** add `#[cfg(target_os = "macos")]` gates. The cache modules are platform-agnostic (SQLite + standard transactions); only `src/machine/apple_silicon.rs` (Story 1.5) carries the `cfg`-gate. The v1.1 Linux NVIDIA additive port (NFR-C5) reuses this code unchanged.
- **Do not** create `src/cache/query.rs` "while you're in there". Tracer-bullet vertical slices: Story 4.1 owns that file. Pre-stubbing violates the slice principle (`MEMORY.md → feedback_tracer_bullet_epics.md`).
- **Do not** widen `rusqlite` features beyond `bundled`. Same rule Story 1.7 documented.
- **Do not** add `PRAGMA synchronous` / `PRAGMA cache_size` / etc. tuning in this story. Story 1.7 explicitly deferred those; same here. Tuning is an Epic 6 / `bmad-quick-dev` concern once profiling data exists.
- **Do not** add `// Story 1.12 will wire this` / `// Per architecture.md §X` comments. Same chore-commit `7a6e029` lesson Stories 1.6 / 1.7 carry: the *why* (e.g. `// SQLite's PK index is a covering index for full-PK equality SELECTs; lookup is O(log n).`) goes in the comment; planning-artifact references go in the PR description and are discoverable via `git blame`.
- **Do not** rename `src/cache/cell.rs` to `src/cache/cells.rs` (plural) or `src/cache/row.rs`. The architecture line 901 names it `cell.rs` (singular — it owns the single-cell read/write primitives). Renaming silently breaks every existing reference.
- **Do not** re-export `Cache`, `Cell`, `CellKey` at the crate root or at `cache::*`. Same rule Stories 1.5 / 1.6 / 1.7 applied. Defer to Epic 6's polish story.
- **Do not** write `serde::Serialize` / `serde::Deserialize` derives on `Cell` / `CellKey` "for free". v1 has no consumer (no JSON output of cells, no config-file representation). Adding them is API speculation; Story 4.4 (`lcrc show --format json`) is the first consumer that needs serialization, and even then only for a flat output schema, not necessarily mirroring the on-disk row shape.
- **Do not** add `#[non_exhaustive]` on `Cell` / `CellKey` / `Cache`. Same Story 1.6 deferred-work item rationale: `non_exhaustive` is API-versioning policy owned by Epic 6 / future schema-versioning work, not the primitive author.
- **Do not** validate field values inside `Cache::write_cell` (e.g. assert `cell.key.model_sha.len() == 64`, assert `cell.depth_tier in {"quick","standard","full"}`). Validation is the producer's responsibility (Story 1.6 derives the keys; Story 2.6 / 1.12 enforce the depth_tier vocabulary). The cache primitive trusts its inputs — same rule the migration framework follows for DDL inputs.
- **Do not** trim / lowercase / normalize PK column values inside `lookup_cell`. The lookup is a strict equality match; producers are responsible for canonical formatting (Story 1.6 already locks lowercase-hex for `model_sha` / `params_hash`).

### Previous story intelligence (Story 1.1 → 1.2 → 1.3 → 1.4 → 1.5 → 1.6 → 1.7 → 1.8)

- **Story 1.7 created `src/cache/migrations.rs` with `pub fn open(path) -> Result<Connection, CacheError>`** [Source: `src/cache/migrations.rs:60`]. Story 1.8's `Cache::open` delegates to `migrations::open` and wraps the returned `Connection`. The `migrations::open` signature stays unchanged — additive consumer.
- **Story 1.7 left `src/cache.rs` with the `CacheError` enum at the cache module root** with four variants (`Open`, `Pragma`, `MigrationFailed`, `FutureSchema`) [Source: `src/cache.rs:29-76`]. Story 1.8 appends a fifth variant (`DuplicateCell { key }`) to that enum — additive, no removal of existing variants. Preserve the existing variant order; append `DuplicateCell` as the last variant.
- **Story 1.7 established the per-submodule typed-error reuse pattern** (`CacheError` lives at `src/cache.rs`, shared across `migrations` / `cell` / `query` submodules). Story 1.8 is the first consumer of that reuse pattern beyond `migrations` — `cell.rs` returns `CacheError`-typed errors via the existing enum. Reuse confirms the locked decision Story 1.7 made about scope.
- **Story 1.7 left `src/cache/schema.rs` with `pub const CELLS_DDL_V1`** [Source: `src/cache/schema.rs:18`]. Story 1.8 does NOT touch this file. The cells table's column set already supports every field in the `Cell` struct; no schema additions needed for v1.
- **Story 1.6 added `serde_json` as a direct dep with `serde_json/preserve_order` OFF** [Source: Cargo.toml line 31 + Story 1.6 dev notes]. Story 1.8 reuses `serde_json` for the badges JSON encode/decode. The `preserve_order` flag stays OFF (architecturally intentional for `params_hash` canonicalization); for `Vec<String>` serialization the flag is irrelevant (arrays preserve order regardless).
- **Stories 1.5 / 1.6 / 1.7 all added Display-substring tests for typed-error messages** [Source: `src/machine.rs:147-155`, `src/cache/key.rs:tests`, `src/cache/migrations.rs:199-209`]. Apply the same pattern here: `CacheError::DuplicateCell.Display` includes each PK column name (substring assertion). T3.7 enforces.
- **Story 1.7's deferred-work items are NOT in scope for this story** [Source: `_bmad-output/implementation-artifacts/deferred-work.md:15-18`]. The two `SQLITE_NOTADB` and `read_user_version` corruption-recovery items belong to a future hardening story alongside `lcrc verify` (Epic 5) and Story 1.12 CLI wiring respectively.
- **Story 1.6's deferred-work items are NOT in scope for this story** [Source: `_bmad-output/implementation-artifacts/deferred-work.md:20-28`]. The `Params::temp.is_finite()` validation deferred-work item names Story 1.8 as the candidate consumer ("Defer to Story 1.8 — the first consumer that decides UX policy on bad `Params`"). However: Story 1.8 does NOT consume `Params` directly. `Cell::params_hash` is the *derived* hex string, and that string came from `cache::key::params_hash` whose error path is the producer's concern (Story 1.12 / 2.6). Confirming: this deferred-work item stays deferred — Story 1.8 is the cache-write consumer, not the params-derivation consumer. The right time to revisit is Story 2.6 (multi-model orchestrator) when the per-cell `Params` construction lives.
- **Stories 1.4 / 1.5 / 1.6 / 1.7 carried clippy gates that would have been masked by permission-blocked clippy in dev sessions** [Source: Story 1.7 § Previous story intelligence line 367-372]. **Run `cargo clippy --all-targets --all-features -- -D warnings` locally** before pushing this story (T5.3) — local mirror is not optional. Specifically watch for the lints listed in T5.3.
- **Story 1.7 added NO new dependencies** and left `Cargo.lock` unchanged. Story 1.8 also adds no new deps — `rusqlite`, `serde_json`, `tempfile`, `thiserror`, `std::*` are all locked. `Cargo.lock` should be unchanged after `cargo build`.
- **Per-story branch + PR + squash-merge workflow** [Source: `MEMORY.md → feedback_lcrc_branch_pr_workflow.md`]. The branch `story/1-8-cache-cell-write-read-api-with-atomic-semantics` is already checked out per the activation context. Push commits, open PR, wait for green CI, squash-merge with branch deletion via `scripts/bmad-auto.sh` (or the orchestrator's manual equivalent).
- **Tracer-bullet vertical-slice principle was honored in Stories 1.1 → 1.7** [Source: `MEMORY.md → feedback_tracer_bullet_epics.md`]. This story's slice is thin: the cell read/write primitives + their tests, no consumer wiring. Stories 1.12 takes the full vertical CLI → scan → cache → cell write.
- **Apply the chore commit `ee6a89f` lesson** [Source: Story 1.7 line 384]: do not write `// Story 1.12 wires this` / `// Per architecture.md §Cell Schema` in code comments — the *why* (e.g. `// rusqlite's Transaction RAII-rolls back on drop without commit; this is what makes write_cell atomic.`) goes in the comment; the planning artifact reference goes in the PR description and is discoverable via `git blame`.
- **Story 1.5 / 1.6 / 1.7 deferred items** are NOT in scope for Story 1.8 — they belong to a `bmad-quick-dev` pass over `src/machine/apple_silicon.rs` / `src/cache/key.rs` / `src/cache/migrations.rs`, not the new cache cell module.

### Git intelligence summary

- Recent commits (newest first per repo state at story creation): `babff77` (fix: sudden exit in auto-bmad), `1bd7814` (Story 1.7: SQLite schema + migrations framework — PR #6), `ba42e15` (Story 1.6: Cache key helpers — PR #5), `f98d307` (Story 1.5: Machine fingerprint module — PR #4), `3cb7e77` (bmad-auto retry transient GH API failures — PR #2), `ee6a89f` (chore: strip planning-meta comments from Story 1.4 modules — PR #3), `91b95be` (Story 1.4: clap CLI root — PR #1).
- The `1bd7814` (Story 1.7) commit landed `src/cache.rs` extension (CacheError enum), `src/cache/schema.rs` (CELLS_DDL_V1), `src/cache/migrations.rs` (open + helpers + tests), and `tests/cache_migrations.rs`. **Inspect `src/cache.rs` (76 lines, four-variant CacheError + three pub mod declarations + module doc)** — Story 1.8 extends this file with `pub mod cell;` + a fifth variant. Do NOT replace the file; surgically add to it.
- The `ba42e15` (Story 1.6) commit landed `src/cache/key.rs` with the canonical key helpers (`model_sha`, `params_hash`, `backend_build`, `machine_fingerprint`). Story 1.8's `CellKey` carries the *outputs* of those helpers as `String` fields — the type integration contract is "producers call `cache::key::*` to populate `CellKey` strings".
- The `f98d307` (Story 1.5) commit landed `src/machine.rs` + `src/machine/apple_silicon.rs`. The `MachineFingerprint::as_str()` API is what `cache::key::machine_fingerprint(&fp)` delegates to; Story 1.8 has no direct dependency on `MachineFingerprint` (CellKey carries the already-derived string).
- The `ee6a89f` chore commit is informative: it stripped `// Per Story 1.4` / `// FR3 placeholder` planning-meta comments from the post-1.4 modules. **Apply the same restraint** in this story — comments explain *why* (constraints, invariants, non-obvious choices), not which planning artifact owns the change.
- Current `src/` (post-1.7) contains 18 files: `main.rs`, `lib.rs`, `error.rs`, `exit_code.rs`, `output.rs`, `cli.rs`, `cli/scan.rs`, `cli/show.rs`, `cli/verify.rs`, `util.rs`, `util/tracing.rs`, `version.rs`, `machine.rs`, `machine/apple_silicon.rs`, `cache.rs`, `cache/key.rs`, `cache/migrations.rs`, `cache/schema.rs`. After this story: 19 files (+ `cache/cell.rs`).
- `tests/` (post-1.7) contains 4 files: `cli_exit_codes.rs`, `cli_help_version.rs`, `machine_fingerprint.rs`, `cache_migrations.rs`. After this story: 5 files (+ `cache_roundtrip.rs`).
- Current branch `story/1-8-cache-cell-write-read-api-with-atomic-semantics` is checked out per `gitStatus`; working tree status was clean at story-creation time.
- The `actions/checkout@v5` deferred item from Story 1.2 [`_bmad-output/implementation-artifacts/deferred-work.md` line 38] is **not** in scope for this story; soft deadline 2026-06-02 (≈ 4 weeks out as of 2026-05-07).
- The Story 1.7 deferred items in `deferred-work.md` lines 15-18 are **not** in scope for this story.
- The Story 1.6 deferred items in `deferred-work.md` lines 20-28 are **not** in scope for this story; the `Params::temp.is_finite()` item names Story 1.8 as a candidate, but Story 1.8 does not consume `Params` directly (it consumes the already-derived `params_hash` string) — the right time to revisit is Story 2.6.
- No release tags exist; pre-v0.1.0 development. The `Cargo.toml` `version = "0.0.1"` pin (line 3) stays.
- **Cold-cache wall times** [Source: Stories 1.3 / 1.6 / 1.7 completion notes]: clippy ~19.6 s baseline, full test ~18.3 s baseline. Story 1.8 adds the 10K-cell perf test which populates 10K rows. **Expected wall-clock impact**: the perf-test populate phase adds ~3-15 s to the test suite (depending on machine + CI runner). If CI test-suite wall-clock jumps past 60 s, profile the populate phase and consider the bulk-INSERT optimization noted in T4.8.
- **`Cargo.lock` is NOT modified by this story** (unlike Story 1.6 which added `serde_json`). All deps used here (`rusqlite`, `serde_json`, `tempfile`, `thiserror`, `std::*`) are already locked. CI cache hits the warm `Swatinem/rust-cache@v2` entry from Story 1.7's push (the cache key includes `Cargo.lock` hash; unchanged → warm hit).

### Latest tech information (Rust ecosystem — relevant to this story only)

- **`rusqlite::Connection::transaction()`** [Source: rusqlite 0.32 docs]: returns a `rusqlite::Transaction<'_>` borrowed from the `Connection`. The `Transaction` type has `commit() -> Result<()>` and `rollback() -> Result<()>` methods. **Drop without commit = automatic rollback** via the `Drop` impl. This is the RAII guarantee that makes `write_cell`'s atomicity work correctly in the panic/early-return paths.
- **`rusqlite::Connection::unchecked_transaction()`** [Source: rusqlite 0.32 docs]: identical semantics to `transaction()` but takes `&self` instead of `&mut self`. Documented as "useful when you want to use a transaction in a context where you don't have `&mut self` (e.g. in a `RefCell`)". Used here in T4.7 for the `catch_unwind` test (which can't propagate the `&mut` borrow across the unwind boundary). The "unchecked" name refers to "no compile-time prevention of nested-tx misuse" (which is impossible to enforce when the connection borrow is shared) — atomicity semantics are unchanged.
- **`rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY`** [Source: rusqlite 0.32 ffi re-exports + SQLite docs]: the extended error code `1555` = `(SQLITE_CONSTRAINT << 8) | SQLITE_CONSTRAINT_PRIMARYKEY_SUBCODE`. This is the canonical signal for "INSERT failed because of a PK collision". rusqlite re-exports the constant via `rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY`; the typed discriminator pattern is `if let rusqlite::Error::SqliteFailure(ffi::Error { code: ErrorCode::ConstraintViolation, extended_code: rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY }, _) = source`.
- **`rusqlite::params!`** [Source: rusqlite 0.32 docs]: a macro that produces a `&[&dyn ToSql]` from a comma-separated list of values. Each value must implement `ToSql` (which Rust's `String`, `i64`, `f64`, `bool`, `&str`, `Option<T>`, etc. all do via blanket impls). Use for the 19-column INSERT VALUES clause.
- **`rusqlite::Connection::query_row(sql, params, f)`** [Source: rusqlite 0.32 docs]: prepares the statement, binds the params, executes the query, and applies the closure `f: FnOnce(&Row) -> Result<T>` to the first row. Returns `Err(rusqlite::Error::QueryReturnedNoRows)` if no row matches — discriminate on this for the `Ok(None)` `lookup_cell` case.
- **`rusqlite` `FromSql for Option<T>`** [Source: rusqlite 0.32 docs]: NULL → `None`, non-NULL → `Some(T)`. Use this for the seven nullable columns (`duration_seconds` / `tokens_per_sec` / `ttft_seconds` / `peak_rss_bytes` / `power_watts` / `thermal_state` / `badges`).
- **`serde_json::to_string(&Vec<String>)`** [Source: serde_json 1.x docs]: emits a JSON array literal (`"[]"` for empty, `"[\"a\",\"b\"]"` for two-element). Order is preserved (JSON arrays are ordered). Failure is impossible for `Vec<String>` (strings always JSON-encode), but the `Result` is preserved per workspace lints.
- **`serde_json::from_str::<Vec<String>>(&s)`** [Source: serde_json 1.x docs]: parses a JSON array literal back into a `Vec<String>`. Failure (malformed JSON, mismatched type) returns `serde_json::Error`; wrap as `CacheError::Pragma { source: rusqlite::Error::FromSqlConversionFailure(...) }` to preserve the type-error chain.
- **`std::panic::catch_unwind`** [Source: std docs]: catches a panic that propagates out of the closure. Requires `R: UnwindSafe`. Returns `Ok(R)` on normal closure exit, `Err(Box<dyn Any + Send>)` on panic. Used in T4.7 for the panic-rollback simulation.
- **`tempfile::TempDir::new()`** [Source: tempfile 3.x docs]: creates a new temp dir under `std::env::temp_dir()`; `.path()` returns `&Path`; the dir + contents are RAII-deleted on drop. Standard test fixture.
- **SQLite WAL mode + lookup performance** [Source: SQLite docs + Story 1.7]: WAL mode (enabled by `migrations::open`) allows lock-free concurrent readers alongside a single writer. Composite-PK lookups via `WHERE pk_col_1 = ? AND ... AND pk_col_7 = ?` use the implicit PK index (B-tree); lookup is O(log n). At 10K rows, log₂(10K) ≈ 13.3 — single-microsecond probe latency, far under the 100 ms NFR-P5 budget.

### Project Structure Notes

The architecture's `src/` directory map [`_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" lines 896-902] places:
- `src/cache.rs` at line 896 (annotation: "Cache struct, public API (FR24-FR31)") — this story DELIVERS the struct + the public API surface (`Cache::open`, `write_cell`, `lookup_cell`)
- `src/cache/cell.rs` at line 901 (annotation: "Cell struct, read/write (atomic transactions)") — this story authors it
- `src/cache/key.rs` at line 900 (annotation: "canonical key computation") — landed by Story 1.6
- `src/cache/migrations.rs` at line 899 (annotation: "PRAGMA user_version + migration scripts") — landed by Story 1.7
- `src/cache/schema.rs` at line 898 (annotation: "SQL DDL constants") — landed by Story 1.7
- `src/cache/query.rs` at line 902 (annotation: "leaderboard, drift, sample queries") — Story 4.1 owns

Story 1.8 lands `cell.rs`. The "Cache struct, public API" promise of architecture line 896 is fully delivered: the `CacheError` enum (Story 1.7) + the `Cache` struct + the read/write API (this story). Story 1.12 wires the CLI consumer; Story 4.1 + Story 5.1 author the query / verify modules that grow the public surface further.

The architectural-boundaries table at architecture.md line 998 names `src/cache/*` as the sole owner of the SQLite database: "rusqlite/sqlx; schema + migrations + queries". After this story merges, the boundary is enforced *conventionally* via the T5.5 grep contract (no `rusqlite::` or cells-table SQL outside the cache modules); *structurally* it's enforced because `Cache` is the only public type that holds a `Connection`, and consumers go through its method surface for all read/write operations.

The single architectural judgment call in this story is **whether to split `cell.rs` and `query.rs` for the `lookup_cell` primitive**. Alternatives:
- (a) `lookup_cell` lives in `cell.rs` alongside `write_cell`. **Locked.**
- (b) `lookup_cell` lives in a fresh `src/cache/query.rs`; `cell.rs` only contains `write_cell`.
- (c) Both files exist; `cell.rs` contains the value types + `write_cell`; `query.rs` contains `lookup_cell`.

Choice **(a)** is locked. Reasoning: `lookup_cell` is a single-cell read primitive over the same `Cell` value type that `write_cell` produces — they are symmetric primitives over the same domain object. `query.rs` is reserved (per architecture line 902) for higher-level query patterns (leaderboard aggregations, drift comparisons, sample queries) that operate on *collections* of cells, not single cells. Splitting `lookup_cell` from `write_cell` would either fragment a single primitive across two files (option c) or leave `cell.rs` half-empty (option b). The tracer-bullet principle reinforces (a): create only what this story needs, defer `query.rs` to its first consumer.

The four `pub` entrypoints / values from this story are:
- `lcrc::cache::cell::Cache` — the public connection wrapper
- `lcrc::cache::cell::Cell` — the cell value type (19 fields)
- `lcrc::cache::cell::CellKey` — the composite-PK value type (7 fields)
- `lcrc::cache::CacheError::DuplicateCell { key }` — the new error variant

`#[cfg(target_os = "macos")]` does **NOT** appear in `src/cache/cell.rs` or `src/cache.rs` extensions. The cells-table read/write code is platform-agnostic (SQLite + standard transactions); only `src/machine/apple_silicon.rs` (Story 1.5) carries the `cfg`-gate.

No conflicts detected between this story's plan and the existing codebase or planning artifacts.

### References

- [Source: `_bmad-output/planning-artifacts/epics.md` § "Story 1.8: Cache cell write/read API with atomic semantics" (lines 545-571)] — the AC source
- [Source: `_bmad-output/planning-artifacts/epics.md` § "Epic 1: Integration spine — one cell, one row, end-to-end"] — epic context (FR24/FR25/FR26 cache surface is in Epic 1's FR coverage)
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Cache Architecture" (lines 242-296)] — the cache decisions: SQLite + cells table + atomicity + WAL + PRAGMA user_version migration discipline
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Cell schema (`cells` table)" (lines 252-282)] — the SQL DDL spec; Story 1.7 landed the table; Story 1.8 reads/writes against it
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Atomic-Write Discipline" (lines 692-705)] — the `pub async fn write_cell` example illustrating the single-transaction pattern; Story 1.8 implements the sync version of this primitive
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Cache Key Canonicalization" (lines 720-729)] — Story 1.6 reference; the `CellKey` carries the strings produced by `cache::key::*` helpers
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Module Organization" (AR-26)] — `src/cache.rs` parent + `src/cache/*` submodule pattern
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" (lines 896-902)] — `src/cache/cell.rs` placement (line 901), `src/cache/query.rs` deferred (line 902)
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Architectural Boundaries" (line 998)] — `SQLite database | src/cache/* | rusqlite/sqlx; schema + migrations + queries` — single-owner contract
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Requirements → Structure Mapping" (lines 1038-1045)] — FR24/FR25 (`src/cache/{key,schema,cell}.rs`); FR26 (`src/cache/query.rs::lookup` — but `lookup_cell` lands in `cell.rs` per Story 1.8 § "Project Structure Notes"); FR27 (`src/cache/cell.rs` atomic write); FR31 (per-cell metadata)
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Implementation Sequence" (line 537)] — "Cache cell write/read primitives" follows "SQLite schema + migration framework" in the sequence
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Implementation Patterns" / AR-30] — atomic-write discipline applies to `write_cell` (single SQLite transaction)
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Tracing / Logging" (line 770)] — no tracing events at the primitive layer
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Enforcement Summary" (lines 820-832)] — "Write cells inside a single SQLite transaction; never partially" (line 826) — Story 1.8's literal contract
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Cross-Cutting NFRs" (line 1041, 1073)] — `NFR-R1, R2 (resumability + atomicity): src/cache/cell.rs + src/scan/signal.rs + tests/scan_resumability.rs` — Story 1.8 lands the cell.rs half + the cache_roundtrip.rs atomicity test
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Implementation Handoff" (line 1281-1282)] — single-source-of-truth modules list; "never bypass `cache::cell::write` for SQLite writes" is Story 1.8's structural contract once this story merges
- [Source: `_bmad-output/planning-artifacts/prd.md` § "FR24"] — `(machine_fingerprint, model_sha, backend_build, params)` cache key — the seven PK dimensions of `CellKey`
- [Source: `_bmad-output/planning-artifacts/prd.md` § "FR25"] — store/retrieve each `(model, task)` cell independently; `Cache::write_cell` / `lookup_cell` is the literal primitive
- [Source: `_bmad-output/planning-artifacts/prd.md` § "FR26"] — cache lookup before measurement; `Cache::lookup_cell` is the primitive consumed by Story 2.6's orchestrator
- [Source: `_bmad-output/planning-artifacts/prd.md` § "FR27"] — partial scan results persisted such that Ctrl-C/OOM/crash mid-scan loses no completed cells; Story 1.8's atomic `write_cell` + Story 1.10's signal handler combine to satisfy this
- [Source: `_bmad-output/planning-artifacts/prd.md` § "FR31"] — per-cell metadata: depth tier, scan timestamp, backend_build, lcrc version, harness/task version, perf metrics — the columns `Cell` carries
- [Source: `_bmad-output/planning-artifacts/prd.md` § "NFR-R1"] — resumability across Ctrl-C/OOM/crash; underwritten by atomic cell writes (Story 1.8) + signal-safe orchestrator teardown (Story 1.10)
- [Source: `_bmad-output/planning-artifacts/prd.md` § "NFR-R2"] — atomicity of cell writes; Story 1.8's literal binding requirement
- [Source: `_bmad-output/planning-artifacts/prd.md` § "NFR-R7"] — concurrency safety; SQLite WAL mode (Story 1.7's AC1) provides lock-free concurrent reads alongside a single writer (Story 1.8 + Story 6.4's scan.lock)
- [Source: `_bmad-output/planning-artifacts/prd.md` § "NFR-P5"] — cache-key lookup <100 ms for 10K cells; Story 1.8's `tests/cache_roundtrip.rs` AC3/AC4 verifies
- [Source: `_bmad-output/implementation-artifacts/1-7-sqlite-schema-migrations-framework.md`] — `src/cache.rs` parent file pattern with shared `CacheError` enum at module root; per-submodule typed-error reuse pattern; "no `From<…> for crate::error::Error` in primitive-author story" rule; Display-substring contract pin pattern; rusqlite `Connection`/`transaction`/`execute_batch` API patterns; `#[cfg(test)] mod tests` exemption pattern
- [Source: `_bmad-output/implementation-artifacts/1-6-cache-key-helpers-in-src-cache-key-rs.md`] — `KeyError` patterns (Display substring assertion, `module_name_repetitions` allow); `serde_json` integration; `cache::key::*` helpers that produce the strings carried in `CellKey`
- [Source: `_bmad-output/implementation-artifacts/1-5-machine-fingerprint-module.md`] — `MachineFingerprint::as_str()` contract (the source of `CellKey::machine_fingerprint`); typed-error pattern via `thiserror`
- [Source: `_bmad-output/implementation-artifacts/1-4-clap-cli-root-lcrc-version-lcrc-help-tracing-subscriber.md`] — file-as-module pattern; clippy local-mirror lesson
- [Source: `_bmad-output/implementation-artifacts/1-3-output-module-full-exitcode-enum-error-layer.md`] — `Error::Preflight` variant (the future boundary mapping target for `CacheError::*` once Story 1.12 wires it)
- [Source: `_bmad-output/implementation-artifacts/1-2-ci-workflow-gates-fmt-clippy-and-tests.md`] — CI gate (macos-14 runner, 8-min budget); `Swatinem/rust-cache@v2` keys on `Cargo.lock` (this story does NOT change `Cargo.lock`)
- [Source: `_bmad-output/implementation-artifacts/1-1-project-scaffold-with-locked-workspace-lints.md`] — workspace lints + dep lockset; `rusqlite` was added here with `bundled` feature
- [Source: `_bmad-output/implementation-artifacts/deferred-work.md`] — Story 1.5/1.6/1.7 deferred items (out of scope here); Story 1.2 `actions/checkout@v5` deferred item (out of scope, soft deadline 2026-06-02). The Story 1.6 `Params::temp.is_finite()` deferred item names Story 1.8 as a candidate consumer; on inspection this story does not consume `Params` directly (it consumes the derived `params_hash` string), so the item stays deferred to Story 2.6 (multi-model orchestrator)
- [Source: `src/cache.rs` (Story 1.7)] — current parent-module file with `CacheError` enum (4 variants); this story extends it with `pub mod cell;` + the fifth variant
- [Source: `src/cache/migrations.rs` (Story 1.7)] — `pub fn open(path) -> Result<Connection, CacheError>` — the function `Cache::open` delegates to
- [Source: `src/cache/schema.rs` (Story 1.7)] — `pub const CELLS_DDL_V1` — the DDL Story 1.8 reads and writes against (no schema changes)
- [Source: `src/cache/key.rs` (Story 1.6)] — canonical key derivation helpers; producers of `CellKey` strings
- [Source: `src/error.rs:18`] — `Error` enum (the future boundary mapping target, deferred to Story 1.12)
- [Source: `src/exit_code.rs:30-34`] — `ExitCode::ConfigError = 10` and `ExitCode::PreflightFailed = 11` (the eventual exit-code home of `CacheError`-derived `Error::Preflight`)
- [Source: `Cargo.toml` line 45] — `rusqlite = { version = "0.32", features = ["bundled"] }` — locked
- [Source: `Cargo.toml` line 31] — `serde_json = "1"` — locked, used here for `badges` round-trip
- [Source: `Cargo.toml` line 49] — `tempfile = "3"` — locked, used here for tests
- [Source: `Cargo.toml` line 60] — `thiserror = "2"` — locked, used here for `CacheError::DuplicateCell`
- [Source: `<claude-auto-memory>/feedback_tracer_bullet_epics.md`] — vertical-slice principle (no pre-stubbing future-story files like `src/cache/query.rs`)
- [Source: `<claude-auto-memory>/feedback_lcrc_branch_pr_workflow.md`] — branch-then-PR-then-squash workflow
- [Source: `<claude-auto-memory>/CLAUDE.md` → "HIGH-PRECEDENCE RULES" → "Comments explain WHY, never planning meta"] — code comments justify *why* a non-obvious choice was made; do not reference Story / Epic / FR identifiers in comments
- [Source: `<claude-auto-memory>/CLAUDE.md` → "HIGH-PRECEDENCE RULES" → "No absolute or machine-specific paths"] — all paths in code/docs are relative to repo root

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List
