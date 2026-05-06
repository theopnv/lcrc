# Story 1.5: Machine fingerprint module

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a developer (Theop or future contributor),
I want `MachineFingerprint::detect()` to return a deterministic string of the form `"<chip>-<ram_gb>GB-<gpu_cores>gpu"` (e.g. `"M1Pro-32GB-14gpu"`),
so that cache cells can be keyed on hardware identity (per FR24) with byte-stable output across macOS patch-level upgrades (per NFR-C2) and a clean `Err` on non–Apple-Silicon platforms (per NFR-C1).

## Acceptance Criteria

1. **AC1 (Apple Silicon detection):** Given a Mac running on Apple Silicon, when `MachineFingerprint::detect()` is invoked, then it returns `Ok(MachineFingerprint)` whose canonical string matches the regex `^M[1-4](Pro|Max|Ultra)?-\d+GB-\d+gpu$` (e.g. `"M1Pro-32GB-14gpu"`, `"M2-16GB-10gpu"`, `"M3Max-64GB-40gpu"`). Verified by an integration test in `tests/machine_fingerprint.rs` running against the live host (macos-14 CI runners are Apple Silicon).
2. **AC2 (NFR-C2 patch-level stability):** Given the same chip + RAM + GPU-core inputs presented twice (representing pre- and post-macOS-patch state), when the canonical fingerprint string is rendered both times, then the two strings are byte-identical. Verified by a unit test that feeds identical mocked `sysctl`/`ioreg` outputs through the parse + render chain twice and asserts string equality.
3. **AC3 (NFR-C1 unsupported hardware):** Given an Intel Mac (`sysctl machdep.cpu.brand_string` → `"Intel(R) Core(TM) ..."`) or a Linux host (sysctl call fails or returns non–Apple output), when `detect()` is invoked, then it returns `Err(FingerprintError::UnsupportedHardware { reason })` whose `Display` rendering contains the substring `"unsupported hardware"` and explains which input failed (chip brand string mismatch vs. sysctl/ioreg exec failure).
4. **AC4 (chip coverage):** Given unit tests with mocked `sysctl` brand-string outputs, when they run, then they cover M1, M1 Pro, M1 Max, M2, M3, and M4 chip detection (one test or one parametrized assertion per variant — at minimum the six listed chips). Each test asserts the rendered fingerprint substring before the first dash matches the expected `"M1"` / `"M1Pro"` / `"M1Max"` / `"M2"` / `"M3"` / `"M4"` chip token.
5. **AC5 (callable from `dev` discipline):** All Story 1.3 grep gates continue to pass on the post-story tree:
   - `git grep -nE "println!|eprintln!|print!|eprint!|dbg!" -- 'src/**/*.rs' ':!src/output.rs'` → 0 matches.
   - `git grep -nE "\.unwrap\(\)|\.expect\(|panic!\(" -- 'src/**/*.rs'` → 0 matches outside `#[cfg(test)]` blocks.
   - `git grep -nE "process::exit|std::process::exit" -- 'src/**/*.rs'` → exactly one *call site* match in `src/main.rs` (doc-comment matches in the same file are pre-existing, accepted by the Story 1.3 baseline).

## Tasks / Subtasks

- [ ] **T1. Author `src/machine/apple_silicon.rs` — platform-specific detection + pure parse/render (AC: #1, #2, #3, #4)**
  - [ ] T1.1 Create the directory `src/machine/` and the file `src/machine/apple_silicon.rs`. Per AR-26 file-as-module style, the parent `src/machine.rs` (T2) declares `pub(crate) mod apple_silicon;`. The file-level `//!` doc cites architecture.md §"Complete Project Directory Structure" line 912 ("`src/machine/apple_silicon.rs # chip + RAM + GPU core detection`") and architecture.md §"Module Wiring (FR-by-FR)" line 1004 (sysctl chip detection + RAM + GPU cores).
  - [ ] T1.2 Define `pub(crate) enum Chip { M1, M1Pro, M1Max, M1Ultra, M2, M2Pro, M2Max, M2Ultra, M3, M3Pro, M3Max, M4, M4Pro, M4Max }` with `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`. Implement `pub(crate) fn token(self) -> &'static str` returning the dash-prefix of the canonical fingerprint string for that chip (e.g. `Chip::M1Pro.token() == "M1Pro"`, `Chip::M2.token() == "M2"`). The token spelling is **load-bearing** — every cache cell ever written is keyed on this string; do not rename without a cache migration. **Why** the variant list ends at M4Max: M1–M4 covers every Apple Silicon chip shipped through the Story 1.5 author date (2026-05-06); M5 / M5 Pro / etc. variants are added in their own future story when they ship and the dev confirms the brand-string suffix Apple chooses (Apple has used `" Pro"`, `" Max"`, `" Ultra"` consistently since M1, but the story adding the M5 chip variant must verify and document the new tokens before merging).
  - [ ] T1.3 Implement `pub(crate) fn parse_chip(brand_string: &str) -> Result<Chip, FingerprintError>` (where `FingerprintError` is defined in T2.2). The parser: trims whitespace, requires the literal prefix `"Apple "`, strips it, then matches the suffix against the chip-name table. The mapping is exact (`"M1"` → `Chip::M1`, `"M1 Pro"` → `Chip::M1Pro`, …). On any mismatch (no `"Apple "` prefix, unknown suffix), returns `Err(FingerprintError::UnsupportedHardware { reason: format!("unsupported chip brand string: {brand_string:?}") })`. **Critical:** the mapping table must collapse the space (`"M1 Pro"` brand string → `"M1Pro"` token) — the canonical fingerprint format omits the space (architecture.md line 726: `"M1Pro-32GB-14gpu"`, NOT `"M1 Pro-32GB-14gpu"`). Existing cache cells (none yet, but post-1.6) depend on this collapse.
  - [ ] T1.4 Implement `pub(crate) fn parse_ram_bytes(s: &str) -> Result<u64, FingerprintError>`. Trims whitespace, parses as `u64` (returning `Err(FingerprintError::ParseError { source: ... })` on any parse failure), and returns the byte count. Then implement `pub(crate) fn ram_bytes_to_gb(bytes: u64) -> u64` that performs **integer division** `bytes / (1024 * 1024 * 1024)` (binary GiB, NOT decimal GB — `sysctl hw.memsize` reports the binary-prefixed memory size; `34_359_738_368 / 2^30 == 32` exactly for a real 32GB rig). Document the unit choice (`GB` in the canonical string is shorthand for GiB by convention; this matches every Apple "32GB" SKU label which is also binary-prefix). Do **not** round; truncate.
  - [ ] T1.5 Implement `pub(crate) fn parse_gpu_cores_from_ioreg(ioreg_output: &str) -> Result<u32, FingerprintError>`. Strategy: scan the `ioreg -l` output line-by-line for the substring `"gpu-core-count"`; on the first match, extract the integer that follows `"= "` on the same line (e.g. line `        | |   "gpu-core-count" = 14` → returns `14`). If no line matches or the integer parse fails, return `Err(FingerprintError::UnsupportedHardware { reason: "ioreg output does not expose gpu-core-count (non-Apple-Silicon GPU?)".into() })`. **Why** parse via plain string scanning (not the `regex` crate): adding `regex` violates AR-4 (Curated Dependencies — locked); this single substring scan stays inside the locked dependency set. The parser is tolerant of extra whitespace / quoting variations (it splits on `=` and trims) but does not try to recover from a corrupt byte stream — corrupt input is a hardware-detection failure, surface it.
  - [ ] T1.6 Implement `pub(crate) fn render(chip: Chip, ram_gb: u64, gpu_cores: u32) -> String` returning `format!("{chip_token}-{ram_gb}GB-{gpu_cores}gpu", chip_token = chip.token())`. **This is the single source of truth for the fingerprint string format** — Story 1.6 (`cache::key::machine_fingerprint(&fp)`) reads the `MachineFingerprint`'s `as_str()` directly; the cell schema (architecture.md §"Cell Schema" line 257) stores this string verbatim as `machine_fingerprint TEXT NOT NULL`. Changing this format is a breaking cache-schema change.
  - [ ] T1.7 Implement `pub(crate) async fn read_chip() -> Result<Chip, FingerprintError>` that runs `tokio::process::Command::new("sysctl").args(["-n", "machdep.cpu.brand_string"]).output().await`, maps any I/O failure to `Err(FingerprintError::SysctlExecFailed { source: io_err })`, requires non-zero `output.status.success()` (else returns the same I/O error variant carrying the captured stderr in the source), then converts `output.stdout` to UTF-8 and feeds it to `parse_chip`. Same shape for `pub(crate) async fn read_ram_bytes() -> Result<u64, FingerprintError>` (using `args(["-n", "hw.memsize"])`) and `pub(crate) async fn read_gpu_cores() -> Result<u32, FingerprintError>` (using `tokio::process::Command::new("ioreg").args(["-l"]).output().await` and feeding stdout to `parse_gpu_cores_from_ioreg`). **Why** `tokio::process` (not `std::process`): AR-3 binding — "All I/O via `tokio::fs` and `tokio::process`; no `std::fs` / `std::process`". Even though detection runs once at scan startup, the call site (`cli/scan.rs::run` per architecture line 1094) is inside the future `#[tokio::main]`-flavored runtime; using `std::process` here forces a sync-bridge later or breaks the rule.
  - [ ] T1.8 Add `#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] mod tests { ... }` at file end. Test bodies (covering AC2, AC3, AC4):
    - `parse_chip` for each of the six AC4-listed brand strings: `"Apple M1"`, `"Apple M1 Pro"`, `"Apple M1 Max"`, `"Apple M2"`, `"Apple M3"`, `"Apple M4"` → returns the matching `Chip` variant. (Optional: also include `"Apple M2 Pro"` and `"Apple M3 Max"` for binning coverage; not required by AC4 but cheap.) Each assertion combines `parse_chip` with `Chip::token()` and asserts the rendered token (`"M1"`, `"M1Pro"`, `"M1Max"`, `"M2"`, `"M3"`, `"M4"`).
    - `parse_chip` rejection: `"Intel(R) Core(TM) i9-9880H CPU @ 2.30GHz"` (Intel Mac), `""` (empty), `"Apple M99 Hyperthreaded"` (unknown suffix) → all return `Err(FingerprintError::UnsupportedHardware { .. })` with `to_string()` containing `"unsupported"`.
    - `parse_ram_bytes` for `"34359738368\n"` → `Ok(34_359_738_368)`; `parse_ram_bytes("")` → `Err(FingerprintError::ParseError { .. })`.
    - `ram_bytes_to_gb(34_359_738_368) == 32`; `ram_bytes_to_gb(17_179_869_184) == 16`; `ram_bytes_to_gb(68_719_476_736) == 64`.
    - `parse_gpu_cores_from_ioreg` for a fixture string containing the literal line `"        | |   \"gpu-core-count\" = 14"` → `Ok(14)`; for an empty string → `Err(FingerprintError::UnsupportedHardware { .. })`; for a string containing the substring but no integer (`"\"gpu-core-count\" = "`) → `Err(FingerprintError::UnsupportedHardware { .. })`.
    - `render(Chip::M1Pro, 32, 14) == "M1Pro-32GB-14gpu"`; `render(Chip::M2, 16, 10) == "M2-16GB-10gpu"`; `render(Chip::M3Max, 64, 40) == "M3Max-64GB-40gpu"`.
    - **AC2 stability:** `render(Chip::M1Pro, 32, 14)` called twice in the same test produces byte-identical strings (`assert_eq!`). Companion: feed the same mocked sysctl-stdout strings through `parse_chip` + `parse_ram_bytes` + `ram_bytes_to_gb` + `parse_gpu_cores_from_ioreg` + `render` twice, assert equal — this exercises the whole pure pipeline as the patch-stability simulation.
    - Do **not** test `read_chip` / `read_ram_bytes` / `read_gpu_cores` directly here — they exec real subprocesses which aren't sandbox-friendly across CI environments and aren't deterministic. AC1 covers the integration via `tests/machine_fingerprint.rs` (T4).

- [ ] **T2. Author `src/machine.rs` — public API surface (AC: #1, #2, #3)**
  - [ ] T2.1 Create `src/machine.rs` with `pub(crate) mod apple_silicon;`. The file-level `//!` doc cites architecture.md §"Complete Project Directory Structure" line 910 (`src/machine.rs # MachineFingerprint (FR24, NFR-C2)`) and names the responsibilities: "owner of the `MachineFingerprint` type and the `detect` entry point; delegates platform-specific I/O to the `apple_silicon` submodule. Per FR24 the canonical fingerprint string is one of the seven cache-cell PK dimensions; per NFR-C2 it must be byte-stable across macOS patch-level upgrades. The pure parse/render functions in `apple_silicon` are responsible for that stability — `detect()` is just the I/O wrapper."
  - [ ] T2.2 Define `pub enum FingerprintError` deriving `Debug` + `thiserror::Error` with three variants:
    ```rust
    /// Hardware (chip / GPU) does not match a known Apple Silicon configuration.
    /// NFR-C1: lcrc supports macOS Apple Silicon only; Intel Macs and Linux
    /// hit this branch.
    #[error("unsupported hardware: {reason}")]
    UnsupportedHardware { reason: String },

    /// Underlying `sysctl` invocation failed (binary missing, non-zero exit, …).
    /// On Linux this fires when `sysctl machdep.cpu.brand_string` returns no
    /// such MIB; on a corrupted macOS install it fires if `/usr/sbin/sysctl`
    /// is missing.
    #[error("sysctl execution failed")]
    SysctlExecFailed { #[source] source: std::io::Error },

    /// Underlying `ioreg` invocation failed (binary missing, non-zero exit, …).
    /// macOS-only by construction; on non-macOS hosts this fires before
    /// `parse_gpu_cores_from_ioreg` ever runs.
    #[error("ioreg execution failed")]
    IoregExecFailed { #[source] source: std::io::Error },

    /// Sysctl returned data that could not be parsed (e.g. RAM bytes that
    /// don't fit `u64`, brand string in an unexpected encoding). Distinct
    /// from `UnsupportedHardware` to keep "the data is corrupt" separable
    /// from "the hardware is the wrong shape" in diagnostics.
    #[error("parse error: {message}")]
    ParseError { message: String },
    ```
    Reason for the structured `ParseError` (vs. wrapping `std::num::ParseIntError`): the parser also checks UTF-8 validity of stdout, and the `message` field carries enough context (which input was being parsed) without forcing the caller to inspect a downcast `source`. Keep `Display` rendering stable — the AC3 substring assertion (`"unsupported hardware"`) keys off the literal text in the `UnsupportedHardware` arm's `#[error(...)]` template.
  - [ ] T2.3 Define the public type:
    ```rust
    /// Canonical hardware identity used as the first dimension of every cache
    /// cell's PK (FR24). The wrapped string format is
    /// `"<chip-token>-<ram_gib>GB-<gpu_cores>gpu"` (e.g. `"M1Pro-32GB-14gpu"`).
    /// Construct via [`MachineFingerprint::detect`] only — no public constructor
    /// from raw strings, to keep [`crate::cache::key`] (Story 1.6) the sole
    /// caller that derives the cache-key string from a `MachineFingerprint`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct MachineFingerprint(String);
    ```
    Add `pub fn as_str(&self) -> &str { &self.0 }` and `impl std::fmt::Display for MachineFingerprint`. Do **not** derive `serde::Serialize` / `Deserialize` in this story — the cell-schema serialization layer (Story 1.7+) decides how to persist this; deriving now invites premature API commitment. Do **not** derive `Hash` / `Ord` until a call site demands them; cache lookup uses the `as_str()` directly through SQLite.
  - [ ] T2.4 Implement the entry point:
    ```rust
    impl MachineFingerprint {
        /// Detect the host hardware and return the canonical fingerprint.
        ///
        /// # Errors
        ///
        /// Returns [`FingerprintError::UnsupportedHardware`] on Intel Macs,
        /// Linux hosts, and any Apple Silicon variant whose chip brand string
        /// is not in the supported table. Returns
        /// [`FingerprintError::SysctlExecFailed`] /
        /// [`FingerprintError::IoregExecFailed`] when the underlying probes
        /// cannot be invoked. Returns [`FingerprintError::ParseError`] when
        /// probe output is structurally unexpected.
        pub async fn detect() -> Result<Self, FingerprintError> {
            let chip = apple_silicon::read_chip().await?;
            let ram_bytes = apple_silicon::read_ram_bytes().await?;
            let gpu_cores = apple_silicon::read_gpu_cores().await?;
            let ram_gb = apple_silicon::ram_bytes_to_gb(ram_bytes);
            Ok(Self(apple_silicon::render(chip, ram_gb, gpu_cores)))
        }

        /// Pure constructor for tests and internal composition (Story 1.6's
        /// `cache::key::machine_fingerprint` reads via `as_str` and never
        /// constructs).
        #[cfg(test)]
        pub(crate) fn from_canonical_string(s: String) -> Self { Self(s) }
    }
    ```
    Note the **probe order** (chip → RAM → GPU): if the host is Intel/Linux, the chip probe fails first and the function returns early; we never spend cycles on `ioreg` (which is macOS-only and would fail with a confusing `IoregExecFailed` on Linux instead of the more meaningful `UnsupportedHardware`).
  - [ ] T2.5 Add `#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] mod tests` covering:
    - **AC2 NFR-C2 stability via the public API:** construct two fingerprints from the same canonical string via `from_canonical_string` (the test-only helper) and assert `as_str()` byte-equal. Then construct via the pure `apple_silicon::render` for the same `(Chip, ram_gb, gpu_cores)` tuple and assert byte-equal to the canonical `from_canonical_string` value. This exercises the public `as_str` contract and locks the patch-stability invariant at the type-API level (the apple_silicon-level test in T1.8 covers the parse + render pipeline).
    - **`Display` → `as_str` round-trip:** `format!("{}", fp) == fp.as_str()`.
    - **`FingerprintError::UnsupportedHardware` Display contains `"unsupported hardware"`** — locks the AC3 substring requirement into the type definition (so a future thiserror message-template edit that drops the literal would fail this test).

- [ ] **T3. Wire `src/lib.rs` (AC: #1, #2, #3, #4)**
  - [ ] T3.1 Update `src/lib.rs`. Insert `pub mod machine;` between `pub mod exit_code;` and `pub mod output;` (alphabetical order matches the established Story 1.3/1.4 pattern). Do **not** modify any other line — `run()`, the `#![cfg_attr(not(test), forbid(unsafe_code))]` outer attribute, the `//!` crate doc, etc. all stay as Story 1.4 left them. The Story 1.4 `cli::parse_and_dispatch()` body is unchanged; this story does not wire `MachineFingerprint::detect()` into any command path yet (Story 1.12 wires it into `cli::scan::run` per architecture data flow line 1094).

- [ ] **T4. Author `tests/machine_fingerprint.rs` — integration test (AC: #1)**
  - [ ] T4.1 Create `tests/machine_fingerprint.rs` with `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` at file top (test-binary file-level allowance, matching the Story 1.4 `tests/cli_help_version.rs:1` pattern).
  - [ ] T4.2 `#[cfg(target_os = "macos")] #[tokio::test] async fn detect_returns_apple_silicon_canonical_string()` — calls `lcrc::machine::MachineFingerprint::detect().await.unwrap()`, asserts `as_str()` matches the regex `^M[1-4](Pro|Max|Ultra)?-\d+GB-\d+gpu$`. **Implementation note** since `regex` is not in the dep set: do the check by manual byte-walking (`split('-')` into three parts, check the chip token against an explicit allow-list, parse `\d+GB` and `\d+gpu` numerically). Or even simpler: assert the three structural properties — (a) `as_str().split('-').count() == 3`, (b) the first token is one of the explicit set `{"M1","M1Pro","M1Max","M1Ultra","M2","M2Pro","M2Max","M2Ultra","M3","M3Pro","M3Max","M4","M4Pro","M4Max"}`, (c) the second token ends with `"GB"` and the prefix parses as `u64`, (d) the third token ends with `"gpu"` and the prefix parses as `u32`. **Why** the explicit allow-list (not a regex match like `"M[1-4]..."`): a future M5 chip silently passing this test would let an Apple Silicon variant slip through without the dev confirming the brand-string suffix; explicit listing forces a deliberate update.
  - [ ] T4.3 `#[cfg(not(target_os = "macos"))] #[tokio::test] async fn detect_returns_unsupported_hardware_on_non_macos()` — calls `MachineFingerprint::detect().await`, asserts `Err(_)` and that the `Display` rendering contains `"unsupported"`. **Why** the `cfg`-gated mirror: keeps the integration test surface meaningful on both CI matrices (today macos-14 only per `.github/workflows/ci.yml`, but a future Linux-NVIDIA additive port for v1.1 per NFR-C5 needs this gate already in place to stay green). On the current macOS-only CI, this test compiles to nothing.
  - [ ] T4.4 Do **not** add a third integration test for AC2 stability — the in-module unit tests in T1.8 / T2.5 already cover patch stability with deterministic mocked inputs; an integration test calling `detect()` twice on the live host would just be a tautology (the system isn't going to change between two calls one ms apart). Defer the cross-OS-patch integration test to Story 5.x where `lcrc verify` is the user-facing surface for it (architecture.md §"Module Wiring" line 1044: `"FR30 (OS-patch stability): src/machine/apple_silicon.rs + tested by tests/machine_fingerprint.rs"` — this story authors the file with the AC1 test; FR30 (Story 5.1+) extends it).

- [ ] **T5. Verify the discipline on the local tree (AC: #5)**
  - [ ] T5.1 Run Story 1.3's AC1 grep verbatim: `git grep -nE "println!|eprintln!|print!|eprint!|dbg!" -- 'src/**/*.rs' ':!src/output.rs'` and confirm zero matches. Specifically the new `src/machine.rs` and `src/machine/apple_silicon.rs` must not call any print macro — all user-facing output flows through `lcrc::output::*` or `tracing::*` (and this story emits neither — `MachineFingerprint::detect()` returns `Result` and the call site, when wired in 1.12, will log via `tracing`).
  - [ ] T5.2 Run Story 1.3's AC5 grep verbatim: `git grep -nE "\.unwrap\(\)|\.expect\(|panic!\(" -- 'src/**/*.rs'` and confirm zero matches outside `#[cfg(test)]` blocks. Specifically the `read_chip` / `read_ram_bytes` / `read_gpu_cores` helpers must propagate I/O errors via `?` and the typed `FingerprintError`, never `.unwrap()` on the `Output` struct's `.status` or `.stdout`.
  - [ ] T5.3 Run Story 1.3's AC3 grep verbatim: `git grep -nE "process::exit|std::process::exit" -- 'src/**/*.rs'` and confirm exactly one *call site* in `src/main.rs:15` (existing). Doc-comment matches in `src/main.rs` / `src/cli.rs` / `src/exit_code.rs` are accepted by Story 1.3's baseline.
  - [ ] T5.4 Local CI mirror: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features`. All three must pass on the clean tree before push. Watch the `missing_docs = "warn"` lint specifically — every `pub` item in `src/machine.rs` (the `MachineFingerprint` struct, `as_str`, `Display` impl, `detect`, and every `FingerprintError` variant) needs a `///` doc. Record clippy + test cold wall times in Completion Notes (informational; no AC, but tracks the Story 1.2 AC4 budget over time — Story 1.4's baseline was clippy ~22s / test ~0.5s after the clap-derive add).
  - [ ] T5.5 Push to the existing feature branch `story/1-5-machine-fingerprint-module` (already checked out per `gitStatus` in the activation context). Per `MEMORY.md → feedback_lcrc_branch_pr_workflow.md`: per-story branch + PR against main; squash-merge with branch deletion, orchestrated by `scripts/bmad-auto.sh`.

## Dev Notes

### Scope discipline (read this first)

This story authors **three files** (two new + one updated):

- **New (Rust source):** `src/machine.rs`, `src/machine/apple_silicon.rs`
- **New (test):** `tests/machine_fingerprint.rs`
- **Updated:** `src/lib.rs` (adds `pub mod machine;` — single line)

This story does **not**:

- Wire `MachineFingerprint::detect()` into any command path. Architecture data flow (architecture.md line 1094) places the call at `cli::scan::run` step 4; that's Story 1.12's wiring (`End-to-end one-cell scan`) when scan is no longer a stub. The current `src/cli/scan.rs::run()` stays a stub returning `Ok(())` after printing the "not yet implemented" diagnostic from Story 1.4.
- Add a `From<FingerprintError> for crate::error::Error` impl. The boundary conversion is owned by the wiring story (1.12) — adding it now creates dead API surface and forces a decision (Preflight vs. a new variant) before the call site exists. The wiring story decides based on user-facing exit-code semantics: an "unsupported hardware" detection failure during scan startup is a preflight failure → `Error::Preflight(fp_err.to_string())` → `ExitCode::PreflightFailed (11)`.
- Author `src/cache/key.rs` or `cache::key::machine_fingerprint` helper. That's Story 1.6's deliverable. Story 1.6 takes a `&MachineFingerprint` and returns its `as_str()` (or owns the conversion). Story 1.5's `MachineFingerprint::as_str()` is the contract Story 1.6 reads through; do not pre-add a `to_cache_key()` method here.
- Add a chip-detection cache or memoization. `MachineFingerprint::detect()` is called once per scan; the cost (~5–10ms for two `sysctl` execs + one `ioreg` exec) is not on the hot path. Adding `OnceLock` here is premature optimization and complicates the test surface.
- Add Linux/Intel-Mac stub implementations beyond returning the `UnsupportedHardware` error. NFR-C5 says "v1 architecture must not preclude Linux NVIDIA support in v1.1 — i.e., platform-specific code... is factored cleanly such that Linux additions are additive, not architectural rewrites." The current shape (single `apple_silicon` submodule under `machine.rs`) supports an additive `linux_nvidia` submodule in v1.1 with a `#[cfg(target_os = ...)]`-gated dispatcher in `machine.rs::detect`; this story's architecture leaves that door open without pre-stubbing it.
- Add new dependencies to `Cargo.toml`. `tokio` (for `tokio::process::Command`) is locked in Story 1.1 (Cargo.toml line 35, `features = ["full"]` includes `process` + `macros` for `#[tokio::test]`); `thiserror` is locked (line 59); `std::process::Output` parsing is std-only. **Do not** add `regex`, `sysinfo`, `mac-address`, `byte-unit`, or any other "convenience" crate — the parse/render functions are 30 lines of std-only Rust.
- Modify `Cargo.toml` at all (no new deps; no new `[dev-dependencies]`).
- Touch `src/main.rs`, `src/cli.rs`, `src/cli/*.rs`, `src/error.rs`, `src/exit_code.rs`, `src/output.rs`, `src/util.rs`, `src/util/tracing.rs`, `src/version.rs`, or `build.rs`. None of those need to change for Story 1.5.

### Architecture compliance (binding constraints)

- **Single source of truth for the canonical fingerprint string format** [Source: architecture.md §"Cache Key Canonicalization" line 726]: `format!("{chip}-{ram_gb}GB-{gpu_cores}gpu")`. The `apple_silicon::render` function in T1.6 is the **only** place this format string lives. Story 1.6's `cache::key::machine_fingerprint` reads `MachineFingerprint::as_str()`; it must not re-format. Any future code that needs a fingerprint string asks the `MachineFingerprint` for it; no inline `format!` calls.
- **Apple Silicon only in v1, with v1.1 Linux NVIDIA additive** [Source: architecture.md §"Architecture Validation Results" line 1177 + NFR-C5]: The submodule layout (`src/machine.rs` as the dispatcher; `src/machine/apple_silicon.rs` as the macOS impl) is the structural form of "platform-specific code factored cleanly such that Linux additions are additive." A future `src/machine/linux_nvidia.rs` slots in beside `apple_silicon.rs` with a `#[cfg(target_os = ...)]` arm in `MachineFingerprint::detect`. Do not collapse the two-file structure into a single `src/machine.rs` — that breaks the additive shape.
- **All process I/O via `tokio::process`, never `std::process`** [Source: AR-3 + architecture.md line 165]: `read_chip` / `read_ram_bytes` / `read_gpu_cores` use `tokio::process::Command`. Story 1.5 is the **first** story to introduce a `tokio::process::Command` call site in the codebase (Story 1.1 locked the dep; nothing exec'd a process until now). Do not use `std::process::Command::output()` even though it's tempting for "simple sync code" — the call site (1.12 wiring) is async and a `block_on` wrapper would re-introduce the synchronous-bridge antipattern AR-3 is specifically forbidding.
- **No `std::fs` / `std::process` anywhere except `build.rs`** [Source: AR-3]: `build.rs` is exempt by cargo design (separate compilation unit; runs at build time). All runtime code uses `tokio::*`. The two file-level `//!` comments in `src/machine.rs` and `src/machine/apple_silicon.rs` should not need to mention this — it's enforced by tree-wide grep, not local convention.
- **No `unsafe` anywhere** [Source: AR-27 + Cargo.toml line 77]: `unsafe_code = "forbid"` is workspace-level. The temptation in this story is to call `libc::sysctlbyname` directly via FFI — DO NOT. The shell-out approach (`tokio::process::Command::new("sysctl")...`) is the architecture-blessed path and stays inside the `forbid(unsafe_code)` envelope.
- **Workspace lints — `unwrap_used`, `expect_used`, `panic = "deny"`** [Source: AR-27 + Cargo.toml lines 80–84]: All four `read_*` and `parse_*` functions use `?` propagation against the typed `FingerprintError`. The integration test `tests/machine_fingerprint.rs` and the in-module `mod tests { ... }` blocks use the file-level `#![allow(...)]` (file-level for the integration test; per-module-attribute for the in-module ones — both patterns established by Stories 1.3 and 1.4).
- **`missing_docs = "warn"`** [Source: Cargo.toml line 78]: Every `pub` item gets a `///` doc. `MachineFingerprint`, `MachineFingerprint::detect`, `MachineFingerprint::as_str`, the four `FingerprintError` variants — all need docs. The variants' `#[error("…")]` template doubles as the `Display` impl but does NOT count as a `///` doc — add a separate `///` line above each variant.
- **MSRV 1.95** [Source: Cargo.toml line 5]: All language constructs in this story (`tokio::process::Command::output().await`, `std::io::Error`, plain string scanning) are stable well before 1.95. No nightly-only features.
- **Crate is binary + library** [Source: architecture.md §"Complete Project Directory Structure" line 874–876 + Story 1.3 T1.2]: `tests/machine_fingerprint.rs` exercises the library API (`use lcrc::machine::MachineFingerprint;`). The integration test does not spawn the `lcrc` binary (that's `tests/cli_help_version.rs` and `tests/cli_exit_codes.rs`'s job).
- **Tracing / logging discipline** [Source: AR-31 + architecture.md §"Tracing / Logging" line 770]: This story emits **no** tracing events anywhere. The `MachineFingerprint::detect()` path is silent on success (returns the value); on failure, it returns `Err(FingerprintError)` and the caller (when 1.12 wires it in) decides whether to `tracing::warn!` before propagating. Adding `tracing::info!("detected fingerprint = {fp}")` here is premature instrumentation; defer to the wiring story.
- **Atomic-write discipline** [Source: AR-30]: N/A in this story — `MachineFingerprint::detect()` does no disk I/O.

### Resolved decisions (don't re-litigate)

These are choices that the dev agent might be tempted to revisit. Each is locked here with rationale.

- **Shell out to `sysctl` and `ioreg` via `tokio::process`** (NOT FFI to `libc::sysctlbyname`, NOT a sysctl-wrapper crate). Why: (a) `unsafe_code = "forbid"` rules out raw FFI; (b) AR-4 dependency lockset rules out adding a `sysctl-rs` / `mac-address` crate; (c) the cost of three subprocess execs at scan startup is ~5–10ms, well below the NFR-P7 budget for any operation; (d) shell-out is trivially mockable in tests by feeding canned strings to the pure parse functions.
- **`detect()` is `async`** (not sync, not `block_on`). Why: AR-3 binding ("`tokio::process`, never `std::process`") forces async at the I/O layer; the call site (1.12 `cli::scan::run`) is already inside the future `#[tokio::main]`-flavored runtime; tests use `#[tokio::test]` which Story 1.1's `tokio = { features = ["full"] }` already supports. Sync-bridge wrappers (`Runtime::new().block_on`) are an antipattern the architecture explicitly avoids.
- **Probe order: chip → RAM → GPU**. Why: chip is the strongest signal of platform support — on Intel/Linux the `parse_chip` step fails fast, returning `UnsupportedHardware` before we waste an `ioreg` exec that on Linux would fail with a confusing `IoregExecFailed` instead of the more meaningful "you're on the wrong platform" error.
- **RAM unit is binary GiB, displayed as `"GB"`**. Why: (a) `sysctl hw.memsize` reports the binary-prefixed size; (b) Apple's marketing labels (every "32GB" SKU) are also binary-prefix; (c) the canonical fingerprint string uses `"GB"` for compactness, not for unit precision. Document this in a `///` doc on `ram_bytes_to_gb` and move on.
- **Chip variant list ends at M4Max**. Why: M1–M4 covers every Apple Silicon chip shipped through 2026-05-06 (story author date). M5/M6 variants are added in their own story when they ship and the dev confirms Apple's brand-string suffix choice. Pre-stubbing a `Chip::M5` variant now risks a token-spelling mismatch (e.g. Apple ships "M5 Mini Pro" or some new naming we can't predict) that would force a cache-key-breaking renaming.
- **GPU core count parsed by plain string scan, not `regex` crate**. Why: dependency discipline (AR-4 — `regex` is not in the locked set). The single substring scan in `parse_gpu_cores_from_ioreg` is 10 lines of std-only Rust. Adding `regex` for one match is the kind of dependency creep AR-4 specifically forbids.
- **No `From<FingerprintError> for Error` impl in this story**. Why: dead API — no current call site converts. Story 1.12 (the wiring story) decides on the boundary mapping (preflight vs. new variant) when the call site exists. Adding it now is YAGNI.
- **No tracing events emitted**. Why: this story is a library module with no user-facing output; events belong at the call site (1.12). Emitting `tracing::info!("detected: {fp}")` here couples the detection module to the observability scheme prematurely.
- **Integration test asserts canonical-format STRUCTURE, not exact string**. Why: the test runs on macos-14 GitHub Actions runners (M1, ~7-core GPU + 16GB RAM typically) AND on the dev's M1 Pro 32GB 14-core, and we don't want the test to brittleneck to one specific hardware profile. The structural check (chip token in allow-list, `<n>GB`/`<n>gpu` suffixes parse) is the AC1 contract.

### Library / framework requirements (no new dependencies)

| Crate | Version (Cargo.toml line) | Use in this story |
|---|---|---|
| `tokio` | `1` (line 35), with `full` features | `tokio::process::Command::output().await` for the three sysctl/ioreg exec calls in `apple_silicon::read_*`. The `full` feature includes `process` (for `tokio::process`) and `macros` (for `#[tokio::test]`). Do **not** narrow the feature set in this story; Story 1.1 locked `full` deliberately to defer per-feature trimming until acceptance check #1 perf gates. |
| `thiserror` | `2` (line 59) | `#[derive(Error)]` on `FingerprintError`. The `#[source]` attribute on `SysctlExecFailed`/`IoregExecFailed` carries the underlying `std::io::Error` for diagnostic chains. |
| `std::process::Output` (std) | — | Used to inspect `.status.success()`, `.stdout`, `.stderr` of each `tokio::process::Command::output()` result. Note: `tokio::process::Command::output()` returns `std::process::Output` (re-exported), not a tokio type. |

**Do not** add: `regex` (dependency creep — see Resolved Decisions), `sysctl` / `mac-address` / `sysinfo` (out of AR-4 lockset), any `cfg-if` / `cfg-aliases` (the file-as-module structure already handles platform branching cleanly).

**Do not** widen the `tokio` feature set or pin a specific patch version — Story 1.1's lockset is binding.

### File structure requirements (this story only)

Files created or updated:

```
src/
  lib.rs                       # UPDATE: insert `pub mod machine;` between `exit_code` and `output`
  machine.rs                   # NEW: MachineFingerprint type, FingerprintError, async detect()
  machine/
    apple_silicon.rs           # NEW: Chip enum + parse_chip + parse_ram_bytes + parse_gpu_cores_from_ioreg + render + read_chip / read_ram_bytes / read_gpu_cores
tests/
  machine_fingerprint.rs       # NEW: integration test for AC1 (real-rig detect()) + cfg-gated non-macos mirror
```

Files **NOT** created by this story (deferred to listed owner stories — do not pre-stub):

- `src/cache.rs`, `src/cache/key.rs`, `src/cache/schema.rs`, `src/cache/cell.rs`, `src/cache/migrations.rs`, `src/cache/query.rs` — Stories 1.6 (`key.rs`) and 1.7 (`schema.rs` + `migrations.rs`) and 1.8 (`cell.rs` + `query.rs`)
- `src/discovery.rs`, `src/discovery/llama_cpp.rs`, `src/discovery/gguf.rs`, `src/discovery/fit_gate.rs` — Story 2.1 (`Backend` trait + llama.cpp model discovery)
- `src/sandbox*` — Stories 1.9 / 1.10 / 2.7
- `src/scan*` — Stories 1.10 / 1.11 / 1.12 / 2.6 / 2.13 / 2.15
- Any other architecture-named module — owned by their respective stories

### Testing requirements

This story authors **two test surfaces**:

1. **In-module unit tests** (T1.8 + T2.5) — verify each module's contract in isolation.
   - `src/machine/apple_silicon.rs::tests`: covers `parse_chip` (M1, M1 Pro, M1 Max, M2, M3, M4 — AC4), `parse_ram_bytes`, `ram_bytes_to_gb`, `parse_gpu_cores_from_ioreg`, `render`, the unsupported-hardware rejection paths (Intel brand string + empty input + unknown suffix — AC3), and the patch-stability simulation (same mocked inputs → byte-identical strings — AC2). Pattern is the documented Story 1.4 pattern: `#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] mod tests { ... }` at file end.
   - `src/machine.rs::tests`: covers `MachineFingerprint`'s `as_str` / `Display` round-trip and the `FingerprintError::UnsupportedHardware` Display-substring contract.
2. **Integration test** `tests/machine_fingerprint.rs` (T4) — black-box test of the library API.
   - `#[cfg(target_os = "macos")] #[tokio::test]` — calls real `detect()` and asserts the structural pattern of the canonical string (AC1). Runs on macos-14 CI and on the dev's local rig.
   - `#[cfg(not(target_os = "macos"))] #[tokio::test]` — calls `detect()` on a non-macOS host and asserts `Err` with `"unsupported"` Display. Currently compiles to nothing (CI is macos-only) but stays in place for the v1.1 NFR-C5 Linux additive port.

The existing `tests/cli_exit_codes.rs::ok_path_exits_0` and the entire `tests/cli_help_version.rs` suite from Story 1.4 must continue to pass. This story does not touch any code path those tests exercise; if any of them goes red after this story's commit, the dev wired something wrong outside the story scope — investigate before relaxing.

### Anti-patterns to avoid (LLM-developer pitfalls)

- **Do not** call `libc::sysctlbyname` via FFI. `unsafe_code = "forbid"` rules it out; the shell-out approach is the architecture-blessed path. Even if a "small unsafe block in a test" is tempting, do not.
- **Do not** add a `regex = "1"` dependency to parse `ioreg` output. Plain string scanning (`.lines().find_map(...)`) is 10 lines and stays inside the AR-4 lockset.
- **Do not** add a `sysctl-rs` / `mac-address` / `sysinfo` / `byte-unit` crate. Same dependency-discipline reasoning.
- **Do not** make `MachineFingerprint::detect()` synchronous via `tokio::runtime::Runtime::new().block_on(...)` or `futures::executor::block_on(...)`. AR-3 forbids `std::process` and the architecture data flow places the call inside `#[tokio::main]`'s runtime; sync-bridging is an antipattern the architecture is specifically avoiding.
- **Do not** call `std::process::Command::new("sysctl").output()` "for simplicity". Use `tokio::process::Command::new("sysctl")...output().await`. The `tokio::process::Command` API is identical in shape; the only difference is `.await` at the call site.
- **Do not** unwrap `Output.status` or `Output.stdout` outside `#[cfg(test)]`. Inspect `.status.success()` and convert `.stdout` to UTF-8 via `String::from_utf8(...).map_err(|e| FingerprintError::ParseError { message: format!("non-utf8 sysctl output: {e}") })?` (or equivalent).
- **Do not** derive `serde::Serialize` / `Deserialize` on `MachineFingerprint`. The cell-schema serialization decision lives in Story 1.7+; deriving now creates premature API commitment.
- **Do not** derive `Hash` / `Ord` on `MachineFingerprint`. SQLite cache lookup uses the canonical string directly; no in-memory hash maps need it in v1.
- **Do not** add a public constructor `MachineFingerprint::new(s: String)`. The `#[cfg(test)] from_canonical_string` helper is the only non-`detect` constructor. A public `new` would let downstream code (or a future story author) bypass the detection contract and invent fingerprints — defeating FR24 stability.
- **Do not** memoize `detect()` with `OnceLock<MachineFingerprint>` "for performance." The function is called exactly once per scan; memoization complicates the test surface (need to reset between tests) without measurable benefit.
- **Do not** add `tracing::info!("detected fingerprint = {fp}")` inside `detect()`. Observability events at this layer couple the module to the tracing scheme prematurely; the wiring story (1.12) decides whether and where to log.
- **Do not** use `std::env::var` to fake test inputs (e.g. `MACHINE_FINGERPRINT_OVERRIDE`). Story 1.5 establishes the patch-stability contract via mocked inputs to the **pure** parse/render functions; environment-based overrides are a config-loading concern that belongs to `src/config/*` (Story 6.1+), not to a hardware-detection module.
- **Do not** add a fallback `MachineFingerprint::default()` returning `"unknown-0GB-0gpu"`. NFR-C1 binding: an unsupported platform is an `Err`, not a degraded `Ok`. A `Default` impl invites the wiring story to skip error handling.
- **Do not** create `src/machine/intel.rs` or `src/machine/linux_nvidia.rs` "for completeness." NFR-C5 says additions are additive; pre-stubbing them now violates the tracer-bullet vertical-slice principle (see `MEMORY.md → feedback_tracer_bullet_epics.md`).
- **Do not** re-export `MachineFingerprint` at the crate root from `src/lib.rs` (e.g. `pub use machine::MachineFingerprint;`). Callers use the fully-qualified path `lcrc::machine::MachineFingerprint`. Re-exports are a v1-surface-locking decision; defer to the v1 polish story (Epic 6).
- **Do not** populate the `ScanArgs` / `ShowArgs` / `VerifyArgs` clap structs from Story 1.4 with a `--machine-fingerprint <override>` flag. There is no AC for an override; cache-busting via a hardware-spoofing CLI flag is a footgun that no current FR requires.
- **Do not** add a `From<std::io::Error> for FingerprintError` blanket impl. The two execution-failure variants (`SysctlExecFailed` / `IoregExecFailed`) carry the io error in named `source` fields; a blanket `From` would let a future call site convert any I/O error into `SysctlExecFailed` even when the I/O came from `ioreg` (or somewhere else entirely). Use explicit `.map_err(|e| FingerprintError::SysctlExecFailed { source: e })` at the two call sites.

### Previous story intelligence (Story 1.1 → 1.2 → 1.3 → 1.4 → 1.5)

- **Story 1.4 left `src/util.rs` declaring `pub mod tracing;` and `src/util/tracing.rs` installing the global subscriber** [Source: src/util.rs + src/util/tracing.rs]. This story's `src/machine.rs` follows the same pattern: a parent file (file-as-module per AR-26) declaring `pub(crate) mod apple_silicon;`, with the per-platform impl in `src/machine/apple_silicon.rs`. The shape is intentionally parallel — the dev who reads the codebase next sees the same convention.
- **Story 1.4 added `tests/cli_help_version.rs` as the second integration test using the `#![allow(clippy::unwrap_used, ...)]` file-top pattern** [Source: tests/cli_help_version.rs:1]. This story's `tests/machine_fingerprint.rs` is the third integration test and follows the same pattern (T4.1).
- **Story 1.4's review surfaced two clippy CI gate failures** [Source: 1-4-… Review Findings, "Clippy `needless_pass_by_value` on `dispatch(cli: Cli)`" + "Clippy `unnecessary_wraps` on `render_root_help`"] that were masked because `cargo clippy` was permission-blocked in the dev session. **Run `cargo clippy --all-targets --all-features -- -D warnings` locally** before pushing this story (T5.4) — the only authoritative gate for clippy is CI, but a local mirror catches the cheap stuff. Specifically watch for:
  - `clippy::needless_pass_by_value` on any function taking `MachineFingerprint` by value when only `as_str()` is read — pass `&MachineFingerprint` instead.
  - `clippy::unnecessary_wraps` on `read_chip` / `read_ram_bytes` / `read_gpu_cores` — these MUST return `Result` (multiple failure modes); the lint shouldn't fire, but if a future refactor makes one of them infallible, change the signature.
  - `clippy::missing_errors_doc` on `pub` functions returning `Result` — every `Result`-returning `pub` function in this story (`detect()`) needs a `# Errors` section in its `///` doc.
  - `clippy::needless_pass_by_ref_mut` etc. on the parse helpers — the `parse_*` functions take `&str`, not `&mut`, so this shouldn't fire.
- **Story 1.4's review surfaced an inverted-inequality test bug** [Source: 1-4-… Review Findings, `version_warm_under_200ms`] reasoning: warm wall time is a *lower bound* on cold wall time, not an upper bound. **Apply the same care** to any inequality reasoning in this story's tests — there are no latency assertions here, but if you add one (e.g. "sysctl exec under 100ms"), be precise about which direction the inequality gates.
- **Story 1.4's `build.rs` rerun-if-changed missed `.git/packed-refs`** [Source: 1-4-… Review Findings, "`build.rs` rerun-if-changed missed `.git/packed-refs`"]. **N/A here** — this story does not touch `build.rs`. Mentioning so the dev does not feel obligated to "fix it again."
- **Story 1.3 cold-cache wall times** [Source: 1-3-… Completion Notes]: clippy ~19.6s, test ~18.3s. **Story 1.4's expected creep** after clap-derive: small. **Story 1.5's expected creep** after `tokio::process` first-use: small (`tokio::process` is already in the compiled `tokio` blob; first-use doesn't re-link). If clippy or test wall time jumps >10× (e.g. clippy >200s), investigate before pushing — that signals an unwanted dep was added.
- **`Cargo.lock` is committed; `Swatinem/rust-cache@v2` keys on it** [Source: 1-2-… Architecture compliance]. This story does **not** add dependencies. `Cargo.lock` should not change. If `cargo build` *does* alter `Cargo.lock`, that's a smell — investigate before committing.
- **Tracer-bullet vertical-slice principle was honored in 1.1 / 1.2 / 1.3 / 1.4** [Source: `MEMORY.md → feedback_tracer_bullet_epics.md`]. This story's slice is thin: detection module + its tests, no wiring. The wiring story (1.12) takes the full vertical from CLI → scan → fingerprint → cache → sandbox → backend → report. Pre-wiring fingerprint into 1.12's stub here would inflate this story past its single concern.
- **Per-story branch + PR + squash-merge workflow** [Source: `MEMORY.md → feedback_lcrc_branch_pr_workflow.md`]. The branch `story/1-5-machine-fingerprint-module` is already checked out per `gitStatus` in the activation context. Push commits, open PR, wait for green CI, squash-merge with branch deletion via `scripts/bmad-auto.sh` (or the orchestrator's manual equivalent).
- **Apply the chore commit `7a6e029` lesson to your own comments** [Source: 1-4-… Git intelligence summary]: do not write `// Story 1.5 wires this` or `// Per epics.md FR24` — the *why* (e.g., `// Probe order: chip first → fail fast on Intel/Linux before wasting an ioreg exec`) goes in the comment; the planning artifact reference goes in the PR description and is discoverable via `git blame`.

### Git intelligence summary

- Recent commits (newest first per repo state at story creation): `3cb7e77` (bmad-auto retry transient GH API failures + friction-report pause), `ee6a89f` (chore: strip planning-meta comments from Story 1.4 modules), `91b95be` (Story 1.4: clap CLI root + `--version` + `--help` + tracing subscriber), `84f426e` (bmad auto mode infra), `7a6e029` (chore: removed low-value comments).
- The `ee6a89f` commit is informative: it stripped `// Per Story 1.4` / `// FR3 placeholder` planning-meta comments from the post-1.4 modules. **Apply the same restraint** in this story — comments explain *why* (constraints, invariants, non-obvious choices), not which planning artifact owns the change.
- Current `src/` (post-1.4) contains 11 files: `main.rs`, `lib.rs`, `error.rs`, `exit_code.rs`, `output.rs`, `cli.rs`, `cli/scan.rs`, `cli/show.rs`, `cli/verify.rs`, `util.rs`, `util/tracing.rs`, `version.rs`. After this story: 13 files (+ `machine.rs`, `machine/apple_silicon.rs`).
- `tests/` (post-1.4) contains 2 files: `cli_exit_codes.rs`, `cli_help_version.rs`. After this story: 3 files (+ `machine_fingerprint.rs`).
- Current branch `story/1-5-machine-fingerprint-module` is checked out (from `gitStatus`); working tree status was clean at story-creation time.
- The `actions/checkout@v5` deferred item from Story 1.2 [`_bmad-output/implementation-artifacts/deferred-work.md`] is **not** in scope for this story; soft deadline 2026-06-02 (≈ 4 weeks out as of 2026-05-06).
- No release tags exist; pre-v0.1.0 development. The `Cargo.toml` `version = "0.0.1"` pin (line 3) stays.

### Latest tech information (Rust ecosystem — relevant to this story only)

- **`tokio::process::Command`** [Source: tokio 1.x docs]: same builder shape as `std::process::Command` — `Command::new("sysctl").args(["-n", "machdep.cpu.brand_string"]).output().await`. Returns `Result<std::process::Output, std::io::Error>` (note: `Output` is the std type, re-exported). Inspect `.status.success()` for non-zero exit; `.stdout` and `.stderr` are `Vec<u8>`. The async `output()` future polls until the child exits. No `--features = ["process"]` toggle needed — the `full` features set Cargo.toml line 35 enables `process` already.
- **`#[tokio::test]`** [Source: tokio 1.x docs]: requires the `macros` feature (included in `full`). The macro generates a single-threaded runtime by default; for `#[tokio::test(flavor = "multi_thread")]` the runtime is multi-threaded but slower to spin up. **Use the default** for these tests — they spawn one subprocess each, no parallelism gain from multi-thread.
- **`std::process::Output`** [Source: std 1.95]: `pub struct Output { pub status: ExitStatus, pub stdout: Vec<u8>, pub stderr: Vec<u8> }`. `ExitStatus::success(&self) -> bool`.
- **`std::io::Error`** [Source: std 1.95]: returned by `tokio::process::Command::output().await` when the binary cannot be found or exec fails (NOT when the binary runs and returns non-zero — that's `output.status.code() != Some(0)`). Distinguish in the dev's mind: `io::Error` from `output()` = `SysctlExecFailed { source: e }`; `output.status.success() == false` = also `SysctlExecFailed { source: io::Error::new(io::ErrorKind::Other, format!("sysctl exited {code}: {stderr}")) }` (synthetic io error so the variant payload stays uniform).
- **`String::from_utf8`** [Source: std 1.95]: returns `Result<String, FromUtf8Error>`. The error wrapper carries the original `Vec<u8>` for recovery; in this story we discard it and emit a `FingerprintError::ParseError`.
- **`thiserror` 2.0** [Source: thiserror docs]: `#[derive(Error)]`, `#[error("...")]` for Display templates, `#[source]` for the error-chain pointer (used for the `io::Error` payload in `SysctlExecFailed` / `IoregExecFailed`). Already locked in Story 1.1.

### Project Structure Notes

The architecture's `src/` directory map [architecture.md §"Complete Project Directory Structure" lines 910–912] places `machine.rs` at `src/machine.rs` and the platform impl at `src/machine/apple_silicon.rs`. This story authors both, matching the file-as-module style (AR-26).

The single architectural judgment call in this story is the **`Chip` enum design** — alternatives:
- (a) `String` as the chip token — flexible but loses type safety; a typo `"M1pro"` would silently corrupt cache keys.
- (b) `enum Chip { ... }` with explicit variants — type-safe but requires updating the enum when Apple ships a new chip generation.
- (c) `&'static str` from a parse table — same flexibility issues as (a).

Choice **(b)** is locked. The cost (one enum-variant addition per chip generation) is paid in the future story that adds support for the new chip; the benefit (compile-time enforcement that the canonical token spelling is consistent) is paid every day during development.

The `#[cfg(target_os = "macos")]` gate in `tests/machine_fingerprint.rs` (T4.2 / T4.3) is the only conditional-compile in the story. The library code (`src/machine.rs`, `src/machine/apple_silicon.rs`) is **not** `cfg`-gated — it compiles on every target. On non-macOS, `read_chip` returns `SysctlExecFailed` or `UnsupportedHardware` (depending on whether `sysctl` exists in `PATH`), which is the user-visible "wrong platform" surface. This keeps the v1.1 Linux-NVIDIA additive port (NFR-C5) clean: `src/machine.rs::detect` evolves into a `#[cfg(target_os = ...)]`-arm dispatcher; `apple_silicon.rs` stays unchanged.

No conflicts detected.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.5: Machine fingerprint module] — the AC source
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 1: Integration spine — one cell, one row, end-to-end] — epic context (FR24 is in Epic 1's FR coverage)
- [Source: _bmad-output/planning-artifacts/epics.md#FR Coverage Map] — FR24 ("cell PK with all 7 dimensions") schedule = Epic 1; FR30 ("`machine_fingerprint` stability across OS patches") = Epic 5
- [Source: _bmad-output/planning-artifacts/architecture.md#Cache Key Canonicalization] — `machine_fingerprint = format!("{chip}-{ram_gb}GB-{gpu_cores}gpu")` single source of truth
- [Source: _bmad-output/planning-artifacts/architecture.md#Cell Schema] — `machine_fingerprint TEXT NOT NULL` first PK column; `e.g. "M1Pro-32GB-14gpu"` comment locks the canonical format
- [Source: _bmad-output/planning-artifacts/architecture.md#Module Organization] — file-as-module style (AR-26); one trait per module file
- [Source: _bmad-output/planning-artifacts/architecture.md#Complete Project Directory Structure] — `src/machine.rs` (line 910) and `src/machine/apple_silicon.rs` (line 912) placement
- [Source: _bmad-output/planning-artifacts/architecture.md#Module Wiring (FR-by-FR)] — `Machine fingerprint | src/machine/apple_silicon.rs | sysctl chip detection, RAM, GPU cores` (line 1004)
- [Source: _bmad-output/planning-artifacts/architecture.md#Module Wiring (FR-by-FR)] — `FR30 (OS-patch stability): src/machine/apple_silicon.rs + tested by tests/machine_fingerprint.rs` (line 1044)
- [Source: _bmad-output/planning-artifacts/architecture.md#Data Flow — One Scan Cycle] — `4. machine::fingerprint() → MachineFingerprint` step (line 1094); names the future call site at `cli/scan.rs::run` (Story 1.12)
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions / AR-3] — `tokio::fs` / `tokio::process`, never `std::fs` / `std::process`
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions / AR-26] — file-as-module style; one trait per module file
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions / AR-27] — `unsafe_code = "forbid"`; `unwrap_used` / `expect_used` / `panic` deny outside tests
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions / AR-29] — two-layer error discipline (`thiserror` typed errors at module boundaries)
- [Source: _bmad-output/planning-artifacts/architecture.md#Tracing / Logging] — no `tracing::error!` for expected failures
- [Source: _bmad-output/planning-artifacts/architecture.md#Architecture Validation Results] — NFR-C5: Linux NVIDIA additive port must stay possible without architectural rewrites (line 1177)
- [Source: _bmad-output/planning-artifacts/prd.md#FR24] — `(machine_fingerprint, model_sha, backend_build, params)` cache key + chip generation + RAM + GPU cores definition
- [Source: _bmad-output/planning-artifacts/prd.md#NFR-C1] — macOS 12+ Apple Silicon only; Intel and pre-Monterey unsupported
- [Source: _bmad-output/planning-artifacts/prd.md#NFR-C2] — `machine_fingerprint` stable across macOS patch-level upgrades
- [Source: _bmad-output/planning-artifacts/prd.md#NFR-C5] — platform-specific code factored cleanly so Linux NVIDIA is additive
- [Source: _bmad-output/implementation-artifacts/1-4-clap-cli-root-lcrc-version-lcrc-help-tracing-subscriber.md] — file-as-module pattern (`src/util.rs` + `src/util/tracing.rs`); test exemption pattern; integration test file-level `#![allow(...)]` shape
- [Source: _bmad-output/implementation-artifacts/1-3-output-module-full-exitcode-enum-error-layer.md] — `Error` type + `Error::Preflight` variant (the future boundary mapping target for `FingerprintError`)
- [Source: _bmad-output/implementation-artifacts/1-1-project-scaffold-with-locked-workspace-lints.md] — workspace lints + dep lockset (no new deps in this story)
- [Source: _bmad-output/implementation-artifacts/1-2-ci-workflow-gates-fmt-clippy-and-tests.md] — CI gate (macos-14 runner, 8-min budget); this story's tests run on push
- [Source: <claude-auto-memory>/feedback_tracer_bullet_epics.md] — vertical-slice principle (no pre-stubbing future-story files like `src/cache/key.rs`)
- [Source: <claude-auto-memory>/feedback_lcrc_branch_pr_workflow.md] — branch-then-PR-then-squash workflow

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List

### Review Findings

Code review of branch `story/1-5-machine-fingerprint-module` vs `main` on 2026-05-06. Three review layers (Blind Hunter, Edge Case Hunter, Acceptance Auditor) returned consistent signal; 12 findings triaged into 3 patches (applied), 3 deferred, 6 dismissed.

- [x] [Review][Patch] Linux integration test substring assertion incompatible with `SysctlExecFailed` Display [tests/machine_fingerprint.rs:60] — broadened the assertion to accept either `"unsupported"` or `"execution failed"`; both are valid NFR-C1 surfaces depending on whether `sysctl` is present on the host.
- [x] [Review][Patch] `SysctlExecFailed` / `IoregExecFailed` Display dropped underlying source [src/machine.rs:34, src/machine.rs:45] — added `: {source}` to both `#[error(...)]` templates so `err.to_string()` carries the real diagnostic without forcing callers to walk `.source()`.
- [x] [Review][Patch] `parse_gpu_cores_from_ioreg` substring match too loose [src/machine/apple_silicon.rs:124] — gated on the quoted ioreg key `"\"gpu-core-count\""` so a hypothetical future ioreg key with `gpu-core-count` as a substring cannot collide.
- [x] [Review][Defer] Multiple `gpu-core-count` lines on Mac Pro Ultra silently picks first — needs investigation on real multi-AGX hardware before a fix; first-match might already be the canonical SoC value.
- [x] [Review][Defer] No subprocess timeout in `run_capture` — would need `tokio::time::timeout` (already in the locked feature set); defensive only, real binaries don't hang in practice.
- [x] [Review][Defer] Boundary-input test gaps (BOM, embedded `\n`, u64 overflow, NBSP) — defensive coverage; production parsers reject these as `UnsupportedHardware` / `ParseError` already.
