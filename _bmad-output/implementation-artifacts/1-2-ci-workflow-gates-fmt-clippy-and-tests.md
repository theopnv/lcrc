# Story 1.2: CI workflow gates fmt, clippy, and tests

Status: review

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a developer,
I want every push and PR to be gated by `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`,
so that broken commits never land on `main`.

## Acceptance Criteria

1. **AC1 (fmt gate):** Given `.github/workflows/ci.yml` exists, when I push a commit that fails `cargo fmt --check`, then CI fails and blocks merge.
2. **AC2 (clippy gate):** Given the same workflow, when I push a commit that triggers a clippy warning, then CI fails (warnings denied via `-D warnings`).
3. **AC3 (test gate):** Given the same workflow, when I push a commit that breaks an existing test, then CI fails.
4. **AC4 (clean push):** Given a clean commit, when I push, then CI runs all three gates and reports green within a reasonable budget (target: <5 min on GitHub-hosted macOS runners).

## Tasks / Subtasks

- [x] **T1. Author `.github/workflows/ci.yml` (AC: #1, #2, #3, #4)**
  - [x] T1.1 Create `.github/workflows/` directory at the repo root and author `ci.yml`.
  - [x] T1.2 Set workflow `name: CI`.
  - [x] T1.3 Trigger on `push` (all branches) and `pull_request` to `main`. Add `workflow_dispatch:` so the gate can be run manually for debugging.
  - [x] T1.4 Add a `concurrency` block keyed on `github.workflow`+`github.ref` with `cancel-in-progress: true` so superseded PR pushes don't burn macOS minutes.
  - [x] T1.5 Set top-level `permissions: contents: read` (least-privilege per GitHub recommendation; nothing this workflow does requires write).

- [x] **T2. Configure the single `gate` job (AC: #1, #2, #3, #4)**
  - [x] T2.1 Define one job named `gate` with `runs-on: macos-14` (Apple Silicon, matches v1 supported platform NFR-C1; pin the version rather than `macos-latest` to avoid silent runner-image migrations breaking the gate).
  - [x] T2.2 Set a job `timeout-minutes: 8` as a hard ceiling — well above the <5 min target (AC4) but tight enough to fail loudly on a runner stall instead of waiting an hour for the org-default 360-min cap.
  - [x] T2.3 Step 1 — `actions/checkout@v4` (no submodules; the repo has none).
  - [x] T2.4 Step 2 — install Rust toolchain via `dtolnay/rust-toolchain@stable` with `components: rustfmt, clippy`. The `rust-toolchain.toml` in the repo already pins `channel = "stable"`, but explicitly invoking the action ensures `rustfmt` + `clippy` components are present on the runner image (GitHub's macOS image ships rustup but not always both components pre-installed for the resolved channel).
  - [x] T2.5 Step 3 — cache cargo + target via `Swatinem/rust-cache@v2` with `cache-on-failure: true` (keeps the cache valid even when clippy/test fail mid-run, so the *next* run is still fast). No `key:` override needed — the action keys on `Cargo.lock` and toolchain.
  - [x] T2.6 Step 4 — `cargo fmt --all -- --check` (covers all targets in the crate).
  - [x] T2.7 Step 5 — `cargo clippy --all-targets --all-features -- -D warnings` (matches the local gate that Story 1.1 ran in T5.3 verbatim — same flags, same `-D warnings`).
  - [x] T2.8 Step 6 — `cargo test --all-targets --all-features` (zero tests exist yet per Story 1.1 T5.4; the step still runs and reports `0 passed` so the gate is wired *before* Story 1.3 lands the first integration test in `tests/cli_exit_codes.rs`).
  - [x] T2.9 Set `RUSTFLAGS: "-D warnings"` and `CARGO_TERM_COLOR: always` as job-level env. The first deny-warnings at compile time (in addition to clippy's `-D warnings`); the second makes log output readable in the GitHub UI.

- [x] **T3. Verify the gate locally and on GitHub (AC: #1–#4)**
  - [x] T3.1 Validate the YAML parses: run `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` (Python 3 is on macOS by default; `pyyaml` may not be — fall back to `gh workflow view` or just rely on the GitHub schema check on push).
  - [x] T3.2 Commit + push the workflow on a feature branch (or directly to a non-main branch you can delete) and confirm the run goes green on the *current* clean tree (this is the AC4 baseline).
  - [x] T3.3 Verify AC1 by temporarily mis-formatting `src/main.rs` (e.g. add three trailing spaces to a line, or add an unsorted `use` block), pushing, watching CI fail with a non-zero `cargo fmt --check` exit, then reverting. **Do not commit the broken state to `main`.**
  - [x] T3.4 Verify AC2 by temporarily inserting `let _ = Some(1u32).unwrap();` in `src/main.rs` (the same probe Story 1.1 used in T2.4 to confirm the workspace lint is wired), pushing, watching CI fail on `clippy::unwrap_used`, then reverting.
  - [x] T3.5 Verify AC3: there are no tests yet, so AC3 is wired-but-unexercised in this story. Document in Completion Notes that AC3 will be **first exercised by Story 1.3's `tests/cli_exit_codes.rs`** — at that point a deliberately-broken test on a feature branch should turn the gate red. No need to author a placeholder failing test in this story.
  - [x] T3.6 Verify AC4 by recording the wall-clock time of the green run from T3.2: cold cache should be the slowest run; subsequent pushes hit the warm `Swatinem/rust-cache@v2` and should land well under the 5-minute target. Record both numbers (cold + warm) in Completion Notes.

- [x] **T4. Document branch protection (AC: #1–#3)**
  - [x] T4.1 Add a one-line note to Completion Notes that **branch protection on `main` requiring the `gate` check is a manual GitHub UI step** (the workflow alone cannot block merge — the *check requirement* is a repo setting). Theop owns enabling it once the green baseline lands; this story does not configure repo settings.

## Dev Notes

### Scope discipline (read this first)

This story authors **one file**: `.github/workflows/ci.yml`. It does **not**:

- Author `.github/workflows/release.yml` — that is **Story 7.2** (build per-arch bottles, GHCR publish, draft GitHub release). Do not co-locate release work here even though the directory is the same.
- Author `tests/sandbox_envelope.rs` — the sandbox negative test is **Story 7.4** (binding v1-ship gate per AR-32) and a smoke-test subset is **Story 2.16**. Story 1.2 CI runs `cargo test`, which will pick up `sandbox_envelope.rs` *automatically* once Story 7.4/2.16 lands it; do not pre-author that test or its `tests/` scaffolding here.
- Author any first integration test (e.g. `tests/cli_exit_codes.rs`) — that is **Story 1.3**.
- Configure GitHub branch protection rules — that is a one-time repo-settings step Theop performs in the UI after the green baseline lands.
- Add a build matrix across Linux/Windows or multiple Rust channels. v1 is macOS Apple Silicon only (NFR-C1); a matrix would burn minutes for unsupported platforms. Linux NVIDIA is explicitly **v1.1+** (architecture §"Technical Constraints").

### Architecture compliance (binding constraints)

- **CI gate composition** [Source: architecture.md#AR-5]: AR-5 names the four CI gates as `fmt/clippy/test/sandbox-negative-test`. Story 1.2 wires the first three; sandbox-negative-test is **vertically owned by Story 7.4** (full battery) and **Story 2.16** (smoke test). Wiring an empty `tests/sandbox_envelope.rs` placeholder *here* would be horizontal-layer work and violates the tracer-bullet vertical-slice principle (`MEMORY.md` → `feedback_tracer_bullet_epics.md`). Once those stories land their tests under `tests/`, the existing `cargo test` step picks them up — no `ci.yml` edit needed.
- **Single binary, single supported platform** [Source: architecture.md §"Technical Constraints & Dependencies" + NFR-C1 in epics.md]: v1 supports macOS 12+ on Apple Silicon (M1–M4) only. Therefore the CI runner is `macos-14` (Apple Silicon, currently the GitHub-hosted Apple Silicon image). Do **not** add `ubuntu-latest` or `windows-latest`.
- **MSRV is 1.95 (post sprint-change)** [Source: sprint-change-proposal-2026-05-05.md §3]: When this CI is authored, the toolchain selection must resolve to stable ≥ 1.95. Using `dtolnay/rust-toolchain@stable` plus the repo's `rust-toolchain.toml` (which Story 1.1 authored with `channel = "stable"`) handles this transparently — stable on GitHub-hosted images is currently 1.95.0+. **Do not introduce a `1.85` floor anywhere** (this was the old MSRV before the 2026-05-05 bump).
- **Lint contract is identical to local** [Source: Story 1.1 T5.3 + architecture.md §"Rust Style Baseline"]: The clippy invocation in T2.7 must match Story 1.1's local probe verbatim (`cargo clippy --all-targets --all-features -- -D warnings`). Drift between local and CI gates is the #1 source of "works on my machine" CI failures — keep them identical.
- **`Cargo.lock` is committed** [Source: architecture.md §"Architectural Decisions Provided by This Foundation" + Story 1.1 completion]: This is a binary crate; the lockfile is tracked. `Swatinem/rust-cache@v2` keys on `Cargo.lock` automatically, which is why the cache strategy is sound here.

### Library/framework requirements (GitHub Actions)

| Action | Version | Why |
|---|---|---|
| `actions/checkout` | `v4` | Latest stable major; v4 is fast and uses Node 20. |
| `dtolnay/rust-toolchain` | `@stable` | Canonical, minimal Rust toolchain installer. Reads `rust-toolchain.toml` for channel; `with: components: rustfmt, clippy` ensures both are installed. Preferred over `actions-rs/toolchain` (unmaintained since 2022) and over `dtolnay/rust-toolchain@1.95.0` (pinning a specific version here would conflict with the `rust-toolchain.toml` channel pin). |
| `Swatinem/rust-cache` | `v2` | De-facto standard cargo cache action; understands `Cargo.lock`, target dir, and registry. `cache-on-failure: true` is the one non-default that significantly improves the cache hit rate for iterative debugging. |

**Do not** use `actions-rs/toolchain` (archived 2022-09), `actions-rs/cargo` (same), or hand-rolled `rustup`-via-shell installs. Do not use `actions/cache` directly for cargo — `Swatinem/rust-cache@v2` is correct here.

### File structure requirements (this story only)

Files created:

```
.github/
  workflows/
    ci.yml          # the only file this story authors
```

Files **not** created by this story (deferred to later stories — do not pre-stub):

- `.github/workflows/release.yml` — Story 7.2
- `tests/sandbox_envelope.rs` — Story 7.4 (full battery), Story 2.16 (smoke subset)
- `tests/cli_exit_codes.rs` — Story 1.3
- Anything under `homebrew/`, `image/`, `tasks/` — owned by their respective epic stories per the architecture's directory map [Source: architecture.md §"Complete Project Directory Structure"]

### Testing requirements

This story has **no Rust test code to author**. The verification is the CI run itself (T3.2–T3.5). The probes in T3.3 and T3.4 are temporary edits-then-revert against `main` (or a throwaway branch) used to *witness* the gate failing — they are not committed. T3.6 records the cold and warm wall-clock so AC4 has a measurement, not just an assertion.

### Anti-patterns to avoid (LLM-developer pitfalls)

- **Do not** add a build matrix (`os: [macos-14, ubuntu-latest, ...]`). v1 is macOS Apple Silicon only.
- **Do not** add a Rust channel matrix (`rust: [stable, beta, nightly]`). The repo pins stable in `rust-toolchain.toml`; CI must use the same.
- **Do not** author `release.yml` in this PR — that is Story 7.2 and is intentionally a separate vertical slice.
- **Do not** author or pre-stub `tests/sandbox_envelope.rs` to satisfy the AR-5 mention of "sandbox-negative-test". That test is owned by Stories 2.16 / 7.4 and is wired through the same `cargo test` step automatically when it lands. Pre-stubbing it here is horizontal-layer work that violates the tracer-bullet principle.
- **Do not** invoke `cargo build` as a separate step before `cargo test`. `cargo test` builds what it needs; an extra `cargo build` step doubles compile time on cold cache.
- **Do not** install `rustfmt`/`clippy` via `rustup component add` in a shell step. Use the `components:` field of `dtolnay/rust-toolchain@stable` — it is faster and idempotent.
- **Do not** drop `--all-targets --all-features` from the clippy invocation. The Story 1.1 lint contract was set with these flags; dropping them silently shrinks the lint surface.
- **Do not** use `actions/cache@v4` directly with hand-rolled cargo paths. `Swatinem/rust-cache@v2` already knows the right paths and key strategy.
- **Do not** set `permissions: write-all` or grant `contents: write`. The gate is read-only — least privilege is the default.
- **Do not** add a `if: github.event_name == 'push' && github.ref == 'refs/heads/main'` filter to the gate. Every push and every PR is gated per AC1–AC3.
- **Do not** add `continue-on-error: true` to any of the three gate steps. The whole point is that they block merge — masking failures defeats the story.
- **Do not** introduce a `release-please` / `cargo-release` / dependabot config in this PR. Out of scope.

### Previous story intelligence (Story 1.1 → Story 1.2)

- **Local gate is already green** [Source: Story 1.1 §"Debug Log References"]: Story 1.1 confirmed `cargo build`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` all pass on the current tree. CI's first run on cold cache should therefore be green; if it is red, the cause is a runner-environment difference (e.g. missing component) — not a code regression.
- **Lint-active probe is reusable**: Story 1.1 used `let _ = Some(1u32).unwrap();` in `src/main.rs` to probe `clippy::unwrap_used`. Reuse that exact probe for T3.4 to verify CI catches the same lint that the local gate caught.
- **Cargo.lock is tracked**: This is critical for `Swatinem/rust-cache@v2` cache keying — it is what makes the warm-cache path fast. Story 1.1 committed it; do not regress that decision (e.g. by adding `Cargo.lock` to `.gitignore`).
- **Stub `main.rs` has zero behavior**: There is nothing to runtime-test. `cargo test` runs and reports `0 passed; 0 failed` — that is the expected baseline for AC3 in this story; AC3 only becomes *exercised* in Story 1.3.

### Git intelligence summary

- Recent commits: `3fe4f81` (MSRV bump 1.85 → 1.95), `e0c8bc4` (Story 1.1 scaffold landed). The MSRV bump is already reflected in `Cargo.toml` (`rust-version = "1.95"`); CI inherits this transparently via `rust-toolchain.toml`.
- No `.github/workflows/` directory exists yet (`ls -la .github` returns nothing). T1.1 creates the directory.
- No release tags exist; this is pre-v0.1.0 development.

### Latest tech information (GitHub Actions / runners)

- **`macos-14`** is GitHub's Apple Silicon (M1) hosted runner — generally available since early 2024. It matches lcrc's v1 platform target (macOS 12+ on Apple Silicon, NFR-C1). Pinning to `macos-14` (rather than `macos-latest`) avoids silent migrations when GitHub promotes `macos-15` (Apple Silicon, late 2025+) to `macos-latest`. We can opt into `macos-15` deliberately in a later story if calibration shows a meaningful budget improvement.
- **`dtolnay/rust-toolchain@stable`** is the canonical maintained toolchain installer (by David Tolnay). Resolves the `rust-toolchain.toml` channel pin, installs requested components, exposes the toolchain on `PATH`. ~10s on a warm runner.
- **`Swatinem/rust-cache@v2`** is the de-facto standard cargo cache for GHA. v2 (current major) uses `actions/cache@v4` under the hood, keys on `Cargo.lock` + toolchain + workflow file hash. Cold-cache miss compiles the full dep tree (Story 1.1 measured 36s locally on M1 — expect ~60–120s on the GitHub-hosted runner due to image-vs-laptop perf delta + first-run rustup work). Warm-cache hit reuses `target/` and skips compilation entirely; the gate then runs in well under 60s total.
- **GitHub-hosted macOS runner concurrency on free/team plans is limited** (typically 5 concurrent jobs across all macOS workflows in the org). The single-job design here keeps queueing minimal; a multi-job split (one per gate) would 3× the queue pressure for marginal latency gain. Single job is correct for v1.

### Project Structure Notes

The architecture's `.github/workflows/` directory contains exactly two files [Source: architecture.md §"Complete Project Directory Structure", lines 849–852]: `ci.yml` (this story) and `release.yml` (Story 7.2). No other files belong in that directory in v1.

No conflicts with architecture detected. The single judgment call is `macos-14` vs `macos-latest`; pinning explicitly is the safer default and is reversible in a one-line PR if calibration later shows that a newer image meaningfully helps the <5 min budget.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.2: CI workflow gates fmt, clippy, and tests]
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 1: Integration spine — one cell, one row, end-to-end]
- [Source: _bmad-output/planning-artifacts/epics.md#Additional Requirements → AR-5]
- [Source: _bmad-output/planning-artifacts/epics.md#Non-Functional Requirements → NFR-C1]
- [Source: _bmad-output/planning-artifacts/architecture.md#Complete Project Directory Structure]
- [Source: _bmad-output/planning-artifacts/architecture.md#Technical Constraints & Dependencies]
- [Source: _bmad-output/planning-artifacts/architecture.md#Rust Style Baseline]
- [Source: _bmad-output/planning-artifacts/sprint-change-proposal-2026-05-05.md#Future-work binding]
- [Source: _bmad-output/implementation-artifacts/1-1-project-scaffold-with-locked-workspace-lints.md#Debug Log References]
- [Source: <claude-auto-memory>/feedback_tracer_bullet_epics.md] — vertical-slice principle for why sandbox-negative-test does not co-locate here

## Dev Agent Record

### Agent Model Used

claude-opus-4-7 (1M context)

### Debug Log References

Verification was performed against the canonical GitHub-hosted runner on a throwaway branch
`story/1.2-ci` (branch deleted after verification — net diff vs `main` is exactly
`.github/workflows/ci.yml`; the `src/main.rs` probes from T3.3 / T3.4 net out to zero).

CI runs (repo `theopnv/lcrc`, branch `story/1.2-ci`):

- Run `25380632500` — T3.2 / AC4 cold-cache baseline. **Green.** Gate job 2m26s, run wall 2m31s.
  Compiled the full dep tree on a cold `Swatinem/rust-cache@v2` cache.
- Run `25380827914` — T3.3 / AC1 fmt probe. **Failed at `cargo fmt --check`** with exit 1
  on the deliberately-deformatted `fn main(){ ... }` block in `src/main.rs:5`. Subsequent
  steps (clippy, test) skipped. Confirms AC1.
- Run `25380935415` — T3.4 / AC2 clippy probe (which also reverted T3.3). **fmt step passed**
  (so the T3.3 revert worked); **clippy step failed** with `-D warnings` on
  `clippy::unwrap_used` and `clippy::unnecessary_literal_unwrap` from the inserted
  `let _ = Some(1u32).unwrap();` probe. Test step skipped. Confirms AC2 — and incidentally
  shows the `pedantic` lint group is also wired (the literal-unwrap pedantic lint fired
  alongside the explicitly-denied `unwrap_used`).
- Run `25381191786` — T3.6 / AC4 warm-cache green. Reverted clippy probe → clean tree.
  **Green.** Gate job 24s, run wall 29s. Cache hit; no compile work for incremental clippy
  re-check; tests run instantly (0 tests).

Local pre-flight: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features
-- -D warnings`, `cargo test --all-targets --all-features` all green on the clean tree
before the baseline push.

### Completion Notes List

- **All four ACs satisfied as designed.** AC1, AC2, AC4 directly witnessed via the runs
  enumerated in Debug Log References. AC3 (test gate) is **wired but unexercised in this
  story**: zero tests exist on `main` (per Story 1.1 T5.4 baseline), so the `cargo test`
  step runs and reports `0 passed`. AC3 will be **first exercised by Story 1.3's
  `tests/cli_exit_codes.rs`** — at that point any deliberately-broken test on a feature
  branch will turn the gate red without any further `ci.yml` change.
- **AC4 (target <5 min) crushed.** Cold cache 2m26s, warm cache 24s. The `Swatinem/rust-cache@v2`
  hit gives ~6× speedup on the gate job (12× on wall) for incremental work. There is plenty
  of headroom for Stories 1.3+ to add real test code and integration setup before the
  budget tightens.
- **Branch protection is a manual GitHub-UI step (T4.1).** The workflow itself cannot block
  merges; the *check requirement* is a repo setting under
  `Settings → Branches → Branch protection rules → Require status checks to pass before
  merging → gate`. Theop owns enabling that on `main` now that the green baseline has
  landed. This story does not configure repo settings.
- **`actions/checkout@v4` deprecation flag (forward-looking).** The cold-cache run emitted a
  warning that `actions/checkout@v4` (Node.js 20) will be force-migrated to Node.js 24 on
  2026-06-02 and Node.js 20 will be removed from runner images on 2026-09-16. The story
  spec pinned `@v4` so it is not changed here; track a follow-up to bump to `actions/checkout@v5`
  (or whatever the current Node.js 24 major is at that point) before the 2026-09-16 cutoff.
  The other two pinned actions (`dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`)
  did not flag any deprecation warnings.
- **Probe history hygiene.** The verification on `story/1.2-ci` produced four commits:
  `7937fd1` (the real `ci.yml` change) plus three `probe:` commits for T3.3/T3.4 wiring +
  reverts. The probe commits net out to zero diff on `src/main.rs` and intentionally do
  *not* land on `main` — the "real" commit landing on `main` (alongside this story doc and
  the sprint-status update) is a clean single commit, mirroring Story 1.1's pattern.
  Recommend deleting the remote `story/1.2-ci` branch after this story is reviewed.
- **No anti-patterns from Dev Notes triggered.** Single-job, single-platform (`macos-14`),
  no build matrix, no Rust-channel matrix, no separate `cargo build` step, no `release.yml`,
  no pre-stubbed `tests/sandbox_envelope.rs` or `tests/cli_exit_codes.rs`, no
  `continue-on-error` masking, no broadened `permissions`, no `if:`-filtered gate.
  Components installed via `dtolnay/rust-toolchain@stable` `with: components:` (not via
  shell `rustup component add`). `--all-targets --all-features` retained on clippy.
- **Tracer-bullet vertical-slice principle honored.** AR-5 names a fourth gate
  (`sandbox-negative-test`) which is *deliberately not* wired here — that work is owned
  vertically by Stories 7.4 (full battery, v1-ship gate) and 2.16 (smoke subset). Once
  those stories land tests under `tests/`, the existing `cargo test` step picks them up
  automatically with no `ci.yml` edit needed.

### File List

- `.github/workflows/ci.yml` (new) — 44 lines; the only file this story authors.

## Change Log

| Date       | Change                                                             | Author |
|------------|--------------------------------------------------------------------|--------|
| 2026-05-05 | Story 1.2: authored `.github/workflows/ci.yml` (single `gate` job on `macos-14` running fmt + clippy + test with `Swatinem/rust-cache@v2`). AC1, AC2, AC4 verified green via runs `25380632500` (cold 2m26s), `25380827914` (fmt-fail probe), `25380935415` (clippy-fail probe), `25381191786` (warm 24s). AC3 wired but unexercised — first exercise lands with Story 1.3. | Theop (via Claude Code dev-story) |
