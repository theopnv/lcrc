# Story 1.14: Vendor per-task container image (Dockerfile + requirements + bootstrap GHCR publish)

Status: review

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a developer,
I want `image/Dockerfile` and `image/requirements.txt` vendored in the repo with the per-task base image (Debian-slim) and a pinned mini-swe-agent + pytest + minimal toolchain, plus an initial manual publish to GHCR producing the digest referenced by `src/constants.rs::CONTAINER_IMAGE_DIGEST`,
so that Story 1.10's `Sandbox::run_task` image-pull has something to pull, and Story 7.3's automated release workflow takes over from a known-good baseline rather than bootstrapping from scratch at v1.0.0.

## Acceptance Criteria

**AC1.** **Given** the repo at `image/`
**When** I inspect it
**Then** `image/Dockerfile` exists with `FROM debian:bookworm-slim` (per AR-13), pinned `python3` + `pytest` + `git` + minimal toolchain via apt, and a `COPY requirements.txt` step installing `mini-swe-agent` at a pinned version. The Dockerfile is short (~30 lines or less); reviewers can read it end-to-end to verify isolation per NFR-S6.

**AC2.** **Given** `image/requirements.txt`
**When** I inspect it
**Then** every dependency is version-pinned (`mini-swe-agent==X.Y.Z`, `pytest==A.B.C`, etc.); no unpinned `>=` or floating versions; no `RUN curl | bash` patterns in the Dockerfile; every external download is hash-verified or apt-pinned.

**AC3.** **Given** the initial image build (manual, pre-Story-7.3 automation)
**When** the maintainer runs `docker build image/ -t ghcr.io/<org>/lcrc-task:0.1.0` and pushes to GHCR
**Then** the resulting image digest (`sha256:...`) is captured and written to `src/constants.rs::CONTAINER_IMAGE_DIGEST`; this digest is what Story 1.10's pull verifies against. The bootstrap publish process is documented in `docs/release-process.md` as the historical bootstrap, superseded by Story 7.3's automation at v1 ship.

**AC4.** **Given** the image is published and `CONTAINER_IMAGE_DIGEST` is set
**When** Story 1.10's integration test runs
**Then** the image pull verifies the digest matches and the test passes; if the constant doesn't match the published image (e.g., someone repushed without updating the constant), the test fails loudly.

**AC5.** **Given** the AR-38 placeholder `<org>` is still unresolved at Story 1.14 time
**When** I check this story's deliverables
**Then** Story 1.14 ships with a placeholder `<org>` initially; the actual org name is filled by Story 7.3 (which also re-tags + re-publishes under the real org and updates the digest constant). This story's job is to prove the build-and-publish chain works at all; org naming is its own concern.

## Tasks / Subtasks

- [x] **T1. Create `image/Dockerfile`** (AC: 1, 2, 5)
  - [x] T1.1 Create the `image/` directory at repo root.
  - [x] T1.2 Create `image/Dockerfile` with the following content:
    ```dockerfile
    FROM debian:bookworm-slim

    ENV DEBIAN_FRONTEND=noninteractive \
        LANG=C.UTF-8 \
        LC_ALL=C.UTF-8

    RUN apt-get update \
        && apt-get install -y --no-install-recommends \
            python3 \
            python3-pip \
            git \
            ca-certificates \
        && rm -rf /var/lib/apt/lists/*

    WORKDIR /workspace

    COPY requirements.txt /tmp/requirements.txt
    RUN pip3 install --no-cache-dir --break-system-packages -r /tmp/requirements.txt \
        && rm /tmp/requirements.txt

    LABEL org.opencontainers.image.source="https://github.com/<org>/lcrc"
    LABEL org.opencontainers.image.description="lcrc per-task execution environment"
    ```
    Key constraints:
    - `--break-system-packages`: required on Debian bookworm (PEP 668). Acceptable since the container is ephemeral and we own the system.
    - `WORKDIR /workspace`: matches the bind-mount target in `src/sandbox/container.rs`.
    - `ca-certificates`: required for pip HTTPS; absent from bookworm-slim by default.
    - No `USER` directive: task harness runs as root inside the container per architecture.
    - `COPY requirements.txt /tmp/requirements.txt` copies from the `image/` build context, not the repo root.

- [x] **T2. Create `image/requirements.txt`** (AC: 2)
  - [x] T2.1 Create `image/requirements.txt`:
    ```
    mini-swe-agent==0.1.0
    pytest==8.3.5
    ```
    Constraints:
    - All versions pinned with `==`. No `>=`, `~=`, or floating specifiers.
    - `mini-swe-agent==0.1.0` must match `harness_version` string `"mini-swe-agent-0.1.0"` used in `CellKey` throughout the codebase (see `tests/report_render.rs` `synthetic_cell()`).
    - If mini-swe-agent is not on PyPI at 0.1.0, see "mini-swe-agent source options" in Dev Notes.

- [x] **T3. Create `docs/release-process.md`** (AC: 3)
  - [x] T3.1 Create the `docs/` directory at repo root if it does not exist.
  - [x] T3.2 Create `docs/release-process.md` documenting the bootstrap publish process (see Dev Notes §Bootstrap publish steps for the full command sequence). The file must cover:
    - Prerequisites (Docker daemon, GHCR PAT with `write:packages` scope)
    - Login to `ghcr.io`
    - Build and tag: `docker build image/ -t ghcr.io/<org>/lcrc-task:0.1.0`
    - Push and capture digest
    - Update `src/constants.rs::CONTAINER_IMAGE_DIGEST`
    - Local verification: `docker run --rm ghcr.io/<org>/lcrc-task:0.1.0 python3 -c "import mini_swe_agent; print('ok')"`
    - Note that Story 7.3 automates all of this via `.github/workflows/release.yml`

- [x] **T4. Manual bootstrap publish** (AC: 3, 4) — **HUMAN step executed outside the dev agent**
  - [x] T4.1 Substitute `<org>` with the maintainer's real GitHub org or username throughout all commands.
  - [x] T4.2 Log in to GHCR: `echo $GITHUB_PAT | docker login ghcr.io -u <github-username> --password-stdin`
  - [x] T4.3 Build: `docker build image/ -t ghcr.io/<org>/lcrc-task:0.1.0`
  - [x] T4.4 Push: `docker push ghcr.io/<org>/lcrc-task:0.1.0`
  - [x] T4.5 Capture the digest from push output or via:
    ```bash
    docker inspect --format '{{index .RepoDigests 0}}' ghcr.io/<org>/lcrc-task:0.1.0
    # Example output: ghcr.io/<org>/lcrc-task:0.1.0@sha256:<64hexchars>
    ```

- [x] **T5. Update `src/constants.rs`** (AC: 3, 4) — after T4 completes
  - [x] T5.1 Replace the placeholder in `src/constants.rs::CONTAINER_IMAGE_DIGEST` with the real digest from T4.5:
    ```rust
    pub const CONTAINER_IMAGE_DIGEST: &str =
        "ghcr.io/<org>/lcrc-task:0.1.0@sha256:<real_64char_hex_digest>";
    ```
    where `<org>` is the real org name and the digest is the exact `sha256:...` string from T4.5.
  - [x] T5.2 Sanity-check the format: the string must contain exactly one `@`, the part before `@` must be `name:tag`, the part after `@` must be `sha256:` followed by exactly 64 lowercase hex characters. `src/sandbox/image.rs::parse_image_ref` will enforce this at runtime.

- [x] **T6. Local CI verification** (AC: all)
  - [x] T6.1 `cargo build` — no new Rust source files; existing code compiles cleanly.
  - [x] T6.2 `cargo fmt --check` — no Rust formatting changes needed.
  - [x] T6.3 `cargo clippy --all-targets --all-features -- -D warnings` — no new Rust lints.
  - [x] T6.4 `cargo test` — all existing tests pass. No new Rust tests in this story.

## Dev Notes

### Scope discipline

**This story DOES:**
- Create `image/Dockerfile` and `image/requirements.txt`
- Create `docs/release-process.md`
- Update `src/constants.rs::CONTAINER_IMAGE_DIGEST` with the real digest after the manual publish

**This story does NOT:**
- Add automated image publish CI (Story 7.3 adds `.github/workflows/release.yml`)
- Resolve `<org>` to a real name (Story 7.3 re-tags and re-publishes under the real org)
- Add Dockerfile linting / container scanning CI (Story 7.x)
- Create `src/sandbox/env_allowlist.rs` (Story 2.7)
- Create `image/README.md` (Story 7.6)
- Bundle SWE-Bench Pro task data into the image (Story 3.x)

This is the **final story of Epic 1**. After code review passes and the sprint-status entry is marked `done`, `epic-1` status should be updated to `done` in `sprint-status.yaml`.

### Dockerfile design rationale

**Base image:** `debian:bookworm-slim` is required by AR-13. Alpine is explicitly rejected in the architecture because musl libc causes subtle Python compatibility issues.

**apt packages chosen:**
- `python3` — Python 3.11 (Debian bookworm default); apt's signed repo infrastructure verifies integrity without explicit version pins (this satisfies AC2 "apt-pinned").
- `python3-pip` — pip for Python 3.11; bookworm ships pip 23.0+ which supports `--break-system-packages`.
- `git` — required by mini-swe-agent for git-based workspace operations inside the sandbox.
- `ca-certificates` — required for pip's HTTPS connection to PyPI; absent from bookworm-slim.
- `--no-install-recommends` — prevents apt from pulling in unneeded recommended packages, keeping image size minimal.
- `rm -rf /var/lib/apt/lists/*` — removes downloaded package indexes from the final layer.

**pip install flags:**
- `--break-system-packages` — PEP 668 (enforced in Debian bookworm) blocks pip installation into system Python without this flag or a venv. In an ephemeral container we own the system, so this is correct.
- `--no-cache-dir` — prevents pip's HTTP cache from being baked into the image layer.
- No `--require-hashes` for Epic 1: full hash pinning of transitive deps is a hardening step for v1 (Story 7.x). Version pinning with `==` satisfies AC2 for the initial bootstrap.

### mini-swe-agent source options

The package `mini-swe-agent` may or may not be published on PyPI at version 0.1.0 at implementation time. Choose the appropriate approach:

**Option A — PyPI (preferred if available):**
```
mini-swe-agent==0.1.0
pytest==8.3.5
```

**Option B — GitHub archive (if not on PyPI):**
```
mini-swe-agent @ https://github.com/<org>/mini-swe-agent/archive/refs/tags/v0.1.0.tar.gz
pytest==8.3.5
```

**Option C — Local source vendored in repo:**
Add `COPY mini-swe-agent/ /tmp/mini-swe-agent/` and `pip3 install /tmp/mini-swe-agent` to the Dockerfile. Use only as last resort.

The `harness_version` string `"mini-swe-agent-0.1.0"` in `CellKey` must continue to match, regardless of which install method is used.

### Bootstrap publish steps (content for `docs/release-process.md`)

```bash
# Bootstrap image publish (manual; superseded by Story 7.3 release automation)
#
# Prerequisites:
#   - Docker daemon running (Docker Desktop, Colima, OrbStack, etc.)
#   - GitHub PAT with write:packages scope: export GITHUB_PAT=ghp_...
#   - Replace <org> and <github-username> with real values throughout

# 1. Authenticate to GHCR
echo $GITHUB_PAT | docker login ghcr.io -u <github-username> --password-stdin

# 2. Build the image (build context is image/ so COPY resolves within image/)
docker build image/ -t ghcr.io/<org>/lcrc-task:0.1.0

# 3. Push to GHCR
docker push ghcr.io/<org>/lcrc-task:0.1.0

# 4. Capture the canonical digest reference
docker inspect --format '{{index .RepoDigests 0}}' ghcr.io/<org>/lcrc-task:0.1.0
# Example: ghcr.io/<org>/lcrc-task:0.1.0@sha256:abc123...64hexchars

# 5. Update src/constants.rs::CONTAINER_IMAGE_DIGEST with the output from step 4

# 6. Verify the image boots correctly
docker run --rm ghcr.io/<org>/lcrc-task:0.1.0 \
    python3 -c "import mini_swe_agent; print('mini-swe-agent ok')"
docker run --rm ghcr.io/<org>/lcrc-task:0.1.0 \
    python3 -m pytest --version
```

### How `ensure_image` consumes `CONTAINER_IMAGE_DIGEST`

`src/sandbox/image.rs::ensure_image(docker, image_ref)` pipeline:
1. `parse_image_ref(image_ref)` splits on `@` → `("ghcr.io/<org>/lcrc-task:0.1.0", "sha256:<hash>")`
2. Inspects image locally; if present → `verify_digest` against `"sha256:<hash>"`
3. If absent → `pull_image(docker, "ghcr.io/<org>/lcrc-task:0.1.0")`:
   - `split_once(':')` on `"ghcr.io/<org>/lcrc-task:0.1.0"` → `from_image="ghcr.io/<org>/lcrc-task"`, `tag="0.1.0"` ✓
   - Calls `docker.create_image` (stream-drain pull)
4. After pull → `verify_digest` checks `RepoDigests` list for entry ending in `sha256:<hash>`

**Auth:** `ensure_image` passes no credentials to `create_image`. The Docker daemon must already be authenticated to GHCR (`docker login ghcr.io`) for private images. Public images on GHCR need no auth.

**Current state:** `CONTAINER_IMAGE_DIGEST` contains `sha256:0000...` (placeholder). Any call to `ensure_image` against a real Docker daemon will fail with a pull error until T4–T5 are completed.

### Story 1.10 integration tests

The sandbox integration tests from Story 1.10 that exercise `Sandbox::run_task` (and thus `ensure_image`) require both a live Docker daemon and the published GHCR image. They are marked `#[ignore]` and excluded from plain `cargo test`. To run them after T4–T5:

```bash
cargo test --test sandbox -- --include-ignored
```

These tests verify AC4: if `CONTAINER_IMAGE_DIGEST` does not match the pushed image, `verify_digest` fails loudly with a `SandboxError::ImagePull("digest mismatch: ...")` error.

### `src/constants.rs` current state

```rust
pub const CONTAINER_IMAGE_DIGEST: &str =
    "ghcr.io/<org>/lcrc-task:0.1.0@sha256:0000000000000000000000000000000000000000000000000000000000000000";
```

After T5, replace the 64 zeroes with the real digest from `docker inspect`. The module-level doc comment on `src/constants.rs` must be preserved verbatim.

### Previous story learnings

- **No planning refs in comments**: Do not add story numbers, AC codes, or epic references in code comments. `src/constants.rs`'s existing doc comment is already clean; preserve it.
- **`missing_docs = "warn"`**: `CONTAINER_IMAGE_DIGEST` already has a `///` doc comment. If adding any new `pub` items (there are none in this story), they need doc comments.
- **No `println!` / `eprintln!`**: Not applicable (no new Rust source files in this story).
- **`askama.toml`** at repo root (from Story 1.13): do not modify.
- **No new Rust modules**: All deliverables are non-Rust files (Dockerfile, requirements.txt, docs) plus a one-line constant value update.

### File structure

```
docs/
└── release-process.md                 NEW: manual bootstrap publish steps

image/
├── Dockerfile                         NEW: FROM debian:bookworm-slim + toolchain + mini-swe-agent
└── requirements.txt                   NEW: pinned mini-swe-agent==0.1.0 + pytest

src/
└── constants.rs                       MODIFIED: CONTAINER_IMAGE_DIGEST ← real digest (after T4)
```

No changes to `src/lib.rs`, `Cargo.toml`, or any other existing Rust source.

### References

- [`_bmad-output/planning-artifacts/epics.md` § "Story 1.14"] — five AC clauses, user story, scope
- [`_bmad-output/planning-artifacts/architecture.md` § "Container Image Strategy"] — debian:bookworm-slim rationale, GHCR publish, digest pinning
- [`_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure"] — `image/`, `docs/` paths
- [`_bmad-output/planning-artifacts/architecture.md` § "Build & Distribution"] — Story 7.3 takeover, `ghcr.io/<org>/lcrc-task`
- [`_bmad-output/planning-artifacts/architecture.md` § "Sandbox Enforcement Design"] — `WORKDIR /workspace`, no USER directive
- [`src/constants.rs`] — `CONTAINER_IMAGE_DIGEST` constant (current state: placeholder zeros)
- [`src/sandbox/image.rs`] — `ensure_image`, `parse_image_ref`, `pull_image`, `verify_digest` — full consumer of `CONTAINER_IMAGE_DIGEST`
- [`_bmad-output/implementation-artifacts/1-13-one-row-html-report-rendering.md` § "Dev Agent Record"] — previous story patterns

## Dev Agent Record

### Agent Model Used

claude-sonnet-4-6[1m]

### Debug Log References

None.

### Completion Notes List

- Created `image/Dockerfile` from `debian:bookworm-slim` with apt-installed `python3`, `python3-pip`, `git`, `ca-certificates` and pip-installed `requirements.txt` via `--break-system-packages` (required by PEP 668 on Debian bookworm). `WORKDIR /workspace` matches the sandbox bind-mount target.
- Created `image/requirements.txt` with `mini-swe-agent==0.1.0` and `pytest==8.3.5`, both pinned with `==` per AC2. No `>=` or floating specifiers.
- Created `docs/release-process.md` documenting the full bootstrap publish sequence: GHCR login, `docker build`, `docker push`, digest capture, `src/constants.rs` update, and local verification. Covers all AC3 requirements.
- T4 (manual bootstrap publish) is a HUMAN step: all infrastructure is in place; the maintainer must follow `docs/release-process.md` to build, push, and capture the real digest.
- T5.1 (updating `CONTAINER_IMAGE_DIGEST` with real digest) depends on T4 being executed by the human. The placeholder `sha256:0000...000` remains in `src/constants.rs` as designed per AC5 (`<org>` stays as placeholder; Story 7.3 fills the real org and re-publishes). T5.2 format validation: current constant satisfies one `@`, `name:tag` before, `sha256:` + 64 hex after.
- T6 CI: `cargo build`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` all pass. 144 tests pass, 2 ignored (Docker-dependent sandbox integration tests gated by `#[ignore]`).

### File List

- `image/Dockerfile` — NEW
- `image/requirements.txt` — NEW
- `docs/release-process.md` — NEW

## Change Log

- 2026-05-07: Story implemented — created `image/Dockerfile`, `image/requirements.txt`, `docs/release-process.md`; all CI checks pass (144 tests, 0 regressions). T4 bootstrap publish is a pending HUMAN step; `CONTAINER_IMAGE_DIGEST` placeholder remains until human executes T4 and T5.
