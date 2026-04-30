---
stepsCompleted: [1, 2, 3, 4, 5, 6, 7, 8]
inputDocuments:
  - _bmad-output/planning-artifacts/prd.md
  - _bmad-output/planning-artifacts/product-brief-lcrc.md
  - _bmad-output/planning-artifacts/product-brief-lcrc-distillate.md
  - _bmad-output/planning-artifacts/implementation-readiness-report-2026-04-30.md
  - _bmad-output/brainstorming/brainstorming-session-2026-04-29.md
workflowType: 'architecture'
project_name: 'lcrc'
user_name: 'Theop'
date: '2026-04-30'
lastStep: 8
status: 'complete'
completedAt: '2026-04-30'
---

# Architecture Decision Document — lcrc

_This document builds collaboratively through step-by-step discovery. Sections are appended as we work through each architectural decision together._

## Project Context Analysis

### Requirements Overview

**Functional Requirements:** 56 FRs across 6 categories.
- **Installation & First Run (FR1–FR6):** Homebrew install, zero-config first run, version self-attestation, empty-machine starter pack, canary in report header.
- **Model Discovery & Eligibility (FR7–FR12):** llama.cpp cache scan, format-agnostic `model_sha`, RAM × ctx fit gate, exclusion visibility, configurable extra dirs, `--model` filter.
- **Measurement Execution (FR13–FR23):** canary-first, mini-swe-agent subprocess inside per-task container, default-deny sandbox (no host FS, no network except localhost→llama-server, env-var allowlist), pre-flight runtime detection (exit 11), pinned container image, macOS-native perf metrics with graceful degrade, per-tier wall-clock caps, three-tier breadth-first scan extending the same cache.
- **Cache & Persistence (FR24–FR31):** cell-level cache keyed on `(machine_fingerprint, model_sha, backend_build, params)`; lookup-before-measure; resumability without flags; `lcrc verify --sample N` warn-on-drift; per-cell metadata (depth, timestamp, backend_build, lcrc version, harness/task version, perf metrics).
- **Reporting (FR32–FR43):** single self-contained HTML regenerated per-cell, screenshot-friendly canonical header, Wilson CIs on every row, fixed-enum templated badges (no LLM prose), depth-tier-per-row tagging, structural `low-confidence-CI` on every Quick row, `lcrc show` plain-text mirror, JSON output with `schema_version`.
- **CLI Surface, Configuration & Scripting (FR44–FR54):** non-interactive everywhere, semver-stable exit codes (0/1/2/3/4/5/10/11/12), stdout-results / stderr-progress, layered config (CLI > env > TOML > built-in), startup config validation, `scan.lock` single-writer concurrency, lock-free reads.

**Non-Functional Requirements:** 38 NFRs across 6 categories.
- **Performance (NFR-P1–P9):** Quick ≤25 min (target 15), Standard 1.5–3 h, Full ≤12 h overnight; cache lookup <100 ms; HTML regen <2 s; CLI startup <500 ms; container spin-up <5 s.
- **Reliability (NFR-R1–R8):** resumability across Ctrl-C/OOM/suspend/crash, atomic cell writes, cache durability across patch+minor upgrades, graceful degrade on perf and llama-server failures, idempotency, lock-file concurrency, container teardown on abort.
- **Security (NFR-S1–S7):** default-deny by structural construction, sandbox-violation visibility (badge + exit 2), container runtime as hard dep (no `--unsafe-no-sandbox`), single outbound destination, env-var allowlist, image pinning, no telemetry whatsoever.
- **Compatibility (NFR-C1–C5):** macOS 12+ Apple Silicon only; cache-key durable across OS patches and lcrc patches; vendored versions pinned; Linux NVIDIA must remain additive (not rewrite) for v1.1.
- **Observability (NFR-O1–O4):** streaming feedback per NFR-P8; disk-only state; sockets only for localhost→llama-server + container-runtime control; `--version` self-attestation includes lcrc + harness + task subset + container image + commit hash.
- **Integration (NFR-I1–I6):** llama-server runs on host (one server per cell or per model — architecture decides); mini-swe-agent runs inside per-task container; perf collection from host; rootless container preferred; Homebrew formula `depends_on` runtime; no required cloud/API/auth.

### Scale & Complexity

- **Primary domain:** scientific measurement framework / CLI tooling.
- **Complexity level:** medium — small CLI surface, methodology surface is rich but well-scoped.
- **Estimated architectural components:** 8–10 — model discovery, fit gate, scan orchestrator, sandbox/container manager, llama-server lifecycle manager, harness invocation, perf collector, cache (with versioning + atomic writes + lookup), HTML report renderer, CLI/config/exit-code layer.

### Technical Constraints & Dependencies

- **Platform:** macOS 12+ on Apple Silicon (M1–M4) only in v1; architecture must keep Linux NVIDIA additive for v1.1.
- **Backend:** llama.cpp / `llama-server` only (single-binary, pinned); MLX is a v1 lift-or-defer architecture decision (open Q8).
- **Harness:** mini-swe-agent vendored, run as subprocess **inside** the per-task container.
- **Tasks:** curated SWE-Bench Pro subset, vendored or pinned; static "most-informative-first" ordering shipped with the subset; lifecycle fallback plan owed (open Q4).
- **Container runtime:** hard dependency, no fallback. Choice between Colima / OrbStack / Docker Desktop / Lima deferred to architecture (open Q9). Per-task image pinned per lcrc release.
- **Distribution:** Homebrew formula; `depends_on` chosen runtime.
- **No network, no telemetry, no cloud, no API key, no external service.** The user's machine is the entire dependency graph.
- **Solo developer, no shipping deadline, no marketing/competitive-window framing** (project memory).

### Architecture-Deferred Decisions Carried from PRD

1. **Language / starter template:** Rust vs Python (foundational; drives everything from latency targets to vendoring shape to Homebrew formula).
2. Pass@1 vs pass@k semantics; per-tier wall-clock cap exact values; timeout-as-fail vs timeout-as-skip.
3. `backend_build` cache invalidation policy — whole-cache vs compatibility classifier.
4. Harness / task version representation in cache key — collapse into `backend_build`, add a fifth dimension, or scope by lcrc release version.
5. SWE-Bench Pro lifecycle fallback (licensing/redistribution risk + contamination risk).
6. macOS perf-collection privilege model — `powermetrics`+sudo-per-call vs signed launchd helper vs graceful-degrade-without-power.
7. Cache storage shape — SQLite + JSON blobs vs flat JSON-per-cell vs hybrid.
8. Run-resumability protocol details (cell-level independence makes the trivial answer work for free; stronger guarantees TBD).
9. MLX backend — lift into v1 if low-effort (`mlx_lm.server` shares OpenAI-compatible API surface), otherwise defer to v1.1.
10. Container-runtime selection (Colima / OrbStack / Docker Desktop / Lima).
11. Final TOML config keys + env var names.

### Cross-Cutting Concerns Identified

- **Cache key + cell identity** — every operation (discover, measure, report, verify, resume) touches it; central data model.
- **Sandbox envelope** — wraps every measurement; pre-flight refusal (exit 11) and per-task teardown on abort.
- **Atomicity & resumability** — cell-level independence + atomic writes; "Ctrl-C is safe" is a feature, not a bug-fix.
- **Versioning & pinning** — lcrc semver + mini-swe-agent pin + SWE-Bench Pro subset pin + container image pin, all surfaced in `--version` and per-cell metadata; how to fold harness/task version into the cache key is open.
- **Streaming vs disk state** — stderr discipline (TTY-aware, ETA ≤10s, suppressible) and disk-only (`$XDG_*`) state writes; sockets only for localhost→llama-server and container-runtime control.
- **Format-agnostic identity** — `model_sha` is content-hash; future-proofs for MLX without rearchitecting the data model.
- **Extensibility (constraints, not features):** Linux NVIDIA additive; MLX optional; custom evals factorable; adaptive depth must work on existing cell-level cache; multi-run reliability metric must be addable without rearchitecting.
- **Honesty surfaces** — Wilson CIs, fixed-enum badges, three-state canary header, cache age + `backend_build` inline. Templates only — no LLM prose anywhere in user-facing output.

## Starter Template / Foundation

### Primary Technology Domain

CLI tool / scientific measurement framework. No web framework, no SPA, no database ORM, no GraphQL, no daemon, no server. Single-binary distribution via Homebrew.

### Language: Rust

**Decision rationale (against alternatives):**

The single foundational choice. Evaluated Rust vs Python (the two viable candidates given solo-developer context, the existing Python ecosystem of the wrapped tools, and Theop's familiarity profile). Rust selected because:

1. **NFR-P7 latency budgets are written in a way that strongly favors Rust.** `lcrc --help`/`--version` <200 ms and `lcrc show` <500 ms for 1K cells are met by default with a Rust binary (cold start ~5–20 ms); Python CLIs start at 80–200 ms cold and reach 300–500 ms once SQLite, templating, and HTTP imports load. Achievable with PyOxidizer/import-defer effort, but Rust pays this for free.
2. **NFR-I5 Homebrew formula is cleaner with a single static Rust binary.** Python in Homebrew remains a known-painful packaging story (venv vs system Python, `pipx`-style install dance); Rust binaries ship as Homebrew bottles with no runtime conflicts.
3. **NFR-R1/R2 atomic cell writes and signal-safe resumability** (Ctrl-C / OOM / suspend / crash) are easier to verify in Rust's RAII + explicit error model than in Python's asyncio cancellation semantics.
4. **NFR-C5 Linux NVIDIA additive in v1.1** uses Rust's `#[cfg(target_os = "macos")]` conditional compilation cleanly; Python platform shims work but are messier.
5. **Container as language-isolation boundary.** The Python ecosystem we depend on (mini-swe-agent, SWE-Bench Pro tasks) lives entirely inside the per-task container — the host orchestrator never imports Python, never depends on the user's interpreter version, never navigates `pyenv`/`uv`/`venv`. Container brings its own Python. This means the host language is genuinely free to be Rust without integration penalty.

**Author familiarity:** Theop has C / C++ / C# background — Rust transfer is good (RAII, ownership, async/await all map cleanly). The borrow checker is the single new concept; with a C/C++ mental model, framing it as "the compiler enforces the rules you already follow manually" is the path of least resistance. v1 is also a deliberate Rust-learning opportunity for Theop; no shipping deadline (per project memory) accommodates this.

### Initialization

There is no "create-lcrc-app" — the foundation is vanilla `cargo`:

```bash
cargo new --bin lcrc
cd lcrc
# Set Rust edition 2024 in Cargo.toml; pin MSRV to current stable (Rust 1.85+ at v1 start).
```

This is the first implementation story.

### Curated Dependencies (per integration surface)

Locked at v1 start; revisited in Step 6 (source tree) and Step 4 (where storage shape decides between rusqlite vs serde_json-only).

**CLI surface:**
- `clap` (v4, derive feature) — argument parsing, subcommands, help generation. De-facto standard.
- `etcetera` — XDG base directory resolution (`$XDG_DATA_HOME`, `$XDG_CONFIG_HOME`, `$XDG_STATE_HOME`).
- `is-terminal` + `nu-ansi-term` — TTY detection and color (per FR47, NFR-O1). `indicatif` for streaming progress with ETA (per NFR-P8) on TTY; plain stderr lines on non-TTY.

**Configuration:**
- `serde` + `serde_derive` — serialization frame.
- `toml` — TOML parsing for `~/.config/lcrc/config.toml` (FR49).
- `figment` — layered config (CLI > env > TOML > defaults, per FR50). Validates on startup; failures map to exit code 10 (FR51).

**Async runtime + HTTP:**
- `tokio` (full features) — async runtime for `llama-server` HTTP polling, container management, streaming progress, signal handling.
- `reqwest` — HTTP client to `llama-server` (built on `hyper`+`tokio`).

**Container integration (FR16, FR17a, NFR-I4):**
- `bollard` — async Docker Engine API client. Works against Docker Desktop, Colima, OrbStack, and Lima (all expose the Docker Engine API on a Unix socket). Specific runtime selection is Step 4 (open Q9); the API surface is shared.

**Cache storage (deferred to Step 4 — open Q7 cache shape):**
- If SQLite path: `rusqlite` (sync, bundled SQLite). Simple, mature.
- If flat-JSON path: `serde_json` only.
- Either way: `sha2` for `model_sha` (SHA-256 content hash, format-agnostic per FR8).
- `tempfile` + atomic rename pattern for FR27 / NFR-R2 atomicity.
- `fs2` (or `fd-lock`) for the `scan.lock` file (FR52).

**HTML report (FR32, FR33, FR34):**
- `askama` — compile-time, type-safe, Jinja-like templating. Generates the single self-contained HTML; assets inlined at template time.

**Process & subprocess control:**
- `tokio::process` — spawn `llama-server`, container processes, perf-collection helpers.
- `nix` — Unix signal handling (SIGINT for FR27 resumability, SIGTERM for FR45 exit code 3).

**macOS perf collection (deferred to Step 4 — open Q6 privilege model):**
- v1-likely: shell out to `powermetrics` / `ioreg` via `tokio::process` (privilege model TBD).
- If we go native: `mach2`, `libproc`, `sysctl`, `objc2` for Apple `IOReport` framework. Decision in Step 4.

**Statistics:**
- Wilson-score CI: hand-written (~10 lines, no dependency needed) or via `statrs`. Hand-written keeps the dep tree minimal and the formula reviewable inline.

**Errors & logging:**
- `anyhow` — application-level error propagation (`Result<T>`).
- `thiserror` — typed errors at module boundaries where exit-code mapping matters (`InvalidConfig`, `PreflightFailed`, `SandboxViolation`, `CanaryFailed`, etc., one variant per documented exit code in FR45).
- `tracing` + `tracing-subscriber` — structured logging. By default emits to stderr per stderr discipline (FR46); user redirects to a file themselves (no `--log-file` in v1).

**GGUF parsing (FR8 model_sha + metadata):**
- Either the `ggus` crate (verify currency in Step 6) or a small handwritten parser. Format is documented and stable.

**Time / dates (FR31, FR34, report timestamps, scan history filenames):**
- `time` (RFC 3339 ISO 8601 formatting; modern, well-maintained).

**Testing:**
- `cargo test` (built-in) for unit tests.
- `assert_cmd` + `predicates` for CLI integration tests (exit-code coverage per FR45 is structural).
- `insta` for snapshot testing the HTML report output.
- `proptest` (optional) for cache-key-property tests if needed.

### Architectural Decisions Provided by This Foundation

**Language & Runtime:**
- Rust 2024 edition. MSRV pinned to current stable at v1 start (Rust 1.85+).
- Single static binary; no runtime dependency on Python, Node, or any other interpreter on the host.
- Tokio as the single async runtime; no mixed runtime story.

**Build & Distribution:**
- `cargo build --release` produces `target/release/lcrc`.
- Homebrew formula publishes a bottle (pre-built binary per Apple Silicon arch); fallback path is `cargo install`.
- The formula `depends_on` the chosen container runtime (decided Step 4) per NFR-I5.

**Code Organization (sketched here, locked in Step 6):**
- Cargo workspace not needed in v1 (single binary). Modules are organized inside a single `lcrc` crate; layout sketched in Step 6.

**Development Experience:**
- `cargo check` / `cargo clippy --all-targets --all-features -D warnings` / `cargo fmt --check` baseline.
- Rust-analyzer for IDE / editor support.
- `cargo nextest` (optional, faster test runner) — defer the call.

### What This Foundation Does NOT Provide (and won't add in v1)

- No web framework (no axum, no warp, no actix). lcrc is not a server.
- No database ORM (no diesel, no sea-orm). The data model is small and explicit; `rusqlite`/`serde_json` is enough.
- No GraphQL, no gRPC, no protobuf. Single-machine, single-user, no IPC needs.
- No SPA / WASM / frontend tooling. The HTML report is a single static file; no JS-build toolchain.
- No telemetry / analytics / crash reporting (NFR-S7, NFR-O3 — hard line).

**Note:** Project initialization (`cargo new --bin lcrc` + `Cargo.toml` populated with the curated dependencies above) is the first implementation story.

## Core Architectural Decisions

This section resolves the eleven architecture-deferred decisions from the PRD plus the foundational language choice. Decisions are organized by area; each decision states the option chosen, the rationale, and any v1.1+ extensibility implications.

### Decision Priority Summary

**Critical (block implementation):**
- Storage shape (SQLite, single file)
- Cell schema and PK composition (incl. harness/task version dimensions)
- Container runtime detection model (any Docker-Engine-API-compatible socket; Podman as packaged default)
- Sandbox enforcement design (custom internal Docker network + workspace-only bind + env allowlist)
- `Backend` trait abstraction (only `LlamaCppBackend` impl in v1)
- `TaskSource` trait abstraction (only `SweBenchProSource` impl in v1)
- llama-server lifecycle granularity (per `(model, params)` group)

**Important (shape architecture):**
- Pass@1 semantics with cell schema supporting v1.1+ pass@k extension
- Per-tier wall-clock cap mechanism + working-assumption defaults; final values calibrated before v1 ship
- Timeout = fail with `task-timeout` badge
- Perf collection model (graceful-degrade-without-power for v1)
- backend_build invalidation = structural re-measurement (no compatibility classifier)
- Run resumability = all-or-nothing per cell, atomic SQLite transaction at completion
- Final TOML config keys + env var convention
- Vendoring layout (tasks bundled in repo; container image published to GHCR with digest pinning)
- Apache-2.0 license

**Deferred to v1.1+ (architectural slot reserved, not implemented):**
- MLX backend (`Backend` trait makes this a focused additive change)
- Linux NVIDIA + Windows platform support (`#[cfg]` factoring; platform-specific runtime defaults)
- Signed launchd helper for power metrics (cells already store `power_watts` nullable)
- Custom-eval extension surface (`TaskSource` trait makes this a focused additive change)
- Adaptive depth (Wilson-CI-driven early stop) — cell-level cache supports it natively
- Pass@k multi-trial scoring — adds `trial_id` to PK; no rearchitecture
- Multi-run reliability metric
- `lcrc gc`, `lcrc doctor`, default-no-args wizard mode
- Backend-build compatibility classifier (currently structural re-measurement; classifier could optimize later if it bites)

### Cache Architecture

**Storage shape (PRD Q2):** SQLite single file at `{paths.cache_dir}/lcrc.db`.

- Trivially meets NFR-P5 (cache lookup <100 ms for 10K cells).
- Atomic writes via SQLite transactions satisfy NFR-R2 cleanly.
- Concurrent reads during scan via SQLite WAL mode satisfy NFR-R7 out-of-box.
- Schema migration discipline via `PRAGMA user_version` + numbered migration scripts; satisfies NFR-R3 (cache durable across patch + minor lcrc upgrades; major may require explicit migration).
- The HTML report is the human-facing surface; the cache is the machine surface — JSON-per-file's "human-readable" appeal is irrelevant here.

**Cell schema (`cells` table):**

```sql
CREATE TABLE cells (
    -- identity (PK)
    machine_fingerprint  TEXT NOT NULL,  -- e.g. "M1Pro-32GB-14gpu"
    model_sha            TEXT NOT NULL,  -- SHA-256 hex of model file (format-agnostic)
    backend_build        TEXT NOT NULL,  -- llama.cpp commit + version string
    params_hash          TEXT NOT NULL,  -- SHA-256 of canonical(ctx, temp, threads, n_gpu_layers)
    task_id              TEXT NOT NULL,  -- e.g. "swe-bench-pro:django-1234"
    harness_version      TEXT NOT NULL,  -- vendored mini-swe-agent version
    task_subset_version  TEXT NOT NULL,  -- vendored SWE-Bench Pro subset version

    -- attributes (not part of identity)
    container_image_id   TEXT NOT NULL,  -- per FR17b
    lcrc_version         TEXT NOT NULL,
    depth_tier           TEXT NOT NULL,  -- 'quick' | 'standard' | 'full'
    scan_timestamp       TEXT NOT NULL,  -- ISO 8601
    pass                 INTEGER NOT NULL,  -- 0 or 1
    duration_seconds     REAL,
    tokens_per_sec       REAL,            -- nullable per NFR-R4 graceful degrade
    ttft_seconds         REAL,            -- nullable
    peak_rss_bytes       INTEGER,         -- nullable
    power_watts          REAL,            -- v1: always NULL; v1.1+ launchd helper populates
    thermal_state        TEXT,            -- nullable; sufficient for `thermal-throttled` badge
    badges               TEXT,            -- JSON array of badge strings

    PRIMARY KEY (machine_fingerprint, model_sha, backend_build,
                 params_hash, task_id, harness_version, task_subset_version)
);
```

**Harness/task version representation in cache key (PRD Q4):** `harness_version` and `task_subset_version` are separate PK dimensions. A mini-swe-agent or task-subset upgrade creates new cells on the next scan; old cells stay in cache marked with the old version (visible via `lcrc show --all`, FR42). Lcrc patch versions are NOT in the key, so NFR-R3 (cache durable across patches) holds.

**backend_build invalidation policy (PRD Q3):** No compatibility classifier in v1. Structural re-measurement: `brew upgrade llama.cpp` produces a new `backend_build` string; next scan finds no matching cells for the affected `(model, backend_build)` combos and measures fresh; old cells remain accessible via `--all`. Classifiers are bug factories; the cell-cache architecture makes this cheap natively. Compatibility classifier remains a v1.1+ candidate if structural re-measurement turns out to bite.

**Run resumability protocol (PRD Q9):** All-or-nothing per cell. Atomic SQLite transaction at cell completion. SIGINT / OOM / crash mid-cell = nothing persisted for that cell; next scan re-measures it. A v1 cell = one task = ~minutes of work; nothing meaningful to checkpoint mid-task.

SIGINT teardown order:
1. Cancel pending cell measurements.
2. Tear down running container (best-effort, per NFR-R8).
3. Persist nothing about in-progress cell.
4. Release `scan.lock`.
5. Exit code 3 (FR45).

### Sandbox & Container Runtime

**Container runtime selection (PRD Q10):** Runtime-agnostic detection; **Podman** as the packaged default.

- lcrc never names a runtime in code — it detects whatever Docker-Engine-API-compatible socket is reachable.
- Pre-flight (FR17a) probe order: `LCRC_RUNTIME_DOCKER_HOST` → `DOCKER_HOST` → `/var/run/docker.sock` → Podman default socket → exit 11 with setup instructions.
- `bollard` talks to whatever socket is reachable.
- Homebrew formula `depends_on "podman"` for users without a runtime.
- Users with their own runtime (Colima, OrbStack, Docker Desktop, Lima, Rancher Desktop) work transparently — no second runtime forced on them.
- Podman chosen as packaged default for cross-platform consistency: same recommended runtime on macOS, Linux, and v1.1+ Windows. Apache-2.0, rootless-by-default (security match for our threat model), Red Hat backing.

**Container image strategy:**
- Pre-built per lcrc release, published to `ghcr.io/<org>/lcrc-task:<lcrc-version>`, **pinned by digest** in `src/constants.rs`.
- Dockerfile vendored at `image/Dockerfile` for reviewer verification (per Journey 5 trust audit).
- Base: **Debian-slim** — Alpine's musl libc has a track record of subtle Python issues we don't want to debug.
- First-run pulls the image (one-time, similar to model downloads); subsequent scans warm-cache hit. NFR-P9 (<5 s spin-up) met after first pull.
- Container image identifier (digest) recorded in cell metadata per FR17b.

**Sandbox enforcement design (NFR-S1–S6):**

*Network design:*
- llama-server runs **on the host** (per NFR-I1, to avoid per-task model reload).
- Each scan creates a custom Docker network with **no DNS resolver and no default route to the internet** (Docker `--internal` flag with controlled gateway pinhole, OR custom bridge with explicit egress restriction — concrete mechanism picked at implementation time).
- llama-server reachable from container via `host.docker.internal` (Podman/Colima/Docker Desktop all provide this) at the per-(model,params) port.
- Acceptance check #9 (sandbox negative test) validates the *property* regardless of mechanism.

*Filesystem design:*
- `docker run` with no `-v` flags except `-v /tmp/lcrc-task-<uuid>/workspace:/workspace:rw`.
- Container working dir = `/workspace`. Nothing else from host visible.

*Env design:*
- `--env-file` containing only the documented allowlist (`PATH`, `LANG`, `LC_ALL`, task-specific test-runner config). NEVER `--env` of any host var.
- Credential-bearing host env vars (`AWS_*`, `GH_*`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `HF_TOKEN`, etc.) are absent inside container by construction, not by denylist.

**Container lifecycle (NFR-R8):**
- One ephemeral container per task (per cell).
- All containers and networks tagged `lcrc-scan-id=<uuid>` label so a backstop GC pass (Podman/Docker's own orphan cleanup; or v1.1+ `lcrc gc`) can find leftovers from aborted scans.
- On SIGINT / scan abort: bollard `remove_container` with `force=true`; custom network removed; backstop labels for any missed.

### Measurement Methodology

**Pass@1 vs pass@k (PRD Q7 part 1):** **Pass@1 in v1.**

- Quick (1 task/model) makes pass@k structurally meaningless at k=1.
- Standard (3–5 tasks/model) — better to expand task coverage than repeat the same task for variance.
- Aligns with Scale SEAL's reported methodology on SWE-Bench Pro.
- Wilson CI computes on binary per-task outcomes — clean.
- Cell schema supports v1.1+ pass@k by adding `trial_id` to the PK; old cells = `trial_id=0`. No rearchitecture.

**Multi-run reliability** (eval-variance-across-repeat-scans) is a *separate* concept from pass@k and stays explicitly v1.1+.

**Per-tier wall-clock cap mechanism (PRD Q7 part 2 + Q11 part):** Lock the *mechanism*; calibrate exact values on the reference rig before v1 ship.

- Mechanism: per-tier cap configurable in TOML (`scan.{quick,standard,full}_task_timeout`); capped tasks badged + non-blocking per FR19; timeout-as-fail per next decision.
- Working-assumption defaults: Quick 600s, Standard 900s, Full 1800s, Canary 120s, Server-startup 60s.
- v1-ship gating: if empirical Quick on M1 Pro 32GB / 5-model exceeds the 25-min ceiling (acceptance check #1), the cap tightens before the budget loosens (PRD: "Quick must remain Quick").

**Timeout-as-fail vs timeout-as-skip:** **Timeout = fail, recorded with a `task-timeout` badge.**

- Adds `task-timeout` to the FR36 templated-badge enum (was implicit in FR19; now explicit).
- A model that wedges past the cap is functionally a fail for an agentic task; user shouldn't switch to it.
- Skip would leave a hole in the leaderboard, NULL the `pass` column, and complicate Wilson CI computation.
- The badge distinguishes "wedged" from "agent finished, gave wrong answer" — different signal to the user.

**SWE-Bench Pro lifecycle fallback (PRD Q5):** Vendor the curated subset behind a `TaskSource` trait.

```rust
trait TaskSource {
    fn name(&self) -> &str;                        // "swe-bench-pro"
    fn version(&self) -> &str;                     // bundled subset version
    fn list_tasks(&self) -> Vec<TaskId>;           // ordered most-informative-first
    fn load_task(&self, id: &TaskId) -> TaskSpec;
    fn evaluate(&self, id: &TaskId, workspace: &Path) -> TaskOutcome;
}
```

- v1 ships exactly one impl: `SweBenchProSource`.
- Curated subset bundled at lcrc release time in `tasks/swe-bench-pro/`; pinned by content hash; identifier in cell metadata.
- The `task_subset_version` PK dimension accommodates multiple sources implicitly (source-name + version forms the value).
- v1.1+ custom-eval persona (Journey 6) implements this trait for their own tasks.
- **Documented fallback contingency:** if Pro becomes unusable mid-v1-lifecycle, lcrc ships a v1.x release with an alternative `TaskSource` (candidates: SWE-Bench Lite-with-Verified-cleanup, LiveCodeBench, Multi-SWE-Bench-mini). Interface guarantees the swap doesn't rearchitect anything else.
- **Pre-v1 owed:** confirm Scale's redistribution license for the curated subset. If restricted, fall back to install-time pull with documented brittleness; the architecture doesn't change.

### Backend & Performance

**MLX backend lift-or-defer (PRD Q9):** **Defer MLX to v1.1; lock the `Backend` trait abstraction in v1.**

```rust
trait Backend {
    fn name(&self) -> &str;
    fn version(&self) -> String;            // contributes to backend_build
    fn discover_models(&self) -> Vec<ModelRef>;
    fn estimate_memory(&self, model: &ModelRef, params: &Params) -> ByteSize;
    fn start_server(&self, model: &ModelRef, params: &Params) -> Result<ServerHandle>;
}
```

- v1 ships exactly one impl: `LlamaCppBackend`.
- v1.1 adds `MlxBackend` as a focused additive change (~few hundred lines: subprocess + HTTP + `.safetensors` discovery + memory estimation), exactly satisfying PRD's "must not paint into a corner" constraint.
- Honors brief/brainstorming's "ruthless v1 cuts to one of everything" while keeping the door open structurally.

**macOS perf collection privilege model (PRD Q6):** **Graceful-degrade-without-power for v1; signed launchd helper deferred to v1.1+.**

What v1 collects per cell:
- `tokens_per_sec` — from llama-server's API (it reports this directly)
- `ttft_seconds` — timing the first token in the HTTP response
- `peak_rss_bytes` — `proc_pid_info` polling on llama-server PID (no privilege)
- `thermal_state` — `IOReport` framework for state classification (no privilege; sufficient for the `thermal-throttled` badge)
- `power_watts` — **NULL in v1** (NFR-R4 graceful degrade); v1.1+ launchd helper populates

Cell schema columns stay (`power_watts REAL` nullable); just NULL until v1.1+ helper lands. Zero schema migration when the helper ships.

Honors FR44 (no interactive prompts); no install-time setup friction.

**llama-server lifecycle granularity (NFR-I1):** **One server per `(model, params)` combo.**

Lifecycle protocol:
1. Plan: enumerate cells to measure, group by `(model_sha, params_hash)`.
2. For each group:
   1. Start `llama-server --model <gguf> --ctx <N> ...` on host, bind to a free localhost port.
   2. Health-check: poll `/health` until ready (timeout = `scan.server_startup_timeout`; on failure, badge `server-startup-failure` on affected cells).
   3. For each task in group: spawn ephemeral container connected to the per-task network → container reaches `host.docker.internal:<port>` → mini-swe-agent runs the agentic loop → capture pass/fail + perf → tear down container → reset llama-server KV cache via API.
   4. Stop llama-server, free port.
3. Per NFR-R5, llama-server crashes mid-group: badge `server-crashed` on affected cells, restart server, continue.

Server is on **host** (per NFR-I1); per-task container reaches it via `host.docker.internal` on the constrained per-task network.

Saves model-load cost: 5 model loads regardless of task count, vs N loads with per-cell server.

### Distribution, Configuration & Versioning

**TOML config schema (PRD Q11):**

```toml
[paths]
cache_dir  = "~/.local/share/lcrc/cache"     # SQLite db
report_dir = "~/.local/share/lcrc/reports"
state_dir  = "~/.local/state/lcrc"           # scan.lock

[discovery]
extra_model_dirs = []                         # additional GGUF cache paths

[scan]
default_depth          = "quick"              # quick | standard | full
quick_task_timeout     = 600                  # seconds; calibrated before v1 ship
standard_task_timeout  = 900
full_task_timeout      = 1800
canary_task_timeout    = 120
server_startup_timeout = 60

[runtime]
docker_host = ""                              # empty = auto-detect; explicit overrides

[backend]
llama_server_path = ""                        # empty = $PATH lookup
```

**Env var convention:** `LCRC_<SECTION>_<KEY>` uppercased.
- e.g. `LCRC_PATHS_CACHE_DIR`, `LCRC_SCAN_DEFAULT_DEPTH`, `LCRC_SCAN_QUICK_TASK_TIMEOUT`.
- `LCRC_DISCOVERY_EXTRA_MODEL_DIRS` is colon-separated (PATH-style).

**Runtime socket precedence (5 layers):**
1. CLI flag (none in v1)
2. `LCRC_RUNTIME_DOCKER_HOST`
3. `DOCKER_HOST` (standard convention; `bollard` reads it natively)
4. Auto-probe `/var/run/docker.sock`, then Podman default socket
5. Exit 11 with setup instructions

**Vendoring layout:**

```
lcrc/                              # repo root
├── Cargo.toml
├── src/                           # Rust code
├── tasks/
│   └── swe-bench-pro/             # vendored curated subset
│       ├── manifest.json          # task list + most-informative ordering
│       ├── tasks/                 # per-task fixtures
│       └── canary/                # canary task w/ known-good baseline
├── image/
│   └── Dockerfile                 # task container; reviewers verify
└── README.md
```

CI builds container image from `image/Dockerfile`, bakes mini-swe-agent (pinned via `pip install mini-swe-agent==X.Y.Z`) + bundled SWE-Bench Pro tasks, publishes to GHCR, digest pinned in `src/constants.rs` of the matching lcrc release.

**`lcrc --version` self-attestation (FR3, NFR-O4):**

```
lcrc 0.1.0 (build a1b2c3d4)
  task source: swe-bench-pro 2026.04.30
  harness:     mini-swe-agent 1.2.3
  backend:     llama.cpp (auto-detected at runtime)
  container:   ghcr.io/<org>/lcrc-task@sha256:abc1234...
```

Screenshot-friendly self-attestation: anyone reading a report screenshot + this version output can reconstruct the measurement environment exactly.

**Homebrew formula sketch:**

```ruby
class Lcrc < Formula
  desc "Personal benchmark database for local LLMs"
  homepage "https://github.com/<org>/lcrc"
  url "https://github.com/<org>/lcrc/releases/download/v0.1.0/lcrc-0.1.0.tar.gz"
  sha256 "..."
  license "Apache-2.0"

  depends_on "podman"
  depends_on "llama.cpp"   # provides llama-server binary

  def install
    bin.install "lcrc"
  end

  def caveats
    <<~EOS
      Before first use, start the Podman machine:
        podman machine init
        podman machine start

      Then run:
        lcrc scan
    EOS
  end
end
```

**License:** **Apache-2.0** (single-license).
- Matches the bulk of our Rust dependencies' license.
- Includes a patent grant (modern norm for tools wrapping binary protocols / model interfaces).
- lcrc is a binary tool not a reusable library — single Apache-2.0 is clean and common (vs the dual MIT/Apache-2.0 Rust crate convention).
- Compatible with all our deps (MIT, Apache-2.0, BSD all flow downstream cleanly).

### Implementation Sequence (decision-driven)

The decisions above suggest this implementation order, which Step 6 (source tree) and the Epics/Stories phase should respect:

1. **Project scaffold** (`cargo new --bin lcrc` + Cargo.toml + minimum dependency set + `etcetera` for XDG paths + `clap` for `--version` and `--help`).
2. **SQLite schema + migration framework** (cells table, PRAGMA user_version, migration scripts).
3. **Cache key computation** (machine_fingerprint, model_sha SHA-256, params canonicalization → params_hash).
4. **`Backend` trait + `LlamaCppBackend` impl** (model discovery, memory estimation, server lifecycle).
5. **`TaskSource` trait + `SweBenchProSource` impl** (task list, load, evaluate; vendored subset).
6. **Sandbox/container layer** (bollard wiring; per-task network; image pull on first run; pre-flight runtime detection per FR17a).
7. **Canary task** (FR13/FR14: known-good baseline; runs at start of every scan).
8. **Quick scan orchestrator** (Round 1.4 lifecycle + Round 4.3 server lifecycle).
9. **HTML report renderer** (askama template; canonical header; Wilson CIs; templated badges).
10. **`lcrc show`** (read-only cache view; mirrors HTML rank).
11. **Standard depth** (extends Quick cells without re-measurement).
12. **`lcrc verify --sample N`** (drift detection; warn-not-invalidate).
13. **Full depth** + multi-quant variants.
14. **Acceptance check #9** sandbox negative-test battery (binding gate before v1 ship).
15. **Calibration pass** on M1 Pro 32GB to lock final `*_task_timeout` values.
16. **Homebrew formula + GHCR image publish + release tooling.**

### Cross-Component Dependencies

- The **cell schema** is foundational; every other component depends on it.
- The **`Backend` trait** is queried by model discovery, memory estimation, and the scan orchestrator's grouping logic — must land before any of those.
- The **`TaskSource` trait** is queried by the canary, the orchestrator, and the static task ordering — must land before the orchestrator.
- The **sandbox/container layer** is depended-on by every measurement; pre-flight runtime detection (FR17a) must land before the orchestrator can run.
- The **scan orchestrator** depends on all of: Backend, TaskSource, sandbox, llama-server lifecycle, atomic SQLite writes, signal handling.
- The **HTML report renderer** depends on: cell schema, Wilson CI computation, badge enum, canonical header data sources (`--version` info).
- **Acceptance check #9** depends on the full sandbox layer being in place — it's a binding v1 gate, not optional.

## Implementation Patterns & Consistency Rules

### Pattern Categories Identified

For lcrc on Rust + Tokio + SQLite + Podman/bollard, the AI-agent conflict points that need locked patterns:

1. Rust style baseline (clippy + rustfmt enforce mechanically).
2. Error handling discipline — thiserror vs anyhow split + exit-code mapping.
3. Module organization — where does new code go.
4. stdout/stderr discipline — FR46 is structural; agents must not `println!` for diagnostics.
5. Async discipline — tokio everywhere; no accidental blocking I/O.
6. Atomic-write discipline — cells, HTML report.
7. Cache key canonicalization — params_hash, machine_fingerprint, model_sha, backend_build computed identically every time.
8. Timestamp format — single source of truth.
9. Layered config loading — single loader, single precedence chain.
10. Templated badge enum — fixed source of truth; no agent invents new badges or LLM-prose.
11. Sandbox invariants — must be checked structurally, not by convention.
12. Logging discipline — `tracing` everywhere, never `eprintln!` directly.

### Rust Style Baseline

- **Edition:** Rust 2024. MSRV pinned in `Cargo.toml` to current stable at v1 start (1.85+).
- **rustfmt:** default profile, `cargo fmt --check` is a CI gate.
- **clippy:** `cargo clippy --all-targets --all-features -- -D warnings` is a CI gate.
- **Lints baked in `Cargo.toml`:**
  ```toml
  [lints.rust]
  unsafe_code = "forbid"
  missing_docs = "warn"

  [lints.clippy]
  pedantic = { level = "warn", priority = -1 }
  unwrap_used = "deny"
  expect_used = "deny"      # except in tests
  panic = "deny"            # except in tests
  ```
- **No `unsafe`** in v1 (none of our integrations require it; FFI to mach/libproc/IOReport is via published crates).
- **No `unwrap()` / `expect()` / `panic!()` in non-test code.** Use `?` + typed errors instead.

### Error Handling

Two-layer discipline:

**Module boundaries → `thiserror` typed errors with exit-code mapping.**

```rust
#[derive(thiserror::Error, Debug)]
pub enum CacheError {
    #[error("invalid schema version: {0}")]
    InvalidSchemaVersion(u32),
    // ...
}

#[derive(thiserror::Error, Debug)]
pub enum ScanError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),         // exit 10
    #[error("preflight failed: {0}")]
    Preflight(#[from] PreflightError),    // exit 11
    #[error("canary failed")]
    CanaryFailed,                         // exit 1
    #[error("sandbox violation: {0}")]
    SandboxViolation(String),             // exit 2
    #[error("aborted by signal")]
    AbortedBySignal,                      // exit 3
    // ...
}
```

**Scan-internal application code → `anyhow::Result` with `.context()` propagation.**

```rust
let model = backend.discover_models()
    .context("failed to discover llama.cpp models")?;
```

**Single source of truth for exit codes:** `src/exit_code.rs` defines an `ExitCode` enum mirroring FR45. `main.rs` matches on the top-level error and returns `process::exit(code as i32)`. **No bare numeric exit codes anywhere else.**

```rust
#[repr(i32)]
pub enum ExitCode {
    Ok = 0,
    CanaryFailed = 1,
    SandboxViolation = 2,
    AbortedBySignal = 3,
    CacheEmpty = 4,
    DriftDetected = 5,
    ConfigError = 10,
    PreflightFailed = 11,
    ConcurrentScan = 12,
}
```

### Module Organization

File-as-module style (preferred Rust 2018+ idiom; no `mod.rs`).

Top-level rule: **one trait per module file.** New `Backend` impl → new file in `src/backend/`. New `TaskSource` impl → new file in `src/tasks/`. No mixing of concerns.

Cross-cutting helpers:
- `src/error.rs` — error types + exit-code enum
- `src/output.rs` — the *only* place that writes to stdout/stderr directly
- `src/version.rs` — build info, vendored versions, `--version` rendering
- `src/constants.rs` — pinned values (container image digest, schema version, etc.)

### stdout / stderr Discipline (FR46)

**One module owns all process output: `src/output.rs`.**

```rust
pub fn result(s: &str) { println!("{s}"); }       // stdout: results only
pub fn progress(s: &str) { eprintln!("{s}"); }    // stderr: progress
pub fn diag(s: &str) { eprintln!("{s}"); }        // stderr: diagnostics
```

**Tracing subscriber writes to stderr.** Default level `INFO`; per-cell completion via tracing event with structured fields rendered through a custom subscriber layer.

**Forbidden everywhere except `src/output.rs`:**
- `println!`, `eprintln!`, `print!`, `eprint!`, `dbg!`
- Manual `writeln!(io::stdout(), ...)`

Enforce via clippy custom restriction or simple pre-commit grep.

### Async Discipline

- **All I/O via tokio.** `tokio::fs`, `tokio::process`, not `std::fs` or `std::process`.
- **No `block_on` inside async code.** If sync-only crate is needed, wrap with `tokio::task::spawn_blocking`.
- **One runtime, started in `main`:** `#[tokio::main(flavor = "multi_thread")]`.
- **Cancellation via `tokio_util::sync::CancellationToken`** propagated from the SIGINT handler down into the scan orchestrator and per-cell measurement futures.

### Atomic-Write Discipline

**Cells:** every cell write is a single SQLite transaction.

```rust
pub async fn write_cell(&self, cell: &Cell) -> Result<()> {
    let mut tx = self.db.begin().await?;
    sqlx::query!("INSERT INTO cells ...").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
```

No multi-statement non-transactional cell writes anywhere.

**HTML report:** tempfile + atomic rename.

```rust
pub async fn write_report(path: &Path, html: &str) -> Result<()> {
    let tmp = path.with_extension("html.tmp");
    tokio::fs::write(&tmp, html).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}
```

Same pattern for any other "must-not-corrupt" file (`scan.lock`, etc.).

### Cache Key Canonicalization

**Single source of truth: `src/cache/key.rs`.**

- `model_sha`: `sha2::Sha256` of the GGUF file bytes (streamed; no full-load).
- `params_hash`: `sha2::Sha256` of `serde_json::to_string(&params)` where `params` is a `BTreeMap<&str, Value>` (sorted keys = canonical JSON).
- `machine_fingerprint`: `format!("{chip}-{ram_gb}GB-{gpu_cores}gpu")` — single function, single string format. e.g. `"M1Pro-32GB-14gpu"`.
- `backend_build`: `format!("{name}-{semver}+{commit_short}")` — e.g. `"llama.cpp-b3791+a1b2c3d"`.

**No agent computes any of these inline.** Always call the canonical function from `cache::key`.

### Timestamp Format

**RFC 3339 with `Z` suffix (UTC), millisecond precision.**

```rust
use time::format_description::well_known::Rfc3339;
let ts = OffsetDateTime::now_utc().format(&Rfc3339)?;
// "2026-04-30T14:23:15.412Z"
```

Single helper in `src/util/time.rs`. Used for: `scan_timestamp` cell column, report header date, historical report filenames (`report-2026-04-30T14-23-15.html` — colons replaced with dashes for filename safety).

### Layered Config Loading

**Single function: `config::load() -> Result<Config>`** assembled with `figment`:

```rust
Figment::new()
    .merge(Serialized::defaults(Config::default()))
    .merge(Toml::file(toml_path))
    .merge(Env::prefixed("LCRC_").split("_"))
    // CLI overrides applied separately by clap-derived struct
    .extract()
```

Validation runs immediately after extraction; on failure → `ConfigError` → exit 10 (FR51).

**No agent reads env vars directly with `std::env::var`** outside `config::`. Same for TOML files.

### Tracing / Logging

- **Levels mapped to lcrc semantics:**
  - `ERROR` — non-recoverable; exits with non-zero exit code
  - `WARN` — degraded but continuing (perf null, badge applied, llama-server restart)
  - `INFO` — per-cell completion, scan start/end, server start/stop
  - `DEBUG` — internal state transitions; off by default
  - `TRACE` — wire-level (HTTP requests, container API calls); off by default
- **Targets** named after module path (`lcrc::scan::orchestrator`, `lcrc::sandbox::container`).
- **Structured fields** for cell identity (`model_sha`, `task_id`, etc.) rather than string interpolation.
- **No `tracing::error!` for expected-failure conditions.** Expected failures (canary fail, drift detected, sandbox violation) are conveyed via exit codes + report; `WARN` if anything.

### Templated Badge Enum (FR36 + Step 4 additions)

**Single source of truth: `src/report/badges.rs`.**

```rust
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Badge {
    CtxLimited,
    OomAtN,
    RepetitionLoop,
    ToolCallFormatFailure,
    ThermalThrottled,
    LowConfidenceCi,
    TaskTimeout,             // added Step 4
    ServerStartupFailure,    // added Step 4
    ServerCrashed,           // added Step 4
    SandboxViolation,        // added Step 4
}
```

**No agent adds a badge variant without updating this enum, the HTML template, the README badge glossary, and the JSON schema.** No LLM-generated prose in any report-rendered output.

### Sandbox Invariants — Structural, not Conventional

These are enforced by code organization, not comments:

- **`src/sandbox/container.rs` is the only module that calls `bollard::container::Container::create`.** All other modules go through `Sandbox::run_task(image, workspace, env_allowlist)`.
- **The `Sandbox::run_task` function does not accept a `volumes: Vec<Mount>` argument.** Workspace mount is hard-coded; no extension point.
- **The function does not accept an `env: HashMap` argument.** Env allowlist is a constant in `src/sandbox/env_allowlist.rs`; agents extend the list with code review, not at call sites.
- **The function does not accept a `network_mode: Network` argument.** Network is constructed internally per `src/sandbox/network.rs`; no overrides.
- Acceptance check #9 (sandbox negative test) is a binding test in `tests/sandbox_envelope.rs` — runs an adversarial battery and asserts every attempt fails.

### Testing Patterns

- **Unit tests:** in-module `#[cfg(test)] mod tests { ... }` per file.
- **Integration tests:** in `tests/` at crate root.
- **CLI tests:** `assert_cmd` + `predicates` for exit-code coverage. Every variant in `ExitCode` enum has a corresponding integration test.
- **HTML snapshots:** `insta` for HTML report rendering.
- **Sandbox negative test:** `tests/sandbox_envelope.rs` (acceptance check #9).
- **`expect()` and `unwrap()` are allowed in tests only.**

### Documentation Patterns

- **Doc comments (`///`) on all public items.** `missing_docs = "warn"` lint enforces.
- **No internal narration in comments.** Comments only for non-obvious WHY (constraints, invariants, surprising behavior). Don't restate the WHAT.
- **No "// added for X" / "// used by Y" comments.** PR descriptions and git blame carry that context.

### Enforcement Summary — All AI Agents MUST

- Use `?` + typed/anyhow errors. Never `unwrap` / `expect` / `panic` outside tests.
- Map every exit code through `src/exit_code.rs` `ExitCode` enum.
- Write to stdout/stderr only via `src/output.rs`.
- Use `tokio::fs` / `tokio::process`, never `std::fs` / `std::process`.
- Write cells inside a single SQLite transaction; never partially.
- Compute `model_sha`, `params_hash`, `machine_fingerprint`, `backend_build` only via `cache::key` helpers.
- Format timestamps only via `util::time` helpers.
- Read env vars and TOML only via `config::load`.
- Add badges only by extending the `Badge` enum + template + glossary together.
- Spawn containers only via `Sandbox::run_task`.
- Add `tracing` events at module-pathed targets with structured fields.

## Project Structure & Boundaries

### Complete Project Directory Structure

```
lcrc/                                  # repo root
├── Cargo.toml                         # crate manifest + workspace lints (per Patterns)
├── Cargo.lock                         # committed (binary, not library)
├── README.md                          # honest scope, install, usage; badge glossary
├── LICENSE                            # Apache-2.0
├── CHANGELOG.md
├── .gitignore
├── rust-toolchain.toml                # MSRV pin (Rust 1.85+ stable)
├── rustfmt.toml                       # default profile
│
├── .github/
│   └── workflows/
│       ├── ci.yml                     # fmt --check, clippy -D warnings, test, sandbox negative test
│       └── release.yml                # build per-arch bottles, publish container to GHCR, draft release
│
├── image/                             # task container (FR15, FR17b, NFR-S6)
│   ├── Dockerfile                     # Debian-slim + Python + mini-swe-agent + bundled tasks
│   ├── requirements.txt               # pinned mini-swe-agent + pytest + ...
│   └── README.md                      # rebuild + verify instructions
│
├── tasks/                             # vendored task source data
│   └── swe-bench-pro/
│       ├── manifest.json              # task list + most-informative ordering (FR21)
│       ├── version                    # task_subset_version string
│       ├── tasks/                     # per-task fixtures
│       │   ├── django-1234/
│       │   │   └── spec.json
│       │   └── ...
│       └── canary/                    # canary with known-good baseline (FR13/FR14)
│           ├── spec.json
│           └── baseline.json
│
├── homebrew/
│   └── lcrc.rb                        # Homebrew formula (NFR-I5)
│
├── src/
│   ├── main.rs                        # entry: parse CLI, run, exit with code
│   ├── lib.rs                         # crate root: module decls, run() entry
│   │
│   ├── cli.rs                         # clap-derive CLI root (FR4, FR44)
│   ├── cli/
│   │   ├── scan.rs                    # `lcrc scan` (FR2, FR12, FR20, FR48)
│   │   ├── show.rs                    # `lcrc show` (FR40–FR42)
│   │   ├── verify.rs                  # `lcrc verify` (FR28)
│   │   └── meta.rs                    # `--version` (FR3), `--help` (FR4)
│   │
│   ├── exit_code.rs                   # ExitCode enum (FR45) — single source of truth
│   ├── error.rs                       # top-level Error type, From impls → ExitCode
│   ├── output.rs                      # ONLY module that writes to stdout/stderr (FR46)
│   ├── version.rs                     # build info, --version rendering (FR3, NFR-O4)
│   ├── constants.rs                   # container image digest, schema version, defaults
│   │
│   ├── config.rs                      # Config root + load() function
│   ├── config/
│   │   ├── schema.rs                  # TOML schema struct (serde::Deserialize) (FR49)
│   │   └── env.rs                     # LCRC_* env var parsing (FR50)
│   │
│   ├── cache.rs                       # Cache struct, public API (FR24–FR31)
│   ├── cache/
│   │   ├── schema.rs                  # SQL DDL constants
│   │   ├── migrations.rs              # PRAGMA user_version + migration scripts (NFR-R3)
│   │   ├── key.rs                     # canonical key computation (per Patterns)
│   │   ├── cell.rs                    # Cell struct, read/write (atomic transactions)
│   │   └── query.rs                   # leaderboard, drift, sample queries
│   │
│   ├── discovery.rs                   # discover_models() public entry
│   ├── discovery/
│   │   ├── llama_cpp.rs               # ~/.cache/llama.cpp/ scanner (FR7)
│   │   ├── gguf.rs                    # GGUF header parser → metadata + sha (FR8)
│   │   └── fit_gate.rs                # RAM × ctx-length filter (FR9, FR10)
│   │
│   ├── machine.rs                     # MachineFingerprint (FR24, NFR-C2)
│   ├── machine/
│   │   └── apple_silicon.rs           # chip + RAM + GPU core detection
│   │
│   ├── backend.rs                     # Backend trait (NFR-C5)
│   ├── backend/
│   │   └── llama_cpp.rs               # LlamaCppBackend impl
│   │
│   ├── tasks.rs                       # TaskSource trait
│   ├── tasks/
│   │   └── swe_bench_pro.rs           # SweBenchProSource impl
│   │
│   ├── sandbox.rs                     # Sandbox::run_task() public API (FR16)
│   ├── sandbox/
│   │   ├── runtime.rs                 # Docker-Engine-API socket detection (FR17a)
│   │   ├── container.rs               # ONLY caller of bollard::container (Patterns invariant)
│   │   ├── network.rs                 # internal network + llama-server pinhole (NFR-S4)
│   │   ├── env_allowlist.rs           # const allowlist (NFR-S5)
│   │   ├── image.rs                   # image pull, digest verification (FR17b)
│   │   └── violation.rs               # sandbox-violation event detection (FR17, NFR-S2)
│   │
│   ├── perf.rs                        # PerfCollector trait + collect() entry (FR18)
│   ├── perf/
│   │   ├── tokens.rs                  # tok/s + ttft from llama-server API
│   │   ├── memory.rs                  # peak_rss via proc_pid_info (no privilege)
│   │   └── thermal.rs                 # IOReport thermal_state (no privilege)
│   │
│   ├── scan.rs                        # Scan struct, run() entry (FR13–FR23)
│   ├── scan/
│   │   ├── orchestrator.rs            # per-(model,params) grouping, lifecycle
│   │   ├── canary.rs                  # canary execution (FR13, FR14)
│   │   ├── server_lifecycle.rs        # llama-server start/stop/health (NFR-I1, NFR-R5)
│   │   ├── lock.rs                    # scan.lock file (FR52)
│   │   ├── signal.rs                  # SIGINT handler + CancellationToken (FR27)
│   │   └── timeout.rs                 # per-tier wall-clock cap (FR19)
│   │
│   ├── verify.rs                      # drift detection (FR28, FR29)
│   ├── verify/
│   │   └── sample.rs                  # random sampling, drift comparison
│   │
│   ├── report.rs                      # render_html() entry (FR32, FR33)
│   ├── report/
│   │   ├── badges.rs                  # Badge enum (FR36) — single source
│   │   ├── wilson.rs                  # Wilson-score CI (FR35)
│   │   ├── header.rs                  # canonical header (FR34)
│   │   └── templates/                 # askama templates
│   │       ├── report.html
│   │       ├── header.html
│   │       └── row.html
│   │
│   ├── show.rs                        # plain-text leaderboard (FR40–FR42)
│   │
│   ├── jsonout.rs                     # JSON output schema (FR43, FR54)
│   ├── jsonout/
│   │   └── schema.rs                  # versioned serializable types
│   │
│   ├── starter_pack.rs                # empty-machine UX (FR5)
│   │
│   └── util/
│       ├── time.rs                    # RFC 3339 timestamp helper (per Patterns)
│       └── tracing.rs                 # subscriber setup, custom formatter
│
└── tests/                             # integration tests (separate crates per file)
    ├── cli_exit_codes.rs              # FR45 — every ExitCode variant has a test
    ├── cache_roundtrip.rs             # cell write + read; concurrent reader
    ├── cache_migrations.rs            # NFR-R3 cache durability across upgrades
    ├── machine_fingerprint.rs         # NFR-C2 OS-patch stability
    ├── sandbox_envelope.rs            # acceptance check #9 — adversarial battery (binding gate)
    ├── canary_drift.rs                # FR13/FR14 canary-pass/fail/skipped
    ├── scan_resumability.rs           # NFR-R1 Ctrl-C / OOM mid-scan
    ├── verify_drift.rs                # FR28 drift detection
    ├── report_render.rs               # FR32–FR36 HTML rendering
    ├── snapshots/                     # insta snapshots
    │   └── *.snap
    ├── show_mirror.rs                 # FR40 plain-text mirrors HTML rank (acceptance #8)
    └── concurrency_lock.rs            # FR52, FR53 lock-file behavior
```

Unit tests live inline at the bottom of each `src/**/*.rs` file in `#[cfg(test)] mod tests { ... }` blocks (idiomatic Rust convention; gives access to private items, zero release-binary cost). Integration tests in `tests/` exercise the public API and the binary as a black-box.

### Architectural Boundaries (the "only X talks to Y" rules)

| Boundary | Sole module | What's behind it |
|---|---|---|
| Container runtime API | `src/sandbox/container.rs` | bollard, Docker Engine API socket |
| Container image | `src/sandbox/image.rs` | image pull, digest verify, manifest inspection |
| llama-server lifecycle | `src/scan/server_lifecycle.rs` (via `Backend` trait) | subprocess spawn, health check, KV cache reset |
| llama-server HTTP | `src/backend/llama_cpp.rs` | reqwest calls to `/completion`, `/tokenize`, `/health` |
| SQLite database | `src/cache/*` | rusqlite/sqlx; schema + migrations + queries |
| stdout/stderr | `src/output.rs` + tracing subscriber via `src/util/tracing.rs` | the only allowed writers |
| Env vars + TOML files | `src/config/*` | figment composition; layered precedence |
| Process exit | `src/main.rs` (only) | matches top-level error → ExitCode → process::exit |
| GGUF parsing | `src/discovery/gguf.rs` | binary header parser; format-agnostic SHA-256 entry point |
| macOS perf APIs | `src/perf/*` | proc_pid_info, IOReport, mach_task_basic_info |
| Machine fingerprint | `src/machine/apple_silicon.rs` | sysctl chip detection, RAM, GPU cores |
| Cache key computation | `src/cache/key.rs` | model_sha, params_hash, machine_fingerprint, backend_build helpers |
| Badge enum | `src/report/badges.rs` | FR36 enum + Step 4 additions; single source |
| Sandbox env allowlist | `src/sandbox/env_allowlist.rs` | const list per NFR-S5 |
| Wilson CI math | `src/report/wilson.rs` | self-contained, ~10 LOC, no dep |

### Requirements → Structure Mapping

#### Installation & First Run (FR1–FR6)
- **FR1 (Homebrew install):** `homebrew/lcrc.rb` + `.github/workflows/release.yml`
- **FR2 (`lcrc scan` zero-config):** `src/cli/scan.rs` + defaults in `src/config/schema.rs`
- **FR3 (`--version` self-attestation):** `src/version.rs` + `src/constants.rs` + `src/cli/meta.rs`
- **FR4 (`--help`):** `src/cli/meta.rs` + clap-derive throughout `src/cli/`
- **FR5 (empty-machine UX):** `src/starter_pack.rs` + invoked from `src/cli/scan.rs` when discovery returns ∅
- **FR6 (canary in header):** `src/scan/canary.rs` + `src/report/header.rs`

#### Model Discovery & Eligibility (FR7–FR12)
- **FR7 (llama.cpp cache scan):** `src/discovery/llama_cpp.rs`
- **FR8 (format-agnostic `model_sha`):** `src/discovery/gguf.rs` + `src/cache/key.rs`
- **FR9, FR10 (RAM × ctx fit gate):** `src/discovery/fit_gate.rs`
- **FR11 (extra model dirs):** `src/config/schema.rs` (`[discovery] extra_model_dirs`) + `src/discovery/llama_cpp.rs`
- **FR12 (`--model` filter):** clap arg in `src/cli/{scan,show,verify}.rs` → applied in `src/discovery.rs`

#### Measurement Execution (FR13–FR23)
- **FR13, FR14 (canary):** `src/scan/canary.rs` + `tasks/swe-bench-pro/canary/`
- **FR15 (mini-swe-agent subprocess):** `src/sandbox/container.rs` + `image/Dockerfile`
- **FR16 (default-deny container):** `src/sandbox/{container,network,env_allowlist}.rs`
- **FR17 (sandbox-violation events):** `src/sandbox/violation.rs`
- **FR17a (pre-flight container runtime):** `src/sandbox/runtime.rs`
- **FR17b (image pinning):** `src/constants.rs` + `src/sandbox/image.rs`
- **FR18 (perf metrics):** `src/perf/*`
- **FR19 (per-tier wall-clock cap):** `src/scan/timeout.rs` + config schema
- **FR20–FR23 (depths):** `src/cli/scan.rs` (`--depth`) + `src/scan/orchestrator.rs` + `src/tasks/swe_bench_pro.rs::list_tasks` (static ordering)

#### Cache & Persistence (FR24–FR31)
- **FR24, FR25 (cell key + storage):** `src/cache/{key,schema,cell}.rs`
- **FR26 (lookup before measure):** `src/cache/query.rs::lookup` called by `src/scan/orchestrator.rs`
- **FR27 (resumability):** `src/cache/cell.rs` (atomic write) + `src/scan/signal.rs`
- **FR28 (`lcrc verify`):** `src/cli/verify.rs` + `src/verify/sample.rs`
- **FR29 (warn-on-drift):** `src/verify/sample.rs`
- **FR30 (OS-patch stability):** `src/machine/apple_silicon.rs` + tested by `tests/machine_fingerprint.rs`
- **FR31 (per-cell metadata):** `src/cache/cell.rs` + cell schema

#### Reporting (FR32–FR43)
- **FR32 (self-contained HTML):** `src/report.rs` + `src/report/templates/`
- **FR33 (regenerate after every cell):** `src/scan/orchestrator.rs` calls `src/report::render_html()` per-cell
- **FR34 (canonical header):** `src/report/header.rs`
- **FR35 (Wilson CIs):** `src/report/wilson.rs`
- **FR36 (templated badges):** `src/report/badges.rs`
- **FR37, FR38 (depth tier tagging + Quick `low-confidence-CI`):** `src/cache/cell.rs` (depth_tier column) + template
- **FR39 (default report path):** `src/config/schema.rs` (`[paths] report_dir`) + `src/scan/orchestrator.rs`
- **FR40, FR41, FR42 (`lcrc show`):** `src/cli/show.rs` + `src/show.rs`
- **FR43 (JSON output):** `src/jsonout/*` + `--format json` flag in clap

#### CLI Surface, Configuration & Scripting (FR44–FR54)
- **FR44 (non-interactive):** discipline; no library calls that prompt; tested by `tests/cli_exit_codes.rs`
- **FR45 (exit codes):** `src/exit_code.rs` + `tests/cli_exit_codes.rs` covers every variant
- **FR46 (stdout/stderr discipline):** `src/output.rs` invariant + lint
- **FR47 (per-cell streaming):** `src/util/tracing.rs` custom subscriber + indicatif on TTY
- **FR48 (`--quiet`):** clap flag in `src/cli/scan.rs` → suppresses tracing INFO on stderr
- **FR49 (TOML config):** `src/config/schema.rs`
- **FR50 (layered precedence):** `src/config.rs::load`
- **FR51 (config validation):** `src/config.rs::load` returns `ConfigError` → exit 10
- **FR52 (scan.lock):** `src/scan/lock.rs`
- **FR53 (lock-free reads):** `src/cli/show.rs`/`verify.rs` open SQLite read-only
- **FR54 (stable JSON schemas):** `src/jsonout/schema.rs` + `schema_version` field

#### Cross-Cutting NFRs
- **NFR-S1–S6 (sandbox):** `src/sandbox/*` + `tests/sandbox_envelope.rs` (acceptance check #9)
- **NFR-R1, R2 (resumability + atomicity):** `src/cache/cell.rs` + `src/scan/signal.rs` + `tests/scan_resumability.rs`
- **NFR-R3 (cache durability):** `src/cache/migrations.rs` + `tests/cache_migrations.rs`
- **NFR-C5 (Linux NVIDIA additive):** `Backend` trait abstraction; future Linux files would land at `src/backend/cuda.rs`, `src/perf/linux.rs`, etc., gated by `#[cfg(target_os = "linux")]`
- **NFR-O4 (--version self-attestation):** `src/version.rs` + `src/constants.rs`

### Data Flow — One Scan Cycle

```
┌──────────────────────────────────────────────────────────────────────┐
│ main.rs                                                               │
│  ├─ cli::parse() → ScanArgs                                          │
│  ├─ output::progress("Starting scan...")                             │
│  └─ run() → returns Result<()>                                       │
│       └─ on err: error → ExitCode → process::exit                    │
└──────────────────────────────────────────────────────────────────────┘
                  ↓
┌──────────────────────────────────────────────────────────────────────┐
│ cli/scan.rs::run()                                                    │
│  1. config::load()                          → exit 10 on invalid     │
│  2. sandbox::runtime::detect()              → exit 11 if no socket   │
│  3. scan::lock::acquire()                   → exit 12 if held        │
│  4. machine::fingerprint()                  → MachineFingerprint     │
│  5. scan::run(config, fingerprint)                                   │
│  6. lock released on drop                                            │
└──────────────────────────────────────────────────────────────────────┘
                  ↓
┌──────────────────────────────────────────────────────────────────────┐
│ scan/orchestrator.rs::run()                                           │
│  1. discovery::discover_models() → Vec<ModelRef>                     │
│  2. fit_gate::filter() → Vec<ModelRef>  (FR9 + FR10 visible)         │
│  3. tasks::SweBenchProSource::list_tasks() → Vec<TaskId>             │
│  4. plan: enumerate cells, group by (model_sha, params_hash)         │
│  5. canary::run() → record canary state in report header              │
│  6. for each (model, params) group:                                  │
│      a. server_lifecycle::start(model, params)                       │
│      b. for each task in group:                                      │
│          i.   cache::query::lookup(cell_key) → if hit, skip          │
│          ii.  sandbox::run_task(image, workspace, env_allowlist)     │
│               → spawn container → mini-swe-agent runs → outcome      │
│          iii. perf::collect() → metrics                              │
│          iv.  cache::cell::write(cell)  [atomic transaction]         │
│          v.   report::render_html() → atomic file rename             │
│          vi.  output::progress("Completed: <model>/<task> ...")      │
│      c. server_lifecycle::stop()                                     │
│  7. final report::render_html()                                      │
└──────────────────────────────────────────────────────────────────────┘
                  ↓
              (lock released, exit 0 / 1 / 2)
```

SIGINT path: `signal.rs` → `CancellationToken::cancel()` → orchestrator's per-cell future receives cancel → in-flight container is torn down by `Drop` impls → no cell write happens for the in-progress cell → lock released → exit 3.

### Build & Distribution

- `cargo build --release` produces `target/release/lcrc` (single static binary; no Python or other runtime needed on host).
- CI builds container image from `image/Dockerfile`, tags with lcrc version + digest, pushes to `ghcr.io/<org>/lcrc-task`. Digest pinned in `src/constants.rs` of the matching commit.
- Release workflow:
  1. Tag `v0.1.0` → CI builds Mac arm64 binary as a Homebrew bottle.
  2. CI publishes `ghcr.io/<org>/lcrc-task:0.1.0@sha256:...`.
  3. CI publishes lcrc binary + bottle to GitHub Releases.
  4. `homebrew/lcrc.rb` updated with new SHA + URL (PR or auto-bumped formula in tap repo).

## Architecture Validation Results

### Coherence Validation ✅

**Decision Compatibility:** All locked decisions interoperate cleanly. Rust 2024 + Tokio + SQLite + Podman/bollard + askama have no known version conflicts. The single async runtime + single CLI parser + single error layer + single output module produce one consistent code shape.

**Pattern Consistency:** The Patterns section directly enforces Decisions:
- `src/output.rs` invariant enforces FR46 (stdout/stderr discipline).
- `src/exit_code.rs` enum enforces FR45 (semver-stable exit codes).
- `src/cache/key.rs` canonical helpers enforce FR8 (format-agnostic `model_sha`) and the FR24 cache key.
- `src/sandbox/run_task` shape enforces NFR-S1–S5 by structural construction (no extension args).
- `src/report/badges.rs` `Badge` enum enforces FR36 (no LLM prose, fixed enum).

**Structure Alignment:** Every architectural boundary in Decisions has a single owning module in Project Structure:
- bollard ↔ `src/sandbox/container.rs`
- llama-server HTTP ↔ `src/backend/llama_cpp.rs`
- SQLite ↔ `src/cache/*`
- stdout/stderr ↔ `src/output.rs`
- env vars + TOML ↔ `src/config/*`

No contradictions found.

### Requirements Coverage Validation ✅

**Functional Requirements (56 total: FR1–FR54 + FR17a + FR17b) — 100% coverage.**

| Category | FRs | Coverage |
|---|---|---|
| Installation & First Run | FR1–FR6 | ✅ all mapped to specific modules |
| Model Discovery & Eligibility | FR7–FR12 | ✅ all mapped |
| Measurement Execution | FR13–FR23 | ✅ all mapped (incl. FR17a runtime detection, FR17b image pinning) |
| Cache & Persistence | FR24–FR31 | ✅ all mapped; cell schema covers FR24/FR31 in detail |
| Reporting | FR32–FR43 | ✅ all mapped; askama compile-time templates produce single self-contained HTML (FR32) |
| CLI Surface, Configuration & Scripting | FR44–FR54 | ✅ all mapped; ExitCode enum covers every FR45 variant; figment composition covers FR50 layered precedence |

**Non-Functional Requirements (38 total) — 100% coverage.**

| Category | NFRs | Coverage |
|---|---|---|
| Performance | NFR-P1–P9 | ✅ Rust binary trivially meets P5–P7; P1/P2/P3/P9 gated by acceptance check #1 + calibration |
| Reliability | NFR-R1–R8 | ✅ resumability + atomicity, graceful llama-server lifecycle, container teardown; SQLite WAL for R7; cache durability via migrations |
| Security | NFR-S1–S7 | ✅ default-deny structural sandbox; FR17a hard runtime dependency (no `--unsafe-no-sandbox`); FR17b + NFR-S6 image pinning; no telemetry by construction |
| Compatibility & Portability | NFR-C1–C5 | ✅ Apple Silicon factoring; machine_fingerprint stability tested; cache migration discipline; trait abstractions keep Linux NVIDIA, MLX, custom evals additive |
| Observability | NFR-O1–O4 | ✅ tracing subscriber + indicatif; disk-only state; no telemetry; `--version` self-attestation format locked |
| Integration | NFR-I1–I6 | ✅ llama-server on host, mini-swe-agent inside container, perf from host, rootless preferred via Podman default, Homebrew formula sketched, no cloud/API/auth |

**Acceptance Checks (PRD §"Measurable Outcomes" #1–#9) — 100% coverage.**

| Check | Coverage |
|---|---|
| #1 Quick ≤25 min on M1 Pro 32GB / 5-model | ✅ NFR-P1 + calibration before v1 ship |
| #2 Standard extension stable top-3 | ✅ NFR-P2 + cache-extending depth design |
| #3 Full overnight, no rank inversions | ✅ NFR-P3 + cache extending |
| #4 Streaming CLI + on-disk regen | ✅ FR47 + src/util/tracing.rs + per-cell render_html call |
| #5 Report contents (header, CIs, badges, depth tags) | ✅ FR32–FR38 + askama templates |
| #6 Canary header three states | ✅ FR13/FR14 + canary task design |
| #7 Drift detection numerical | ✅ FR28 + src/verify/sample.rs |
| #8 CLI mirror identical rank | ✅ FR40 + tests/show_mirror.rs as binding |
| #9 Sandbox negative test (binding gate) | ✅ tests/sandbox_envelope.rs + structural sandbox design |

**PRD Open Methodology Questions — 7 + 4 implicit (language, MLX, container runtime, TOML config) → 11/11 resolved.**

### Implementation Readiness Validation ✅

**Decision Completeness:** Every decision has chosen option + rationale + module location + cross-reference. No "TBD at implementation time" items that block scaffolding.

**Structure Completeness:** Tree is concrete. Every FR maps to a specific file. Integration tests cover every ExitCode variant + acceptance check #8 + #9. No placeholder `__util__` directories.

**Pattern Completeness:** All AI-agent conflict points addressed (Rust style, errors, modules, output discipline, async, atomicity, key canonicalization, timestamps, config, logging, badges, sandbox invariants). Enforcement is structural where possible, conventional where not.

### Gap Analysis

**Critical gaps (block implementation):** NONE.

**Important gaps (don't block; need closing pre-v1 ship):**
- **Wall-clock cap values** are working assumptions (600s / 900s / 1800s). Calibration on M1 Pro 32GB / 5-model set is owed. Mechanism is locked; values are tunable.
- **SWE-Bench Pro redistribution license verification** is owed. Architecture provides the fallback (`TaskSource` trait + alternative-source contingency); the *legal* answer determines whether the v1 release vendors the subset or pulls at install time.
- **GHCR organization name** (`<org>` placeholder throughout) needs to be filled at v1 release.

**Nice-to-have gaps (deferred consciously):**
- **Concrete container-network mechanism** (Docker `--internal` flag with controlled gateway pinhole vs custom bridge with iptables-style egress restriction vs socat sidecar) — deferred to implementation; acceptance check #9 verifies the property regardless of mechanism.
- **MLX backend implementation** — slot reserved via `Backend` trait; v1.1+ adds `MlxBackend`.
- **Signed launchd helper** for power metrics — slot reserved via nullable `power_watts` cell column; v1.1+ delivers.
- **`lcrc gc` / `lcrc doctor` / wizard mode** — v1.1+ per PRD Growth Features.
- **Adaptive depth, multi-run reliability, custom-eval extension** — all v1.1+; cell schema and trait abstractions support each without rearchitecture.

### Architecture Completeness Checklist

**Requirements Analysis**
- [x] Project context thoroughly analyzed
- [x] Scale and complexity assessed
- [x] Technical constraints identified
- [x] Cross-cutting concerns mapped

**Architectural Decisions**
- [x] Critical decisions documented with versions
- [x] Technology stack fully specified
- [x] Integration patterns defined
- [x] Performance considerations addressed

**Implementation Patterns**
- [x] Naming conventions established (idiomatic Rust + clippy enforced)
- [x] Structure patterns defined (file-as-module + one-trait-per-module)
- [x] Communication patterns specified (output module + tracing discipline)
- [x] Process patterns documented (errors → ExitCode, atomic writes, cancellation)

**Project Structure**
- [x] Complete directory structure defined
- [x] Component boundaries established
- [x] Integration points mapped
- [x] Requirements to structure mapping complete

**16 / 16 checked.**

### Architecture Readiness Assessment

**Overall Status:** **READY FOR IMPLEMENTATION**

All 16 checklist items checked; no Critical Gaps. Three Important Gaps (wall-clock cap values, SWE-Bench Pro license, GHCR org name) are pre-v1-ship items, not implementation blockers — implementation can proceed in parallel with their resolution.

**Confidence Level:** **High.**

Reasons:
- The PRD is unusually explicit about acceptance criteria; the architecture maps each one to a specific module + test.
- The decisions intentionally lean structural (sandbox, atomicity, output discipline, single-source modules) rather than conventional — fewer ways for AI agents to drift.
- Trait abstractions (`Backend`, `TaskSource`) are minimal and only introduce complexity that PRD's growth features actually require.
- The few deferrals (container-network mechanism, perf launchd helper, MLX) are deferred *with named architectural slots*, not as TODOs.

**Key Strengths:**
- **Cache-as-product structure** is foundational and load-bearing: cell-level keying, atomic writes, cache-extending depths, drift detection, and resumability all derive from one consistent data model.
- **Default-deny sandbox is structural, not conventional.** `Sandbox::run_task` accepts no extensibility arguments that could leak; FR16/NFR-S1–S6 + acceptance #9 are enforced by code shape.
- **Cross-platform OSS by default.** Podman + Apache-2.0 dependency tree means the v1.1+ Windows/Linux story changes recommendations, not architecture.
- **Solo-developer tractable.** Single binary, single async runtime, single backend impl, single task source impl, single container runtime client. Each "single" reserves the trait slot for v1.1+ widening.
- **Honest deferrals.** What's not in v1 is named with its v1.1+ landing spot, not hidden as ambition.

**Areas for Future Enhancement:**
- v1.1+ adaptive depth (Wilson-CI-driven early stop) — schema supports it; orchestrator would gain a planning pass.
- v1.1+ pass@k via `trial_id` PK extension — additive migration.
- v1.1+ custom-eval extension surface (Journey 6) — `TaskSource` trait is the API.
- v1.1+ Linux NVIDIA — `Backend` trait + `#[cfg(target_os = "linux")]` factoring.
- v2 harness-as-axis comparison — would require a `Harness` trait abstraction (one not introduced in v1; v1 hardcodes mini-swe-agent).

### Implementation Handoff

**AI Agent Guidelines:**
- Follow all locked decisions (Rust 2024 + Tokio + SQLite + Podman + askama + bollard).
- Respect the "single source of truth" modules (`src/output.rs`, `src/exit_code.rs`, `src/cache/key.rs`, `src/report/badges.rs`, `src/sandbox/env_allowlist.rs`).
- Never bypass `Sandbox::run_task` for container creation; never bypass `cache::cell::write` for SQLite writes; never `println!`/`eprintln!` outside `src/output.rs`.
- When extending the Badge enum, update enum + template + README glossary + JSON schema in the same commit.
- Use `#[cfg(target_os = "macos")]` on every macOS-specific module so v1.1 Linux additions are clean diffs.

**First Implementation Priority:**

```bash
cargo new --bin lcrc
```

Then populate `Cargo.toml` with the curated dependency list (Foundation section) + workspace lints (Patterns section), and lay down empty module files matching the Project Structure tree. This is implementation story #1 from the 16-step sequence in Decisions.

### Pre-v1 Owed (not architecture decisions; tracked here for visibility)

- Confirm Scale's redistribution terms for SWE-Bench Pro curated subset.
- Calibrate `*_task_timeout` values empirically on M1 Pro 32GB / 5-model set.
- Fill GHCR organization name (`<org>` placeholder) at release time.

