# Story 1.3: Output module + full ExitCode enum + error layer

Status: review

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a developer,
I want all process output routed through `src/output.rs` and all exit codes routed through the `ExitCode` enum (with all 9 variants defined from day one),
so that the CLI contract per FR45 is locked from v0.1.0 and stdout/stderr discipline is structurally enforced.

## Acceptance Criteria

1. **AC1 (output discipline):** Given the codebase outside `src/output.rs` and `tests/`, when I grep for `println!|eprintln!|print!|eprint!|dbg!`, then there are zero matches.
2. **AC2 (full ExitCode enum):** Given `src/exit_code.rs`, when I inspect the `ExitCode` enum, then it declares all 9 variants `{Ok=0, CanaryFailed=1, SandboxViolation=2, AbortedBySignal=3, CacheEmpty=4, DriftDetected=5, ConfigError=10, PreflightFailed=11, ConcurrentScan=12}` with `#[repr(i32)]` — even though most trigger paths are wired in later epics.
3. **AC3 (single process::exit call site):** Given `src/main.rs`, when the top-level error is matched, then it converts to an `ExitCode` and calls `process::exit(code as i32)` exactly once; no other module calls `process::exit` directly.
4. **AC4 (typed error layer with From → ExitCode):** Given `src/error.rs`, when I inspect it, then it defines a top-level `Error` type with `From` impls for each module-level typed error, mapping each to the appropriate `ExitCode` variant.
5. **AC5 (no panics outside tests):** Given any non-test module, when I grep for `unwrap()`, `expect(`, or `panic!(`, then there are zero matches outside test code.

## Tasks / Subtasks

- [x] **T1. Promote the crate to a binary + library and declare the new single-source-of-truth modules (AC: #1, #2, #3, #4)**
  - [x] T1.1 Author `src/lib.rs` as the crate root. Add `#![cfg_attr(not(test), forbid(unsafe_code))]` (workspace lint already forbids — this is documentation in code) and module declarations for the four new modules: `pub mod exit_code; pub mod output; pub mod error;` plus `pub fn run() -> Result<(), error::Error> { Ok(()) }` as the no-op orchestrator entry that Story 1.4 will fill in. Add `#![doc = "..."]` describing crate scope (one paragraph; matches the binary's existing top-level comment).
  - [x] T1.2 Update `Cargo.toml` to declare both targets explicitly. Add `[lib] name = "lcrc" path = "src/lib.rs"` immediately under the existing `[[bin]]` block. The lib name matches the bin name; cargo handles the disambiguation. Confirm `cargo build` still produces only the `lcrc` binary as the install artifact (lib is internal, not published).
  - [x] T1.3 Confirm no existing `[lints.clippy]` regression: the workspace lints (`unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`, `pedantic = warn`) apply to *both* the bin and lib targets without re-declaration — they live under `[lints]` at the package level which covers all targets.

- [x] **T2. Author `src/exit_code.rs` — single source of truth for FR45 (AC: #2, #3, #4)**
  - [x] T2.1 Define `#[repr(i32)] #[derive(Debug, Clone, Copy, Eq, PartialEq)] pub enum ExitCode` with **exactly** these 9 variants and **exactly** these discriminants: `Ok = 0`, `CanaryFailed = 1`, `SandboxViolation = 2`, `AbortedBySignal = 3`, `CacheEmpty = 4`, `DriftDetected = 5`, `ConfigError = 10`, `PreflightFailed = 11`, `ConcurrentScan = 12`. The 6→10 gap is intentional (FR45 contract); do not renumber to make them contiguous.
  - [x] T2.2 Add a `///`-doc on each variant naming the originating FR/AR and the epic that wires its trigger path (per the FR45 row in epics.md `FR Coverage Map`): `Ok` always; `CanaryFailed` Epic 2; `SandboxViolation` Epic 2; `AbortedBySignal` Epic 1 (FR27); `CacheEmpty` Epic 4; `DriftDetected` Epic 5; `ConfigError` Epic 6; `PreflightFailed` Epic 1 (FR17a); `ConcurrentScan` Epic 6 (FR52). This is the only place these mappings live in code.
  - [x] T2.3 Implement `impl ExitCode { #[must_use] pub const fn as_i32(self) -> i32 { self as i32 } }`. Provides a typed conversion site so callers don't write `code as i32` inline (clippy::pedantic prefers explicit named conversions). The `#[must_use]` reinforces "this value matters — don't drop it on the floor".
  - [x] T2.4 Implement `impl std::fmt::Display for ExitCode` rendering the variant's snake-case name (`"canary_failed"`, etc.) — used by tracing structured fields in later epics. Pedantic-friendly: derive `Debug`, hand-write `Display`. Do **not** derive `Display` from `thiserror` here — `ExitCode` is not an error type; it is a process-exit contract.
  - [x] T2.5 Add a `#[cfg(test)] mod tests` block at file end with `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`. Cover: (a) `as_i32()` returns the documented numeric for every variant (this is the FR45 contract test — failure here means we silently broke a public CLI contract); (b) `Display` round-trips to snake-case; (c) **byte-comparison of the variant set** via an exhaustive `match` that fails to compile if a variant is added/removed without updating the test (Rust's exhaustiveness check substitutes for a manual `#[non_exhaustive]` audit).

- [x] **T3. Author `src/output.rs` — the only module that writes to stdout/stderr (AC: #1)**
  - [x] T3.1 Implement four pub functions matching architecture.md §"stdout / stderr Discipline (FR46)": `pub fn result(s: &str) { println!("{s}"); }` (stdout — results only); `pub fn progress(s: &str) { eprintln!("{s}"); }` (stderr — progress); `pub fn diag(s: &str) { eprintln!("{s}"); }` (stderr — diagnostics). Add a fourth helper `pub fn result_line<T: std::fmt::Display>(item: &T)` so callers don't pre-format with `format!` for trivial single-value emits. All four are sync; tracing handles async/structured logging via Story 1.4's subscriber — `output.rs` is for direct user-facing writes only.
  - [x] T3.2 Add file-level `#![allow(clippy::print_stdout, clippy::print_stderr)]` so that `println!`/`eprintln!` here do **not** trip pedantic restrictions when those lints are added in a later story. Today they are not denied at workspace level (only `unwrap`/`expect`/`panic` are), but the allow makes the intent self-documenting and is forward-compatible.
  - [x] T3.3 Add `///` doc comments naming the four bands: stdout = results (the FR46 pipe-friendly contract — `lcrc show --format json | jq` requires this); stderr = progress + diagnostics (FR47, FR51 stderr error messages). Note that `tracing` events also land on stderr via the subscriber installed in Story 1.4 — keep that mental model when adding new write sites in later stories.
  - [x] T3.4 Add a `#[cfg(test)] mod tests` (with the test-only allow attr) covering: `result_line` formats correctly for `&str` and integers (sanity smoke; the real exit-code/discipline test lives in `tests/cli_exit_codes.rs`).

- [x] **T4. Author `src/error.rs` — top-level Error with From → ExitCode mapping (AC: #4, #5)**
  - [x] T4.1 Define `#[derive(thiserror::Error, Debug)] pub enum Error` as the top-level error variant carrying every module-level typed error that maps to a non-Ok `ExitCode`. v1 of this enum carries placeholder variants for the four module-level error types named in the architecture but not yet authored:
    - `#[error("preflight failed: {0}")] Preflight(String)` — Story 1.9 (FR17a) replaces `String` with `#[from] PreflightError` once that type lands in `src/sandbox/runtime.rs`.
    - `#[error("config error: {0}")] Config(String)` — Story 6.3 (FR51) replaces with `#[from] ConfigError`.
    - `#[error("aborted by signal")] AbortedBySignal` — Story 2.15 (FR27) wires this through the SIGINT handler.
    - `#[error("concurrent scan in progress (holding pid {0})")] ConcurrentScan(u32)` — Story 6.4 (FR52) wires this through `src/scan/lock.rs`.
    - Plus `#[error(transparent)] Other(#[from] anyhow::Error)` as the catch-all for `anyhow::Result` propagation (per architecture.md §"Error Handling" two-layer discipline).
  - [x] T4.2 Implement `impl Error { #[must_use] pub fn exit_code(&self) -> ExitCode { match self { ... } } }`. Match is **exhaustive** (no `_` arm) so adding a variant later is a compile-error until the dev maps it to a code: `Preflight(_) → ExitCode::PreflightFailed`, `Config(_) → ExitCode::ConfigError`, `AbortedBySignal → ExitCode::AbortedBySignal`, `ConcurrentScan(_) → ExitCode::ConcurrentScan`, `Other(_) → ExitCode::PreflightFailed` (catch-all maps to 11 — generic anyhow errors during initialization are pre-flight by definition until later epics introduce specific typed errors).
  - [x] T4.3 Document in module-level `//!` doc the two-layer discipline (architecture.md §"Error Handling"): module boundaries return `thiserror` typed errors that `From`-into `Error::Variant`; intra-module application code uses `anyhow::Result` with `.context()`. Single match site lives in `main.rs`.
  - [x] T4.4 Add `#[cfg(test)] mod tests` (with test-only allow attr) covering: every variant's `exit_code()` returns the spec'd `ExitCode`. The exhaustive match in T4.2 plus this test together enforce that no future PR breaks the FR45 mapping silently.

- [x] **T5. Wire `src/main.rs` to the new layer — single `process::exit` call site (AC: #3, #5)**
  - [x] T5.1 Replace the current stub `fn main() { ... }` with a `fn main()` that:
    1. Calls `lcrc::run()` (the no-op `Ok(())` from T1.1).
    2. Maps `Ok(())` → `ExitCode::Ok` and `Err(e)` → `e.exit_code()` via the T4.2 helper, with an `Err(ref e)` arm that *first* calls `lcrc::output::diag(&format!("error: {e}"))` so the user sees what failed before the process dies.
    3. Calls `std::process::exit(code.as_i32())` — **the single permitted `process::exit` call site in the entire crate.**
  - [x] T5.2 Keep `main.rs` deliberately tiny (target ≤ 25 lines including doc comment). The CLI parsing, tracing subscriber init, and real `run()` body all land in Story 1.4. This story's `main.rs` only proves the *plumbing*: error → `ExitCode` → `process::exit`.
  - [x] T5.3 Add a top-level `//! ` doc comment naming the invariant: "This is the **only** module in the crate that calls `std::process::exit`. All errors flow up here as `lcrc::error::Error`, are diagnosed via `lcrc::output::diag`, then converted to `ExitCode` and surrendered." Future PRs that need a different exit semantics extend the `Error` enum + `From` impls; they do **not** add a second `process::exit` call.
  - [x] T5.4 Confirm no `#[allow]` for `unwrap_used`/`expect_used`/`panic` is needed in `main.rs`: the body uses only `match` + `?`-free composition.

- [x] **T6. Author `tests/cli_exit_codes.rs` — first integration test (also exercises Story 1.2 AC3) (AC: #2, #3)**
  - [x] T6.1 Create `tests/cli_exit_codes.rs` with `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` at file top — `assert_cmd` and `predicates` are panicky-by-design in tests (the whole point of `assert!(...)` is to panic on failure).
  - [x] T6.2 Add `#[test] fn ok_path_exits_0()` using `assert_cmd::Command::cargo_bin("lcrc").unwrap()` to run the built binary with no args, asserting exit code is `0`. This is the first test that exists in the repo — it makes Story 1.2's AC3 (the `cargo test` gate) **truly exercised** for the first time. Document this in Completion Notes (Story 1.2 explicitly forecast this).
  - [x] T6.3 Add `#[test] fn exit_code_enum_full_contract()` as a unit-style integration test importing `lcrc::exit_code::ExitCode` and asserting every variant's numeric value matches FR45 spec. Belt-and-braces with T2.5 — the unit test catches in-module regressions; this one catches accidental re-export breakage (`pub` visibility of the enum from `lib.rs`).
  - [x] T6.4 Do **not** add tests for the 6 not-yet-wired exit codes (1, 2, 4, 5, 10, 12). They will be added by their respective owner stories per the FR45 epic mapping. Pre-stubbing them as `#[ignore]` is horizontal-layer work and violates the tracer-bullet vertical-slice principle (`MEMORY.md` → `feedback_tracer_bullet_epics.md`). Exits 0, 3, and 11 *are* in this epic per the FR Coverage Map: 0 covered by T6.2; 3 (`AbortedBySignal`) and 11 (`PreflightFailed`) trigger paths land in Stories 2.15 and 1.9 respectively — their tests come with those stories.

- [x] **T7. Verify the full discipline on the local tree (AC: #1, #2, #3, #4, #5)**
  - [x] T7.1 Run the AC1 grep verbatim: `git grep -nE "println!|eprintln!|print!|eprint!|dbg!" -- 'src/**/*.rs' ':!src/output.rs'` and confirm zero matches. (Note: `git grep` respects `.gitignore` and does not search `tests/` if you scope to `src/` — which is the AC1 scope.) The `src/main.rs` body must use `lcrc::output::diag`, never `eprintln!`.
  - [x] T7.2 Run the AC5 grep verbatim: `git grep -nE "\.unwrap\(\)|\.expect\(|panic!\(" -- 'src/**/*.rs'` and confirm zero matches. Test modules under `#[cfg(test)]` are excluded by source-path scope; the workspace lint also excludes them via the per-test `#[allow]` attrs introduced in T2.5/T3.4/T4.4.
  - [x] T7.3 Run the AC3 grep: `git grep -nE "process::exit|std::process::exit" -- 'src/**/*.rs'` and confirm exactly one match in `src/main.rs`.
  - [x] T7.4 Local CI mirror: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features`. All three must pass on the clean tree before push. Record cold-cache wall times for clippy + test in Completion Notes (informational; budget-tracking for AC4 of Story 1.2 which is now actually exercised).
  - [x] T7.5 Push to a feature branch (e.g. `story/1.3-output-exit-error`) and confirm GitHub CI (Story 1.2's `gate` job) goes green on the new test, exercising AC3 of Story 1.2 for the first time. After verification, merge to `main` via clean single commit (matching Stories 1.1/1.2 pattern). Optional probe: temporarily break `ok_path_exits_0` (e.g., force `process::exit(7)` in main), push, watch CI fail on the test step, then revert. This *witnesses* the test gate; it is the first time on the project that AC3 of Story 1.2 has a non-trivial signal. **— Modified per maintainer direction (2026-05-05): skipped feature-branch + CI break-probe; landed as a single commit on `main` locally for the maintainer to push manually. The break-probe witness moment is therefore deferred (not blocking; the test will still run on every push going forward).**

## Dev Notes

### Scope discipline (read this first)

This story authors **six files** (five new + one updated):

- **New:** `src/lib.rs`, `src/exit_code.rs`, `src/output.rs`, `src/error.rs`, `tests/cli_exit_codes.rs`
- **Updated:** `src/main.rs` (replaces the Story 1.1 stub), `Cargo.toml` (adds `[lib]` block)

This story does **not**:

- Author `src/cli.rs` or any clap-derive structs — that is **Story 1.4**. The `main.rs` here calls a no-op `lcrc::run()` and returns `Ok(())`. No subcommand parsing.
- Author `src/util/tracing.rs` or install a tracing subscriber — also **Story 1.4**. Tracing events emitted before the subscriber lands are silent; that's intended.
- Author `src/sandbox/runtime.rs` (Story 1.9) or `src/scan/signal.rs` (Story 2.15) or `src/scan/lock.rs` (Story 6.4) or `src/config.rs` (Story 6.1) — those stories own the *trigger paths* for exits 11/3/12/10. This story only locks the **contract** (the `ExitCode` enum + `Error` enum's `From → ExitCode` mapping) so those stories can plug their typed errors in mechanically.
- Pre-stub `tests/sandbox_envelope.rs` (Stories 7.4/2.16) or any other future test file. The single `tests/cli_exit_codes.rs` test in T6.2 is the *only* new test this story authors — it's the test that gives Story 1.2's CI gate (AC3) something to actually grade for the first time.
- Add new dependencies to `Cargo.toml`. Everything needed (`thiserror`, `anyhow`, `assert_cmd`, `predicates`) was already locked in Story 1.1.
- Define a custom clippy restriction or a pre-commit hook that *enforces* the AC1 grep / AC3 grep / AC5 grep at lint time. Those grep invariants are verified by the AC checklist in this story (T7.1–T7.3) and by visual inspection in code review for future PRs. A tighter structural enforcement (a `clippy.toml` `disallowed-macros` list) is **deferred work** — log it after this story lands if the pattern starts drifting.

### Architecture compliance (binding constraints)

- **Single source of truth for exit codes** [Source: architecture.md §"Error Handling" + AR-28]: `src/exit_code.rs` defines `ExitCode`. **No bare numeric exit codes anywhere else in the crate.** `main.rs` is the only place that calls `process::exit`. New typed errors plug in by extending `Error` in `src/error.rs` + adding a `From` impl + adding a match arm in `Error::exit_code()`. The exhaustive match (no `_` arm) makes "forgot to map" a compile error.
- **Single source of truth for stdout/stderr writes** [Source: architecture.md §"stdout / stderr Discipline (FR46)" + AR-28]: Only `src/output.rs` may call `println!`/`eprintln!`/`print!`/`eprint!`/`dbg!`. The four pub functions (`result`, `result_line`, `progress`, `diag`) are the entire public API. New write sites = new pub function in `output.rs`, never an inline macro.
- **Two-layer error discipline** [Source: architecture.md §"Error Handling"]: Module boundaries → `thiserror` typed enums; intra-module → `anyhow::Result` with `.context()`. The `Error` type in `src/error.rs` is the **top-level** sum that `main.rs` matches on; module-level error types (e.g. `PreflightError`, `ConfigError`) get added by their owner stories and `From`-into `Error::Variant`. Story 1.3 ships placeholder variants carrying `String` payload; owner stories swap `String` → `#[from] OwnerError` when their typed errors land. **Do not** invent module-level error types here that haven't been authored yet — the placeholder String payload is the right shape until those modules exist.
- **Crate is a binary that also exposes a library** [Source: architecture.md §"Complete Project Directory Structure" lines 874–875]: Both `src/main.rs` (entry point) and `src/lib.rs` (crate root with module decls + `run()`) are first-class. `Cargo.toml` declares both via `[[bin]]` (already present from Story 1.1) + `[lib]` (added in T1.2). Same package name covers both targets. This split lets integration tests in `tests/` import `lcrc::exit_code::ExitCode` (T6.3) without exec'ing the binary.
- **No `unsafe` anywhere** [Source: AR-27 + Cargo.toml line 79]: `unsafe_code = "forbid"` is workspace-level. The `enum ExitCode` with `#[repr(i32)]` and `as i32` casts are safe Rust (numeric primitive cast); no `unsafe` block needed.
- **Workspace lint exemption pattern for tests** [Source: Story 1.1 §T2.4 + Cargo.toml lines 73–77]: Every `#[cfg(test)] mod tests` block in this story uses `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` directly above the `mod tests` declaration. This is the documented per-file opt-out (Cargo.toml comment lines 73–77 specify it verbatim). Do **not** add a workspace-level test exemption — the per-mod `#[allow]` is the chosen pattern.
- **MSRV stays 1.95** [Source: sprint-change-proposal-2026-05-05.md + Cargo.toml line 5]: All language constructs in this story (enum with explicit discriminants, `#[repr(i32)]`, `const fn`, `thiserror` derive, `anyhow::Error`, `#[must_use]`, `match` exhaustiveness, edition 2024) are stable well before 1.95. No nightly-only features.

### Library / framework requirements (no new dependencies)

| Crate | Version (Cargo.toml) | Use in this story |
|---|---|---|
| `thiserror` | `2` | Derives `Error` + `Display` for the top-level `Error` enum in `src/error.rs`. Do **not** swap to `anyhow::Error` for the top-level type — `thiserror` is for module-boundary typed errors per architecture §"Error Handling"; `anyhow` is the *intra-module* application-layer companion (carried as `Error::Other(#[from] anyhow::Error)`). |
| `anyhow` | `1` | Carried in `Error::Other(#[from] anyhow::Error)` as the catch-all for application-level propagation. Not used directly in this story's bodies; it's there so future story bodies can `.context("...")?` and bubble up. |
| `assert_cmd` | `2` (dev-dep) | `tests/cli_exit_codes.rs` uses `assert_cmd::Command::cargo_bin("lcrc")` to run the binary as a black-box and assert exit codes. Already locked in Story 1.1's `[dev-dependencies]`. |
| `predicates` | `3` (dev-dep) | Companion to `assert_cmd` for output assertions if needed (not strictly required for the two tests in T6 — exit-code asserts are built into `assert_cmd::Assert`). Already locked. |

**Do not** add: `displaydoc` (overlaps with `thiserror::Display`), `eyre` / `color-eyre` (architecture chose `anyhow`), `exitcode` crate (we own the enum; an external crate with a different set defeats the whole point of AR-28's single source of truth), `tracing` integration in `output.rs` (tracing subscriber is Story 1.4; `output.rs` is for *direct* user-facing writes, not structured logs).

### File structure requirements (this story only)

Files created or updated:

```
Cargo.toml             # UPDATE: add [lib] block declaring src/lib.rs (and nothing else)
src/
  main.rs              # UPDATE: replace Story 1.1 stub with the single process::exit site
  lib.rs               # NEW: crate root — pub mod decls + no-op `run()` entry
  exit_code.rs         # NEW: ExitCode enum (FR45) — single source of truth
  output.rs            # NEW: ONLY module that writes to stdout/stderr (FR46)
  error.rs             # NEW: top-level Error type, From impls → ExitCode
tests/
  cli_exit_codes.rs    # NEW: first integration test; exercises Story 1.2 AC3
```

Files **NOT** created by this story (deferred to listed owner stories — do not pre-stub):

- `src/cli.rs` and `src/cli/{scan,show,verify,meta}.rs` — Story 1.4 + later epic stories
- `src/util/tracing.rs` — Story 1.4
- `src/sandbox/runtime.rs` (which will define `PreflightError`) — Story 1.9
- `src/config.rs` and `src/config/{schema,env}.rs` (which will define `ConfigError`) — Stories 6.1–6.3
- `src/scan/signal.rs`, `src/scan/lock.rs` — Stories 2.15, 6.4
- Any of the other directories named in architecture.md §"Complete Project Directory Structure" (`src/cache/`, `src/sandbox/`, `src/discovery/`, `src/backend/`, `src/perf/`, `src/scan/`, `src/report/`, etc.) — owned by their respective epic stories per the directory map

### Testing requirements

This story authors **two test surfaces**:

1. **In-module unit tests** (T2.5, T3.4, T4.4) — verify each module's contract in isolation. Pattern is the documented Story 1.1 pattern: `#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] mod tests { ... }` at file end.
2. **Integration test** `tests/cli_exit_codes.rs` (T6) — black-box test of the built `lcrc` binary via `assert_cmd::Command::cargo_bin("lcrc")`, plus a re-export sanity test that imports `lcrc::exit_code::ExitCode` from the lib crate and asserts numeric discriminants.

Coverage scope is **only the variants whose trigger paths are wired in Epic 1** (per the FR45 row in epics.md `FR Coverage Map`): 0 (covered by T6.2 — binary with no args exits clean), 3 (`AbortedBySignal` — wired by Story 2.15, test lands with that story), 11 (`PreflightFailed` — wired by Story 1.9, test lands with that story). The other six variants (1, 2, 4, 5, 10, 12) get their integration tests when their owner stories wire trigger paths. Pre-stubbing them as `#[ignore]` here is horizontal-layer work and violates the tracer-bullet vertical-slice principle.

The integration test in T6.2 also doubles as the **first real exercise of Story 1.2's AC3**: prior to this story, `cargo test` returned `0 passed; 0 failed`, so Story 1.2's test gate was wired-but-unexercised (Story 1.2 Completion Notes flagged this explicitly). After this story, an intentionally-broken test on a feature branch will turn the CI gate red. Witness this in T7.5.

### Anti-patterns to avoid (LLM-developer pitfalls)

- **Do not** make `ExitCode` discriminants contiguous (`Ok=0, CanaryFailed=1, ..., ConcurrentScan=8`). The 6→10 gap is the FR45 contract; renumbering breaks every script that has hardcoded `lcrc; if [ $? -eq 11 ]; then ...`. The discriminants in T2.1 are exact — copy them character-for-character.
- **Do not** add `#[non_exhaustive]` to `ExitCode`. The CLI exit-code surface is intentionally **finite and stable** — adding `#[non_exhaustive]` invites future variants and signals to consumers that the set may grow in minor versions, which contradicts FR45's "semver-stable" promise.
- **Do not** derive `Display` on `ExitCode` via `thiserror::Error`. `ExitCode` is **not** an error — it's a process-exit contract. Hand-roll `impl Display` rendering snake-case (T2.4). Errors derive `thiserror::Error`; exit codes do not.
- **Do not** use a `_` (catch-all) arm in `Error::exit_code()` (T4.2). The exhaustive match is a *feature*: it makes "forgot to map a new variant to an ExitCode" a compile-error rather than a runtime miscategorization. Future stories adding `Error` variants must also add the match arm; the compiler enforces this.
- **Do not** swap the `#[error("preflight failed: {0}")] Preflight(String)` placeholder for `#[from] PreflightError` in this story — `src/sandbox/runtime.rs` doesn't exist yet, so the type doesn't exist. The `String` payload is the documented intermediate state until Story 1.9 lands. Same for `Config(String)` and `ConcurrentScan(u32)`.
- **Do not** add `Error::CanaryFailed`, `Error::SandboxViolation`, `Error::CacheEmpty`, `Error::DriftDetected` in this story. Those are Epic 2/4/5 trigger paths and the corresponding typed errors live in `src/scan/`, `src/sandbox/`, `src/cache/`, `src/verify/` modules that don't exist yet. The `ExitCode` *enum* covers all 9; the `Error` *enum* only carries the variants whose trigger paths exist or are being plumbed in this epic. The `From → ExitCode` mapping for the missing variants gets added when the owner story lands its module-level error type.
- **Do not** call `tracing::error!` from `main.rs` for the top-level error before `process::exit`. Use `lcrc::output::diag(&format!("error: {e}"))` per architecture §"Tracing / Logging" — expected failure conditions go through the user-facing diagnostic channel, not `tracing::error!`. `tracing::error!` is reserved for non-recoverable internal failures, which is what calling `diag` + exiting non-zero already conveys.
- **Do not** call `lcrc::output::diag` from anywhere except `main.rs` in this story. Module-level code propagates `Result`; only `main.rs` is allowed to format the final error message for the user. Other modules will land their own diag calls in their owner stories.
- **Do not** wrap `process::exit` in a helper function (`fn exit(code: ExitCode) -> !`). The whole point of AR-28's "no other module calls `process::exit` directly" rule is that the call site is *visually obvious* in code review — wrapping it in a helper hides it. Inline `std::process::exit(code.as_i32())` in `main.rs`.
- **Do not** add `#[allow(clippy::exit)]` to `main.rs`. The pedantic `clippy::exit` lint warns on `process::exit`; in `main.rs` it's the *correct* call. Use `#![allow(clippy::exit)]` at the top of `main.rs` only if the lint actually fires (it may or may not at pedantic level — verify in T7.4). If you do allow it, scope to the file (`#![allow(...)]`), not the workspace.
- **Do not** declare `mod exit_code;`, `mod output;`, `mod error;` in `main.rs` *and* `lib.rs`. The modules live in `lib.rs` (T1.1); `main.rs` accesses them via the `lcrc::` crate path (e.g. `lcrc::error::Error`, `lcrc::output::diag`, `lcrc::exit_code::ExitCode`). Double-declaration is a Rust anti-pattern and produces duplicate-symbol confusion.
- **Do not** add doc-tests (` ``` `-fenced examples in `///` comments) in this story. Doc-tests run as a separate pass in `cargo test --doc` and the workspace lint discipline isn't fully thought through for them yet (Story 1.1 set `missing_docs = "warn"` but didn't address doctest standards). Add doc-tests in a later story if/when there's demand; this story sticks to plain `///` prose.
- **Do not** introduce a `ExitError` or `ExitResult` type alias. Use `Result<(), Error>` directly. Type aliases for short types are noise; a future reader has to chase the alias to learn what it is.
- **Do not** delete the existing `//!` crate-level doc comment in `src/main.rs` (lines 1–3 of the Story 1.1 stub). Update it to reflect this story's role (single `process::exit` site) but preserve the doc-comment style.

### Previous story intelligence (Story 1.1 → Story 1.2 → Story 1.3)

- **Local gate is already green and CI is wired** [Source: Story 1.2 §"Debug Log References"]: `cargo build`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` all pass on a clean tree, and `.github/workflows/ci.yml` runs them on every push. After T7.4–T7.5, the new test from T6.2 is the **first** real assertion the CI test step has ever run. This is the moment Story 1.2's AC3 stops being theoretical.
- **Stub `main.rs` has zero behavior — replacing it is safe** [Source: src/main.rs:5–7 + Story 1.2 dev notes]: The current `fn main() {}` does nothing and has no callers. T5 replaces the entire body without breaking anything. Do not preserve any of the stub's lines other than the `//!` crate doc-comment style.
- **Lint-active probe pattern is reusable** [Source: Story 1.1 T2.4 + Story 1.2 T3.4]: Stories 1.1 and 1.2 both used `let _ = Some(1u32).unwrap();` in `src/main.rs` to probe the `clippy::unwrap_used` lint. After this story, the probe still works the same way (T7.5 optional probe in this story is to *break a test* rather than insert an unwrap — the test gate is what we're proving here, not the lint gate).
- **`Cargo.lock` is committed; `Swatinem/rust-cache@v2` keys on it** [Source: Story 1.2 §"Architecture compliance"]: This story does **not** add dependencies. Cargo.lock should not change beyond the `[lib]` declaration in Cargo.toml (which doesn't itself touch the lock). If `cargo build` *does* alter Cargo.lock, that's a smell — investigate before committing.
- **Tracer-bullet vertical-slice principle was honored in 1.1 + 1.2 and must be honored here** [Source: `MEMORY.md` → `feedback_tracer_bullet_epics.md` + Story 1.2 Completion Notes]: This story takes a thin vertical slice through `main.rs → lib.rs → error → exit_code → output` for the **already-wired** trigger paths (exit 0). It does **not** pre-stub future epics' modules or tests. Per-variant tests for codes 1/2/4/5/10/12 land with their owner stories — that is the principle.
- **Single-commit-on-main pattern** [Source: Story 1.2 §"Probe history hygiene"]: Verify on a feature branch (`story/1.3-output-exit-error`), squash into a clean single commit landing on `main`, delete the remote feature branch after review. Probe edits do not land on `main`.

### Git intelligence summary

- Recent commits (newest first): `a902ab0` (deferred-work.md tracking the `actions/checkout@v5` follow-up from Story 1.2), `a771791` (Story 1.2 CI workflow), `3fe4f81` (MSRV bump 1.85 → 1.95), `e0c8bc4` (Story 1.1 scaffold).
- Current `src/` contains only `main.rs` (8 lines, the Story 1.1 stub). `tests/` directory does not exist yet — `tests/cli_exit_codes.rs` is the first file under it (T6.1 creates the directory).
- No release tags exist; this is pre-v0.1.0 development. The `0.0.1` version pin in `Cargo.toml` line 3 is intentional and stays — Story 1.4 may bump to `0.1.0-dev` or similar; this story does not.
- The deferred-work.md item from Story 1.2 (`actions/checkout@v5` bump, deadline 2026-09-16) is **not** addressed by this story — it remains a tracked deferred item.

### Latest tech information (Rust ecosystem — relevant to this story only)

- **`thiserror` 2.x** [Source: Cargo.toml line 55]: 2.0 (released 2024-12) introduced minor breaking changes from 1.x around `Display` trait derivation but the `#[error("...")]` + `#[from]` patterns used in T4.1 are identical to 1.x. No migration concerns.
- **`anyhow` 1.x**: stable for years, no relevant version-specific concerns. `anyhow::Error` carries a `Send + Sync + 'static` error; the `#[from] anyhow::Error` impl in `Error::Other` works because `anyhow::Error: Send + Sync + 'static` and `thiserror` requires the same for `#[from]` source types.
- **`assert_cmd` 2.x**: `Command::cargo_bin("lcrc")` looks up the binary by Cargo target name (which matches `Cargo.toml` `[[bin]] name = "lcrc"`), invokes it, and returns an `Assert` for chaining. `.success()` asserts exit 0; `.failure()` asserts non-zero; `.code(N)` asserts a specific code. Use `.code(0)` in T6.2 (more explicit than `.success()`) so the test name and assertion both name the FR45 contract.
- **Rust edition 2024 + 1.95**: `let_chains` are stable; `if let Some(...) && cond { ... }` works (not used in this story but worth knowing for `error.rs`). `#[diagnostic::on_unimplemented]` exists if needed for ergonomic error messages on missing `From` impls (also not used here, but it's the "modern Rust" tool for this kind of contract enforcement should this enum grow).

### Project Structure Notes

The architecture's `src/` directory map [architecture.md §"Complete Project Directory Structure", lines 874–890] places `exit_code.rs`, `error.rs`, `output.rs`, `version.rs`, and `constants.rs` as siblings under `src/` (alongside `main.rs` and `lib.rs`). This story authors three of those five (`exit_code.rs`, `error.rs`, `output.rs`); `version.rs` is Story 1.4, and `constants.rs` is Story 1.10/1.14 (when the container image digest gets pinned). No conflict with the architecture; this story matches the directory map exactly.

The single deviation from the directory map is that `lib.rs` is being added in this story rather than Story 1.1. Story 1.1 author note: `lib.rs` was deferred because Story 1.1 had nothing to put in it (no modules to declare). That deferral is resolved here — `lib.rs` lands together with the first three modules it declares.

No conflicts detected. The judgment call in this story is the placeholder `String` payloads in `Error::Preflight(String)` etc. — the alternative (waiting for Stories 1.9/6.1/etc. to land their typed errors before authoring `Error`) would defer the FR45 contract lock past Epic 1 and break the FR45 row in the FR Coverage Map ("Epic 1 (full enum defined)"). The placeholder approach is the cleanest way to honor both AR-28 (single source of truth, locked early) and the tracer-bullet principle (don't pre-author other stories' module-level error types here).

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.3: Output module + full ExitCode enum + error layer]
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 1: Integration spine — one cell, one row, end-to-end]
- [Source: _bmad-output/planning-artifacts/epics.md#FR Coverage Map] — FR45 trigger-path schedule
- [Source: _bmad-output/planning-artifacts/epics.md#Additional Requirements → AR-28] — single-source-of-truth modules
- [Source: _bmad-output/planning-artifacts/epics.md#Additional Requirements → AR-29] — two-layer error discipline
- [Source: _bmad-output/planning-artifacts/epics.md#Additional Requirements → AR-33] — every ExitCode variant has an integration test
- [Source: _bmad-output/planning-artifacts/architecture.md#Error Handling] — typed errors + anyhow + ExitCode enum spec
- [Source: _bmad-output/planning-artifacts/architecture.md#stdout / stderr Discipline (FR46)] — output.rs four-function API
- [Source: _bmad-output/planning-artifacts/architecture.md#Module Organization] — file-as-module style, single-trait-per-file
- [Source: _bmad-output/planning-artifacts/architecture.md#Complete Project Directory Structure] — file placement (lines 874–890)
- [Source: _bmad-output/planning-artifacts/architecture.md#Enforcement Summary — All AI Agents MUST] — codified discipline
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Boundaries (the "only X talks to Y" rules)] — `src/output.rs`, `src/exit_code.rs`, `src/main.rs` boundary rows
- [Source: _bmad-output/planning-artifacts/prd.md#Functional Requirements] — FR44 (non-interactive), FR45 (exit codes), FR46 (stdout/stderr)
- [Source: _bmad-output/implementation-artifacts/1-1-project-scaffold-with-locked-workspace-lints.md] — workspace lints, dependency lockset
- [Source: _bmad-output/implementation-artifacts/1-2-ci-workflow-gates-fmt-clippy-and-tests.md#Completion Notes List] — AC3-of-1.2 first-exercise forecast
- [Source: <claude-auto-memory>/feedback_tracer_bullet_epics.md] — vertical-slice principle (why we don't pre-stub future stories' tests/modules)
- [Source: <claude-auto-memory>/project_lcrc_no_marketing_posture.md] — open-source release; no urgency framing in artifacts

## Dev Agent Record

### Agent Model Used

claude-opus-4-7 (1M context) — bmad-dev-story workflow

### Debug Log References

- Initial `cargo clippy --all-targets --all-features -- -D warnings` flagged two issues:
  1. `clippy::doc_markdown` on `src/exit_code.rs:36` — `FR17a` lacked backticks. Fixed by surrounding the identifier with backticks (the other FR/AR identifiers in the same file did not trigger the heuristic, only the alphanumeric `FR17a` did).
  2. `clippy::match_same_arms` on `src/error.rs` — `Error::Preflight(_)` and `Error::Other(_)` both map to `ExitCode::PreflightFailed`. Story T4.2 explicitly mandates separate arms (no `_` catch-all and no merged `|` patterns) so each variant has a dedicated mapping site. Resolved with a scoped `#[allow(clippy::match_same_arms)]` on `Error::exit_code()` and an inline comment naming Story 1.3 T4.2 as the rationale.
- After both fixes: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-targets --all-features` all clean.

### Completion Notes List

- **Story 1.2 AC3 is now actually exercised for the first time.** Prior to this story, `cargo test` returned `0 passed; 0 failed`, so the CI test step was wired-but-silent. Both `tests/cli_exit_codes.rs::ok_path_exits_0` and `tests/cli_exit_codes.rs::exit_code_enum_full_contract` now run on every push.
- **Test count:** 11 tests pass (3 in `src/exit_code.rs::tests`, 1 in `src/output.rs::tests`, 5 in `src/error.rs::tests`, 2 in `tests/cli_exit_codes.rs`). No regressions; no prior tests existed to regress against.
- **Cold-cache wall times** (T7.4, recorded after `cargo clean`):
  - `cargo clippy --all-targets --all-features -- -D warnings`: **19.6 s** wall (69.7 s CPU at 411% utilization, 4-core machine).
  - `cargo test --all-targets --all-features`: **18.3 s** wall (79.8 s CPU at 483% utilization).
  - Both well within any plausible CI budget for Story 1.2 AC4.
- **AC verification** (T7.1–T7.3 greps, all run via `git grep`):
  - AC1 (`println!|eprintln!|print!|eprint!|dbg!` outside `src/output.rs`): zero matches.
  - AC3 (`process::exit|std::process::exit` in `src/`): exactly one *call site* at `src/main.rs:22`. Two additional matches at `src/main.rs:4` and `src/main.rs:9` are inside the `//!` module doc comment, not call sites.
  - AC5 (`.unwrap()|.expect(|panic!(` in `src/`): zero matches outside `#[cfg(test)]` blocks (and zero across the file scope of `src/` since all such calls live in test modules with the documented per-mod `#[allow]`).
- **Files match the architecture directory map exactly** for the three modules this story owns (`exit_code.rs`, `output.rs`, `error.rs` as siblings under `src/`). `lib.rs` was added in this story rather than Story 1.1 per the Dev Notes deferral.
- **`Cargo.lock` was not modified** — no new dependencies were added.
- **`main.rs` body is 11 lines (excluding the 12-line doc comment header)**, well under the ≤ 25-line target in T5.2.
- **T7.5 modified per maintainer direction (2026-05-05):** the GitHub CI verification step and optional break-probe were skipped; the work was landed as a single commit on `main` locally for the maintainer to push manually. The break-probe witness moment for Story 1.2 AC3 is deferred — the new test still runs on every push going forward, just without the explicit "watch the gate fail then go green" demonstration.

### File List

- `Cargo.toml` — modified: added `[lib] name = "lcrc" path = "src/lib.rs"` block immediately after the existing `[[bin]]` block.
- `src/main.rs` — modified: replaced the Story 1.1 stub with the single `process::exit` call site that matches `lcrc::run()`'s `Result`, renders any error via `lcrc::output::diag`, and surrenders with `code.as_i32()`.
- `src/lib.rs` — new: crate root with `#![cfg_attr(not(test), forbid(unsafe_code))]`, `pub mod {error, exit_code, output};`, and the no-op `pub fn run() -> Result<(), error::Error>`.
- `src/exit_code.rs` — new: `#[repr(i32)] pub enum ExitCode` with all 9 FR45 variants, `impl ExitCode { pub const fn as_i32 }`, `impl Display`, and a `#[cfg(test)] mod tests` covering numeric contract, snake-case `Display`, and exhaustive variant-set check.
- `src/output.rs` — new: `pub fn result`, `pub fn result_line<T: Display>`, `pub fn progress`, `pub fn diag`; the only module that writes to stdout/stderr.
- `src/error.rs` — new: `#[derive(thiserror::Error, Debug)] pub enum Error` with placeholder `String` payloads for `Preflight`/`Config`/`ConcurrentScan`, plus `AbortedBySignal` and `Other(#[from] anyhow::Error)`; `Error::exit_code()` is an exhaustive match with a scoped `#[allow(clippy::match_same_arms)]` for the `Preflight`/`Other` → `PreflightFailed` parity.
- `tests/cli_exit_codes.rs` — new: first integration test in the repo. `ok_path_exits_0` runs the built binary with no args; `exit_code_enum_full_contract` re-imports `lcrc::exit_code::ExitCode` and asserts every variant's discriminant.

### Change Log

| Date       | Author | Change                                                                                                                                                       |
|------------|--------|--------------------------------------------------------------------------------------------------------------------------------------------------------------|
| 2026-05-05 | Theop / claude-opus-4-7 | Story 1.3 implementation: added `src/lib.rs`, `src/exit_code.rs`, `src/output.rs`, `src/error.rs`, `tests/cli_exit_codes.rs`; rewrote `src/main.rs` as the single `process::exit` call site; declared `[lib]` target in `Cargo.toml`. fmt/clippy/test all clean; AC1/AC3/AC5 greps verified. T7.5 (remote push + CI verification) deferred to maintainer. |
