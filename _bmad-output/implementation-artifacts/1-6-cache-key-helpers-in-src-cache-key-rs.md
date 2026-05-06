# Story 1.6: Cache key helpers in `src/cache/key.rs`

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a developer,
I want canonical helpers for all four cache-key components (`model_sha`, `params_hash`, `machine_fingerprint`, `backend_build`) in `src/cache/key.rs`,
so that no agent computes them inline and inconsistently (per the "single source of truth" cache-key invariant in architecture §"Cache Key Canonicalization").

## Acceptance Criteria

**AC1.** **Given** a real GGUF file path
**When** I call `key::model_sha(path)`
**Then** it returns the SHA-256 hex digest of the file contents, computed via streaming (no full-file load into memory).

**AC2.** **Given** a `Params { ctx, temp, threads, n_gpu_layers }` struct
**When** I call `key::params_hash(&params)`
**Then** it returns the SHA-256 of canonical JSON (BTreeMap-sorted keys), so equivalent params hash identically regardless of struct field ordering.

**AC3.** **Given** a `BackendInfo { name, semver, commit_short }`
**When** I call `key::backend_build(&info)`
**Then** it returns the formatted string `"<name>-<semver>+<commit_short>"` (e.g., `"llama.cpp-b3791+a1b2c3d"`).

**AC4.** **Given** a developer greps the codebase for `model_sha|params_hash|backend_build` outside `src/cache/key.rs`
**When** they inspect matches
**Then** every match is a *call* to a helper, never inline computation.

**AC5 (implied by user-statement "all four cache-key components", carried from architecture §"Cache Key Canonicalization").** **Given** a `&MachineFingerprint` constructed by `machine::MachineFingerprint::detect()`
**When** I call `key::machine_fingerprint(&fp)`
**Then** it returns the canonical fingerprint string (delegating to `MachineFingerprint::as_str().to_string()`); no caller inline-`format!`s the `<chip>-<ram>GB-<gpu>gpu` shape.

## Tasks / Subtasks

- [ ] **T1. Add `serde_json` dependency to `Cargo.toml`** (AC: 2)
  - [ ] T1.1 Append `serde_json = "1"` to the `[dependencies]` table, alphabetized between `serde_derive` and `toml` (the existing TOML group). Default features only — `preserve_order` must stay OFF so `serde_json::Map` remains a `BTreeMap` alias and produces sorted-key output. Do not add `[dev-dependencies]` or any other crate.
  - [ ] T1.2 Run `cargo build` locally; confirm `Cargo.lock` updates with one new transitive set (`serde_json` + `itoa` + `ryu`); commit `Cargo.lock`.

- [ ] **T2. Author `src/cache.rs`** (AC: 1, 2, 3, 4, 5)
  - [ ] T2.1 Create `src/cache.rs` as the parent file-as-module per AR-26. File-level `//!` doc explains: this is the `cache` module root; the `key` submodule owns canonical PK-component derivation; future stories add `schema`, `migrations`, `cell`, `query` submodules per architecture §"Complete Project Directory Structure".
  - [ ] T2.2 Declare `pub mod key;` — single line. No other items.

- [ ] **T3. Author `src/cache/key.rs` — types** (AC: 2, 3)
  - [ ] T3.1 Define `pub struct Params { pub ctx: u32, pub temp: f32, pub threads: u32, pub n_gpu_layers: u32 }` with `#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]`. `///` doc on the struct + each field.
    - Field types match `llama-server` API conventions: `ctx` is positive context-window length (u32), `temp` is sampling temperature (f32, range 0.0..=2.0 in practice), `threads` is CPU thread count (u32), `n_gpu_layers` is layers offloaded to GPU (u32; `0` = CPU-only, `u32::MAX`-equivalent ≈ "all" handled by Story 2.1's backend wiring).
    - **Do not** derive `serde::Deserialize` — there is no current deserialization call site (`Cargo.toml` and TOML config schema are owned by Story 6.1+); deriving now creates dead surface area.
    - **Do not** derive `Hash`/`Ord`/`Eq` — `Eq` is impossible for `f32`; lookup is via the canonical hex digest, not the struct.
  - [ ] T3.2 Define `pub struct BackendInfo { pub name: String, pub semver: String, pub commit_short: String }` with `#[derive(Debug, Clone, PartialEq, Eq)]`. `///` doc on the struct + each field.
    - `name` is a backend identifier slug (e.g., `"llama.cpp"`); `semver` is the build's semver string (e.g., `"b3791"` for llama.cpp build numbers); `commit_short` is the 7-char git short-SHA (e.g., `"a1b2c3d"`).
    - **Do not** derive `serde::Serialize` / `Deserialize` — `BackendInfo` is constructed from `Backend::version()` (Story 2.1) and consumed only by `key::backend_build`; no JSON / TOML round-trip is required.
    - **Do not** add a `Display` impl — the formatting belongs to `key::backend_build`, not to the data type. A `Display` impl would let a future caller bypass the `key` module by `format!("{info}")` — silently equivalent today, but a foothold for divergence the moment any quoting / escaping rule changes.
  - [ ] T3.3 Define a `KeyError` typed-error enum via `thiserror::Error`. Variants:
    - `ModelShaIo { path: PathBuf, source: std::io::Error }` — file open / read failure for `model_sha`. `Display`: `"failed to read model file '{path}' for model_sha: {source}"`.
    - `ParamsHashSerialize { source: serde_json::Error }` — canonical-JSON encoding failure for `params_hash`. Realistically unreachable for the four-field `Params` today, but `serde_json` returns `Result` and AR-27 forbids `unwrap`/`expect` outside tests. `Display`: `"failed to canonicalize params for params_hash: {source}"`.
    - **Do not** add a third variant for `backend_build` — the function is infallible (`String` formatting cannot fail) and adding a never-used variant invites future authors to wire fake fail paths.
    - **Do not** add `From<KeyError> for crate::error::Error`. The two boundary conversions (`Other(anyhow::Error)` is the catch-all today) are wired by the consumer story (Story 1.8 cell writer / Story 1.12 wiring); pre-adding them creates dead API surface and forces a mapping decision (`Preflight` vs. a new variant) before the call site exists. Same rationale Story 1.5 used for `FingerprintError`.

- [ ] **T4. Author `src/cache/key.rs` — `model_sha(path)`** (AC: 1, 4)
  - [ ] T4.1 Signature: `pub async fn model_sha(path: &std::path::Path) -> Result<String, KeyError>`.
    - Async because file I/O goes through `tokio::fs` (AR-3). Return type is the lowercase hex digest as a `String` (no `[u8; 32]` wrapper type — the cache PK column is `TEXT`, and `hex::encode`-style construction inline is two lines).
  - [ ] T4.2 Implementation: open the file with `tokio::fs::File::open(path).await`, wrap in a `tokio::io::BufReader`, allocate one fixed buffer (`[0u8; 64 * 1024]` — 64 KiB is the established Rust streaming-hash buffer size, balances syscall count vs. cache footprint), loop `read(&mut buf).await` until 0 bytes, feed each chunk into `sha2::Sha256::update`, finalize, format as lowercase hex.
    - **Do not** use `tokio::fs::read(path).await` (full-file load — violates AC1).
    - **Do not** use `std::fs::File` + `std::io::copy` (sync I/O — violates AR-3).
    - **Do not** memory-map the file (`memmap2` is not in the locked dep set per AR-4; mmap also tickles macOS sandbox quirks Story 1.10 doesn't want to inherit).
    - **Do not** add a progress-reporting hook here. GGUF files are 1–50 GB; a quick mental math says ~2 GB/s SHA-256 throughput on Apple Silicon → 0.5–25 s per file. Streaming-progress UX is wired by Story 2.13 at the orchestrator layer, not at the hashing primitive.
  - [ ] T4.3 Hex formatting: use `format!` with the `{:02x}` width-pad specifier looped over the 32 output bytes (or the equivalent `Sha256::digest`-then-`hex::encode` pattern). **Do not** add the `hex` crate (not in the locked set per AR-4); the inline `format!` loop is 3 lines and idiomatic.
  - [ ] T4.4 Error mapping: every `?` in the function body propagates either `tokio::fs::File::open` or `tokio::io::AsyncReadExt::read` errors as `std::io::Error` and converts via `.map_err(|source| KeyError::ModelShaIo { path: path.to_path_buf(), source })?`. Do not use a blanket `From<std::io::Error> for KeyError` — same reasoning as Story 1.5's `FingerprintError`: a blanket `From` would let a future call site convert any I/O error into `ModelShaIo` even when the I/O came from somewhere else.
  - [ ] T4.5 Add `# Errors` rustdoc section listing `KeyError::ModelShaIo`, satisfying clippy `missing_errors_doc`.

- [ ] **T5. Author `src/cache/key.rs` — `params_hash(&params)`** (AC: 2, 4)
  - [ ] T5.1 Signature: `pub fn params_hash(params: &Params) -> Result<String, KeyError>`.
    - Sync because no I/O. Returns a 64-char lowercase hex digest as `String`.
    - Take `&Params`, not `Params` by value (clippy `needless_pass_by_value` would otherwise fire — see Story 1.5 review findings § Patch 2 carryover lesson).
  - [ ] T5.2 Implementation:
    ```rust
    let value = serde_json::to_value(params).map_err(|source| KeyError::ParamsHashSerialize { source })?;
    let canonical = serde_json::to_string(&value).map_err(|source| KeyError::ParamsHashSerialize { source })?;
    let digest = sha2::Sha256::digest(canonical.as_bytes());
    Ok(hex_lowercase(&digest))
    ```
    where `hex_lowercase` is the same `format!("{:02x}", b)`-loop helper from T4.3, factored to a private `fn hex_lowercase(bytes: &[u8]) -> String` near the top of the file.
  - [ ] T5.3 The two-step `to_value → to_string` round-trip is **load-bearing for canonicalization**: `serde_json::Value::Object` wraps `serde_json::Map`, which is a `BTreeMap<String, Value>` alias when `preserve_order` is OFF (the default we lock in T1.1). Re-serializing the `Value` therefore emits keys in alphabetical order regardless of `Params`'s struct-declaration order. **Do not** call `serde_json::to_string(params)` directly — that uses `serialize_struct`, which preserves field-declaration order, NOT alphabetical, and would silently corrupt cache keys if a future maintainer reorders fields.
  - [ ] T5.4 Add `# Errors` rustdoc section listing `KeyError::ParamsHashSerialize` with the note "infallible in practice for the current `Params` shape; Result preserved because AR-27 forbids unwrap/expect outside tests".

- [ ] **T6. Author `src/cache/key.rs` — `backend_build(&info)`** (AC: 3, 4)
  - [ ] T6.1 Signature: `pub fn backend_build(info: &BackendInfo) -> String`.
    - Infallible — `format!` does not return `Result`. No `KeyError` variant for this function (see T3.3).
    - Take `&BackendInfo` for the same `needless_pass_by_value` reasoning as T5.1.
  - [ ] T6.2 Body: `format!("{}-{}+{}", info.name, info.semver, info.commit_short)`. Single line. No `# Errors` doc section (no `Result` return).
  - [ ] T6.3 **Do not** sanity-check `info.commit_short.len() == 7` or strip whitespace — the input is constructed by `Backend::version()` (Story 2.1) which owns its own format contract; cache-key derivation is a pure formatter, not a validator. Validation belongs at the source.

- [ ] **T7. Author `src/cache/key.rs` — `machine_fingerprint(&fp)`** (AC: 5, 4)
  - [ ] T7.1 Signature: `pub fn machine_fingerprint(fp: &crate::machine::MachineFingerprint) -> String`.
    - Infallible — borrows the canonical string from the wrapped `MachineFingerprint` and clones it.
  - [ ] T7.2 Body: `fp.as_str().to_string()`. Single line. The `MachineFingerprint` type owns the `format!("{chip}-{ram_gb}GB-{gpu_cores}gpu")` invariant (Story 1.5 § "Architecture compliance"); this helper is the documented integration point that Story 1.5 deferred.
  - [ ] T7.3 **Do not** add a method on `MachineFingerprint` itself (e.g., `fn to_cache_key(&self) -> String`). Story 1.5 § "Resolved decisions" explicitly forbade pre-adding it. The `key` module is the single owner of "code that builds cache-PK column values"; widening `MachineFingerprint`'s API to include cache concerns muddies the boundary.

- [ ] **T8. In-module unit tests in `src/cache/key.rs`** (AC: 1, 2, 3, 5)
  - [ ] T8.1 File-end test module: `#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] mod tests { ... }` (matches Stories 1.3 / 1.4 / 1.5 pattern).
  - [ ] T8.2 `model_sha` tests:
    - Hashing a temp-file with known content (e.g., the bytes `b"hello world\n"`) returns the known SHA-256 hex digest `a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447` — pin the well-known fixture so the test is reviewable without running it.
    - Hashing a 1 MiB synthetic file (`vec![0xab; 1024 * 1024]`) is consistent with `Sha256::digest` of the same buffer in-memory (round-trip equivalence — the streaming and bulk paths must agree).
    - Hashing a 200 KiB file (well past the 64 KiB buffer boundary) confirms the multi-chunk loop terminates and accumulates correctly.
    - Hashing a non-existent path returns `Err(KeyError::ModelShaIo { .. })` whose `Display` contains the literal substring `"failed to read model file"` (Display contract pin per Story 1.5 § AC3 lesson).
    - Use `tempfile::NamedTempFile` for the on-disk fixtures (`tempfile` is locked in `Cargo.toml` line 53). All `model_sha` tests are `#[tokio::test]` (the function is async).
  - [ ] T8.3 `params_hash` tests:
    - Two `Params` instances constructed in different field orders (`Params { ctx: 4096, temp: 0.2, threads: 8, n_gpu_layers: 99 }` vs. an identical `Params` re-built field-by-field) hash identically.
    - The hex output is exactly 64 chars and only `[0-9a-f]` (regex-free check via `s.chars().all(|c| c.is_ascii_hexdigit() && (!c.is_ascii_alphabetic() || c.is_ascii_lowercase()))`).
    - A pinned reference fixture: `Params { ctx: 4096, temp: 0.2, threads: 8, n_gpu_layers: 99 }` produces a specific 64-char hex digest. **Compute the expected value once during dev** (write the `Params`, run the test once, paste the actual into the assertion) — the assertion then guards against any silent format change (key reordering, float rendering, etc.) that would break cache-key stability across lcrc versions. NFR-R3 (cache durable across patch upgrades) is the binding requirement this test pins.
    - Changing one field by an epsilon (`temp: 0.2` → `temp: 0.20000001f32`) produces a different digest — confirms `temp` participates in the hash.
  - [ ] T8.4 `backend_build` tests:
    - `BackendInfo { name: "llama.cpp".into(), semver: "b3791".into(), commit_short: "a1b2c3d".into() }` formats to exactly `"llama.cpp-b3791+a1b2c3d"` (the architecture's locked example string at architecture.md §"Cache Key Canonicalization").
    - Empty-field cases (`name: ""`, `semver: ""`) format to `"-+a1b2c3d"`-style strings without panicking. Document via comment that empty inputs are the *source*'s problem (Story 2.1 backend), not the formatter's.
  - [ ] T8.5 `machine_fingerprint` test:
    - Construct a `MachineFingerprint` via `MachineFingerprint::from_canonical_string("M1Pro-32GB-14gpu".into())` (the `#[cfg(test)]` constructor Story 1.5 added at `src/machine.rs:106`) and assert `key::machine_fingerprint(&fp) == "M1Pro-32GB-14gpu"`.
    - This is the only test in this story that crosses module boundaries (`crate::machine::*`); document via comment that this is intentional — it pins the Story 1.5 → 1.6 contract in code.

- [ ] **T9. Wire the `cache` module into the library** (AC: 4)
  - [ ] T9.1 Edit `src/lib.rs`: insert `pub mod cache;` between `pub mod cli;` and `pub mod error;`, preserving the existing alphabetical ordering of the module declarations (`cli`, `error`, `exit_code`, `machine`, `output`, `util`, `version` → after edit: `cache`, `cli`, `error`, `exit_code`, `machine`, `output`, `util`, `version`).
  - [ ] T9.2 Do **not** re-export `cache::key::*` at the crate root. Same Story 1.5 § "Anti-patterns" rule for `MachineFingerprint`: callers use the fully-qualified path `lcrc::cache::key::model_sha(...)`. Re-exports are a v1-API-surface-locking decision; defer to Epic 6's polish story.

- [ ] **T10. Local CI mirror** (AC: 1, 2, 3, 4, 5)
  - [ ] T10.1 Run `cargo build` — confirms the new module compiles and `Cargo.lock` adds `serde_json` + transitives only.
  - [ ] T10.2 Run `cargo fmt` — apply rustfmt; commit any reformatted lines.
  - [ ] T10.3 Run `cargo clippy --all-targets --all-features -- -D warnings` locally (Story 1.4 review surfaced two clippy gates that were masked because clippy was permission-blocked in the dev session — local mirror is not optional).
  - [ ] T10.4 Run `cargo test` — confirms all in-module tests in `src/cache/key.rs::tests` pass and the existing `tests/cli_*.rs` + `tests/machine_fingerprint.rs` integration tests still pass.
  - [ ] T10.5 Manual grep AC4 check: `git grep -nE 'model_sha|params_hash|backend_build' src/ tests/ | grep -v '^src/cache/key.rs:' | grep -v '^_bmad-output/' | grep -v 'tests/.*key' ` — every remaining line must be either (a) a `///` doc-comment / module-level `//!` cross-reference (rare in this story; only Story 1.5's existing `src/machine.rs:71` comment about Story 1.6 qualifies), or (b) absent (no match). If any inline `format!`/`Sha256::digest(...)` call site exists outside `src/cache/key.rs`, that's an AC4 failure.

## Dev Notes

### Scope discipline (read this first)

This story authors **two new files** and **updates two existing files**:

- **New (Rust source):** `src/cache.rs` (parent module declaration), `src/cache/key.rs` (the four helpers + `Params` + `BackendInfo` + `KeyError`)
- **Updated:** `src/lib.rs` (insert `pub mod cache;` — single line), `Cargo.toml` (add `serde_json = "1"` — single dep line + `Cargo.lock` regenerated)

This story does **not**:

- Wire any `key::*` helper into a call site. There is **no consumer** in v1's current scope: Story 1.7 (`cells` table schema + migrations) defines the columns, Story 1.8 (cell write/read API) calls `key::*` to assemble the PK at write time, Story 1.12 (end-to-end one-cell scan) wires the actual scan path. Story 1.6 is a pure-library story authoring four primitives — exactly the same shape as Story 1.5 (machine fingerprint module).
- Author `src/cache/schema.rs`, `src/cache/migrations.rs`, `src/cache/cell.rs`, or `src/cache/query.rs`. Architecture §"Complete Project Directory Structure" maps each to a separate story (Story 1.7: `schema.rs` + `migrations.rs`; Story 1.8: `cell.rs` + `query.rs`). Pre-stubbing them now violates the tracer-bullet vertical-slice principle (`MEMORY.md → feedback_tracer_bullet_epics.md`).
- Add `rusqlite` calls. `rusqlite` is locked in `Cargo.toml` line 50 from Story 1.1, but this story touches no SQLite code path. `rusqlite::Connection::open` first appears in Story 1.7.
- Define a `pub struct CacheKey { machine_fingerprint, model_sha, backend_build, params_hash, task_id, harness_version, task_subset_version }` aggregate type. The architecture's PK has seven dimensions; only four have helpers in this story (the other three — `task_id`, `harness_version`, `task_subset_version` — are direct `String` columns that come from `TaskSource` in Story 2.3). Bundling them into an aggregate now is API speculation; Story 1.8 (the first consumer that needs the aggregate) decides the shape based on its actual SQLite-binding ergonomics.
- Add `tracing` events anywhere. Same rule as Story 1.5 § "Architecture compliance" line 170: this story's helpers are silent on success; observability events belong at the call site (Story 1.8 / 1.12), not at the primitive layer.
- Add `From<KeyError> for crate::error::Error`. Same rule as Story 1.5: dead API until a call site exists; mapping decision deferred to the consumer story.
- Add a `MachineFingerprint::to_cache_key()` method on the Story 1.5 type. Story 1.5 § "Resolved decisions" line 152 explicitly forbade it. The `cache::key` module owns the cache-key vocabulary.
- Touch `src/main.rs`, `src/cli.rs`, `src/cli/*.rs`, `src/error.rs`, `src/exit_code.rs`, `src/output.rs`, `src/util.rs`, `src/util/tracing.rs`, `src/version.rs`, `src/machine.rs`, `src/machine/apple_silicon.rs`, `tests/cli_exit_codes.rs`, `tests/cli_help_version.rs`, `tests/machine_fingerprint.rs`, or `build.rs`. None of those need to change for Story 1.6.
- Add an integration test in `tests/`. Architecture §"Complete Project Directory Structure" lines 972–986 lists no `tests/cache_key*.rs` file — the cache-key invariant is tested via `tests/cache_roundtrip.rs` (Story 1.8) and `tests/cache_migrations.rs` (Story 1.7) once the cells table exists. In-module unit tests in `#[cfg(test)] mod tests { ... }` are sufficient and idiomatic for the pure-function helpers Story 1.6 authors.
- Author or update `tasks/swe-bench-pro/manifest.json` or any vendored task data. `task_id` / `task_subset_version` come from `TaskSource` impls in Story 2.3.
- Touch `image/Dockerfile` or `image/requirements.txt`. Container concerns are owned by Story 1.10 and Story 1.14.

### Architecture compliance (binding constraints)

- **Single source of truth: `src/cache/key.rs`** [Source: architecture.md §"Cache Key Canonicalization" line 722–729]: `model_sha`, `params_hash`, `machine_fingerprint`, `backend_build` are computed *only* in this module. No agent inline-formats `format!("{}-{}+{}", ...)` for `backend_build`, no agent inline-`Sha256::digest`s a file for `model_sha`, no agent inline-builds the params canonical encoding, no agent inline-`format!`s the `<chip>-<ram>GB-<gpu>gpu` shape (the last is delegated to `MachineFingerprint::as_str()`, which Story 1.5 made the format-string owner). Every cache-key column value flows through one of the four `key::*` functions.
- **No `unsafe` anywhere** [Source: AR-27 + Cargo.toml line 77]: `unsafe_code = "forbid"` is workspace-level. `sha2`, `serde_json`, `tokio::fs`, `tokio::io::AsyncReadExt` are all `#![forbid(unsafe_code)]`-compatible from the host's perspective (they may use `unsafe` internally; that's their problem, not ours).
- **All file I/O via `tokio::fs` / `tokio::io`, never `std::fs` / `std::io::Read`** [Source: AR-3 + architecture.md line 165 + Story 1.5 § Architecture compliance]: `model_sha` opens the GGUF via `tokio::fs::File::open(path).await`. The `read` loop uses `tokio::io::AsyncReadExt::read`. `std::io::Read::read_exact` and friends are forbidden — even though they would be syntactically simpler, the architecture has zero tolerance for sync-bridging in the I/O layer (consumer is async; sync-bridge wrappers are an antipattern).
- **No `std::process` anywhere** [Source: AR-3]: N/A in this story — `cache::key` does no subprocess execution.
- **Workspace lints — `unwrap_used`, `expect_used`, `panic = "deny"`** [Source: AR-27 + Cargo.toml lines 80–84]: All `?` propagation against `KeyError`. The in-module test block uses `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` at the `mod tests` attribute (per-module-attribute pattern established by Stories 1.3 / 1.4 / 1.5).
- **`missing_docs = "warn"`** [Source: Cargo.toml line 78]: Every `pub` item gets a `///` doc. That's: `Params` + its 4 fields, `BackendInfo` + its 3 fields, `KeyError` + its 2 variants, `model_sha`, `params_hash`, `backend_build`, `machine_fingerprint`. Eleven items. Each `Result`-returning `pub fn` also needs a `# Errors` rustdoc section (clippy `missing_errors_doc`).
- **MSRV 1.95** [Source: Cargo.toml line 5]: `tokio::fs::File::open`, `tokio::io::AsyncReadExt::read`, `serde_json::to_value` / `to_string`, `sha2::Sha256::{new,update,digest,finalize}`, `format!("{:02x}", ...)`, `std::path::Path` / `PathBuf` are all stable since well before Rust 1.95. No nightly-only features.
- **Crate is binary + library** [Source: architecture.md §"Complete Project Directory Structure" line 874–876 + Story 1.3 T1.2]: `cache::key` is library-only and accessible as `lcrc::cache::key::*` from integration tests in future stories. This story adds no integration test, so the library-vs-binary distinction is invisible at the test surface but binding for future consumers (Stories 1.7, 1.8, 1.12).
- **Tracing / logging discipline** [Source: AR-31 + architecture.md §"Tracing / Logging" line 770]: This story emits **no** tracing events anywhere. `model_sha` is silent on success (returns the digest); `params_hash` and `backend_build` are pure CPU. On `KeyError`, the function returns `Err`; the caller (Story 1.8 / 1.12) decides whether to `tracing::warn!` before propagating.
- **Atomic-write discipline** [Source: AR-30]: N/A in this story — `cache::key` does no disk *writes*. `model_sha` only *reads* the GGUF.
- **`Cargo.lock` is committed; `Swatinem/rust-cache@v2` keys on it** [Source: Story 1.2 § Architecture compliance]. This story **does** add a dependency (`serde_json`); `Cargo.lock` will gain `serde_json` + `itoa` + `ryu` (the two float-formatting helpers `serde_json` pulls in). Commit the updated `Cargo.lock`. The CI cache key will rotate once on first push; subsequent pushes hit the warm cache normally.

### Resolved decisions (don't re-litigate)

These are choices the dev agent might be tempted to revisit. Each is locked here with rationale.

- **Add `serde_json` to `Cargo.toml`** (NOT a hand-rolled canonical-JSON encoder, NOT `toml` in place of JSON, NOT `bincode` / `cbor` / any other encoder). Why: (a) architecture.md line 725 explicitly names `serde_json::to_string(&params)` as the canonical encoding — deviating would diverge from the locked design; (b) hand-rolling canonical JSON for floats is a known foot-gun (which `f64::Display` rendering? trailing-zero stripping? `0.1` vs `1e-1`?) — `serde_json::Number` makes one specific choice and pins it; (c) `serde_json` is the most-deployed Rust crate (>1B downloads) and structurally stable; the dependency cost is two transitive crates (`itoa`, `ryu`) totalling ~50 KB compile artifact; (d) AR-4's "locked dependency set" is binding *for this story's scope*, but adding deps when an architectural decision explicitly mandates the crate is the documented escape hatch — same pattern Story 1.4 used to lock `clap`'s `derive` feature without re-litigating clap itself.
- **`serde_json` default features only — `preserve_order` stays OFF.** Why: with `preserve_order` OFF, `serde_json::Map` is a `BTreeMap<String, Value>` alias and re-serialization emits keys in sorted order — that's exactly what AC2's "BTreeMap-sorted keys" requires. Enabling `preserve_order` would silently break canonicalization the moment `Params` field declaration order changes.
- **Two-step `serde_json::to_value` → `serde_json::to_string` for `params_hash`** (NOT a single `to_string(params)` call). Why: a direct `to_string` on a struct uses `serialize_struct` and emits fields in *declaration order*; reordering struct fields would change the cache-key digest silently. The `to_value` indirection forces routing through `Map` (BTreeMap-backed), which sorts keys deterministically. The cost is one extra heap allocation; the benefit is cache-key stability across struct refactors. NFR-R3 (cache durable across lcrc patch upgrades) is the binding requirement.
- **`hex` crate is NOT added** (manual `format!("{:02x}", ...)` loop). Why: AR-4 lockset discipline — `hex = "0.4"` would be a 200-line dep for what is 3 lines of inline `format!`. Same reasoning Story 1.5 used to reject `regex` for one substring scan.
- **`tempfile::NamedTempFile` for `model_sha` test fixtures** (NOT manually managing temp paths in `/tmp/` or `std::env::temp_dir()`). Why: `tempfile` is locked in Cargo.toml line 53 (Story 1.1's lockset includes it for the FR27 / NFR-R2 atomicity-by-rename pattern Story 1.8 will use). Using `NamedTempFile` in the cache-key tests is the same crate at zero additional cost and gives RAII cleanup.
- **`model_sha` returns lowercase hex `String`, not `[u8; 32]` or a newtype** (e.g., `pub struct ModelSha([u8; 32])`). Why: the cache PK column is `TEXT` (architecture.md §"Cell schema" line 258); the `String` ends up at the SQLite binding layer in Story 1.8 anyway. A newtype wrapper would force unwrapping at the binding site, gaining no type safety in v1's single-call-site context. If a v2 wants `[u8; 32]` for hash arithmetic, the migration is a `parse_hex_to_bytes` helper added in the future story that needs it — backward-compatible with cells already in the cache.
- **`Params` derives `Serialize` only, NOT `Deserialize`** (and NOT `Hash` / `Ord` / `Eq`). Why: `Eq` is impossible for `f32`; `Hash`/`Ord` are no-op for the lookup pathway (lookup is on the canonical hex digest, not the struct); `Deserialize` is unused (no current TOML / JSON config consumes it). Each derive added now is dead surface area until a consumer materializes; pre-deriving is API speculation.
- **`BackendInfo` derives no serde traits and has no `Display` impl.** Why: it's a pure data carrier consumed only by `key::backend_build`. A `Display` impl would let callers inline-`format!("{info}")`, which is silently equivalent today but creates a bypass path that AC4 forbids. No serde derives — it never round-trips through any serializer.
- **`KeyError` has two variants only** (`ModelShaIo`, `ParamsHashSerialize`); no variant for `backend_build`. Why: `backend_build` is `format!`, which is infallible. Adding a `BackendBuildInvalid` variant for "validation that doesn't happen" would invite future authors to wire fake validation paths. The validator (if v2 wants one) belongs at the source — `Backend::version()` in Story 2.1.
- **No public constructor `model_sha::from_hex(s: &str)` or `Params::new(...)`.** Why: `model_sha` returns the digest of *file bytes*, period. A `from_hex` constructor would let callers fabricate `model_sha` values for cache cells they didn't actually measure — defeating cache integrity. `Params` is a plain struct with `pub` fields; explicit construction at call sites makes the four-field shape visible. A `Params::new(ctx, temp, threads, n_gpu_layers)` builder adds zero value over `Params { ctx, temp, threads, n_gpu_layers }` and risks parameter-order bugs (four numeric args, easy to swap).
- **In-module unit tests, no integration test in `tests/`.** Why: architecture §"Complete Project Directory Structure" lines 972–986 lists `tests/cache_roundtrip.rs` (Story 1.8) and `tests/cache_migrations.rs` (Story 1.7), not `tests/cache_key.rs`. The cache-key invariant is tested *through* the cells table once it exists. Adding `tests/cache_key.rs` now would be duplicate coverage with Story 1.8's roundtrip.
- **64 KiB streaming buffer for `model_sha`** (NOT 4 KiB, NOT 1 MiB, NOT `tokio::io::copy`'s default). Why: 64 KiB is the established Rust-ecosystem streaming-hash buffer size (matches `BufReader::new`'s 8 KiB × 8 = 64 KiB sweet-spot referenced in `std::io::BufReader` docs); 4 KiB increases syscall count without benefit on modern SSDs; 1 MiB is wasted L1 cache footprint for a cold-page-fault-bound workload. `tokio::io::copy` would work but writes to a sink we don't have (we feed the bytes into `Sha256::update`); the manual loop is 6 lines and explicit.

### Library / framework requirements

| Crate | Version (Cargo.toml line) | Use in this story |
|---|---|---|
| `serde` | `1` (line 27) | `#[derive(Serialize)]` on `Params`. Already locked. |
| `serde_derive` | `1` (line 28) | `Serialize` proc-macro. Already locked. |
| `serde_json` | `1` (NEW — added in T1.1) | `to_value` + `to_string` for canonical-JSON encoding of `Params`. **Default features only**; `preserve_order` must stay OFF (see § "Resolved decisions"). |
| `sha2` | `0.10` (line 52) | `Sha256::new` + `Sha256::update` for `model_sha` streaming digest; `Sha256::digest` for the bulk-mode `params_hash`. Already locked. |
| `tokio` | `1` (line 35), with `full` features | `tokio::fs::File::open`, `tokio::io::BufReader`, `tokio::io::AsyncReadExt::read` for `model_sha`'s streaming I/O. The `full` feature already includes `fs` + `io-util` + `macros` (for `#[tokio::test]`). Do not narrow. |
| `tempfile` | `3` (line 53) | `NamedTempFile` for the `model_sha` unit-test fixtures. Already locked. |
| `thiserror` | `2` (line 59) | `#[derive(Error)]` on `KeyError`. Already locked. |
| `std::path::{Path, PathBuf}` (std) | — | `model_sha` parameter type + `KeyError::ModelShaIo.path` field. |
| `std::collections::BTreeMap` (std) | — | Imported transitively via `serde_json::Map` (no direct use needed; documented here for clarity). |

**Do not** add: `hex` (use the inline `format!` loop), `bytes` (Vec<u8> is fine), `byteorder` (no endianness concerns), `memmap2` (sandbox / portability concerns documented in T4.2), `rayon` (no parallelism here — single-file streaming, single-pass hashing), `proptest` (in-module unit tests with pinned fixtures are sufficient at this scale; `proptest` could be added by Story 1.8 if cache-roundtrip property tests want it, per architecture.md line 173 "`proptest` (optional) for cache-key-property tests if needed").

**Do not** widen the `tokio` / `serde` / `sha2` / `tempfile` / `thiserror` feature sets — Story 1.1's lockset is binding for everything except the explicit `serde_json` addition this story introduces.

### File structure requirements (this story only)

Files created or updated:

```
Cargo.toml                       # UPDATE: add `serde_json = "1"` to [dependencies]
Cargo.lock                       # AUTO-UPDATE: regenerated by `cargo build`; commit
src/
  lib.rs                         # UPDATE: insert `pub mod cache;` between `cli` and `error`
  cache.rs                       # NEW: parent module, declares `pub mod key;`
  cache/
    key.rs                       # NEW: Params + BackendInfo + KeyError + four pub fns + in-module tests
```

Files **NOT** created by this story (deferred to listed owner stories — do not pre-stub):

- `src/cache/schema.rs`, `src/cache/migrations.rs` — Story 1.7 (SQLite schema + migrations framework)
- `src/cache/cell.rs`, `src/cache/query.rs` — Story 1.8 (cache cell write/read API)
- `src/discovery.rs`, `src/discovery/llama_cpp.rs`, `src/discovery/gguf.rs`, `src/discovery/fit_gate.rs` — Story 2.1 (`Backend` trait + llama.cpp model discovery) and downstream
- `src/sandbox*` — Stories 1.9 / 1.10 / 2.7
- `src/scan*` — Stories 1.10 / 1.11 / 1.12 / 2.6 / 2.13 / 2.15
- `src/backend.rs`, `src/backend/llama_cpp.rs` — Story 2.1
- `src/tasks.rs`, `src/tasks/swe_bench_pro.rs` — Story 2.3
- `tests/cache_roundtrip.rs` — Story 1.8
- `tests/cache_migrations.rs` — Story 1.7
- Any other architecture-named module — owned by their respective stories per architecture.md §"Complete Project Directory Structure"

### Testing requirements

This story authors **one test surface** (no integration test):

**In-module unit tests** (T8) — verify each helper's contract in isolation, in `src/cache/key.rs::tests`:

- `model_sha` — known-fixture pin (`b"hello world\n"` → `a948904f...a447`), bulk-vs-streaming round-trip equivalence on a 1 MiB synthetic file, multi-chunk loop verification on a 200 KiB file, `ModelShaIo` Display-substring contract on missing-file. All `#[tokio::test]`.
- `params_hash` — field-order independence (two equivalent `Params` hash identically), output shape (64-char lowercase hex), pinned reference digest (`Params { ctx: 4096, temp: 0.2, threads: 8, n_gpu_layers: 99 }` → specific value computed once, pasted in), epsilon sensitivity (`temp` change produces different digest).
- `backend_build` — locked example string match (`"llama.cpp-b3791+a1b2c3d"`), empty-field robustness (no panic).
- `machine_fingerprint` — round-trip with `MachineFingerprint::from_canonical_string` (Story 1.5's `#[cfg(test)]` constructor at `src/machine.rs:106`).

Pattern is the documented Story 1.4 / 1.5 pattern: `#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] mod tests { ... }` at file end.

The existing `tests/cli_exit_codes.rs::ok_path_exits_0`, the `tests/cli_help_version.rs` suite, and `tests/machine_fingerprint.rs` from Stories 1.4 / 1.5 must continue to pass. This story does not touch any code path those tests exercise; if any of them goes red after this story's commit, the dev wired something wrong outside the story scope — investigate before relaxing.

The grep AC4 check (T10.5) is a manual code-review check, not an automated test. Architecture has no `tests/cache_key_no_inline.rs` (the structural enforcement is conventional + grep at PR review, same as the "no `println!` outside `src/output.rs`" rule which is also enforced by review, not by a test that scans the source tree).

### Anti-patterns to avoid (LLM-developer pitfalls)

- **Do not** use `tokio::fs::read(path).await` for `model_sha`. That loads the entire file into memory — violates AC1 ("computed via streaming, no full-file load into memory"). GGUF models are 1–50 GB; a full load would OOM the host on the 50 GB end. Use `tokio::fs::File::open` + `BufReader` + chunked `read` loop.
- **Do not** use `std::fs::File` + `std::io::Read::read_to_end` "for simplicity". AR-3 forbids `std::fs` and `std::io` in runtime code; the call site (Story 1.8 / 1.12) is async; sync-bridging is the antipattern AR-3 specifically forbids. Use `tokio::fs::File` + `tokio::io::AsyncReadExt`.
- **Do not** memory-map the GGUF (`memmap2` crate). Not in the locked dep set per AR-4. Also tickles macOS sandbox quirks (Story 1.10 doesn't want to inherit them) and complicates the Linux-NVIDIA additive port (Story 1.5 § NFR-C5).
- **Do not** add the `hex` crate. The inline `format!("{:02x}", b)` loop is 3 lines and idiomatic. Same dependency-discipline reasoning Story 1.5 used to reject `regex` for one substring scan.
- **Do not** call `serde_json::to_string(&params)` directly on a `&Params`. That uses `serialize_struct` and emits fields in *struct-declaration* order, NOT alphabetical. Cache keys would silently change the moment a future maintainer reorders `Params` fields. Use the two-step `to_value` → `to_string` indirection (T5.2).
- **Do not** enable the `preserve_order` feature on `serde_json` (default features only — T1.1). With `preserve_order` on, `Map` is `IndexMap` (insertion-order), which silently breaks the canonicalization invariant.
- **Do not** add `#[derive(Hash, Eq, Ord, PartialOrd)]` on `Params`. `Eq` is impossible for `f32`, and the cache-lookup pathway is on the hex digest (a `String`), not the struct. Adding these derives is dead surface area and `Eq` would not even compile.
- **Do not** add `#[derive(Serialize, Deserialize)]` on `BackendInfo`. It never round-trips through a serializer; it is constructed by `Backend::version()` (Story 2.1) and consumed by `key::backend_build`. Pre-deriving is API speculation.
- **Do not** add a `Display` impl on `BackendInfo`. A `Display` impl would let callers inline-`format!("{info}")` — silently equivalent today but creates a bypass path that AC4 forbids. The format string lives in one place: `key::backend_build`.
- **Do not** add a `pub fn from_hex(s: &str) -> ModelSha` (or any other "construct from raw hex" helper). `model_sha` returns the digest of *actual file bytes*; allowing fabrication from a hex string would let callers populate cache cells with `model_sha` values that don't correspond to a measured file. Cache integrity contract.
- **Do not** add a `Params::new(ctx: u32, temp: f32, threads: u32, n_gpu_layers: u32)` constructor. The four numeric args are easy to swap (parameter-order footgun); explicit `Params { ctx, temp, threads, n_gpu_layers }` at the call site uses field names and is impossible to misorder.
- **Do not** memoize `model_sha(path)` with a `Mutex<HashMap<PathBuf, String>>` "for performance". Each scan hashes each model file at most once; the orchestrator (Story 2.6 / 1.12) is the right layer to dedup per-scan. Memoizing here couples the primitive to the scan lifecycle and complicates the test surface.
- **Do not** add `tracing::info!("computed model_sha for {path}: {digest}")` inside any helper. Same Story 1.5 § "Anti-patterns" rule line 246: observability events at this layer couple the module to the tracing scheme prematurely; the wiring story (1.12) decides whether and where to log.
- **Do not** add a `From<std::io::Error> for KeyError` blanket impl. Same Story 1.5 reasoning: a blanket `From` would let a future call site convert any I/O error into `ModelShaIo` even when the I/O came from somewhere else. Use explicit `.map_err(|source| KeyError::ModelShaIo { path: ..., source })` at the two call sites.
- **Do not** create `src/cache/cell.rs` or `src/cache/schema.rs` "while you're in there." NFR-C5-style additive principle (and tracer-bullet vertical slices) — Stories 1.7 and 1.8 own those files; pre-stubbing them violates the slice principle (`MEMORY.md → feedback_tracer_bullet_epics.md`).
- **Do not** re-export `cache::key::*` at the crate root from `src/lib.rs` (e.g., `pub use cache::key::{model_sha, params_hash, backend_build, machine_fingerprint};`). Callers use the fully-qualified path `lcrc::cache::key::model_sha(...)`. Re-exports are a v1-API-surface-locking decision; defer to Epic 6's polish story (same rule Story 1.5 § "Anti-patterns" applied to `MachineFingerprint`).
- **Do not** rename `src/cache/key.rs` to `src/cache/keys.rs` or `src/cache_key.rs` "for clarity". The architecture's project-structure tree at architecture.md line 900 names it `key.rs`; Story 1.5 § "Architecture compliance" already cited this file by its locked path. Renaming silently breaks every existing reference.
- **Do not** add a `pub struct CacheKey` aggregate type bundling all 7 PK dimensions. Story 1.8 (the first consumer that needs to assemble the full PK at SQLite-bind time) owns that decision based on its actual binding ergonomics. Pre-defining the aggregate is API speculation.
- **Do not** add `#[cfg(target_os = "macos")]` gates to `src/cache/key.rs`. The four helpers are platform-agnostic — SHA-256 of bytes, format strings, JSON canonicalization. Gating the module breaks the v1.1 Linux-NVIDIA additive port (NFR-C5) for no benefit. The only platform-gated code in v1 is `src/machine/apple_silicon.rs` (Story 1.5).
- **Do not** add a `model_sha_bulk(bytes: &[u8])` second public function "for tests." Tests can construct a temp file via `tempfile::NamedTempFile` and use the same public `model_sha(path)` API; test-only public surface area is itself a smell (the in-module test block at the file end can use private helpers freely if needed).
- **Do not** add a `--cache-key-debug` CLI flag or any other observability hook into the CLI surface. Story 1.4's `ScanArgs` / `ShowArgs` / `VerifyArgs` from `src/cli/*.rs` stay untouched (see § "Scope discipline").

### Previous story intelligence (Story 1.1 → 1.2 → 1.3 → 1.4 → 1.5 → 1.6)

- **Story 1.5 created `src/machine.rs` + `src/machine/apple_silicon.rs` with `MachineFingerprint::as_str()`** [Source: src/machine.rs:79]. This story consumes `MachineFingerprint::as_str()` in `key::machine_fingerprint(&fp)` (T7.2). The `MachineFingerprint::from_canonical_string(s: String)` `#[cfg(test)]` constructor at `src/machine.rs:106` is the test-side construction path for Story 1.6's T8.5 test — Story 1.5 added it specifically anticipating Story 1.6's need (see Story 1.5 line 152: "Story 1.6 takes a `&MachineFingerprint` and returns its `as_str()`").
- **Story 1.5 left a doc-comment in `src/machine.rs:71` that explicitly references Story 1.6 as "the sole caller that derives the cache-key string from a `MachineFingerprint`"** [Source: src/machine.rs:71–73]. After this story merges, that contract is satisfied; the doc-comment stays in place as a forward-pointing invariant. **Do not** edit `src/machine.rs` to remove the reference — it documents the intentional boundary, not a planning-meta note.
- **Story 1.5 deferred three items in `_bmad-output/implementation-artifacts/deferred-work.md`** [Source: deferred-work.md lines 17–19]: multi-AGX `gpu-core-count` first-match issue, no `run_capture` subprocess timeout, boundary-input test gaps. **None of these are in scope for Story 1.6** — they belong to a `bmad-quick-dev` pass over `src/machine/apple_silicon.rs`, not the cache-key module.
- **Story 1.4's review surfaced two clippy gates that were masked because clippy was permission-blocked in the dev session** [Source: 1-5-… story line 258]: `clippy::needless_pass_by_value` and `clippy::unnecessary_wraps`. **Run `cargo clippy --all-targets --all-features -- -D warnings` locally** before pushing this story (T10.3) — the only authoritative gate is CI, but a local mirror catches the cheap stuff. Specifically watch for:
  - `clippy::needless_pass_by_value` on `params_hash(params: Params)` / `backend_build(info: BackendInfo)` / `machine_fingerprint(fp: MachineFingerprint)` if you accidentally write by-value parameters — pass `&Params` / `&BackendInfo` / `&MachineFingerprint` instead.
  - `clippy::unnecessary_wraps` on `backend_build` if you accidentally return `Result<String, KeyError>` — `backend_build` is `format!`-based and infallible; the signature must be `pub fn backend_build(info: &BackendInfo) -> String`.
  - `clippy::missing_errors_doc` on `pub` functions returning `Result` — both `model_sha` and `params_hash` need `# Errors` rustdoc sections (T4.5, T5.4).
  - `clippy::needless_pass_by_ref_mut` shouldn't fire (no `&mut` params anywhere in the public surface).
  - `clippy::cast_precision_loss` may surface around `f32` if you do arithmetic on `temp`; this story does no arithmetic on `temp`, only canonical encoding, so the lint shouldn't fire.
- **Story 1.5's review surfaced a `Display` template that dropped the underlying `source`** [Source: 1-5-… Review Findings § Patch 2]. Apply the same lesson here: `KeyError::ModelShaIo.Display` includes `: {source}` so the diagnostic chain is visible without forcing callers to walk `.source()`. Same for `ParamsHashSerialize`.
- **Story 1.5's review surfaced a substring match too loose in `parse_gpu_cores_from_ioreg`** [Source: 1-5-… Review Findings § Patch 3]. Mentioned only as a substring-matching cautionary tale — N/A here, this story does no substring matching.
- **Story 1.3 cold-cache wall times** [Source: 1-3-… Completion Notes via 1-5-… line 265]: clippy ~19.6s, test ~18.3s. **Story 1.6's expected creep** after `serde_json` first-use: small (`serde_json` + `itoa` + `ryu` are tiny crates; together <2 s additional compile time on a cold cache). If clippy or test wall time jumps >3× (e.g., clippy >60s), investigate before pushing — that signals an unwanted dep was added, or the `serde_json` feature set widened.
- **`Cargo.lock` IS modified by this story** (unlike Stories 1.4 / 1.5 which added no deps). `Swatinem/rust-cache@v2`'s key includes `Cargo.lock` hash; the first push will miss the cache and rebuild from scratch (~3–5 min on macos-14). Subsequent pushes hit the warm cache. This is expected and normal. Do not panic when CI shows a slow first run.
- **Per-story branch + PR + squash-merge workflow** [Source: `MEMORY.md → feedback_lcrc_branch_pr_workflow.md`]. The branch `story/1-6-cache-key-helpers-in-src-cache-key-rs` is already checked out per `gitStatus` in the activation context. Push commits, open PR, wait for green CI, squash-merge with branch deletion via `scripts/bmad-auto.sh` (or the orchestrator's manual equivalent).
- **Tracer-bullet vertical-slice principle was honored in 1.1 / 1.2 / 1.3 / 1.4 / 1.5** [Source: `MEMORY.md → feedback_tracer_bullet_epics.md`]. This story's slice is thin: cache-key primitives + their tests, no consumer wiring. Stories 1.7 / 1.8 / 1.12 take the full vertical from CLI → scan → fingerprint → cache. Pre-wiring `key::*` into 1.7's stub here would inflate this story past its single concern.
- **Apply the chore commit `7a6e029` lesson** [Source: 1-4-… Git intelligence summary via 1-5-… line 269]: do not write `// Story 1.7 wires this` or `// Per architecture.md §Cache Key Canonicalization` in code comments — the *why* (e.g., `// Sorted-key BTreeMap encoding pinned: reordering Params fields must not change cache keys`) goes in the comment; the planning artifact reference goes in the PR description and is discoverable via `git blame`.

### Git intelligence summary

- Recent commits (newest first per repo state at story creation): `f98d307` (Story 1.5: Machine fingerprint module — PR #4), `3cb7e77` (bmad-auto retry transient GH API failures + friction-report pause — PR #2), `ee6a89f` (chore: strip planning-meta comments from Story 1.4 modules — PR #3), `91b95be` (Story 1.4: clap CLI root + `--version` + `--help` + tracing subscriber — PR #1), `84f426e` (bmad auto mode infra).
- The `f98d307` (Story 1.5) commit landed `src/machine.rs` + `src/machine/apple_silicon.rs` + `tests/machine_fingerprint.rs`. **Inspect `src/machine.rs:79` (`MachineFingerprint::as_str`) and `src/machine.rs:106` (`from_canonical_string` `#[cfg(test)]`)** — both are direct dependencies of Story 1.6's T7.2 and T8.5 respectively. Do not re-derive their behavior; consume them as documented.
- The `ee6a89f` chore commit is informative: it stripped `// Per Story 1.4` / `// FR3 placeholder` planning-meta comments from the post-1.4 modules. **Apply the same restraint** in this story — comments explain *why* (constraints, invariants, non-obvious choices), not which planning artifact owns the change. The CLAUDE.md global "HIGH-PRECEDENCE RULES" → "Comments explain WHY, never planning meta" makes this binding.
- Current `src/` (post-1.5) contains 13 files: `main.rs`, `lib.rs`, `error.rs`, `exit_code.rs`, `output.rs`, `cli.rs`, `cli/scan.rs`, `cli/show.rs`, `cli/verify.rs`, `util.rs`, `util/tracing.rs`, `version.rs`, `machine.rs`, `machine/apple_silicon.rs`. After this story: 15 files (+ `cache.rs`, `cache/key.rs`).
- `tests/` (post-1.5) contains 3 files: `cli_exit_codes.rs`, `cli_help_version.rs`, `machine_fingerprint.rs`. After this story: 3 files (no new integration test — see § "Testing requirements").
- Current branch `story/1-6-cache-key-helpers-in-src-cache-key-rs` is checked out (from `gitStatus`); working tree status was clean at story-creation time.
- The `actions/checkout@v5` deferred item from Story 1.2 [`_bmad-output/implementation-artifacts/deferred-work.md` line 23] is **not** in scope for this story; soft deadline 2026-06-02 (≈ 4 weeks out as of 2026-05-06).
- The three Story 1.5 deferred items in `deferred-work.md` lines 17–19 (multi-AGX gpu-core-count, no subprocess timeout in `run_capture`, boundary-input test gaps in `apple_silicon::tests`) are **not** in scope for this story — they belong to a future maintenance pass over `src/machine/apple_silicon.rs`, not to the cache-key module.
- No release tags exist; pre-v0.1.0 development. The `Cargo.toml` `version = "0.0.1"` pin (line 3) stays.

### Latest tech information (Rust ecosystem — relevant to this story only)

- **`serde_json` 1.x** [Source: serde-rs/json docs]: stable; `serde_json::Map` is a type alias for `BTreeMap<String, Value>` when the `preserve_order` cargo feature is OFF (default). `to_value(&T) -> Result<Value>` round-trips a `T: Serialize` through the `Value` enum; `to_string(&Value) -> Result<String>` emits compact JSON without pretty-printing or trailing whitespace. The two-step `to_value → to_string` pattern is the canonical way to force key sorting on a struct that derives `Serialize`. `serde_json::Number` represents floats via `f64::Display`, which is deterministic per Rust release (Rust 1.55+ uses Grisu / Dragonbox for float-to-string and is now spec'd as part of the std contract).
- **`sha2` 0.10.x** [Source: RustCrypto/hashes docs]: `Sha256::new()` returns a fresh state; `state.update(&bytes)` is the streaming-feed entrypoint; `state.finalize()` returns `GenericArray<u8, U32>` (32-byte digest). The bulk-mode shortcut `Sha256::digest(&bytes)` is functionally identical to `let mut s = Sha256::new(); s.update(&bytes); s.finalize()` and is fine for the small `params_hash` input. Both APIs are stable; no breaking changes since 0.10.0.
- **`tokio::fs::File::open`** [Source: tokio 1.x docs]: returns `Result<tokio::fs::File, std::io::Error>`; the underlying `std::fs::File::open` runs on a blocking thread-pool worker (tokio's `spawn_blocking` model). For SHA-256 of a 1–50 GB GGUF on Apple Silicon, the I/O is the bottleneck (~2 GB/s sustained on a NVMe SSD; ~6 GB/s on the internal M1 Pro storage); CPU is far from the gate.
- **`tokio::io::AsyncReadExt::read`** [Source: tokio 1.x docs]: requires `use tokio::io::AsyncReadExt;` to bring the trait into scope. Returns `Result<usize, std::io::Error>` where `Ok(0)` signals EOF — the loop terminates when `read(&mut buf).await? == 0`.
- **`tokio::io::BufReader`** [Source: tokio 1.x docs]: wraps any `AsyncRead` with an internal buffer; default buffer size is 8 KiB. We layer our own 64 KiB buffer on top (the `read` argument) for the explicit reason that `BufReader`'s 8 KiB buffer is too small for large-file streaming and forces extra copies; alternatively, use `BufReader::with_capacity(64 * 1024, file)` and call `read` with the raw 64 KiB buffer (slightly cleaner). Either path is acceptable; the dev picks based on which reads more cleanly.
- **`#[tokio::test]`** [Source: tokio 1.x docs via Story 1.5]: requires the `macros` feature (included in `full`). The macro generates a single-threaded runtime by default; **use the default** for the `model_sha` tests — they read one small temp file each, no parallelism gain from multi-thread. (The 1 MiB / 200 KiB tests are single-future, single-file; no fan-out.)
- **`tempfile::NamedTempFile`** [Source: tempfile 3.x docs]: RAII handle that deletes the temp file on drop. `.path()` returns a `&Path` for handing to `tokio::fs::File::open`; `.write_all(&bytes)` writes synchronously (it implements `std::io::Write`, NOT `tokio::io::AsyncWrite` — synchronous writes from the test code are fine, the constraint is only on *production* code per AR-3). For test setup: `let mut tf = NamedTempFile::new()?; tf.write_all(b"hello world\n")?; tf.flush()?;` then `tokio::fs::File::open(tf.path()).await?` in the assertion-side code.
- **`thiserror` 2.0** [Source: thiserror docs via Story 1.5]: `#[derive(Error)]`, `#[error("...")]` for Display templates with named-field interpolation (`{source}`, `{path}`). `#[source]` for the error-chain pointer (used for `io::Error` and `serde_json::Error` payloads). Already locked in Story 1.1; no version bump needed.

### Project Structure Notes

The architecture's `src/` directory map [architecture.md §"Complete Project Directory Structure" lines 896–902] places:
- `src/cache.rs` at line 896 (annotation: "Cache struct, public API (FR24–FR31)")
- `src/cache/key.rs` at line 900 (annotation: "canonical key computation (per Patterns)")

Story 1.6 authors `src/cache.rs` as a **module declaration file only** (file-as-module per AR-26: declares `pub mod key;`). The "Cache struct, public API" promise of architecture line 896 is **NOT** delivered in Story 1.6 — that's Story 1.8 (cache cell write/read API). Story 1.6's `src/cache.rs` is a thin parent module file; Story 1.8 grows it (without rewriting).

The architectural-boundaries table at architecture.md line 1005 names `src/cache/key.rs` as the sole owner of "model_sha, params_hash, machine_fingerprint, backend_build helpers" — Story 1.6 lands the body of that promise. After this story merges, the boundary is enforced *conventionally* via the AC4 grep contract; *structurally* it's enforced once Story 1.8 ships and the only legitimate consumer (cell-write code) is the only call site outside this module.

The single architectural judgment call in this story is the **`Params` struct shape** — alternatives:
- (a) `Params { ctx: u32, temp: f32, threads: u32, n_gpu_layers: u32 }` — explicit named fields, type-safe, four numerics.
- (b) `Params(BTreeMap<String, serde_json::Value>)` — schemaless; trivially future-proof for v1.1+ pass@k or other params.
- (c) `Params(serde_json::Value)` — same as (b) but with a thinner wrapper.

Choice **(a)** is locked. The cost (one struct-definition update per param-shape change, with care to preserve canonical-encoding stability via the T5.2 pattern) is paid in the future story that adds the new field; the benefit (compile-time enforcement of the four current params + named-field construction at every call site) is paid every day during development. The architecture's params-list at `cells.params_hash` column comment (architecture.md line 260: "SHA-256 of canonical(ctx, temp, threads, n_gpu_layers)") names exactly these four fields, locking the v1 shape.

The `BackendInfo` shape similarly mirrors architecture.md line 727: `format!("{name}-{semver}+{commit_short}")` — three named string fields, no enum / no validation. Same reasoning as `Params (a)`.

The four `pub fn` entrypoints (`model_sha`, `params_hash`, `backend_build`, `machine_fingerprint`) match the `cell` PK columns at architecture.md lines 257–260 + the (Story 1.5) `MachineFingerprint::as_str` contract. The remaining three PK dimensions (`task_id`, `harness_version`, `task_subset_version` at architecture.md lines 261–263) are not in this story — they're plain `String`s sourced from the `TaskSource` impl in Story 2.3.

`#[cfg(target_os = "macos")]` does **NOT** appear in `src/cache/key.rs` or `src/cache.rs`. The cache-key primitives are platform-agnostic (SHA-256, JSON canonicalization, format strings); only `src/machine/apple_silicon.rs` (Story 1.5) carries the `cfg`-gate, and `key::machine_fingerprint(&MachineFingerprint)` consumes the result via the platform-agnostic `MachineFingerprint::as_str()` API. The v1.1 Linux-NVIDIA additive port (NFR-C5) drops in via a new `src/machine/linux_nvidia.rs` (Story 1.5 § Architecture compliance line 162) without touching `src/cache/key.rs`.

No conflicts detected between this story's plan and the existing codebase or planning artifacts.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.6: Cache key helpers in src/cache/key.rs] — the AC source
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 1: Integration spine — one cell, one row, end-to-end] — epic context (FR24 cache-key composition is in Epic 1's FR coverage)
- [Source: _bmad-output/planning-artifacts/architecture.md#Cache Key Canonicalization (lines 720–729)] — the four canonical formulas; "Single source of truth: `src/cache/key.rs`"
- [Source: _bmad-output/planning-artifacts/architecture.md#Cell schema (lines 252–282)] — the `cells` table PK columns; locks `model_sha TEXT`, `backend_build TEXT`, `params_hash TEXT`, `machine_fingerprint TEXT` types
- [Source: _bmad-output/planning-artifacts/architecture.md#Cache Architecture (lines 242–296)] — surrounding cache decisions (storage shape, atomicity, resumability) for context; this story implements only the key-derivation primitive
- [Source: _bmad-output/planning-artifacts/architecture.md#Curated Dependencies (lines 116–173)] — `sha2` for `model_sha` (line 140); `serde_json` is mentioned at line 139 as the JSON path's dep but not yet locked (Story 1.6 adds it)
- [Source: _bmad-output/planning-artifacts/architecture.md#Module Organization / file-as-module (AR-26)] — `src/cache.rs` parent + `src/cache/key.rs` submodule pattern; one trait per module file
- [Source: _bmad-output/planning-artifacts/architecture.md#Complete Project Directory Structure (lines 896–902)] — `src/cache.rs` (line 896) and `src/cache/key.rs` (line 900) placement
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Boundaries (line 1005)] — `Cache key computation | src/cache/key.rs | model_sha, params_hash, machine_fingerprint, backend_build helpers` — single-owner contract
- [Source: _bmad-output/planning-artifacts/architecture.md#Requirements → Structure Mapping (lines 1022, 1038–1041)] — FR8 (`src/discovery/gguf.rs + src/cache/key.rs`); FR24/FR25 (`src/cache/{key,schema,cell}.rs`)
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions / AR-3] — `tokio::fs` / `tokio::process`, never `std::fs` / `std::process`
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions / AR-4] — locked dependency set; new deps require explicit architectural mandate (this story adds `serde_json` per architecture line 725 mandate)
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions / AR-26] — file-as-module style; one trait per module file
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions / AR-27] — `unsafe_code = "forbid"`; `unwrap_used` / `expect_used` / `panic` deny outside tests
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions / AR-29] — two-layer error discipline (`thiserror` typed errors at module boundaries)
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions / AR-30] — atomic-write discipline (N/A in this story; mentioned for completeness)
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions / AR-31] — tracing discipline; this story emits no tracing events
- [Source: _bmad-output/planning-artifacts/architecture.md#Tracing / Logging] — no `tracing::error!` for expected failures; structured fields (`model_sha`, etc.) named once `Backend::version()` and cell-writer wire them
- [Source: _bmad-output/planning-artifacts/architecture.md#Enforcement Summary (lines 820–832)] — "Compute `model_sha`, `params_hash`, `machine_fingerprint`, `backend_build` only via `cache::key` helpers" (line 827) — the AC4 invariant in architecture form
- [Source: _bmad-output/planning-artifacts/architecture.md#Implementation Handoff (lines 1279–1284)] — single-source-of-truth modules list includes `src/cache/key.rs`
- [Source: _bmad-output/planning-artifacts/prd.md#FR8] — format-agnostic `model_sha` (SHA-256 content hash; future-proofs MLX without rearchitecting)
- [Source: _bmad-output/planning-artifacts/prd.md#FR24] — `(machine_fingerprint, model_sha, backend_build, params)` cache key
- [Source: _bmad-output/planning-artifacts/prd.md#NFR-R3] — cache durable across patch + minor lcrc upgrades; the `params_hash` pinned-fixture test in T8.3 guards this contract
- [Source: _bmad-output/implementation-artifacts/1-5-machine-fingerprint-module.md] — `MachineFingerprint::as_str()` contract (the integration point for `key::machine_fingerprint`); file-as-module pattern; per-module-attribute test exemption pattern; "no `From<…> for Error` impl in primitive-author story" rule
- [Source: _bmad-output/implementation-artifacts/1-4-clap-cli-root-lcrc-version-lcrc-help-tracing-subscriber.md] — file-as-module pattern (`src/util.rs` + `src/util/tracing.rs`); test exemption pattern; clippy local-mirror lesson
- [Source: _bmad-output/implementation-artifacts/1-3-output-module-full-exitcode-enum-error-layer.md] — `Error` type + `Error::Preflight` variant (the future boundary mapping target for `KeyError` once Story 1.8 / 1.12 wire it)
- [Source: _bmad-output/implementation-artifacts/1-2-ci-workflow-gates-fmt-clippy-and-tests.md] — CI gate (macos-14 runner, 8-min budget); `Swatinem/rust-cache@v2` keys on `Cargo.lock` (this story rotates the cache key once)
- [Source: _bmad-output/implementation-artifacts/1-1-project-scaffold-with-locked-workspace-lints.md] — workspace lints + dep lockset; `tempfile` was added here (in scope for this story's tests)
- [Source: _bmad-output/implementation-artifacts/deferred-work.md] — Story 1.5 deferred items (out of scope here); Story 1.2 `actions/checkout@v5` deferred item (out of scope, soft deadline 2026-06-02)
- [Source: src/machine.rs:79] — `MachineFingerprint::as_str(&self) -> &str` — consumed by `key::machine_fingerprint`
- [Source: src/machine.rs:106] — `MachineFingerprint::from_canonical_string(s: String)` `#[cfg(test)]` — consumed by Story 1.6's T8.5 test
- [Source: src/machine.rs:71–73] — Story 1.5's forward-pointing doc-comment naming Story 1.6 as the sole `cache::key` caller; stays in place after this story
- [Source: Cargo.toml lines 50, 52, 53, 59] — `rusqlite` (locked, not used here), `sha2` (used here for streaming + bulk SHA-256), `tempfile` (used here for test fixtures), `thiserror` (used here for `KeyError`)
- [Source: <claude-auto-memory>/feedback_tracer_bullet_epics.md] — vertical-slice principle (no pre-stubbing future-story files like `src/cache/cell.rs`)
- [Source: <claude-auto-memory>/feedback_lcrc_branch_pr_workflow.md] — branch-then-PR-then-squash workflow
- [Source: ~/.claude/CLAUDE.md → "HIGH-PRECEDENCE RULES" → "Comments explain WHY, never planning meta"] — code comments justify *why* a non-obvious choice was made; do not reference Story / Epic / FR identifiers in comments

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List

### Review Findings
