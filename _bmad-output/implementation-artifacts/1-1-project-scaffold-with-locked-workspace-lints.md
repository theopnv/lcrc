# Story 1.1: Project scaffold with locked workspace lints

Status: review

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a developer (Theop or future contributor),
I want a Rust project scaffolded with the architecture's curated dependencies and workspace lints baked into `Cargo.toml`,
so that quality discipline is enforced from the first commit and AI agents inherit the same bar as humans.

## Acceptance Criteria

1. **AC1 (build):** Given a fresh clone of the repo, when I run `cargo build`, then the build succeeds on Rust 1.95+ stable with edition 2024.
2. **AC2 (lints baked in):** Given the project root, when I inspect `Cargo.toml`, then `[lints.rust]` declares `unsafe_code = "forbid"` and `missing_docs = "warn"`, and `[lints.clippy]` declares `pedantic = { level = "warn", priority = -1 }`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"` (with test-only exemptions documented in code).
3. **AC3 (curated dependency set):** Given the project root, when I inspect the dependency list, then it matches the architecture's locked set: `clap` v4 (with `derive`), `etcetera`, `is-terminal`, `nu-ansi-term`, `indicatif`, `serde`, `serde_derive`, `toml`, `figment`, `tokio` (full features), `reqwest`, `bollard`, `rusqlite` (bundled), `sha2`, `tempfile`, `fs2` (or `fd-lock`), `askama`, `nix`, `anyhow`, `thiserror`, `tracing`, `tracing-subscriber`, `time`, plus a GGUF parser dependency or module placeholder.
4. **AC4 (rustfmt clean):** Given the project root, when I run `cargo fmt --check`, then it succeeds (rustfmt config present at `rustfmt.toml`).
5. **AC5 (MSRV pinned):** Given the project root, when I look at `rust-toolchain.toml`, then it pins MSRV to current stable (Rust 1.95+).

## Tasks / Subtasks

- [x] **T1. Initialize the crate (AC: #1, #5)**
  - [x] T1.1 Run `cargo new --bin lcrc` semantics inside the existing repo root: create `Cargo.toml` and `src/main.rs` directly at the repo root (do **not** create a nested `lcrc/` subdirectory; this repo *is* the crate root).
  - [x] T1.2 Author `rust-toolchain.toml` pinning `channel = "stable"` and `components = ["rustfmt", "clippy"]` (relying on the toolchain manager — rustup/Homebrew Rust — to provide stable ≥ 1.95).
  - [x] T1.3 Set `edition = "2024"` and `rust-version = "1.95"` in `[package]`.
  - [x] T1.4 Author a minimal `src/main.rs` that compiles cleanly under the workspace lints (e.g. `fn main() { /* lcrc entry — wired in Story 1.4 */ }`); add a crate-level doc comment to satisfy `missing_docs = "warn"` for the binary target.

- [x] **T2. Bake in workspace lints (AC: #2)**
  - [x] T2.1 Add `[lints.rust]` table with `unsafe_code = "forbid"` and `missing_docs = "warn"`.
  - [x] T2.2 Add `[lints.clippy]` table with `pedantic = { level = "warn", priority = -1 }`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`.
  - [x] T2.3 Document the test-only exemption pattern as a comment above the `[lints.clippy]` table: `// #[cfg(test)] modules opt out via #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] at the module level — applied per file when tests are added.`
  - [x] T2.4 Verify lints are active by adding (and then removing) a temporary `let _ = Some(1u32).unwrap();` in `main.rs` — `cargo clippy --all-targets --all-features -- -D warnings` must fail.

- [x] **T3. Add curated dependencies with version constraints (AC: #3)**
  - [x] T3.1 In `[dependencies]`, add the locked set with conservative caret-version constraints (latest stable on crates.io; see Library/Framework Requirements below for canonical names + features).
  - [x] T3.2 Enable required features explicitly: `clap = { version = "4", features = ["derive"] }`, `tokio = { version = "1", features = ["full"] }`, `serde = { version = "1", features = ["derive"] }`, `rusqlite = { version = "0.32", features = ["bundled"] }`, `tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }`, `figment = { version = "0.10", features = ["toml", "env"] }`, `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }`.
  - [x] T3.3 Pick **one** of the GGUF options and document the choice in a one-line comment in `Cargo.toml`:
        - **Option A (preferred):** add `ggus = "<latest>"` if the crate exists and is maintained on crates.io.
        - **Option B (fallback):** add `// gguf parser: handwritten, lands in Story 2.2 (`src/discovery/gguf.rs`)` — no dependency line yet.
  - [x] T3.4 Add `[dev-dependencies]`: `assert_cmd = "2"`, `predicates = "3"`, `insta = { version = "1", features = ["yaml"] }`. (Architecture lists these for later test stories — adding them now prevents churn in the lockfile when Story 1.2/1.3 wire CI and unit tests.)
  - [x] T3.5 Run `cargo build` (downloads + compiles the dep tree). This is the canonical AC1 check; it also generates `Cargo.lock` which **must be committed** (this is a binary crate, per architecture §"Build & Distribution").

- [x] **T4. rustfmt configuration (AC: #4)**
  - [x] T4.1 Create `rustfmt.toml` with the default profile. The architecture says "default profile" — leaving the file effectively empty or with a single explanatory comment (`# default profile per architecture §"Rust Style Baseline"`) is fine.
  - [x] T4.2 Run `cargo fmt --check` — it must succeed.

- [x] **T5. Verify the full gate locally (AC: #1–#5)**
  - [x] T5.1 `cargo build` → succeeds.
  - [x] T5.2 `cargo fmt --check` → succeeds.
  - [x] T5.3 `cargo clippy --all-targets --all-features -- -D warnings` → succeeds. (Story 1.2 wires this into CI; running it locally now catches issues the workspace lints surface from the dependency graph.)
  - [x] T5.4 `cargo test` → runs (no tests yet; output should report `0 passed; 0 failed`).

## Dev Notes

### Scope discipline (read this first)

This story scaffolds the crate. **It does not create the full `src/` module tree** sketched in architecture §"Complete Project Directory Structure" — those modules land across Stories 1.3–1.14 and beyond. Concretely, the only `src/` file you create is `src/main.rs` with a stub body. Do **not** pre-create `src/output.rs`, `src/exit_code.rs`, `src/cache/`, etc. — that is Story 1.3+ work. Resist the urge to "set up everything at once"; each later story owns its own files and verifies its own ACs.

### Architecture compliance (binding constraints)

- **Edition + MSRV** [Source: architecture.md#Rust Style Baseline]: Rust 2024 edition, MSRV pinned to current stable at v1 start (Rust 1.95+). Local toolchain `rustc --version` reports `1.95.0`; the `rust-version = "1.95"` field documents the contract for downstream consumers, matching the active stable channel.
- **Single binary, no Cargo workspace** [Source: architecture.md#Project Structure & Boundaries (lcrc/ tree, no `[workspace]` table) + AR-26 in epics.md]: The crate manifest is at the **repo root**, not nested. Do not introduce `[workspace]` in v1.
- **File-as-module style** [Source: architecture.md#Module Organization]: No `mod.rs`. Not actively exercised in this story (only `main.rs` exists), but enforce it from the very first added module in subsequent stories.
- **Lints are non-negotiable** [Source: architecture.md#Rust Style Baseline + AR-27]: The exact `[lints.*]` tables in AC2 are copy-paste-able from the architecture — do not improvise variants.
- **`unsafe_code = "forbid"`** [Source: architecture.md#Rust Style Baseline]: No `unsafe` anywhere in v1; integrations that need FFI go through published crates.
- **No `unwrap`/`expect`/`panic` in non-test code** [Source: architecture.md#Enforcement Summary]: Enforced via the clippy lints in AC2. Test modules opt out via `#[allow(...)]` annotations as documented in T2.3.
- **`Cargo.lock` is committed** [Source: architecture.md#Architectural Decisions Provided by This Foundation]: This is a binary crate, not a library — lockfile commits are the standard.

### Library/framework requirements (canonical names + features)

The architecture-locked dependency set [Source: architecture.md#Curated Dependencies (per integration surface) + AR-4 in epics.md]. Use latest stable major version available on crates.io as of 2026-04-30; the major versions below are pinned by architecture choice.

| Crate | Min major | Required features | Purpose / source citation |
|---|---|---|---|
| `clap` | 4 | `derive` | CLI root + subcommands (FR4, FR44) |
| `etcetera` | latest | — | XDG base directory resolution |
| `is-terminal` | latest | — | TTY detection (FR47) |
| `nu-ansi-term` | latest | — | Terminal color (FR47) |
| `indicatif` | latest | — | Streaming progress + ETA (NFR-P8) |
| `serde` | 1 | `derive` | Serialization frame |
| `serde_derive` | 1 | — | (re-exported by `serde` with `derive` feature; explicit dep optional) |
| `toml` | latest | — | TOML parsing (FR49) |
| `figment` | 0.10 | `toml`, `env` | Layered config (FR50) |
| `tokio` | 1 | `full` | Single async runtime (AR-3) |
| `reqwest` | 0.12 | `json`, `rustls-tls` (no default features) | HTTP client to llama-server |
| `bollard` | latest | — | Docker Engine API (FR16, FR17a) |
| `rusqlite` | 0.32+ | `bundled` | Cache storage (AR-7); `bundled` ships SQLite, avoids host libsqlite |
| `sha2` | latest | — | `model_sha`, `params_hash` (FR8, AR-7) |
| `tempfile` | latest | — | Atomic write pattern (NFR-R2) |
| `fs2` | latest | — | `scan.lock` file (FR52); `fd-lock` is an acceptable substitute per AR-4 |
| `askama` | latest | — | HTML report templating (FR32) |
| `nix` | latest | — | Unix signal handling (FR27, FR45) |
| `anyhow` | latest | — | App-level error propagation (AR-29) |
| `thiserror` | latest | — | Module-boundary typed errors (AR-29) |
| `tracing` | latest | — | Structured logging (AR-31, NFR-O1) |
| `tracing-subscriber` | 0.3 | `env-filter`, `fmt` | Subscriber setup; custom formatter lands in Story 1.4 |
| `time` | latest | — | RFC 3339 timestamps (AR-23) |
| GGUF parser | — | — | `ggus` crate **or** placeholder (handwritten parser in Story 2.2) — see T3.3 |

Dev-only deps (added now per T3.4 to stabilize the lockfile early): `assert_cmd`, `predicates`, `insta` [Source: architecture.md#Curated Dependencies → Testing].

### File structure requirements (this story only)

Files created by this story:

```
Cargo.toml              # crate manifest + workspace lints + curated deps
Cargo.lock              # generated by cargo build; committed (binary crate)
rust-toolchain.toml     # MSRV pin
rustfmt.toml            # default profile
src/main.rs             # minimal stub: fn main() { /* wired in Story 1.4 */ }
```

Files **not** created by this story (deferred to later stories — do not pre-stub):
- `src/lib.rs`, `src/cli.rs`, `src/cli/*.rs` — Story 1.4
- `src/output.rs`, `src/exit_code.rs`, `src/error.rs` — Story 1.3
- `src/cache/*`, `src/sandbox/*`, `src/scan/*`, etc. — Stories 1.5+
- `.github/workflows/ci.yml` — Story 1.2
- `image/Dockerfile`, `tasks/`, `homebrew/lcrc.rb` — later stories per epic mapping

`.gitignore` already exists with bmad/Claude entries; **append** `target/` to it (and confirm `Cargo.lock` is **not** ignored — the default `cargo new` `.gitignore` ignores `Cargo.lock` for libraries; this is a binary, so it must be tracked).

### Testing requirements

No tests in this story — there is nothing implementation-bearing to test. The verification gate is `cargo build` + `cargo fmt --check` + `cargo clippy -- -D warnings` running cleanly (T5). Story 1.2 wires those same commands into CI; Story 1.3 lands the first integration test in `tests/cli_exit_codes.rs`. The standard test-module pattern for this codebase [Source: architecture.md#Project Structure & Boundaries] is:

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests { /* ... */ }
```

Document this in T2.3's comment but do not author any test code yet.

### Anti-patterns to avoid (LLM-developer pitfalls)

- **Do not** create a nested `lcrc/` directory — the repo root is the crate root.
- **Do not** add `[workspace]` to `Cargo.toml` (single binary crate per AR-26).
- **Do not** pre-create the full `src/` module tree from architecture §"Project Structure & Boundaries" — those are owned by later stories.
- **Do not** add `unsafe` blocks anywhere — `unsafe_code = "forbid"` will reject the build.
- **Do not** ignore `Cargo.lock` — this is a binary crate; the lockfile is committed.
- **Do not** invent dependencies outside the curated set in AR-4. If a transitive needs a feature flag the curated list doesn't enable, document the addition rather than silently expanding the surface.
- **Do not** silence clippy warnings with broad `#[allow(...)]` to make the build pass — if `pedantic` flags something, fix it or, in narrow well-justified cases, allow with a comment explaining why.
- **Do not** use `cargo new` literally inside the existing repo (it refuses non-empty dirs); author `Cargo.toml` and `src/main.rs` by hand.

### Project Structure Notes

The architecture ships an exhaustive `src/` tree [Source: architecture.md#Complete Project Directory Structure]. This story populates **only** the repo-root scaffold (manifest, toolchain pin, formatter config, stub `main.rs`). Subsequent Epic 1 stories incrementally add modules under `src/` — each story creates the files it owns and no others. Keeping that discipline tight means every story's diff is auditable and CI failures point at one story, not a soup of half-stubbed modules.

No conflicts with architecture detected. The single judgment call is the GGUF parser dep (T3.3); both options are explicitly sanctioned by AR-4 and the choice has no architectural downstream impact since `src/discovery/gguf.rs` is a Story 2.2 concern.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.1: Project scaffold with locked workspace lints]
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 1: Integration spine — one cell, one row, end-to-end]
- [Source: _bmad-output/planning-artifacts/architecture.md#Initialization]
- [Source: _bmad-output/planning-artifacts/architecture.md#Curated Dependencies (per integration surface)]
- [Source: _bmad-output/planning-artifacts/architecture.md#Architectural Decisions Provided by This Foundation]
- [Source: _bmad-output/planning-artifacts/architecture.md#Rust Style Baseline]
- [Source: _bmad-output/planning-artifacts/architecture.md#Module Organization]
- [Source: _bmad-output/planning-artifacts/architecture.md#Enforcement Summary — All AI Agents MUST]
- [Source: _bmad-output/planning-artifacts/architecture.md#Complete Project Directory Structure]
- [Source: _bmad-output/planning-artifacts/epics.md#Additional Requirements → AR-1, AR-4, AR-26, AR-27]

## Dev Agent Record

### Agent Model Used

claude-opus-4-7 (1M context) via Claude Code (`/bmad-dev-story`)

### Debug Log References

- `cargo build` → finished `dev` profile in 36.40s (cold; full dep tree resolved + compiled).
- `cargo build` (warm) → finished in 0.39s.
- `cargo fmt --check` → no output (passes).
- `cargo clippy --all-targets --all-features -- -D warnings` → No issues found.
- `cargo test` → `0 passed` (1 suite, 0.00s) — confirms test harness wired, no tests authored this story.
- Lint-active probe: temporarily inserted `let _ = Some(1u32).unwrap();` in `src/main.rs` and ran clippy — failed with `clippy::unwrap-used` (and `clippy::unnecessary-literal-unwrap`), confirming the `unwrap_used = "deny"` lint is wired through `[lints.clippy]`. Probe reverted.
- `git check-ignore Cargo.lock` → not ignored (binary crate; committed per architecture).

### Completion Notes List

- Crate manifest authored at the repo root (no nested `lcrc/` dir, no `[workspace]` table) per AR-26.
- Edition `2024`, `rust-version = "1.95"` set; local toolchain is rustc `1.95.0` (Homebrew), satisfying MSRV.
- `rust-toolchain.toml` pins `channel = "stable"` and pulls `rustfmt`/`clippy` components.
- Workspace lints baked in exactly per AC2: `unsafe_code = "forbid"`, `missing_docs = "warn"`, `pedantic = warn (priority -1)`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`. The test-only exemption pattern is documented as a comment block above the `[lints.rust]` table.
- Curated dependencies all present with required features wired (clap/derive, tokio/full, serde/derive, rusqlite/bundled, figment/toml+env, reqwest with `default-features = false` + `json`+`rustls-tls`, tracing-subscriber/env-filter+fmt). Conservative caret constraints; cargo resolved each to its current stable.
- T3.3 GGUF decision: **Option A** chosen — `ggus = "0.5"` (resolved to v0.5.1, `InfiniTensor/gguf`, MIT, last released 2025-06-05). Documented inline in `Cargo.toml`. Story 2.2 will adopt this crate rather than handwriting a parser.
- `nix` feature `signal` added explicitly — default features were trimmed in recent `nix` releases, and the architecture's downstream signal-handling work (FR27, FR45) needs the `signal` module exposed. This is a tightening within the curated surface, not an addition outside AR-4.
- `time` features `formatting` + `macros` enabled — required for RFC 3339 formatting (AR-23) that Story 1.4+ will exercise; adding now stabilizes the lockfile.
- `Cargo.lock` generated and tracked (binary crate per architecture §"Build & Distribution").
- `target/` appended to `.gitignore`.
- Stub `src/main.rs` carries a crate-level doc comment to satisfy `missing_docs = "warn"` for the binary target.

### File List

Created:
- `Cargo.toml`
- `Cargo.lock`
- `rust-toolchain.toml`
- `rustfmt.toml`
- `src/main.rs`

Modified:
- `.gitignore` (appended `# ── Rust ──` block with `target/`)

## Change Log

| Date       | Change                                                             | Author |
|------------|--------------------------------------------------------------------|--------|
| 2026-05-05 | Story 1.1 scaffold landed: Cargo.toml + lints + curated deps + lock + rustfmt + toolchain pin + stub main; gate (build/fmt/clippy/test) green. | Theop (via Claude Code dev-story) |
