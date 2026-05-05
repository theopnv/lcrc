# Sprint Change Proposal — MSRV Bump 1.85 → 1.95

**Date:** 2026-05-05
**Author:** Theop (via Correct Course workflow)
**Scope classification:** Minor
**Mode:** Batch
**Pinning semantics:** Floor bump (Option 1) — keep "current stable at v1 start (Rust X+)" pattern, shift floor to 1.95

---

## 1. Issue Summary

The project artifacts pin Rust MSRV to **1.85**, which corresponds to the Rust 2024 edition's first stable release (Feb 2025). The current latest stable Rust is **1.95.0** (released roughly one year later). The 1.85 floor was chosen as a conservative "edition 2024 minimum"; the intent of the architecture wording — *"current stable at v1 start"* — has drifted from reality because the v1 start window has moved.

Story 1.1's Dev Notes already acknowledge this drift explicitly (line 62): *"Local toolchain check: `rustc --version` already reports `1.95.0` on this machine, so build will succeed; the `rust-version = "1.85"` field documents the contract for downstream consumers."*

The discrepancy is now **discovered** and we want the contract to reflect actual development reality (1.95) so downstream consumers, CI, and AI implementation agents working on later stories all build against a consistent toolchain target.

## 2. Impact Analysis

### Epic Impact
- **Epic 1 (Foundations & Scaffolding)** — touched only at AR-1 line and Story 1.1's AC wording. No epic restructure required. No epic deletion/reordering.
- **Epics 2–N** — no impact. None reference the 1.85 floor.

### Story Impact
- **Story 1.1 (Project Scaffold)** — already **completed and committed** (commit `e0c8bc4`). The story's AC1, AC5, T1.3, Dev Notes, and Completion Notes reference 1.85. Updating the story file is **rewriting an already-shipped contract** to keep the implementation artifact consistent with the new MSRV. This is not re-doing work — `Cargo.toml` will need a one-line change and the artifact text edited to match.
- **Future stories (1.2+)** — none reference 1.85 yet. No proactive changes required.

### Artifact Conflicts
- `architecture.md` — 4 occurrences (lines 111, 178, 583, 846)
- `epics.md` — 3 occurrences (lines 163, 371, 387)
- `1-1-project-scaffold-with-locked-workspace-lints.md` — 6 occurrences (lines 15, 19, 25, 26, 62, 185)
- `Cargo.toml:5` — `rust-version = "1.85"` field
- `rust-toolchain.toml` — **no change** (already `channel = "stable"`, no version pin)
- `.github/workflows/` — does not yet exist (Story 1.2 territory); when authored, must use 1.95 floor in any matrix or `actions-rs/toolchain` selection

### Technical Impact
- **MSRV compatibility:** Raising the floor from 1.85 → 1.95 is **strictly more permissive** for dependencies. Every crate currently in `Cargo.toml` (clap 4, tokio 1, reqwest 0.12, bollard 0.18, rusqlite 0.32, askama 0.12, ggus 0.5, etc.) has an MSRV well below 1.95. No dependency needs to be downgraded, replaced, or version-pinned defensively.
- **Workspace lints (Story 1.1 deliverable):** `unsafe_code = "forbid"`, `missing_docs = "warn"`, `pedantic = "warn"` (priority -1), `unwrap_used/expect_used/panic = "deny"` — all of these lints are stable in clippy's interface. Clippy *adds* new pedantic lints across releases (1.86 → 1.95), but because `pedantic` is `warn` (not `deny`), new lint hits surface as warnings and do not break the build. Existing scaffolded code (`fn main()` stub) has zero lint surface to regress against. **No regression risk** to AC2/AC4 enforcement.
- **No build-graph changes:** `Cargo.lock` carries no MSRV info; no resolver bump needed.

### Sprint Plan Impact
- No change to story sequencing. No change to estimates. No new story added or removed.

## 3. Recommended Approach

**Direct Adjustment** — version-string update across artifacts and one Cargo.toml field. No rollback, no MSRV scope review. The change is mechanical and low-risk.

**Effort:** ~5 minutes of edits + `cargo build` verification.
**Risk:** Low — no dependency conflicts possible (raising floor), no lint regressions (only `warn`-level lints added across versions), no CI yet to update.
**Timeline impact:** Zero.

## 4. Detailed Change Proposals

> Wording rule throughout: keep the "current stable at v1 start (Rust X+)" idiom intact; only the version number changes. The `(at v1 start)` phrase is preserved because the architecture's intent is *"the stable Rust available when v1 was scaffolded"*, and that anchor is now 1.95, not 1.85.

### A. Code

#### Change A1 — `Cargo.toml`

**File:** `Cargo.toml`
**Section:** `[package]`

```diff
- rust-version = "1.85"
+ rust-version = "1.95"
```

**Rationale:** Single source of truth for the contract downstream consumers see via `cargo metadata`. Local toolchain is already 1.95.0 so build is unaffected.

#### No-op A2 — `rust-toolchain.toml`

**File:** `rust-toolchain.toml`
**Status:** No change. Currently `channel = "stable"` with no version pin — auto-resolves to whatever the toolchain manager (rustup/Homebrew) considers current stable. This is consistent with the floor-bump semantics chosen.

### B. Architecture (`_bmad-output/planning-artifacts/architecture.md`)

#### Change B1 — Line 111 (cargo new comment in scaffold instructions)

```diff
- # Set Rust edition 2024 in Cargo.toml; pin MSRV to current stable (Rust 1.85+ at v1 start).
+ # Set Rust edition 2024 in Cargo.toml; pin MSRV to current stable (Rust 1.95+ at v1 start).
```

#### Change B2 — Line 178 (Architectural Decisions § Language & Runtime)

```diff
- - Rust 2024 edition. MSRV pinned to current stable at v1 start (Rust 1.85+).
+ - Rust 2024 edition. MSRV pinned to current stable at v1 start (Rust 1.95+).
```

#### Change B3 — Line 583 (Rust Style Baseline § Edition)

```diff
- - **Edition:** Rust 2024. MSRV pinned in `Cargo.toml` to current stable at v1 start (1.85+).
+ - **Edition:** Rust 2024. MSRV pinned in `Cargo.toml` to current stable at v1 start (1.95+).
```

#### Change B4 — Line 846 (Project Directory Structure tree comment)

```diff
- ├── rust-toolchain.toml                # MSRV pin (Rust 1.85+ stable)
+ ├── rust-toolchain.toml                # MSRV pin (Rust 1.95+ stable)
```

### C. Epics (`_bmad-output/planning-artifacts/epics.md`)

#### Change C1 — Line 163 (AR-1)

```diff
- - AR-1: Implementation language is Rust, edition 2024, MSRV pinned to current stable at v1 start (Rust 1.85+); single static binary distribution.
+ - AR-1: Implementation language is Rust, edition 2024, MSRV pinned to current stable at v1 start (Rust 1.95+); single static binary distribution.
```

#### Change C2 — Line 371 (Story 1.1 Gherkin AC for build)

```diff
- **Then** the build succeeds on Rust 1.85+ stable with edition 2024.
+ **Then** the build succeeds on Rust 1.95+ stable with edition 2024.
```

#### Change C3 — Line 387 (Story 1.1 Gherkin AC for rust-toolchain.toml MSRV)

```diff
- **Then** it pins MSRV to current stable (Rust 1.85+).
+ **Then** it pins MSRV to current stable (Rust 1.95+).
```

### D. Story 1.1 Implementation Artifact (`_bmad-output/implementation-artifacts/1-1-project-scaffold-with-locked-workspace-lints.md`)

#### Change D1 — Line 15 (AC1)

```diff
- 1. **AC1 (build):** Given a fresh clone of the repo, when I run `cargo build`, then the build succeeds on Rust 1.85+ stable with edition 2024.
+ 1. **AC1 (build):** Given a fresh clone of the repo, when I run `cargo build`, then the build succeeds on Rust 1.95+ stable with edition 2024.
```

#### Change D2 — Line 19 (AC5)

```diff
- 5. **AC5 (MSRV pinned):** Given the project root, when I look at `rust-toolchain.toml`, then it pins MSRV to current stable (Rust 1.85+).
+ 5. **AC5 (MSRV pinned):** Given the project root, when I look at `rust-toolchain.toml`, then it pins MSRV to current stable (Rust 1.95+).
```

#### Change D3 — Line 25 (T1.2 task description)

```diff
-   - [x] T1.2 Author `rust-toolchain.toml` pinning `channel = "stable"` and `components = ["rustfmt", "clippy"]` (relying on the toolchain manager — rustup/Homebrew Rust — to provide stable ≥ 1.85).
+   - [x] T1.2 Author `rust-toolchain.toml` pinning `channel = "stable"` and `components = ["rustfmt", "clippy"]` (relying on the toolchain manager — rustup/Homebrew Rust — to provide stable ≥ 1.95).
```

#### Change D4 — Line 26 (T1.3 task description)

```diff
-   - [x] T1.3 Set `edition = "2024"` and `rust-version = "1.85"` in `[package]`.
+   - [x] T1.3 Set `edition = "2024"` and `rust-version = "1.95"` in `[package]`.
```

#### Change D5 — Line 62 (Dev Notes — Edition + MSRV)

```diff
- - **Edition + MSRV** [Source: architecture.md#Rust Style Baseline]: Rust 2024 edition, MSRV pinned to current stable at v1 start (Rust 1.85+). Local toolchain check: `rustc --version` already reports `1.95.0` on this machine, so build will succeed; the `rust-version = "1.85"` field documents the contract for downstream consumers.
+ - **Edition + MSRV** [Source: architecture.md#Rust Style Baseline]: Rust 2024 edition, MSRV pinned to current stable at v1 start (Rust 1.95+). Local toolchain `rustc --version` reports `1.95.0`; the `rust-version = "1.95"` field documents the contract for downstream consumers, matching the active stable channel.
```

**Rationale:** The original prose called out a *gap* between the documented MSRV (1.85) and the actual local toolchain (1.95). With the contract bumped to 1.95, the gap is closed and the prose can be tightened to reflect parity rather than divergence.

#### Change D6 — Line 185 (Completion Notes)

```diff
- - Edition `2024`, `rust-version = "1.85"` set; local toolchain is rustc `1.95.0` (Homebrew), satisfying MSRV.
+ - Edition `2024`, `rust-version = "1.95"` set; local toolchain is rustc `1.95.0` (Homebrew), satisfying MSRV.
```

### E. Future Story Authoring Note (no edit, but binding for upcoming work)

When **Story 1.2 (CI workflow)** is authored, the GitHub Actions Rust toolchain selection must use stable (which will resolve to ≥1.95 at runtime). If a matrix is introduced, the floor row must be `1.95`, not `1.85`. This proposal does not pre-create that artifact; it just binds the constraint for whoever runs `bmad-create-story` for 1.2.

## 5. Implementation Handoff

**Scope:** Minor — direct implementation by Developer agent (or by main-context Claude in a fresh window).

**Deliverables for the implementing agent:**
1. Apply all edits in §4 (changes A1, B1–B4, C1–C3, D1–D6).
2. Run `cargo build` to verify the MSRV bump compiles cleanly. Expected: success (no actual code change, only the `rust-version` metadata field shifts).
3. Run `cargo clippy --all-targets --all-features -- -D warnings` and `cargo fmt --check` to confirm Story 1.1's lint gates still pass.
4. Run `cargo --version`/`rustc --version` and confirm both report ≥1.95.
5. Commit with message: `chore: bump MSRV from 1.85 to 1.95 across artifacts`.

**Success criteria:**
- All ten artifact files (1 code + 3 docs) updated.
- `cargo build`, `cargo clippy ... -D warnings`, `cargo fmt --check` all green.
- Single commit with no other changes piggy-backed.

**Recipients:** Developer agent (or main Claude). No PM or Architect escalation required — semantics of architecture and epics are unchanged; only one numeric anchor moves.

---

## Appendix — Checklist Findings (compressed)

- **[x]** Triggering issue clearly described
- **[x]** Impact assessed across PRD (no impact), Epics (1 AR + 2 ACs), Architecture (4 lines), Stories (1 story file, 6 lines), Code (1 line)
- **[N/A]** PRD review — no MSRV mention in PRD
- **[N/A]** UX impact — no UX surface affected
- **[x]** Dependency MSRV audit — bumping floor is permissive; no deps require >1.95
- **[x]** Lint regression check — `pedantic = "warn"` is non-blocking; new clippy lints across 1.86–1.95 cannot fail the build
- **[x]** Sprint plan integrity — no story added/removed/reordered
- **[x]** Recovery / rollback path — trivial (revert the commit)
- **[x]** Future-work binding documented — Story 1.2 CI must be authored against 1.95
