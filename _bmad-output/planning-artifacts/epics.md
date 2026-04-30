---
stepsCompleted: [1, 2, 3, 4]
status: complete
completedAt: '2026-04-30'
inputDocuments:
  - _bmad-output/planning-artifacts/prd.md
  - _bmad-output/planning-artifacts/architecture.md
designPrinciples:
  - tracer-bullet vertical slices — each epic = thin end-to-end path through all integration layers, demoable on its own
---

# lcrc - Epic Breakdown

## Overview

This document provides the complete epic and story breakdown for lcrc, decomposing the requirements from the PRD and Architecture into implementable stories. (UX Design document intentionally absent: lcrc is a CLI tool with no UI surface.)

**Design principle for epics & stories:** Tracer-bullet vertical slices. Each epic is a thin end-to-end path through all integration layers (cache + sandbox + harness + report + CLI), demoable or verifiable on its own. We prefer many thin slices over few thick ones; we never structure epics horizontally (no "Epic 1: build the database").

## Requirements Inventory

### Functional Requirements

**Installation & First Run**

- FR1: User can install lcrc via Homebrew (`brew install lcrc`) on macOS Apple Silicon.
- FR2: User can run `lcrc scan` immediately after install with zero prior configuration; sensible defaults cover all required behavior.
- FR3: User can invoke `lcrc --version` to see the lcrc semver, the vendored mini-swe-agent version, the vendored SWE-Bench Pro subset version, and the build commit hash.
- FR4: User can invoke `lcrc --help` for a usage summary; per-subcommand help is available via `lcrc <subcommand> --help`.
- FR5: When no eligible models are detected on first run, user can see the empty-machine UX: a one-paragraph explainer plus a hardcoded starter pack of 3–5 small models with exact copy-paste-ready download commands.
- FR6: User can run `lcrc scan` on any installed-model set and see the canary's pass/fail/skipped state rendered prominently in the report header.

**Model Discovery & Eligibility**

- FR7: System can detect installed models in the llama.cpp local cache directory (`~/.cache/llama.cpp/...`).
- FR8: System can compute a format-agnostic content hash (`model_sha`) for each detected model.
- FR9: System can filter detected models by RAM × default-context-length budget, excluding models that would not fit on the user's machine.
- FR10: User can see in the CLI output and the report which detected models were excluded by the fit gate and why (e.g., "RAM-budget exceeded at default ctx").
- FR11: System can extend model discovery to additional directories specified by configuration (`paths.extra_model_dirs` in `~/.config/lcrc/config.toml`).
- FR12: User can restrict any scan, show, or verify operation to a subset of models via `--model <pattern>` (substring match against model name or `model_sha` prefix).

**Measurement Execution**

- FR13: System can run a canary task at the start of every `lcrc scan` invocation regardless of `--depth`.
- FR14: System can render the canary's outcome as one of `canary-pass`, `canary-fail`, or `canary-skipped` in the report header; `canary-fail` does not block the report from being written.
- FR15: System can execute SWE-Bench Pro tasks against each fit-eligible model via mini-swe-agent wrapped as a subprocess.
- FR16: System can run each per-task measurement inside a default-deny isolation envelope structurally implemented as a per-task ephemeral container. The container starts with: no host filesystem mounted (only the per-task workspace bind-mounted read-write), no network access (only a single allowed localhost route to the host's `llama-server` port), no host environment variables (only a documented per-task allowlist of safe variables). Every other host file path, network destination, and environment variable is non-existent from inside the container — blocked by structural construction, not by enumerated policy.
- FR17: System can record sandbox-violation events — any attempted access that the container blocks but the model still tried — as templated badges on the affected row and as report-surfaced events; sandbox violations cause `lcrc scan` to exit with code `2`.
- FR17a: System detects the presence of a supported container runtime at scan pre-flight time. If no supported runtime is available or running, `lcrc scan` exits with code `11` and prints setup instructions to stderr; no measurement is attempted.
- FR17b: System pins the per-task container image (or image-build recipe) per lcrc release; the image identifier is recorded in cell metadata so a measurement is reproducible against the exact toolchain it ran under.
- FR18: System can collect macOS-native perf metrics — tok/s, ttft, peak RSS, power, thermal — for each measured cell; metrics that cannot be collected are recorded as null/unavailable rather than blocking measurement.
- FR19: System can enforce a per-tier per-task wall-clock cap; capped tasks record a timeout-equivalent badge and do not block the scan from continuing.
- FR20: System can execute scans at three depths via `--depth quick|standard|full`; each successive depth extends the previous depth's cells with additional task measurements rather than replacing them.
- FR21: Quick depth runs the canary plus 1 SWE-Bench Pro task per model — specifically task #1 in the static "most-informative-first" task ordering shipped with the curated subset.
- FR22: Standard depth extends each model's cell to 3–5 tasks (Quick's task plus the next 2–4 in the static ordering).
- FR23: Full depth extends each model's cell to the full curated SWE-Bench Pro subset and adds quant/ctx variants beyond the default.

**Cache & Persistence**

- FR24: System can key each measurement cell on `(machine_fingerprint, model_sha, backend_build, params)`, where `machine_fingerprint` = chip generation + RAM size + GPU core count, and `params` = ctx length, sampler temperature, threads, `n_gpu_layers`.
- FR25: System can store and retrieve each `(model, task)` cell independently; cells are the unit of caching, measurement, resumability, and depth extension.
- FR26: System can perform a cache lookup before measuring a cell; cells already present and matching the current cache key are not re-measured within a single scan or across scans.
- FR27: System can persist partial scan results such that Ctrl-C, OOM, or crash mid-scan does not lose completed cells; the next `lcrc scan` invocation resumes by skipping cells already in the cache. No `--resume` flag is required.
- FR28: User can run `lcrc verify --sample N` to re-measure N sampled cached cells and see a numerical drift report (cached value, new value, delta, CI overlap per cell).
- FR29: System defaults `lcrc verify` to warn on drift; cells are not invalidated unless the user re-runs `lcrc scan` against the affected models.
- FR30: System treats macOS patch-level upgrades as machine-fingerprint-stable (cells remain valid); `backend_build` changes invalidate affected cells per the architecture's structural re-measurement policy.
- FR31: System can record per-cell metadata: depth tier that produced the cell, scan timestamp, `backend_build`, lcrc version, vendored harness/task version, perf metrics collected.

**Reporting**

- FR32: System can render a single self-contained static HTML report file to disk; the file requires no external network access to view.
- FR33: System regenerates the HTML report on disk after every cell completes during a scan; the user refreshes the browser tab manually.
- FR34: System renders a canonical screenshot-friendly header on the HTML report containing, without scrolling: machine fingerprint, scan date, lcrc version, `backend_build`, canary state.
- FR35: System renders Wilson-score confidence intervals on every leaderboard pass-rate.
- FR36: System renders templated failure-mode badges on every applicable row from a fixed enum: `ctx-limited`, `OOM-at-N`, `repetition-loop`, `tool-call-format-failure`, `thermal-throttled`, `low-confidence-CI`, plus sandbox-violation tags, plus the architecture-added `task-timeout`, `server-startup-failure`, `server-crashed`. No LLM-generated prose explanations.
- FR37: System tags each leaderboard cell with the depth tier (Quick / Standard / Full) that produced it.
- FR38: Every Quick-tier row carries a `low-confidence-CI` badge by structural default to discourage default-switch decisions on Quick-only data.
- FR39: System writes the HTML report to a default location of `$XDG_DATA_HOME/lcrc/reports/latest.html` plus a timestamped historical file (`report-<ISO8601>.html`); user can override via `--report-path <path>` on `lcrc scan`.
- FR40: User can run `lcrc show` to see a plain-text leaderboard view in the terminal that ranks identically to the HTML report.
- FR41: User can filter `lcrc show` output via `--model <pattern>`, `--depth <tier>`, `--limit N`.
- FR42: User can include cells for uninstalled models or outdated `backend_build`s in `lcrc show` output via `--all` (default: hidden, mirroring HTML report behavior).
- FR43: User can request JSON output via `--format json` on `lcrc show` and `lcrc verify`; JSON outputs carry a top-level `schema_version` field. Default `--format` is `text`.

**CLI Surface, Configuration & Scripting**

- FR44: System runs every command non-interactively; there are no interactive prompts on any subcommand at any depth.
- FR45: System exits with documented, semver-stable exit codes per subcommand: `0` success; `1` canary failed; `2` sandbox violations occurred; `3` scan aborted by signal; `4` cache empty (`lcrc show`); `5` drift detected (`lcrc verify`); `10` configuration error; `11` pre-flight failure; `12` concurrent `lcrc scan` in progress.
- FR46: System writes results (text or JSON) to stdout and progress, diagnostics, and errors to stderr; output streams are pipe-friendly.
- FR47: System emits per-cell completion lines and an estimated-remaining clock to stderr during scan execution; stderr output uses color when stderr is a TTY and plain text otherwise.
- FR48: User can suppress per-cell streaming progress via `--quiet`/`-q` on `lcrc scan`; the report still regenerates after every cell, results still write to disk, exit codes are unchanged.
- FR49: System reads optional configuration from a TOML file at `$XDG_CONFIG_HOME/lcrc/config.toml`; every key has a documented default.
- FR50: System resolves configuration with layered precedence: CLI flag > environment variable > config file > built-in default.
- FR51: System validates the config file on startup; invalid keys or values fail fast with a stderr message pointing at the offending line, exit code `10`.
- FR52: System enforces single-writer concurrency on `lcrc scan` via a lock file at `$XDG_STATE_HOME/lcrc/scan.lock`; concurrent `scan` invocations exit immediately with code `12` and a stderr message identifying the holding PID.
- FR53: System allows `lcrc show` and `lcrc verify` to run concurrently with each other and with a running `lcrc scan` (read-only operations are lock-free).
- FR54: System exposes stable JSON output schemas with backward-compatible additions only within a major version; breaking schema changes bump the major.

### NonFunctional Requirements

**Performance**

- NFR-P1: `lcrc scan --depth quick` on a 5-model fit-eligible installed set completes in ≤25 minutes wall-clock on M1 Pro 32GB, target ~15 minutes. Container spin-up overhead included.
- NFR-P2: `lcrc scan --depth standard` extending a Quick-populated cache for the same 5-model set completes in ~1.5–3 hours wall-clock.
- NFR-P3: `lcrc scan --depth full` for the same 5-model set completes overnight (≤12 hours wall-clock target on the reference rig).
- NFR-P4: No single SWE-Bench Pro task at Quick depth exceeds the architecture-locked cap (working assumption: 600 seconds). Capped tasks record a timeout-equivalent badge.
- NFR-P5: Cache-key lookup before measurement completes in <100 ms for a cache containing up to 10,000 cells.
- NFR-P6: HTML report regeneration after a cell completes finishes in <2 seconds for a cache containing up to 1,000 cells.
- NFR-P7: `lcrc show` returns rendered output in <500 ms for a cache containing up to 1,000 cells. `lcrc --help` and `lcrc --version` return in <200 ms.
- NFR-P8: The CLI estimated-remaining clock during `lcrc scan` updates at least once every 10 seconds; per-cell completion lines appear within 1 second of the cell finishing.
- NFR-P9: Container creation, workspace mount, and shutdown overhead per task is <5 seconds on the reference rig with the chosen runtime.

**Reliability**

- NFR-R1: A `lcrc scan` interrupted by Ctrl-C, OOM, host suspend/resume, or crash loses no completed cells. Next invocation resumes by skipping cells already in the cache, no flags required.
- NFR-R2: A cell write is atomic: a partially-completed measurement either appears fully in the cache after success or does not appear at all. No half-written cells.
- NFR-R3: A cache populated by lcrc version `X.Y.Z` is readable by version `X.Y.(Z+n)` and `X.(Y+n).0`. Major version upgrades may require explicit migration; lcrc must detect a too-old cache schema and exit with a clear error.
- NFR-R4: If perf metrics cannot be collected, affected metrics are recorded as null/unavailable per cell and the scan continues. Missing perf metrics never abort a scan.
- NFR-R5: `llama-server` startup failures, mid-task crashes, hangs, or unexpected exits are detected via timeout and surfaced as a templated badge on the affected cell. The scan continues with the next cell.
- NFR-R6: Repeated `lcrc scan` invocations against an unchanged installed-model set + cache + `backend_build` produce no new measurements (cache hit on every cell). `lcrc verify --sample N` is non-destructive.
- NFR-R7: A concurrent `lcrc scan` invocation never partially overlaps another; the lock file prevents both from progressing simultaneously. `lcrc show` and `lcrc verify` reads remain consistent during a concurrent scan.
- NFR-R8: When `lcrc scan` aborts (Ctrl-C, crash, OOM), any per-task container that was running is torn down by lcrc on best-effort basis; orphaned containers do not accumulate across scans.

**Security**

- NFR-S1: The per-task container starts with: no host filesystem mounted (only the per-task workspace bind-mounted), no network access except a single allowed localhost route to the host's `llama-server` port, and no host environment variables except a documented per-task allowlist. Default-deny by structural construction; not by enumerated policy.
- NFR-S2: Any sandbox-violation event surfaces as a templated badge on the affected row AND causes `lcrc scan` to exit with code `2`. There is no "silent pass" path through the envelope. Acceptance check #9 verifies this with an adversarial-task battery.
- NFR-S3: lcrc requires a supported container runtime to be installed and running. At scan pre-flight, if no supported runtime is detected, lcrc exits with code `11`; no measurement is attempted under any "weak isolation" mode. No `--unsafe-no-sandbox` flag.
- NFR-S4: The container's network configuration permits exactly one outbound destination: the host's `llama-server` on a specific port. DNS, public-internet, host-other-port, and same-bridge other-container connectivity are blocked.
- NFR-S5: The per-task container receives only environment variables on a documented allowlist. Credential-bearing variables (`AWS_*`, `GH_*`, `GITHUB_TOKEN`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `HF_TOKEN`, etc.) are not passed in. The allowlist is finite and documented.
- NFR-S6: The per-task container image is pinned per lcrc release. Image identifier (digest) recorded in cell metadata. Image content (base OS, language toolchains, test runners) is documented; reviewers can read the image spec to verify isolation directly.
- NFR-S7: lcrc collects no usage telemetry, no crash reports, no anonymized usage statistics, no opt-in or opt-out telemetry of any kind in v1.

**Compatibility & Portability**

- NFR-C1: lcrc runs on macOS 12 Monterey or later on Apple Silicon (M1, M2, M3, M4 generations). Intel Mac and pre-Monterey macOS are explicitly unsupported.
- NFR-C2: A `machine_fingerprint` computed before a macOS patch-level upgrade matches the `machine_fingerprint` computed after.
- NFR-C3: A cache populated by lcrc `X.Y.Z` reads correctly under lcrc `X.Y.(Z+n)`. Patch versions never invalidate caches.
- NFR-C4: Vendored `mini-swe-agent` and SWE-Bench Pro subset versions are pinned by lcrc release. lcrc never silently accepts an unpinned version. Container image is similarly pinned.
- NFR-C5: v1 architecture must not preclude Linux NVIDIA support in v1.1 — platform-specific code is factored cleanly such that Linux additions are additive, not architectural rewrites.

**Observability**

- NFR-O1: During `lcrc scan` execution, the CLI emits per-cell completion lines and a per-model progress line to stderr; the estimated-remaining clock updates at least every 10 seconds.
- NFR-O2: lcrc writes only to disk (cache, HTML reports, lock file). lcrc opens no network sockets except (a) localhost to `llama-server` and (b) the local container runtime's control socket.
- NFR-O3: Per NFR-S7, lcrc has no telemetry, crash reporting, or usage analytics surface. Non-negotiable in v1.
- NFR-O4: `lcrc --version` reports lcrc semver, vendored mini-swe-agent version, vendored SWE-Bench Pro subset version, container image identifier, and build commit hash. Sufficient to reproduce a measurement environment from a screenshot or report.

**Integration**

- NFR-I1: lcrc starts and manages `llama-server` instances per measurement (one server per `(model, params)` group). The server runs on the host. Per-task containers connect via a constrained localhost route. Server crashes/hangs detected via documented timeouts and recovered per NFR-R5.
- NFR-I2: lcrc invokes `mini-swe-agent` as a vendored subprocess inside the per-task container (so the agent itself is also subject to the isolation envelope). Subprocess crashes surface as templated badges.
- NFR-I3: lcrc collects macOS perf metrics from the host. v1 mechanism: graceful-degrade-without-power (no privilege required); `power_watts` always NULL in v1, populated by v1.1+ launchd helper.
- NFR-I4: lcrc detects the presence of a supported container runtime at scan pre-flight. lcrc uses any Docker-Engine-API-compatible socket; rootless container support preferred where the runtime offers it.
- NFR-I5: lcrc ships as a Homebrew formula. The formula `depends_on` Podman (packaged default) and `llama.cpp` so that `brew install lcrc` pulls in dependencies.
- NFR-I6: lcrc requires no external service, no API key, no auth flow, no remote endpoint to function in v1.

### Additional Requirements

These are technical requirements from the Architecture document that constrain implementation beyond what the PRD specifies.

**Language, Toolchain & Distribution**

- AR-1: Implementation language is Rust, edition 2024, MSRV pinned to current stable at v1 start (Rust 1.85+); single static binary distribution.
- AR-2: License is Apache-2.0 (single-license).
- AR-3: Single async runtime: Tokio (`#[tokio::main(flavor = "multi_thread")]`). All I/O via `tokio::fs` and `tokio::process`; no `std::fs` / `std::process`; no `block_on` inside async code.
- AR-4: Curated dependency list locked at v1 start: `clap` v4 (CLI), `etcetera` (XDG paths), `is-terminal` + `nu-ansi-term` + `indicatif` (TTY/progress), `serde` + `serde_derive` + `toml` + `figment` (config), `tokio` + `reqwest` (async/HTTP), `bollard` (container API), `rusqlite` (SQLite), `sha2` (hashing), `tempfile` + `fs2`/`fd-lock` (atomicity & lock file), `askama` (HTML templating), `nix` (signals), `anyhow` + `thiserror` (errors), `tracing` + `tracing-subscriber` (logging), `time` (RFC 3339 timestamps), GGUF parser (`ggus` crate or handwritten).
- AR-5: Build & release pipeline via GitHub Actions (`.github/workflows/ci.yml` for fmt/clippy/test/sandbox-negative-test gates; `.github/workflows/release.yml` builds per-arch bottles, publishes container image to GHCR, drafts the GitHub release).
- AR-6: Homebrew formula at `homebrew/lcrc.rb` with `depends_on "podman"` and `depends_on "llama.cpp"`; caveats document `podman machine init && podman machine start` first-run steps.

**Cache Storage**

- AR-7: Cache storage shape is SQLite single file at `{paths.cache_dir}/lcrc.db` with WAL mode for concurrent reads. Schema migration discipline via `PRAGMA user_version` + numbered migration scripts.
- AR-8: Cell schema PK includes seven dimensions: `(machine_fingerprint, model_sha, backend_build, params_hash, task_id, harness_version, task_subset_version)`. `harness_version` and `task_subset_version` are independent PK dimensions.
- AR-9: Cell write is a single atomic SQLite transaction per cell. SIGINT/OOM/crash mid-cell = nothing persisted for that cell; next scan re-measures it. All-or-nothing per cell.
- AR-10: `backend_build` invalidation is structural re-measurement (no compatibility classifier in v1) — `brew upgrade llama.cpp` produces a new `backend_build` string; next scan finds no matching cells and measures fresh; old cells remain accessible via `lcrc show --all`.

**Sandbox & Container Runtime**

- AR-11: Container runtime detection is runtime-agnostic; lcrc never names a runtime in code. Pre-flight probe order: `LCRC_RUNTIME_DOCKER_HOST` → `DOCKER_HOST` → `/var/run/docker.sock` → Podman default socket → exit 11.
- AR-12: Packaged-default runtime is Podman (cross-platform consistency, rootless-by-default, Apache-2.0). Users with their own runtime (Colima, OrbStack, Docker Desktop, Lima, Rancher Desktop) work transparently via the shared Docker Engine API.
- AR-13: Per-task container image is pre-built per lcrc release, published to `ghcr.io/<org>/lcrc-task:<lcrc-version>`, **digest-pinned** in `src/constants.rs`. Dockerfile vendored at `image/Dockerfile` for reviewer verification. Base image: Debian-slim.
- AR-14: Sandbox enforcement is structural, not conventional. Network design: scan creates a custom Docker network with no DNS resolver and no default route to the internet; llama-server reachable from container via `host.docker.internal` at the per-(model,params) port. Filesystem: `docker run` with no `-v` flags except the per-task workspace mount. Env: `--env-file` containing only the documented allowlist; never `--env` of any host var.
- AR-15: Container lifecycle: one ephemeral container per task. All containers and networks tagged `lcrc-scan-id=<uuid>` for backstop GC. On SIGINT/abort: bollard `remove_container` with `force=true`; custom network removed.

**Measurement Methodology**

- AR-16: Pass@1 in v1. Cell schema accommodates v1.1+ pass@k by adding `trial_id` to PK; old cells = `trial_id=0`. No rearchitecture.
- AR-17: Per-tier wall-clock cap mechanism is locked; values are working assumptions calibrated empirically before v1 ship: Quick 600s, Standard 900s, Full 1800s, Canary 120s, Server-startup 60s. If empirical Quick on M1 Pro 32GB / 5-model exceeds the 25-min ceiling, the cap tightens before the budget loosens.
- AR-18: Timeout = fail, recorded with a `task-timeout` badge.
- AR-19: SWE-Bench Pro lifecycle fallback: vendor the curated subset behind a `TaskSource` trait. v1 ships exactly one impl: `SweBenchProSource`. v1.x can swap to alternative `TaskSource` (SWE-Bench Lite cleanup, LiveCodeBench, Multi-SWE-Bench-mini) without rearchitecture.
- AR-20: `Backend` trait abstraction; v1 ships exactly one impl: `LlamaCppBackend`. MLX is deferred to v1.1+ as a focused additive change.
- AR-21: llama-server lifecycle granularity: one server per `(model, params)` combo. Plan groups cells by `(model_sha, params_hash)`; for each group: start server → for each task in group spawn ephemeral container → tear down container → reset KV cache → next task → stop server. Saves model-load cost.
- AR-22: macOS perf collection in v1: graceful-degrade-without-power. v1 collects `tokens_per_sec` (from llama-server API), `ttft_seconds` (HTTP timing), `peak_rss_bytes` (`proc_pid_info` polling, no privilege), `thermal_state` (`IOReport` framework, no privilege). `power_watts` is always NULL in v1.

**Configuration & Environment**

- AR-23: TOML config schema locked: `[paths] cache_dir, report_dir, state_dir`; `[discovery] extra_model_dirs`; `[scan] default_depth, quick_task_timeout, standard_task_timeout, full_task_timeout, canary_task_timeout, server_startup_timeout`; `[runtime] docker_host`; `[backend] llama_server_path`.
- AR-24: Env var convention: `LCRC_<SECTION>_<KEY>` uppercased (e.g., `LCRC_PATHS_CACHE_DIR`, `LCRC_SCAN_DEFAULT_DEPTH`). `LCRC_DISCOVERY_EXTRA_MODEL_DIRS` is colon-separated PATH-style.

**Vendoring & Project Layout**

- AR-25: Vendoring layout: `tasks/swe-bench-pro/` (manifest.json with task list + most-informative ordering, version file, per-task fixtures, canary directory with known-good baseline). `image/Dockerfile` plus `image/requirements.txt` (pinned mini-swe-agent + pytest + ...).
- AR-26: Single-binary crate (no Cargo workspace in v1). Module organization is file-as-module style (no `mod.rs`). One trait per module file: new `Backend` impl → new file in `src/backend/`; new `TaskSource` impl → new file in `src/tasks/`.

**Code Quality & Patterns (Architecture-Locked)**

- AR-27: Workspace lints baked into `Cargo.toml`: `unsafe_code = "forbid"`, `missing_docs = "warn"`, `clippy::pedantic = warn`, `unwrap_used = "deny"` (except tests), `expect_used = "deny"` (except tests), `panic = "deny"` (except tests). CI gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`.
- AR-28: Single-source-of-truth modules:
  - `src/exit_code.rs` — `ExitCode` enum mirroring FR45; **no bare numeric exit codes anywhere else**.
  - `src/output.rs` — the **only** module that writes to stdout/stderr (`println!`/`eprintln!`/etc forbidden elsewhere).
  - `src/cache/key.rs` — canonical `model_sha`, `params_hash`, `machine_fingerprint`, `backend_build` helpers; agents never compute these inline.
  - `src/util/time.rs` — RFC 3339 with `Z` suffix (UTC), millisecond precision; single timestamp helper.
  - `src/config.rs` — single `config::load()` function via `figment`; agents never read env vars or TOML files outside this module.
  - `src/report/badges.rs` — `Badge` enum is single source of truth; extending requires updating enum + template + README glossary + JSON schema in the same commit.
  - `src/sandbox/env_allowlist.rs` — `const` allowlist; extension by code review only.
  - `src/sandbox/container.rs` — only module that calls `bollard::container::Container::create`; all other modules go through `Sandbox::run_task(image, workspace, env_allowlist)` which has no extension args (no `volumes`, no `env`, no `network_mode`).
- AR-29: Error handling: two-layer discipline — `thiserror` typed errors at module boundaries with exit-code mapping; `anyhow::Result` with `.context()` for application-level propagation. Single `main.rs` matches top-level error → `ExitCode` → `process::exit`.
- AR-30: Atomic-write discipline: every cell write is a single SQLite transaction; HTML report write is tempfile + atomic rename; same pattern for `scan.lock` and any "must-not-corrupt" file.
- AR-31: Tracing/logging discipline: `tracing` everywhere with module-pathed targets and structured fields; never `eprintln!` directly; default level `INFO`; expected failures (canary fail, drift, sandbox violation) conveyed via exit codes + report, not `tracing::error!`.

**Acceptance Tests**

- AR-32: Acceptance check #9 (sandbox negative test) is a binding v1-ship gate, implemented at `tests/sandbox_envelope.rs`. Adversarial battery includes: arbitrary host file reads (`cat /etc/passwd`, `find ~/`, `cat /Users/*/Documents/*`), arbitrary outbound network (`curl https://example.com`, DNS resolution), sibling-task workspace enumeration, credential env var reads. Every attempt must fail at the container boundary; run exits with code `2`.
- AR-33: Every variant in `ExitCode` enum has a corresponding integration test in `tests/cli_exit_codes.rs`.
- AR-34: HTML report rendering covered by `insta` snapshot tests in `tests/report_render.rs`.
- AR-35: Acceptance check #8 (CLI mirror identical rank to HTML) is a binding test at `tests/show_mirror.rs`.

**Pre-v1 Owed (tracked here for visibility; not architecture decisions)**

- AR-36: Confirm Scale's redistribution license terms for SWE-Bench Pro curated subset before v1 release. Architecture provides fallback (TaskSource trait + alternative-source contingency) if vendoring is restricted.
- AR-37: Calibrate `*_task_timeout` values empirically on M1 Pro 32GB / 5-model set before v1 ship.
- AR-38: Fill GHCR organization name (`<org>` placeholder throughout code and Dockerfile) at v1 release.

### UX Design Requirements

_Not applicable — lcrc is a CLI tool with no UI surface. The conventional `cli_tool` PRD sections `visual_design`, `ux_principles`, and `touch_interactions` are explicitly skipped per PRD §"Project-Type Overview"._

### FR Coverage Map

| FR | Epic | Note |
|---|---|---|
| FR1 | 7 | Homebrew formula |
| FR2 | 1 → 2 | skeleton scan in 1, real-model scan in 2 |
| FR3 | 1 (stub) → 6 (full self-attestation) | `--version` completes when image+harness+task pinned |
| FR4 | 1 (skeleton) → 6 (full per-subcommand) | clap-derive grows with subcommands |
| FR5 | 2 | empty-machine starter pack |
| FR6 | 2 | canary in HTML header |
| FR7 | 2 | llama.cpp cache scan |
| FR8 | 1 | format-agnostic `model_sha` exercised on real GGUF in Epic 1; applied at scale in Epic 2 |
| FR9 | 2 | RAM × ctx fit gate |
| FR10 | 2 | exclusion visibility |
| FR11 | 6 | `paths.extra_model_dirs` (config-driven) |
| FR12 | 3 | `--model <pattern>` filter |
| FR13 | 2 | canary-first execution |
| FR14 | 2 | 3-state header (`canary-pass` / `canary-fail` / `canary-skipped`) |
| FR15 | 1 → 2 | mini-swe-agent in container (hardcoded in 1; per-model in 2) |
| FR16 | 1 (workspace mount + custom network + image pull) → 2 (env allowlist) | structural default-deny on 2/3 axes in Epic 1; env axis completes in Epic 2 |
| FR17 | 2 | sandbox-violation events + exit 2 |
| FR17a | 1 | preflight container runtime detection + exit 11 |
| FR17b | 1 | image digest pinning + cell metadata |
| FR18 | 2 | perf metrics graceful-degrade |
| FR19 | 2 | per-tier wall-clock cap (Quick) + `task-timeout` badge |
| FR20 | 1 (flag accepted) → 2 (`quick` works) → 3 (`standard`, `full` work) | depth handling lands incrementally |
| FR21 | 2 | Quick = canary + 1 task/model |
| FR22 | 3 | Standard = 3–5 tasks |
| FR23 | 3 | Full = curated subset + quant/ctx variants |
| FR24 | 1 | cell PK with all 7 dimensions |
| FR25 | 1 | independent cell storage |
| FR26 | 1 (lookup-before-measure) → 3 (user-visible cache extension) | cache reuse across depths |
| FR27 | 1 (atomic write) → 2 (Ctrl-C → exit 3 → next scan resumes) | resumability |
| FR28 | 5 | `lcrc verify --sample N` |
| FR29 | 5 | warn-not-invalidate default |
| FR30 | 5 | `machine_fingerprint` stability across OS patches |
| FR31 | 1 → 2 | per-cell metadata (fields populated as features land) |
| FR32 | 1 → 2 | self-contained HTML (one row in 1, full leaderboard in 2) |
| FR33 | 1 | regenerate after every cell |
| FR34 | 2 | canonical screenshot-friendly header (all 5 fields above the fold) |
| FR35 | 2 | Wilson-score CIs |
| FR36 | 2 (initial enum) → 7 (full enum verified by adversarial battery) | templated badges grow as scenarios land |
| FR37 | 2 | depth tier tagging |
| FR38 | 2 | `low-confidence-CI` structural on Quick rows |
| FR39 | 1 (default path) → 3 (timestamped + `--report-path` override) | report output paths |
| FR40 | 4 | `lcrc show` plain-text mirror |
| FR41 | 4 | show filters (`--model`, `--depth`, `--limit`) |
| FR42 | 4 | `--all` (uninstalled / outdated `backend_build`) |
| FR43 | 4 | `--format json` with `schema_version` |
| FR44 | 1 | non-interactive everywhere |
| FR45 | 1 (full enum defined; trigger paths 0/3/11 wired) → 2 (+1, +2 paths) → 4 (+4 path) → 5 (+5 path) → 6 (+10, +12 paths) | enum contract locked in Epic 1; trigger paths fill in per epic |
| FR46 | 1 | `src/output.rs` discipline |
| FR47 | 2 | per-cell streaming + ETA |
| FR48 | 2 | `--quiet` |
| FR49 | 6 | TOML config |
| FR50 | 6 | layered precedence |
| FR51 | 6 | config validation → exit 10 |
| FR52 | 6 | `scan.lock` + exit 12 |
| FR53 | 4 | lock-free reads concurrent with scan |
| FR54 | 4 | stable JSON schema |

## Epic List

Designed as tracer-bullet vertical slices: each epic is a thin end-to-end path through cache + sandbox + harness + llama-server + report + CLI, demoable or verifiable on its own. Epics build on prior modules but each one stands alone in user value.

**Hardening from red-team review (2026-04-30):** Epic 1's sandbox now ships structurally default-deny on workspace + network axes (not just workspace). Epic 1 ships the full `ExitCode` enum (all 9 variants) so the public contract is locked from v0.1.0. Epic 1 exercises the real GGUF parser on a real model file (no synthetic fixtures). Epic 2 adds two early-detection stories: a sandbox adversarial smoke test (subset of acceptance #9) and a Quick budget sanity check on the M1 Pro 32GB reference rig. Both feed back into Epic 2 design before Epic 3 builds on top.

**Second-pass hardening from Code Review Gauntlet + Self-Consistency Validation (2026-04-30):** Folded 9 priority items: (1) Badge enum's 10 variants now have full attachment coverage across Epic 2 (Story 2.18: server-startup-failure, server-crashed, thermal-throttled) and Epic 3 (Story 3.6: ctx-limited, oom-at-n, repetition-loop, tool-call-format-failure). (2) Per-task container image vendoring is now an explicit Story 1.14 (`image/Dockerfile` + `image/requirements.txt` + bootstrap GHCR publish), not implicit in Story 1.10's pull AC. (3) `--report-path` resolved to true OVERRIDE semantics (Story 3.5) — when set, the default `latest.html` and timestamped historical files are NOT written; the override is the single canonical output. (4) Canary task now runs against a hardcoded canary-only model (small GGUF, vendored as release artifact, SHA-pinned) rather than ambiguous orchestration against a discovered model (Stories 2.3 + 2.5). (5) Sandbox port-pinning is now structurally enforced via iptables rules in the runtime's network configuration (Story 1.10 ACs); third-party runtimes that don't expose rule injection cause preflight to exit 11 — no degraded "DNS denial only" mode. (6) Three "decided at implementation time" deferrals are now decided: S1.8 = constraint-violation on same-PK write (lookup-before-measure invariant violation surfaces loudly; no UPSERT); S3.5 = exit 10 (ConfigError) on `--report-path` write failure (no new exit-code variant); S4.2 = `lcrc show --depth <tier>` recomputes pass-rate from filtered cells only (filter is symmetric across tiers). (7) Verify gains a Ctrl-C-during-flight AC (Story 5.1). (8) Release pipeline gains a v0.9.0-rc1 dry-run gate (Story 7.8) so v1.0.0 is the second end-to-end exercise of the release chain, not the first.

### Epic 1: Integration spine — one cell, one row, end-to-end

Lay down the project scaffold and prove every layer interlocks for a single hardcoded measurement. After this epic, `lcrc scan` (with a hardcoded path to one of Theop's real GGUFs) runs the canary task against that model inside a structurally default-deny container (no host filesystem except per-task workspace, no network except a localhost pinhole to llama-server, custom Docker network with no DNS), persists one cell to SQLite, and renders a one-row `latest.html` on disk. The cache, sandbox network envelope, llama-server lifecycle, real GGUF parser (SHA + metadata), harness invocation, and HTML renderer are all wired. The full `ExitCode` enum (all 9 variants) lands in `src/exit_code.rs` — even though most trigger paths fill in across later epics, the public contract is locked from v0.1.0. This epic resolves integration risk before we invest in elaboration.

**Sandbox in this epic is structurally default-deny on 2 of 3 axes (workspace + network); env allowlist enforcement is the only sandbox dimension that slips to Epic 2.**

**FRs covered:** FR2, FR3 (placeholder fields), FR4 (skeleton), FR8 (real `model_sha` from real GGUF parser), FR15, FR16 (workspace mount + custom Docker network with no DNS + llama-server pinhole — env allowlist deferred to Epic 2), FR17a, FR17b, FR20 (flag accepted; only `quick` valid), FR24, FR25, FR26 (lookup-before-measure), FR27 (atomic per-cell write), FR31 (basic metadata), FR32, FR33, FR39 (default path only), FR44, FR45 (full enum defined; trigger paths for exits 0, 3, 11 wired), FR46

### Epic 2: Quick scan — real models, real leaderboard

Extend the spine into the v1 Quick experience. `lcrc scan` discovers installed GGUFs in `~/.cache/llama.cpp/`, applies the RAM × ctx fit gate (with exclusion visibility), runs the canary first with 3-state header rendering, then runs 1 SWE-Bench Pro task per model from the static "most-informative-first" ordering. The sandbox now adds the env allowlist enforcement on top of Epic 1's workspace + network envelope, completing structural default-deny on all three axes. The CLI streams per-cell completion with an ETA; the HTML report grows the canonical screenshot-friendly header, Wilson CIs on every row, structural `low-confidence-CI` badge, depth tier tagging, and templated failure badges. Sandbox-violation events are detected and surfaced (exit 2). Resumability works end-to-end (Ctrl-C → exit 3 → next scan skips cached cells). Empty-machine UX directs to a hardcoded starter pack.

**Two early-detection stories ship in this epic to surface architectural failures while there's still time to fix them:**
- **Sandbox adversarial smoke test:** 3–4 of the highest-signal probes from acceptance check #9 (`cat /etc/passwd`, `curl https://example.com`, `env | grep TOKEN`, sibling-workspace fishing) run as a fast integration test. Surfaces structural sandbox holes in Epic 2 instead of Epic 7. Full adversarial battery still gates v1 ship in Epic 7.
- **Quick budget sanity check:** Run `lcrc scan` on M1 Pro 32GB / 5-model, log wall-clock, fail loudly if >25 min. If Quick blows the budget, we tighten per-task cap or task selection *now* — before Epic 3 builds Standard/Full on top of an over-budget Quick. Final calibration of exact `*_task_timeout` values still happens in Epic 7.

**FRs covered:** FR5, FR6, FR7, FR9, FR10, FR13, FR14, FR15 (per-model), FR16 (env allowlist — completes structural default-deny started in Epic 1), FR17, FR18, FR19, FR20 (`quick`), FR21, FR27 (full Ctrl-C resumability), FR31 (more metadata), FR32 (full leaderboard), FR34, FR35, FR36 (initial badge enum), FR37, FR38, FR45 (trigger paths for exits 1, 2 wired), FR47, FR48

### Epic 3: Standard & Full depths — cache extension proves cache-as-product

Add the two heavier tiers and per-model scoping. `lcrc scan --depth standard` extends each cell to 3–5 tasks, skipping every cell already in the cache from a prior Quick scan; `--depth full` extends to the complete curated subset plus quant/ctx variants, intended for overnight runs. `--model <pattern>` lets the user scope a scan to a single new model. `--report-path` overrides the default output location; timestamped historical report files are written alongside `latest.html`.

**FRs covered:** FR12, FR20 (`standard`, `full`), FR22, FR23, FR26 (cache extension visible to user), FR39 (timestamped + `--report-path`)

### Epic 4: `lcrc show` — read-only leaderboard view

Add the terminal-side leaderboard. `lcrc show` prints a plain-text fixed-width table that ranks identically to the HTML report (acceptance check #8 is a binding test). `--format json` produces stable JSON output with a top-level `schema_version` field, pipe-friendly to `jq`. Filters: `--model <pattern>`, `--depth <tier>`, `--limit N`, `--all` (include cells for uninstalled models or outdated `backend_build`s, mirroring HTML behavior). All `lcrc show` invocations open SQLite read-only and may run concurrently with an active `lcrc scan`.

**FRs covered:** FR40, FR41, FR42, FR43, FR45 (+ exit 4 for empty cache), FR53, FR54

### Epic 5: `lcrc verify` — drift detection

Add the trust-audit surface. `lcrc verify --sample N` re-measures N randomly-sampled cached cells inside the same sandbox envelope and emits a numerical drift report (cached value, new value, delta, CI overlap per cell). Default behavior is **warn**, not invalidate — to act on drift, the user runs `lcrc scan` against the affected models. Exit code 5 when drift is detected; `--format json` for machine-readable drift output. `machine_fingerprint` stability across macOS patch-level upgrades is verified here (FR30) since drift detection is its first user-facing surface.

**FRs covered:** FR28, FR29, FR30, FR45 (+ exit 5)

### Epic 6: Config, concurrency & CLI polish — safe for cron / CI / Makefile

Make lcrc production-ready as a scriptable CLI. TOML config at `$XDG_CONFIG_HOME/lcrc/config.toml` with figment-backed layered precedence (CLI flag > env var > TOML > built-in default). Config validation fails fast with line-pointing errors (exit 10). The `scan.lock` lock file enforces single-writer concurrency on `lcrc scan` (exit 12 with holding PID on stderr); `lcrc show` and `lcrc verify` remain lock-free. `paths.extra_model_dirs` extends discovery beyond the default llama.cpp cache. `lcrc --version` carries full self-attestation (vendored mini-swe-agent version + SWE-Bench Pro subset version + container image digest + commit hash); `lcrc --help` and per-subcommand help are complete and polished.

**FRs covered:** FR3 (full self-attestation), FR4 (full per-subcommand help), FR11, FR45 (+ exits 10, 12), FR49, FR50, FR51, FR52

### Epic 7: Distribution, sandbox audit & calibration — v1 ship gate

Ship-gate epic. Homebrew formula (`brew install lcrc` works end-to-end on a clean Mac with `depends_on "podman" + "llama.cpp"`); GitHub Actions release workflow builds per-arch bottles and publishes the per-task container image to GHCR with digest pinning. Acceptance check #9 sandbox negative test is a binding test in `tests/sandbox_envelope.rs` running the **full adversarial battery** (every probe documented in AR-32, not just the smoke-test subset from Epic 2) — every out-of-envelope attempt must fail at the container boundary (exit 2). Per-tier `*_task_timeout` values are **finally locked** based on data gathered from Epic 2's sanity check + ongoing Epic 3 runs. README, badge glossary, and JSON schema docs are published. SWE-Bench Pro redistribution license is confirmed (or fallback path documented). GHCR organization name is filled.

**FRs covered:** FR1, FR36 (full enum verified by full adversarial battery); also binding verification of NFR-S1–S6 via acceptance check #9. Resolves AR-32 (full battery), AR-36, AR-37 (final lock), AR-38.

---

## Epic 1: Integration spine — one cell, one row, end-to-end

**Goal:** Lay down the project scaffold and prove every layer interlocks for a single hardcoded measurement. After Epic 1, `lcrc scan` (with a hardcoded path to one of Theop's real GGUFs) runs the canary task against that model inside a structurally default-deny container (workspace mount + custom Docker network with no DNS + llama-server pinhole), persists one cell to SQLite, and renders a one-row `latest.html`. The full `ExitCode` enum lands in `src/exit_code.rs` from day one.

### Story 1.1: Project scaffold with locked workspace lints

As a developer (Theop or future contributor),
I want a Rust project scaffolded with the architecture's curated dependencies and workspace lints baked into `Cargo.toml`,
So that quality discipline is enforced from the first commit and AI agents inherit the same bar as humans.

**Acceptance Criteria:**

**Given** a fresh clone of the repo
**When** I run `cargo build`
**Then** the build succeeds on Rust 1.85+ stable with edition 2024.

**Given** the project root
**When** I inspect `Cargo.toml`
**Then** `[lints.rust]` declares `unsafe_code = "forbid"` and `[lints.clippy]` declares `pedantic = warn`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"` (with test-only exemptions).

**Given** the project root
**When** I inspect the dependency list
**Then** it matches the architecture's locked set: `clap` v4, `etcetera`, `is-terminal`, `nu-ansi-term`, `indicatif`, `serde`, `serde_derive`, `toml`, `figment`, `tokio`, `reqwest`, `bollard`, `rusqlite`, `sha2`, `tempfile`, `fs2`, `askama`, `nix`, `anyhow`, `thiserror`, `tracing`, `tracing-subscriber`, `time`, plus a GGUF parser dependency or module placeholder.

**Given** the project root
**When** I run `cargo fmt --check`
**Then** it succeeds (rustfmt config present).

**Given** the project root
**When** I look at `rust-toolchain.toml`
**Then** it pins MSRV to current stable (Rust 1.85+).

### Story 1.2: CI workflow gates fmt, clippy, and tests

As a developer,
I want every push and PR to be gated by `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`,
So that broken commits never land on `main`.

**Acceptance Criteria:**

**Given** `.github/workflows/ci.yml` exists
**When** I push a commit that fails `cargo fmt --check`
**Then** CI fails and blocks merge.

**Given** the same workflow
**When** I push a commit that triggers a clippy warning
**Then** CI fails (warnings denied via `-D warnings`).

**Given** the same workflow
**When** I push a commit that breaks an existing test
**Then** CI fails.

**Given** a clean commit
**When** I push
**Then** CI runs all three gates and reports green within a reasonable budget (target: <5 min on GitHub-hosted macOS runners).

### Story 1.3: Output module + full ExitCode enum + error layer

As a developer,
I want all process output routed through `src/output.rs` and all exit codes routed through the `ExitCode` enum (with all 9 variants defined from day one),
So that the CLI contract per FR45 is locked from v0.1.0 and stdout/stderr discipline is structurally enforced.

**Acceptance Criteria:**

**Given** the codebase outside `src/output.rs` and `tests/`
**When** I grep for `println!|eprintln!|print!|eprint!|dbg!`
**Then** there are zero matches.

**Given** `src/exit_code.rs`
**When** I inspect the `ExitCode` enum
**Then** it declares all 9 variants `{Ok=0, CanaryFailed=1, SandboxViolation=2, AbortedBySignal=3, CacheEmpty=4, DriftDetected=5, ConfigError=10, PreflightFailed=11, ConcurrentScan=12}` with `#[repr(i32)]` — even though most trigger paths are wired in later epics.

**Given** `src/main.rs`
**When** the top-level error is matched
**Then** it converts to an `ExitCode` and calls `process::exit(code as i32)` exactly once; no other module calls `process::exit` directly.

**Given** `src/error.rs`
**When** I inspect it
**Then** it defines a top-level `Error` type with `From` impls for each module-level typed error, mapping each to the appropriate `ExitCode` variant.

**Given** any non-test module
**When** I grep for `unwrap()`, `expect(`, or `panic!(`
**Then** there are zero matches outside test code.

### Story 1.4: clap CLI root + `lcrc --version` + `lcrc --help` + tracing subscriber

As Theop,
I want `lcrc --version` and `lcrc --help` to work end-to-end,
So that I can verify lcrc is installed and discover its subcommand surface.

**Acceptance Criteria:**

**Given** a built `lcrc` binary
**When** I run `lcrc --version`
**Then** it prints `lcrc <semver> (build <commit-short>)` plus placeholder lines for vendored mini-swe-agent / SWE-Bench Pro subset / container image (filled in Epic 6).

**Given** the binary
**When** I run `lcrc --help`
**Then** it prints a usage summary listing subcommands `scan`, `show`, `verify` (subcommand bodies stubbed where not yet implemented).

**Given** the binary
**When** I run `lcrc <subcommand> --help`
**Then** it prints per-subcommand usage from clap-derive.

**Given** any invocation
**When** tracing emits an event
**Then** it writes to stderr with module-pathed targets and structured fields (default level INFO; subscriber installed in `src/util/tracing.rs`).

**Given** `lcrc --version`
**When** measured cold
**Then** it returns in <200 ms (NFR-P7).

### Story 1.5: Machine fingerprint module

As a developer,
I want `MachineFingerprint::detect()` to return a deterministic string of the form `"<chip>-<ram_gb>GB-<gpu_cores>gpu"`,
So that cells can be keyed by hardware identity stably across macOS patch upgrades.

**Acceptance Criteria:**

**Given** a Mac running on Apple Silicon
**When** I call `MachineFingerprint::detect()`
**Then** it returns a string like `"M1Pro-32GB-14gpu"` (chip generation + RAM size + GPU core count).

**Given** the same Mac before and after a macOS patch-level upgrade (simulated in unit test)
**When** I call `detect()` both times
**Then** the strings are byte-identical (NFR-C2).

**Given** a non-Apple-Silicon platform (Intel Mac, Linux)
**When** I call `detect()`
**Then** it returns `Err` with a clear "unsupported hardware" message (NFR-C1).

**Given** unit tests with mocked `sysctl` output
**When** they run
**Then** they cover M1, M1 Pro, M1 Max, M2, M3, M4 chip detection.

### Story 1.6: Cache key helpers in `src/cache/key.rs`

As a developer,
I want canonical helpers for all four cache-key components (`model_sha`, `params_hash`, `machine_fingerprint`, `backend_build`) in `src/cache/key.rs`,
So that no agent computes them inline and inconsistently (per AR-28).

**Acceptance Criteria:**

**Given** a real GGUF file path
**When** I call `key::model_sha(path)`
**Then** it returns the SHA-256 hex digest of the file contents, computed via streaming (no full-file load into memory).

**Given** a `Params { ctx, temp, threads, n_gpu_layers }` struct
**When** I call `key::params_hash(&params)`
**Then** it returns the SHA-256 of canonical JSON (BTreeMap-sorted keys), so equivalent params hash identically regardless of struct field ordering.

**Given** a `BackendInfo { name, semver, commit_short }`
**When** I call `key::backend_build(&info)`
**Then** it returns the formatted string `"<name>-<semver>+<commit_short>"` (e.g., `"llama.cpp-b3791+a1b2c3d"`).

**Given** a developer greps the codebase for `model_sha|params_hash|backend_build` outside `src/cache/key.rs`
**When** they inspect matches
**Then** every match is a *call* to a helper, never inline computation.

### Story 1.7: SQLite schema + migrations framework

As a developer,
I want a SQLite cache file with `PRAGMA user_version` migration discipline and the full `cells` table schema per AR-8,
So that cells can be persisted with the architecture-locked PK and the cache survives lcrc patch upgrades (NFR-R3).

**Acceptance Criteria:**

**Given** lcrc is invoked for the first time
**When** the cache initializes
**Then** it creates `{paths.cache_dir}/lcrc.db` with WAL mode enabled (`PRAGMA journal_mode=WAL`).

**Given** the cache file
**When** I inspect the schema
**Then** the `cells` table matches the architecture spec — 7-dim PK (`machine_fingerprint, model_sha, backend_build, params_hash, task_id, harness_version, task_subset_version`) plus all metadata columns (`container_image_id`, `lcrc_version`, `depth_tier`, `scan_timestamp`, `pass`, `duration_seconds`, `tokens_per_sec`, `ttft_seconds`, `peak_rss_bytes`, `power_watts` (nullable), `thermal_state`, `badges` (JSON array)).

**Given** a cache populated by lcrc `0.1.0`
**When** lcrc `0.1.1` opens it
**Then** it reads cleanly without migration (NFR-R3 patch durability).

**Given** a cache with `PRAGMA user_version = N` and lcrc expects `N+1`
**When** lcrc opens it
**Then** the matching numbered migration script runs and bumps `user_version` to `N+1`.

**Given** a cache with `PRAGMA user_version` newer than lcrc supports
**When** lcrc opens it
**Then** it returns `Err(CacheError::FutureSchema)` and the CLI exits with a clear stderr message.

### Story 1.8: Cache cell write/read API with atomic semantics

As a developer,
I want `Cache::write_cell(&Cell) -> Result<()>` and `Cache::lookup_cell(&CellKey) -> Result<Option<Cell>>` with single-transaction atomic semantics,
So that no half-written cells exist (NFR-R2) and lookups before measurement work correctly (FR26).

**Acceptance Criteria:**

**Given** a fresh cache
**When** I call `write_cell(&cell)`
**Then** the cell is inserted within a single SQLite transaction; subsequent `lookup_cell()` for the same PK returns `Ok(Some(cell))`.

**Given** a `write_cell()` invocation that aborts mid-transaction (simulated by panic inside the transaction in a test)
**When** the cache is reopened
**Then** the cell is NOT present (NFR-R2).

**Given** a cache with up to 10,000 cells (populated synthetically in a test)
**When** I call `lookup_cell()` for an existing key
**Then** it returns `Ok(Some(cell))` in <100 ms (NFR-P5).

**Given** the same cache
**When** I call `lookup_cell()` for a nonexistent key
**Then** it returns `Ok(None)` in the same budget.

**Given** two `write_cell()` calls target the same PK (whether concurrent or sequential)
**When** the second call attempts its INSERT
**Then** it returns `Err(CacheError::DuplicateCell)` mapped from `SQLITE_CONSTRAINT_PRIMARYKEY`. UPSERT is **not** used: the lookup-before-measure invariant (FR26) plus single-writer enforcement (FR52, `scan.lock`) means a same-PK write indicates an upstream bug — surfacing it loudly is the design, not a defect to paper over with silent overwrite.

### Story 1.9: Container runtime preflight with socket precedence chain

As Theop,
I want `lcrc scan` to fail fast with helpful setup instructions if no container runtime is reachable,
So that I'm not stuck debugging a cryptic Docker socket error (FR17a, NFR-S3).

**Acceptance Criteria:**

**Given** a Mac with no container runtime installed
**When** I run `lcrc scan`
**Then** it exits 11 and prints to stderr setup instructions including `brew install podman` + `podman machine init` + `podman machine start`.

**Given** a Mac with a runtime installed but the machine not started
**When** I run `lcrc scan`
**Then** it exits 11 with instructions to start the machine.

**Given** a Mac with `podman machine` running (or any Docker-Engine-API-compatible socket reachable)
**When** I run `lcrc scan`
**Then** preflight succeeds and the scan proceeds to the next stage.

**Given** the env var `LCRC_RUNTIME_DOCKER_HOST` is set to a reachable socket
**When** I run `lcrc scan`
**Then** preflight uses that socket first, ahead of `DOCKER_HOST` and the auto-probe paths.

**Given** preflight probes the precedence chain in order `LCRC_RUNTIME_DOCKER_HOST` → `DOCKER_HOST` → `/var/run/docker.sock` → Podman default socket
**When** all probes fail
**Then** exit 11 with the setup instructions.

**Given** the preflight succeeds via socket X
**When** the run continues
**Then** stderr logs `detected container runtime at <socket-path>` at INFO level.

### Story 1.10: `Sandbox::run_task` with workspace mount + custom default-deny network

As a developer,
I want `Sandbox::run_task(image_digest, workspace_path) -> Result<TaskOutcome>` to spawn an ephemeral container with a workspace bind-mount, no other host filesystem visibility, and a custom Docker network whose outbound traffic is restricted by iptables rules to a single host:port (the llama-server),
So that every measurement runs in a structurally default-deny envelope from Epic 1 onward — port-pinning enforced structurally at the network layer, not by best-effort DNS denial alone (FR16 workspace + network axes; env axis follows in Epic 2).

**Acceptance Criteria:**

**Given** the pinned container image digest in `src/constants.rs`
**When** `Sandbox::run_task` is called and the image is not yet local
**Then** it pulls the image from GHCR, verifies the digest matches the constant, and caches it locally.

**Given** a per-task `workspace_path`
**When** the container starts
**Then** `/workspace` inside the container is the bind-mounted host path (rw); inspecting from inside the container shows no other host directories visible.

**Given** the container starts
**When** the agent attempts `cat /etc/passwd` (intending the host's)
**Then** it reads the *image's* `/etc/passwd` (Debian-slim's default), not the host's — the host filesystem is structurally absent.

**Given** the container starts
**When** the agent attempts `curl https://example.com`
**Then** the request fails (no DNS resolver on the custom network; no default route to the internet).

**Given** the container starts and `host.docker.internal:<llama-port>` is reachable from the host
**When** the agent connects to that endpoint
**Then** the connection succeeds — this is the only allowed network destination.

**Given** the container is started and the custom Docker network is configured
**When** the agent attempts `nc -zv host.docker.internal 22` (or any host-gateway port other than the llama-server's)
**Then** the connection fails (refused or timeout) — outbound is structurally restricted to a single host:port via iptables rules installed in the runtime's network namespace, not by best-effort DNS denial alone.

**Given** lcrc's packaged-default runtime is Podman (per AR-12)
**When** lcrc creates the per-scan network
**Then** iptables rules (configured via Podman's CNI/Netavark backend) drop all container outbound traffic except DNAT'd packets to the host's llama-server port; rule installation is verified at scan preflight by exercising the negative probe above against a probe sentinel port.

**Given** a third-party Docker-API-compatible runtime (Colima, OrbStack, Docker Desktop) is detected at preflight
**When** lcrc cannot install equivalent iptables/nftables rules through the runtime's exposed surface
**Then** lcrc exits 11 with a documented "structural port-pin unavailable on this runtime; reinstall with the packaged Podman or use a runtime that exposes network rule injection" message. Per NFR-S3, no `--unsafe-no-sandbox` fallback exists; either the sandbox is structural or scan refuses to run.

**Given** `Sandbox::run_task` returns (success, failure, or panic)
**When** I check container state
**Then** the container has been removed via `bollard::remove_container(force=true)`; no orphan containers accumulate (NFR-R8).

**Given** the function signature of `Sandbox::run_task`
**When** inspected
**Then** it accepts NO `volumes`, `env`, or `network_mode` extension arguments — workspace mount and network construction are hard-coded internally (AR-28 structural enforcement).

### Story 1.11: llama-server lifecycle

As a developer,
I want `LlamaServer::start(model_path, params) -> Result<ServerHandle>` to spawn `llama-server`, poll `/health` until ready (with a configurable timeout), and terminate cleanly on `Drop`,
So that each measurement has a known-ready server to talk to and no orphan processes leak (NFR-I1, NFR-R5).

**Acceptance Criteria:**

**Given** a real GGUF and `Params { ctx: 4096, ... }`
**When** I call `LlamaServer::start`
**Then** it spawns `llama-server` on a free localhost port and returns a handle once `/health` returns OK (within `server_startup_timeout`, default 60s).

**Given** a returned `ServerHandle`
**When** I drop it
**Then** `llama-server` is terminated (SIGTERM, brief wait, SIGKILL fallback) and the port is freed.

**Given** a model file that fails to load (corrupt GGUF in a test fixture)
**When** I call `start`
**Then** it returns `Err(ServerStartupFailure)` with the failure reason; no orphan process exists.

**Given** the server hangs on startup (mocked in test)
**When** the timeout expires
**Then** `start` returns `Err(ServerStartupFailure)` and any spawned process is killed.

**Given** two concurrent `start` calls in a test scenario
**When** both complete
**Then** they bind to different ports and operate independently.

### Story 1.12: End-to-end one-cell scan (no HTML yet)

As Theop,
I want `lcrc scan` (with a hardcoded GGUF path baked in for now) to run the canary task against that model inside the sandbox, capture pass/fail, and persist exactly one cell to the cache,
So that I can verify every layer of the integration spine works end-to-end before we invest in elaboration.

**Acceptance Criteria:**

**Given** a working preflight, a real GGUF at the hardcoded path, and an empty cache
**When** I run `lcrc scan`
**Then** it executes the full pipeline: (1) preflight succeeds, (2) llama-server starts for the model, (3) cell key is computed via `cache::key`, (4) cache lookup misses, (5) `Sandbox::run_task` spawns the container with workspace + network envelope, (6) mini-swe-agent runs the canary task inside the container against the localhost llama-server pinhole, (7) outcome (pass/fail + duration) is captured, (8) cell is written atomically with all metadata, (9) container + server are torn down, (10) process exits 0.

**Given** the cell was written
**When** I inspect the SQLite file directly with `SELECT * FROM cells`
**Then** there is exactly one row with all PK columns and metadata populated (`depth_tier='quick'`, `scan_timestamp` is RFC 3339 UTC, `container_image_id` matches the pinned digest, `lcrc_version` matches the binary).

**Given** I run `lcrc scan` a second time with no changes
**When** it executes
**Then** preflight succeeds, cache lookup hits (FR26), no measurement runs, exit 0 — fully idempotent (NFR-R6).

**Given** Ctrl-C is sent during the in-container measurement
**When** the SIGINT handler fires
**Then** no cell is written for the in-progress task, the container is torn down on best-effort basis, exit 3 (FR27 + NFR-R8).

**Given** preflight fails (no runtime reachable)
**When** I run `lcrc scan`
**Then** exit 11 with setup instructions; no measurement attempted; no llama-server spawned.

### Story 1.13: One-row HTML report rendering

As Theop,
I want `lcrc scan` to write a `latest.html` file containing one row showing the canary measurement after each cell write,
So that I can open the report in a browser and verify the renderer interlocks with the cache (FR32, FR33, FR39 default path only).

**Acceptance Criteria:**

**Given** a successful `lcrc scan` produces a cell
**When** the scan completes
**Then** `$XDG_DATA_HOME/lcrc/reports/latest.html` exists.

**Given** that file
**When** I open it in a browser
**Then** it renders one row containing model name, task name, pass/fail, scan timestamp. (No Wilson CIs, no badges, no canonical header, no streaming — those land in Epic 2.)

**Given** the file is being written
**When** the write process is killed midway (simulated)
**Then** the existing `latest.html` (if any) is unaffected — the writer uses tempfile + atomic rename per AR-30.

**Given** the file is opened in an offline browser (no network)
**When** it loads
**Then** it renders fully — no external CSS, JS, fonts, or images (FR32 self-contained).

**Given** the HTML render completes
**When** I inspect the file mode
**Then** it is readable by the user and not world-writable.

### Story 1.14: Vendor per-task container image (Dockerfile + requirements + bootstrap GHCR publish)

As a developer,
I want `image/Dockerfile` and `image/requirements.txt` vendored in the repo with the per-task base image (Debian-slim) and a pinned mini-swe-agent + pytest + minimal toolchain, plus an initial manual publish to GHCR producing the digest referenced by `src/constants.rs::CONTAINER_IMAGE_DIGEST`,
So that Story 1.10's `Sandbox::run_task` image-pull has something to pull, and Story 7.3's automated release workflow takes over from a known-good baseline rather than bootstrapping from scratch at v1.0.0.

**Acceptance Criteria:**

**Given** the repo at `image/`
**When** I inspect it
**Then** `image/Dockerfile` exists with `FROM debian:bookworm-slim` (per AR-13), pinned `python3` + `pytest` + `git` + minimal toolchain via apt, and a `COPY image/requirements.txt` step installing `mini-swe-agent` at a pinned version. The Dockerfile is short (~30 lines or less); reviewers can read it end-to-end to verify isolation per NFR-S6.

**Given** `image/requirements.txt`
**When** I inspect it
**Then** every dependency is version-pinned (`mini-swe-agent==X.Y.Z`, `pytest==A.B.C`, etc.); no unpinned `>=` or floating versions; no `RUN curl | bash` patterns in the Dockerfile; every external download is hash-verified or apt-pinned.

**Given** the initial image build (manual, pre-Story-7.3 automation)
**When** the maintainer runs `docker build image/ -t ghcr.io/<org>/lcrc-task:0.1.0` and pushes to GHCR
**Then** the resulting image digest (`sha256:...`) is captured and written to `src/constants.rs::CONTAINER_IMAGE_DIGEST`; this digest is what Story 1.10's pull verifies against. The bootstrap publish process is documented in `docs/release-process.md` as the historical bootstrap, superseded by Story 7.3's automation at v1 ship.

**Given** the image is published and `CONTAINER_IMAGE_DIGEST` is set
**When** Story 1.10's integration test runs
**Then** the image pull verifies the digest matches and the test passes; if the constant doesn't match the published image (e.g., someone repushed without updating the constant), the test fails loudly.

**Given** the AR-38 placeholder `<org>` is still unresolved at Story 1.14 time
**When** I check this story's deliverables
**Then** Story 1.14 ships with a placeholder `<org>` initially; the actual org name is filled by Story 7.3 (which also re-tags + re-publishes under the real org and updates the digest constant). This story's job is to prove the build-and-publish chain works at all; org naming is its own concern.

---

## Epic 2: Quick scan — real models, real leaderboard

**Goal:** Extend the spine into the v1 Quick experience. `lcrc scan` discovers installed GGUFs in `~/.cache/llama.cpp/`, applies the RAM × ctx fit gate (with exclusion visibility), runs the canary first with 3-state header rendering, then runs 1 SWE-Bench Pro task per model from the static ordering inside the now-fully-default-deny sandbox (env allowlist completes axis 3). The CLI streams per-cell completion with an ETA; HTML report carries the canonical header, Wilson CIs, structural `low-confidence-CI`, depth-tier tags, and templated badges. Sandbox-violation events surface (exit 2). Two early-detection stories (sandbox adversarial smoke test + Quick budget sanity check) feed back into Epic 2 design before Epic 3.

### Story 2.1: `Backend` trait + `LlamaCppBackend` impl with llama.cpp model discovery

As a developer,
I want the `Backend` trait abstraction (per AR-20) and one impl (`LlamaCppBackend`) that walks `~/.cache/llama.cpp/...` and returns discovered models,
So that v1 has a clean extension slot for v1.1+ MLX support and discovery is decoupled from the rest of the orchestrator.

**Acceptance Criteria:**

**Given** `src/backend/mod.rs` (or trait module per AR-26)
**When** I inspect the `Backend` trait
**Then** it declares `name()`, `version()`, `discover_models()`, `estimate_memory()`, `start_server()` methods per the architecture spec.

**Given** a Mac with GGUFs in `~/.cache/llama.cpp/...`
**When** I call `LlamaCppBackend::discover_models()`
**Then** it returns a `Vec<ModelRef>` containing every `*.gguf` file found, with paths and inferred display names.

**Given** an empty `~/.cache/llama.cpp/` directory
**When** I call `discover_models()`
**Then** it returns `Ok(vec![])` (empty vector, not an error).

**Given** a non-readable `~/.cache/llama.cpp/` (permissions stripped in test)
**When** I call `discover_models()`
**Then** it returns `Err` with a clear message; lcrc surfaces this as preflight failure (exit 11).

**Given** the `Backend` trait
**When** I check `src/backend/`
**Then** there is exactly one impl file (`llama_cpp.rs`) per AR-26 (one trait per module file).

### Story 2.2: GGUF parser + RAM × ctx fit gate with exclusion visibility

As Theop,
I want lcrc to inspect each discovered GGUF's metadata and exclude models that won't fit in RAM at their default context length, with the exclusions surfaced in CLI output and the report,
So that I never wait on a measurement that's doomed to OOM, and I can see exactly which models were skipped and why (FR9, FR10).

**Acceptance Criteria:**

**Given** a real GGUF file
**When** I call the parser to read its header
**Then** I get `n_params`, default `n_ctx`, quantization tag, and any other metadata needed by the fit-gate formula.

**Given** a `MachineFingerprint` (from Story 1.5) and a list of discovered `ModelRef`s
**When** I run `fit_gate::filter(machine, models)`
**Then** I receive a partition: `{eligible: Vec<ModelRef>, excluded: Vec<(ModelRef, Reason)>}` where `Reason` is e.g. `"RAM-budget exceeded at default ctx (estimated 38GB > 32GB)"`.

**Given** the fit gate ran
**When** the scan continues
**Then** stderr logs each excluded model + reason at INFO level, and the HTML report renders an "Excluded models" section listing each exclusion.

**Given** a corrupted GGUF (truncated header)
**When** the parser reads it
**Then** it returns `Err` with the file path and a clear message; the model is treated as excluded with reason `"unparseable GGUF header"`; scan does not abort.

### Story 2.3: `TaskSource` trait + `SweBenchProSource` impl + vendored task data

As a developer,
I want the `TaskSource` trait abstraction (per AR-19) plus one impl (`SweBenchProSource`) backed by vendored data at `tasks/swe-bench-pro/`,
So that v1 ships a single curated task source while leaving a clean slot for v1.x fallback (per AR-19) and v1.1+ custom evals.

**Acceptance Criteria:**

**Given** `src/tasks/mod.rs`
**When** I inspect the `TaskSource` trait
**Then** it declares `name()`, `version()`, `list_tasks()`, `load_task()`, `evaluate()` per the architecture spec.

**Given** the repo
**When** I check `tasks/swe-bench-pro/`
**Then** I find `manifest.json` (with `version` field, `tasks[]` array in static "most-informative-first" order, `canary` reference), per-task fixtures under `tasks/`, and a canary task under `canary/` with a known-good baseline plus a hardcoded canary model fixture (small GGUF, <1GB, SHA-256-pinned). The canary model is shipped as a release artifact (not committed to git; not git-LFS) and resolved at first scan via lcrc's asset-fetch helper into `$XDG_DATA_HOME/lcrc/canary/`; subsequent scans use the cached file. Hash is verified on every load.

**Given** `SweBenchProSource::list_tasks()`
**When** called
**Then** it returns task IDs in the manifest's static order (no shuffling).

**Given** a `TaskId` and a per-task `workspace_path`
**When** I call `evaluate(id, workspace)` (after running mini-swe-agent inside the workspace)
**Then** it returns `TaskOutcome { pass: bool, reason: Option<String> }` based on the task's documented evaluation rule (typically a pytest run or a check script).

**Given** the source
**When** I call `version()`
**Then** it returns the `version` field from `manifest.json` — this becomes the cell's `task_subset_version` PK dimension.

### Story 2.4: Initial Badge enum + HTML badge rendering pipeline

As a developer,
I want the `Badge` enum populated with the v1 starting set (per AR-28: `LowConfidenceCi`, `SandboxViolation`, `TaskTimeout`, `ServerStartupFailure`, `ServerCrashed`, plus FR36's existing `CtxLimited`, `OomAtN`, `RepetitionLoop`, `ToolCallFormatFailure`, `ThermalThrottled`) and the HTML template rendering badges as templated chips,
So that subsequent stories (sandbox violation, task timeout, low-confidence CI) can attach badges without inventing the rendering pipeline.

**Acceptance Criteria:**

**Given** `src/report/badges.rs`
**When** I inspect the `Badge` enum
**Then** it declares all 10 variants with `#[serde(rename_all = "kebab-case")]` (so serialization produces `"low-confidence-ci"`, `"sandbox-violation"`, `"task-timeout"`, etc.).

**Given** a cell with `badges: Some(vec![Badge::LowConfidenceCi, Badge::SandboxViolation])`
**When** the HTML template renders the row
**Then** two templated chips appear, each with the kebab-case badge name as both text and CSS class for styling.

**Given** the `Badge` enum
**When** I check the codebase for code that prose-formats failure explanations
**Then** there are zero matches outside the templated chip rendering — no LLM-generated prose anywhere (FR36).

**Given** a developer wants to add a new badge variant in a future epic
**When** they grep for `Badge::` references
**Then** the enum, HTML template, and (Epic 4) JSON schema are the only locations that need updating — single source of truth (AR-28).

### Story 2.5: Canary task execution + 3-state header rendering

As Theop,
I want every `lcrc scan` to run the canary task first and render its outcome (`canary-pass` / `canary-fail` / `canary-skipped`) prominently in the HTML report header,
So that infrastructure drift (harness regression, backend regression, OS change) is visually distinguishable from model behavior change (FR6, FR13, FR14).

**Acceptance Criteria:**

**Given** a `lcrc scan` invocation
**When** the orchestrator starts
**Then** the canary task runs *first*, before any SWE-Bench Pro task, regardless of `--depth`.

**Given** the canary task completes successfully
**When** the report header renders
**Then** it shows a prominent `canary-pass` chip in the canonical header (Story 2.12 places it above the fold).

**Given** the canary task fails (the deliberately-broken model in test, or a real backend regression)
**When** the report renders
**Then** it shows a prominent `canary-fail` chip; the scan continues and the report still gets written; `lcrc scan` exits 1 (FR45 trigger path).

**Given** the canary cannot run (e.g., no fit-eligible model)
**When** the orchestrator handles it
**Then** the header shows `canary-skipped` with a brief reason; subsequent measurement still proceeds; exit 0.

**Given** the canary cell write
**When** I inspect the cache
**Then** the canary cell has `task_id` matching the canary's manifest ID and is keyed identically to other cells (it's a normal cell with a special `task_id`).

**Given** the canary task fixture (per Story 2.3) ships a hardcoded canary-only model (small GGUF, <1GB, SHA-256-pinned, vendored as release artifact)
**When** the canary executes
**Then** it runs against the hardcoded canary model — never against a user-discovered model. Rationale: the canary's purpose is infrastructure validation (harness + sandbox + llama-server lifecycle + report rendering), so the model must be fixed across machines and lcrc versions for the `canary-pass` / `canary-fail` signal to be meaningful. The canary cell's `model_sha` is the pinned canary-model SHA; `task_subset_version` matches the rest of the SWE-Bench Pro subset (canary lives inside the same `manifest.json`).

### Story 2.6: Multi-model orchestrator runs Quick depth

As Theop,
I want `lcrc scan` (default `--depth quick`) to plan the full scan, group cells by `(model_sha, params_hash)`, and run 1 SWE-Bench Pro task per fit-eligible model (task #1 in the static ordering),
So that I get a coarse leaderboard across all my installed models in one invocation (FR20 `quick`, FR21).

**Acceptance Criteria:**

**Given** N fit-eligible models discovered, the canary task, and Quick depth
**When** the orchestrator's planning pass runs
**Then** it produces a plan of `1 + N` cells: 1 canary + 1 SWE-Bench Pro task per model (the first task in the static "most-informative-first" ordering).

**Given** the plan
**When** the orchestrator groups cells
**Then** it groups by `(model_sha, params_hash)` so that one llama-server start covers all the cells in a group (typically 1–2 cells per model in Quick).

**Given** the orchestrator executes
**When** for each `(model, params)` group it: starts llama-server → for each task spawns a per-task container → runs mini-swe-agent → captures outcome → tears down container → resets KV cache → next task → stops llama-server
**Then** every cell completes once; no model is loaded twice; per AR-21.

**Given** the orchestrator finishes
**When** I `SELECT * FROM cells`
**Then** there are `1 + N` rows (or fewer if any cell hit a cache lookup from a prior scan).

**Given** the orchestrator's cell ordering within a group
**When** `cache::lookup_cell()` returns `Some` for a planned cell
**Then** the cell is skipped (no measurement); the orchestrator moves on (FR26 lookup-before-measure honored at multi-model scale).

### Story 2.7: Sandbox env allowlist completes structural default-deny

As a developer,
I want `Sandbox::run_task` to pass an `--env-file` containing only the documented allowlist (per AR-14 + NFR-S5: `PATH`, `LANG`, `LC_ALL`, task-specific test-runner config) and to never use `--env` for any host variable,
So that credential-bearing variables (`AWS_*`, `GH_*`, `OPENAI_API_KEY`, etc.) are absent inside the container by structural construction.

**Acceptance Criteria:**

**Given** `src/sandbox/env_allowlist.rs`
**When** I inspect it
**Then** it defines a `const` `ENV_ALLOWLIST: &[&str]` with exactly the documented variables; extension requires code review.

**Given** the host environment contains `OPENAI_API_KEY`, `HF_TOKEN`, `GITHUB_TOKEN`, `AWS_ACCESS_KEY_ID`
**When** `Sandbox::run_task` spawns the container
**Then** inside the container, `env | grep -iE 'token|key|secret|password'` returns no matches.

**Given** the host environment contains `LANG=en_US.UTF-8`
**When** the container starts
**Then** `echo $LANG` inside the container returns the same value (allowlist passthrough works).

**Given** `src/sandbox/container.rs`
**When** I grep for `bollard` env-related calls
**Then** every container creation uses the `--env-file` path produced from `env_allowlist.rs`; no per-call `env` parameter is exposed.

**Given** the function signature of `Sandbox::run_task` (still per AR-28)
**When** inspected
**Then** it still accepts no `env` argument — the allowlist is hard-coded internally.

### Story 2.8: Sandbox-violation event detection + badge + exit 2

As Theop,
I want the sandbox to detect when the model attempts an out-of-envelope operation (host file read, outbound network, cred env probe), record it as a `sandbox-violation` badge on the affected cell, and cause `lcrc scan` to exit with code 2,
So that no silent-pass path through the envelope exists (FR17, NFR-S2).

**Acceptance Criteria:**

**Given** a per-task workspace, a container running mini-swe-agent against a task whose agent attempts `cat /etc/shadow` (which the kernel rejects in the container)
**When** the task completes
**Then** the cell carries the `Badge::SandboxViolation` badge (in addition to any other applicable badges).

**Given** an attempted outbound network call (e.g., `curl https://evil.example.com`) that fails because the custom network has no DNS / no internet route
**When** detected (via container log inspection or strace-equivalent)
**Then** the cell carries `Badge::SandboxViolation` and the violation is recorded in the report's per-row event list.

**Given** at least one cell in a scan carries `Badge::SandboxViolation`
**When** the scan completes
**Then** `lcrc scan` exits with code 2 (FR45 trigger path); the report is still written with all violation badges visible.

**Given** detection is implemented in `src/sandbox/violation.rs`
**When** I inspect it
**Then** it documents how violations are detected (e.g., scanning container stderr for "Permission denied" patterns + tracking attempted-but-failed syscalls per the runtime's audit log if available).

**Given** zero violations in a clean scan
**When** the scan completes
**Then** no `sandbox-violation` badges appear and exit code is 0 (or 1 if canary failed).

### Story 2.9: Per-tier wall-clock cap + `task-timeout` badge

As Theop,
I want each Quick-tier task to be capped at `quick_task_timeout` (working assumption: 600 s); capped tasks are recorded as fails with a `task-timeout` badge instead of wedging the scan,
So that no single task can blow the Quick budget (FR19, AR-17, AR-18).

**Acceptance Criteria:**

**Given** the orchestrator starts a Quick-tier task
**When** the wall-clock exceeds `quick_task_timeout` (defaulting to 600 s but configurable in code for tests)
**Then** the per-task container is torn down (best-effort), the cell is recorded with `pass=0` and `Badge::TaskTimeout`.

**Given** a task that completes within the cap
**When** the cell writes
**Then** no `task-timeout` badge is attached.

**Given** the orchestrator processes the next task in the group after a timeout
**When** it starts
**Then** llama-server is reset (KV cache cleared) and the next task runs with a fresh per-task container; the prior timeout does not contaminate the next measurement.

**Given** a unit test simulating an agent that hangs forever
**When** the cap fires
**Then** the test verifies the badge attaches, exit code is 0 (or 1 if canary independently failed), and the orchestrator continues.

### Story 2.10: Perf metrics graceful-degrade

As Theop,
I want each cell to record `tokens_per_sec`, `ttft_seconds`, `peak_rss_bytes`, and `thermal_state` from the host while the model runs (no privilege required), with `power_watts` left NULL in v1,
So that the leaderboard carries perf signal alongside pass/fail (FR18, NFR-I3, AR-22).

**Acceptance Criteria:**

**Given** a measurement is running
**When** the orchestrator captures perf
**Then** `tokens_per_sec` and `ttft_seconds` are derived from the llama-server HTTP response timing (no host privilege needed).

**Given** the orchestrator polls `proc_pid_info` on the llama-server PID during the measurement
**When** the measurement completes
**Then** `peak_rss_bytes` is populated with the maximum observed RSS.

**Given** the orchestrator queries the `IOReport` framework
**When** the measurement runs
**Then** `thermal_state` is recorded (string classification sufficient for the `thermal-throttled` badge in Epic 3+).

**Given** any of the perf APIs fails (mocked permission failure)
**When** the cell writes
**Then** the affected metric is NULL in the cell row; the scan continues normally (NFR-R4 graceful degrade).

**Given** the cell row
**When** I inspect it
**Then** `power_watts IS NULL` for every Epic 2 cell (slot reserved for v1.1+ launchd helper per AR-22).

### Story 2.11: Wilson-score CI + structural `low-confidence-CI` + depth-tier tag

As Theop,
I want every leaderboard row to display a Wilson-score confidence interval on its pass-rate, every Quick-tier row to carry a structural `low-confidence-CI` badge, and every cell to be tagged with the depth tier that produced it,
So that I never accidentally make a default-switch decision based on Quick alone (FR35, FR37, FR38).

**Acceptance Criteria:**

**Given** a model with K passes out of N tasks
**When** `wilson::ci(k, n, 0.95)` is called
**Then** it returns `(low, high)` bounds per the Wilson-score formula (hand-written ~10 LOC per architecture; verified against a known-value test vector).

**Given** the HTML row for a model
**When** rendered
**Then** the pass-rate column shows e.g. `"60% (CI: 23–88%)"` with the bounds derived from the cells in cache for that model.

**Given** any cell with `depth_tier='quick'`
**When** the row renders
**Then** the row carries `Badge::LowConfidenceCi` regardless of the actual CI width — structural default per FR38.

**Given** a leaderboard row
**When** rendered
**Then** the row visibly shows the depth tier (Quick / Standard / Full) of the cells producing it (per FR37); when cells from multiple tiers contribute, the rendering documents how (e.g., "Standard (3 tasks); Quick row hidden").

**Given** a Standard-tier row (Epic 3 lookahead — no Standard cells exist yet)
**When** the formula encounters such a row
**Then** the structural `low-confidence-CI` badge is NOT applied (only Quick rows get it by structural default).

### Story 2.12: Canonical screenshot-friendly HTML header

As Theop,
I want the HTML report to render a canonical header containing `machine_fingerprint`, scan date (RFC 3339), `lcrc --version` info, `backend_build`, and the canary state — all visible without scrolling — so that a screenshot pasted into a thread is self-attesting (FR34).

**Acceptance Criteria:**

**Given** a rendered report
**When** I open it in a browser at typical viewport size (1280×800)
**Then** the canonical header is visible above the fold and contains: machine fingerprint string, scan date in RFC 3339 (UTC, with `Z`), lcrc semver + commit short, `backend_build` string, and the canary state chip (`canary-pass` / `canary-fail` / `canary-skipped`).

**Given** the same report
**When** I take a screenshot of just the header region
**Then** the screenshot alone is sufficient to know what hardware + toolchain + canary outcome produced the leaderboard below it.

**Given** the canary state is `canary-fail`
**When** the header renders
**Then** the chip is visually distinct (color or styling) so it cannot be missed at a glance.

**Given** the header HTML
**When** I view the source
**Then** the header is fully styled inline (no external CSS) per FR32 self-contained.

### Story 2.13: Streaming CLI per-cell + ETA + `--quiet`

As Theop,
I want stderr to stream per-cell completion lines and an estimated-remaining clock (updated at least every 10 s) during a scan; with `--quiet` / `-q` suppressing the streaming while keeping all writes and exit codes intact,
So that I can watch progress in real time but also pipe lcrc into Makefiles without noise (FR47, FR48, NFR-O1, NFR-P8).

**Acceptance Criteria:**

**Given** I run `lcrc scan` in a TTY
**When** each cell completes
**Then** stderr emits a line like `[3/12] qwen-3-coder-32b-q4 / django-1234: PASS in 87s` (target: appears within 1 s of cell completion per NFR-P8).

**Given** the same scan
**When** observed for 30 s
**Then** an estimated-remaining clock line updates at least every 10 s (NFR-P8).

**Given** the same scan with `--quiet` or `-q`
**When** I run it
**Then** stderr emits no per-cell or progress lines; `tracing::INFO` is suppressed; the cache is still written; the HTML report is still regenerated; exit codes are unchanged (FR48).

**Given** the same scan run with stderr piped to a file (non-TTY)
**When** I inspect the file
**Then** the streaming output is plain text (no color codes), emitted line-by-line (no `indicatif` carriage-return-overwrites).

**Given** the streaming subscriber
**When** implementations consume tracing events
**Then** `indicatif` handles TTY rendering and a fallback subscriber handles non-TTY plain output; `--quiet` disables both.

### Story 2.14: Empty-machine starter pack UX

As a friend Theop sent lcrc to,
I want `lcrc scan` to detect an empty installed-models set and print a one-paragraph explainer plus a hardcoded list of 3–5 small models with copy-paste-ready download commands,
So that the empty case feels like guidance rather than an error (FR5; PRD Journey 2).

**Acceptance Criteria:**

**Given** `LlamaCppBackend::discover_models()` returns an empty vector AND no other backend has models
**When** `lcrc scan` runs
**Then** stderr prints (a) a one-paragraph explainer ("lcrc measures models you already have — it doesn't download or curate"), (b) a hardcoded list of 3–5 small fit-friendly models with exact `huggingface-cli download ...` or `llama.cpp` pull commands, (c) an invitation to re-run after installing one.

**Given** the empty-machine UX prints
**When** the scan exits
**Then** exit code is 0 (this is not a failure — it's UX); the cache is unchanged; no HTML report is written.

**Given** the empty-machine UX is being prepared
**When** I inspect `src/starter_pack.rs`
**Then** the model list is a hardcoded `const`; lcrc never makes a network call to fetch a recommendation list.

**Given** the user installs one of the suggested models and re-runs `lcrc scan`
**When** the scan executes
**Then** discovery returns the new model, the empty UX no longer triggers, and a single-row Quick scan completes normally.

### Story 2.15: End-to-end Ctrl-C resumability integration test

As Theop,
I want a binding integration test that proves `lcrc scan` survives Ctrl-C mid-scan (cells already completed remain in cache; next invocation resumes by skipping them),
So that NFR-R1 + FR27 are verified end-to-end across the full Quick stack (orchestrator + sandbox + cache + signal handler).

**Acceptance Criteria:**

**Given** a test fixture with a 3-model installed set
**When** the test runs `lcrc scan` and sends SIGINT after the first cell completes
**Then** `lcrc scan` exits 3 within ~1 s; the per-task container that was running is torn down (best-effort).

**Given** the cache after the interrupt
**When** I `SELECT count(*) FROM cells`
**Then** there is exactly 1 row (the completed canary or first model) — no half-written cells (NFR-R2).

**Given** the test then re-runs `lcrc scan`
**When** the scan executes
**Then** it skips the cell already in cache (FR26 lookup-before-measure); measures the remaining cells; exits 0; the resulting leaderboard is identical to what an uninterrupted scan would produce.

**Given** the test is in `tests/scan_resumability.rs`
**When** CI runs it
**Then** it passes deterministically (using a fixture model that returns instantly, not a real GGUF, to keep the test fast).

### Story 2.16: Sandbox adversarial smoke test (subset of acceptance #9)

As a developer,
I want a fast integration test that runs 3–4 of the highest-signal probes from the acceptance #9 adversarial battery (host file read, outbound network, env var probing, sibling-workspace fishing) and asserts every probe is blocked,
So that structural sandbox holes are surfaced in Epic 2 instead of Epic 7 — without paying the full battery's runtime cost in CI (per H3 red-team hardening).

**Acceptance Criteria:**

**Given** the adversarial smoke test in `tests/sandbox_smoke.rs`
**When** CI runs it
**Then** it spawns the per-task container with the same `Sandbox::run_task` API used in production, and runs an adversarial probe script inside.

**Given** the probe script attempts `cat /etc/passwd`, `cat /Users/*/Documents/*.txt`, `ls /private/var`, `curl https://example.com`, `nslookup google.com`, `cat /tmp/lcrc-task-*/output.txt`, and `env | grep -iE 'token|key|secret|password'`
**When** the script runs and the test inspects results
**Then** every probe either fails (read returns empty, network unreachable) OR returns the *image's* file content (not the host's); the test asserts at least one violation is detected by `src/sandbox/violation.rs`.

**Given** the test asserts the run produced at least one `Badge::SandboxViolation`
**When** the cell records
**Then** the badge is present and the test passes.

**Given** this is the smoke test (subset)
**When** I check the test source
**Then** it documents that the *full* adversarial battery (per AR-32 / acceptance #9) lives in Epic 7's `tests/sandbox_envelope.rs` and is the binding v1-ship gate; this test is the early-detection version.

### Story 2.17: Quick budget sanity check on M1 Pro 32GB / 5-model

As Theop,
I want a calibration script (`scripts/quick_budget_check.sh` or equivalent) that runs `lcrc scan` on the M1 Pro 32GB reference rig with my actual installed-model set, logs wall-clock, and fails loudly if Quick exceeds 25 minutes,
So that I catch a Quick budget blowout *now* — before Epic 3 builds Standard/Full on top of an over-budget Quick (per H4 red-team hardening).

**Acceptance Criteria:**

**Given** the script in `scripts/quick_budget_check.sh`
**When** I run it on M1 Pro 32GB / 5-model
**Then** it invokes `lcrc scan --depth quick`, captures wall-clock, prints a summary line `"Quick budget: <elapsed>s vs cap 1500s"`, and exits 0 if elapsed ≤ 1500 s, exit 1 otherwise.

**Given** the script exits 1 (over budget)
**When** I read its stderr output
**Then** it prints actionable guidance: "Tighten `quick_task_timeout` (currently <X> s) or reduce per-task scope before extending to Standard."

**Given** the script runs more than once on the same cache
**When** the second run executes
**Then** it benefits from cache hits (per FR26) and reports the second-run wall-clock separately so cold/warm comparison is visible.

**Given** the script is documented in the README's "calibration" section
**When** Theop reads it
**Then** he knows when to run it (after Story 2.6 Quick orchestrator lands; before starting Epic 3) and how to interpret the output.

**Given** Epic 7 will lock the *final* `*_task_timeout` values (per AR-37 + AR-17)
**When** Theop ships v1
**Then** the values are informed by data from this script + ongoing Epic 3 Standard/Full runs, not by working assumptions alone.

### Story 2.18: Server lifecycle and thermal badge attachment

As Theop,
I want the cell-write path to attach the templated badges `server-startup-failure`, `server-crashed`, and `thermal-throttled` when the corresponding infrastructure event is detected during a measurement,
So that the leaderboard distinguishes infrastructure failures from model behavior, completing 3 of the 7 dormant variants in Story 2.4's `Badge` enum (per FR36 contract).

**Acceptance Criteria:**

**Given** `LlamaServer::start` returns `Err(ServerStartupFailure)` (per Story 1.11) for a planned `(model, params)` group
**When** the orchestrator records the affected cells
**Then** every cell in that group is written with `pass=0` and `Badge::ServerStartupFailure` attached; the orchestrator continues with the next group; no llama-server is left running.

**Given** llama-server crashes mid-task (process exits non-zero, OR stops responding to `/health` for >`server_startup_timeout`, OR closes the inference connection unexpectedly)
**When** the in-flight task is interrupted
**Then** the affected cell is written with `pass=0` and `Badge::ServerCrashed`; the per-task container is torn down; remaining tasks in the same group are written with `pass=0` + `Badge::ServerCrashed` (the model failed structurally; we don't pretend to measure further); the orchestrator restarts llama-server fresh for the next group.

**Given** Story 2.10 collects `thermal_state` via the IOReport framework during a measurement
**When** `thermal_state` indicates throttling at any sample point (e.g., transitions from `IOReportNominal` to `IOReportFair`/`IOReportSerious`/`IOReportCritical`)
**Then** the cell is written with `Badge::ThermalThrottled` (in addition to its pass/fail). The badge does NOT change `pass` — it's a context flag for the human reader explaining a slow result.

**Given** the FR36 badge contract
**When** I inspect `src/scan/orchestrator.rs` (or wherever cells are finalized)
**Then** `ServerStartupFailure`, `ServerCrashed`, and `ThermalThrottled` are attached at the documented detection points; `CtxLimited`, `OomAtN`, `RepetitionLoop`, and `ToolCallFormatFailure` remain dormant in the enum (attached in Epic 3 per Story 3.6, since detection of those benefits from Standard/Full's larger task counts).

**Given** integration tests for the three new badges
**When** CI runs them
**Then** each badge has at least one fixture that triggers its attachment path (e.g., `tests/badges/server_startup_failure.rs` uses a corrupt-GGUF fixture; `tests/badges/server_crashed.rs` uses a fixture that kills llama-server mid-task; `tests/badges/thermal_throttled.rs` mocks `IOReport` to return `IOReportFair`); the cell write is verified.

---

## Epic 3: Standard & Full depths — cache extension proves cache-as-product

**Goal:** Add the two heavier depth tiers and per-model scoping. `lcrc scan --depth standard` extends each cell to 3–5 tasks, skipping every cell already in cache from a prior Quick scan; `--depth full` extends to the complete curated subset plus quant/ctx variants. `--model <pattern>` lets the user scope a scan to a single new model. `--report-path` overrides the default output location; timestamped historical report files are written alongside `latest.html`. The cache-as-product behavior becomes user-visible: Standard reuses Quick's cells; Full reuses Standard's.

### Story 3.1: `--depth standard` extends Quick cells to 3–5 tasks

As Theop,
I want `lcrc scan --depth standard` to extend each model's cell from Quick's 1 task to 3–5 tasks (the next 2–4 in the static "most-informative-first" ordering), reusing every Quick cell already in cache,
So that I get a tight enough leaderboard to make a default-switch decision (FR20 `standard`, FR22, FR26 cache-extension visible).

**Acceptance Criteria:**

**Given** a cache populated by a prior Quick scan (1 task per model)
**When** I run `lcrc scan --depth standard`
**Then** the orchestrator's planning pass enumerates 3–5 tasks per model (Quick's task #1 from the static ordering plus the next 2–4); cache lookup hits on every existing Quick cell; only the new 2–4 cells per model get measured.

**Given** the Standard scan completes
**When** I `SELECT depth_tier, count(*) FROM cells GROUP BY depth_tier`
**Then** I see `quick=N` (preserved) and `standard=2N..4N` (new cells), where N = number of fit-eligible models.

**Given** the HTML report renders after Standard completes
**When** I look at any model's row
**Then** the row's pass-rate is computed from all cells (Quick + Standard) for that model; the Wilson CI is meaningfully tighter than after Quick alone; the row no longer carries the structural `low-confidence-CI` badge (because depth tier is now Standard, not Quick — per Story 2.11).

**Given** the user re-runs `lcrc scan --depth standard` against an unchanged input set + cache
**When** the second invocation executes
**Then** every cell hits cache lookup; no measurement runs; exit 0 (NFR-R6 idempotency).

**Given** the user runs `lcrc scan --depth standard` directly on an empty cache (no prior Quick)
**When** the scan executes
**Then** it measures the full 3–5 tasks per model from scratch; produces a Standard-tier leaderboard; exit 0. (Standard is composable from any starting state.)

### Story 3.2: `--depth full` extends to the full curated subset

As Theop,
I want `lcrc scan --depth full` to extend each model's cell to the complete curated SWE-Bench Pro subset (every task in the manifest), reusing every Standard cell already in cache,
So that overnight runs produce the tightest CIs and most reliable rank order (FR20 `full`, FR23 base, FR26).

**Acceptance Criteria:**

**Given** a cache populated by a prior Standard scan
**When** I run `lcrc scan --depth full`
**Then** the orchestrator measures every remaining task in the manifest (all `manifest.tasks[]`) for each fit-eligible model; existing cells are skipped.

**Given** the Full scan completes
**When** I `SELECT depth_tier, count(*) FROM cells GROUP BY depth_tier`
**Then** I see `quick`, `standard`, and `full` rows representing the cell tiers preserved across depths.

**Given** the HTML report after Full completes
**When** I inspect the top-3 models' rank
**Then** the rank order is documented as "stable enough to compare to Standard's top-3 without rank inversions, unless explained by a templated badge" (PRD acceptance check #3 — verified by inspection here; binding test in Epic 7).

**Given** the user runs `lcrc scan --depth full` against an empty cache directly
**When** the scan executes
**Then** it measures every task per model from scratch; produces a Full-tier leaderboard; exit 0.

**Given** the Full scan exceeds the working-assumption per-task cap (`full_task_timeout`, default 1800 s) on any task
**When** the cap fires
**Then** the cell is recorded with `pass=0` + `Badge::TaskTimeout` (per Story 2.9, applied at Full's higher cap value); the scan continues.

### Story 3.3: Full depth adds quant/ctx variants beyond defaults

As Theop,
I want `lcrc scan --depth full` to additionally measure each model with non-default quant/ctx parameter combinations (per FR23 "and adds quant/ctx variants beyond the default"),
So that Full's overnight run produces enough data to identify whether a different ctx length or quantization tier ranks better (FR23 variants).

**Acceptance Criteria:**

**Given** a fit-eligible model with default `params={ctx: 8192, n_gpu_layers: -1, ...}`
**When** Full depth runs
**Then** the orchestrator additionally schedules cells for at least one variant (e.g., `ctx: 4096` or alternative `n_gpu_layers`); each variant is a separate cell with a distinct `params_hash`.

**Given** the variant set is documented
**When** I inspect `src/scan/orchestrator.rs` (or a config-defined list)
**Then** the Full-depth variant strategy is explicit (e.g., "for each model, add ctx=4096 and ctx=2048 variants if they fit") — not magic numbers scattered through code.

**Given** a variant cell is measured and persisted
**When** I `SELECT params_hash, count(*) FROM cells WHERE depth_tier='full' GROUP BY params_hash`
**Then** I see distinct `params_hash` rows representing each variant.

**Given** the HTML report after Full + variants completes
**When** I look at a model with multiple variants
**Then** the renderer shows variants either as separate rows or as a per-row variant column (rendering choice documented in `src/report/`); each variant's Wilson CI and depth tier are visible.

**Given** a variant that doesn't fit in RAM (e.g., ctx too large)
**When** the orchestrator plans
**Then** the variant is skipped via the fit gate (Story 2.2) before measurement; surfaced in the same exclusion list with reason.

### Story 3.4: `--model <pattern>` filter on `lcrc scan`

As Theop,
I want `lcrc scan --model <pattern>` to restrict the scan to models whose name OR `model_sha` prefix matches the substring `<pattern>`,
So that when I install a single new model I can re-scan just that one without re-touching the others (FR12, PRD Journey 3).

**Acceptance Criteria:**

**Given** N fit-eligible models discovered
**When** I run `lcrc scan --model qwen` (substring match on display name)
**Then** the planning pass restricts to models whose display name contains `"qwen"` (case-insensitive); other models are not measured.

**Given** the same set
**When** I run `lcrc scan --model abc123` (where `abc123` is a `model_sha` prefix)
**Then** the planning pass restricts to models whose `model_sha` starts with `abc123`.

**Given** `--model <pattern>` matches zero models
**When** the scan executes
**Then** stderr prints a clear message ("no models match pattern '<pattern>'"); exit 0; no measurement runs.

**Given** `--model` is implemented
**When** I inspect `src/discovery/` (or wherever the filter lives)
**Then** the filter helper is reusable — `lcrc show --model` (Epic 4) and `lcrc verify --model` (Epic 5) call the same helper rather than duplicating logic.

**Given** a `--model <pattern>` scan
**When** the canary task runs
**Then** the canary still runs (independent of the model filter; canary is infrastructure check, not a model measurement).

### Story 3.5: `--report-path` override + timestamped historical reports

As Theop,
I want `lcrc scan --report-path <path>` to override the default `latest.html` location, AND for every scan to write a timestamped historical report file (`report-<ISO8601>.html`) alongside the default `latest.html`,
So that I can keep a record of past scans for comparison and direct the report to a custom location for sharing (FR39).

**Acceptance Criteria:**

**Given** I run `lcrc scan` with no flags
**When** the scan completes
**Then** two files exist: `$XDG_DATA_HOME/lcrc/reports/latest.html` (overwritten each scan) and `$XDG_DATA_HOME/lcrc/reports/report-<ISO8601>.html` (e.g., `report-2026-04-30T14-23-15Z.html` — colons replaced with dashes for filename safety per AR-30 + util/time).

**Given** I run `lcrc scan --report-path /tmp/myrun.html`
**When** the scan completes
**Then** `/tmp/myrun.html` exists with the report content; the default `$XDG_DATA_HOME/lcrc/reports/latest.html` and timestamped historical file are NOT written. `--report-path` is a true override: it redirects the single canonical report output to the user-supplied path. The user opts out of lcrc's default history-keeping when they pass `--report-path`; if they want both, they invoke twice or set up their own copy step.

**Given** the override path is in a non-existent directory
**When** the scan runs
**Then** lcrc creates the parent directory (best-effort) and writes the file. If creation or write fails (path invalid, permissions denied, disk full, etc.), the measurement still completes and the cache still writes, but lcrc exits 10 (ConfigError) with a stderr message identifying the offending path. Exit 10 is the resolved code: `--report-path` is a CLI-supplied configuration value (per FR50 layered precedence), so a bad value is a config error; no new exit-code variant is introduced.

**Given** the historical report write (default no-override case only)
**When** the file is created
**Then** the filename uses the same RFC 3339 UTC timestamp as the cell `scan_timestamp` for any cell written during this scan (consistency check via `util::time` single helper). When `--report-path` is set, no historical file is written and this AC does not apply.

**Given** the historical reports directory accumulates
**When** I list it after several scans
**Then** I see `latest.html` plus N timestamped files; lcrc does NOT auto-prune (per PRD: `lcrc gc` is v1.1+); the user manages disk themselves.

### Story 3.6: Model-behavior badge detection and attachment

As Theop,
I want the cell-write path to attach `ctx-limited`, `oom-at-n`, `repetition-loop`, and `tool-call-format-failure` badges when the corresponding model-behavior failure mode is detected during a SWE-Bench Pro task,
So that the leaderboard surfaces actionable failure-mode information and the FR36 badge contract is fully delivered before Epic 7's adversarial battery audits the trust story.

**Acceptance Criteria:**

**Given** mini-swe-agent's prompt or accumulated context exceeds the model's `n_ctx` (detected via llama-server's API returning `context_length_exceeded`, OR the agent abandoning the task with a documented context-overflow code)
**When** the cell writes
**Then** `Badge::CtxLimited` is attached; `pass=0`; the badge metadata records the cell-time `n_ctx` value vs the model's hard limit so the human reader can decide whether a different ctx variant (Story 3.3) would help.

**Given** llama-server (or the per-task container) is killed by the kernel OOM killer mid-task
**When** the orchestrator detects the process exit signal (`SIGKILL` with no graceful shutdown trace) or the container's exit code matches OOM patterns (137 etc.)
**Then** the cell is written with `pass=0` and `Badge::OomAtN`; the badge metadata records the model's measured `peak_rss_bytes` (from Story 2.10) at the point of kill, so the fit gate (Story 2.2) can be tuned if OOMs cluster on a quant tier the gate currently passes.

**Given** the agent's output stream contains more than N consecutive identical tokens (heuristic threshold documented in `src/orchestrator/repetition.rs`, e.g., 200+ identical tokens or 500+ tokens forming a recognized loop pattern)
**When** the heuristic fires before the task's wall-clock cap
**Then** the orchestrator aborts the task; the cell is written with `pass=0` and `Badge::RepetitionLoop`; `Badge::TaskTimeout` is NOT attached on the same cell (avoid double-attribution; repetition loop is the more specific finding).

**Given** mini-swe-agent's tool-call output cannot be parsed (malformed JSON, missing required fields, schema violation per the agent's documented tool-call contract)
**When** the agent's parser raises and the task fails as a result
**Then** the cell is written with `pass=0` and `Badge::ToolCallFormatFailure`; this badge does not double-attribute with `Badge::TaskTimeout` either.

**Given** the FR36 badge contract is now fully wired
**When** I inspect `src/scan/orchestrator.rs` (and downstream parsers)
**Then** all 10 `Badge` variants have an attachment path: 3 from Epic 2's earlier stories (`SandboxViolation`, `TaskTimeout`, `LowConfidenceCi`), 3 from Story 2.18 (`ServerStartupFailure`, `ServerCrashed`, `ThermalThrottled`), and 4 from this story (`CtxLimited`, `OomAtN`, `RepetitionLoop`, `ToolCallFormatFailure`). Epic 7's adversarial battery (Story 7.4) audits the resulting badge surface end-to-end.

**Given** Standard/Full depth measurements run more tasks per model than Quick
**When** the new badges are exercised at scale
**Then** at least one fixture per badge has triggered in CI (added as integration tests under `tests/badges/`); badge frequency in `cells.badges` is queryable for future analysis.

---

## Epic 4: `lcrc show` — read-only leaderboard view

**Goal:** Add the terminal-side leaderboard. `lcrc show` prints a plain-text fixed-width table that ranks identically to the HTML report (acceptance check #8 is a binding test). `--format json` produces stable JSON with a top-level `schema_version` field, pipe-friendly. Filters: `--model <pattern>`, `--depth <tier>`, `--limit N`, `--all` (include cells for uninstalled models or outdated `backend_build`s, mirroring HTML behavior). All `lcrc show` invocations open SQLite read-only and may run concurrently with an active `lcrc scan`.

### Story 4.1: `lcrc show` plain-text leaderboard mirroring HTML rank

As Theop,
I want `lcrc show` to print a fixed-width plain-text leaderboard to stdout that ranks identically to the HTML report,
So that I can read the leaderboard from the terminal without opening a browser, and so PRD acceptance check #8 is satisfied (FR40, FR45 exit 4 trigger path).

**Acceptance Criteria:**

**Given** a cache populated with at least one cell
**When** I run `lcrc show`
**Then** stdout receives a fixed-width table sorted by the same rank metric the HTML report uses; columns include model, pass-rate (with Wilson CI), depth tier, badges (templated kebab-case names).

**Given** the cache is empty
**When** I run `lcrc show`
**Then** stderr prints a clear message ("cache is empty — run `lcrc scan` first"); stdout is empty; exit 4 (FR45 trigger path).

**Given** a binding integration test in `tests/show_mirror.rs`
**When** CI runs it against a fixture cache
**Then** the test asserts that the rank order from `lcrc show` matches the rank order from the HTML report (parsed by extracting model rows in document order); identical ranks pass — any divergence fails the build (PRD acceptance check #8).

**Given** `lcrc show` is read-only
**When** I check the SQLite open mode
**Then** the connection is opened with `OpenFlags::SQLITE_OPEN_READ_ONLY`; no write attempted (precondition for Story 4.5 lock-free behavior).

**Given** measured latency
**When** the cache holds up to 1,000 cells
**Then** `lcrc show` returns rendered output in <500 ms (NFR-P7).

### Story 4.2: `lcrc show` filters — `--model`, `--depth`, `--limit`

As Theop,
I want `lcrc show --model <pattern>`, `--depth <tier>`, and `--limit N` to filter the rendered leaderboard,
So that I can scope the view to the slice of the cache I care about (FR41).

**Acceptance Criteria:**

**Given** a populated cache and `lcrc show --model qwen`
**When** the command runs
**Then** only rows whose model name (or `model_sha` prefix) matches `qwen` appear; the filter helper from Story 3.4 is reused (no duplicated logic).

**Given** `lcrc show --depth standard` against a cache where some rows have cells from multiple tiers (e.g., 1 Quick cell + 3 Standard cells per model)
**When** the command runs
**Then** the row's pass-rate and Wilson CI are recomputed using only the Standard-tier cells (3 of 4 in the example); the Quick cell is invisible to the filter. Rows with zero Standard cells are omitted entirely. The filter is symmetric: `--depth quick` would show only Quick-tier contributions, `--depth full` only Full-tier. Recomputation per filter avoids needing a separate "which depth tier wins for the row" rule on the leaderboard render.

**Given** `lcrc show --limit 5`
**When** the command runs against a cache of 12 models
**Then** only the top 5 (by rank metric) appear; all other rows are omitted.

**Given** filters combine: `lcrc show --model qwen --depth standard --limit 3`
**When** the command runs
**Then** the combined filter applies (qwen-name AND standard-depth, top 3 of the resulting set).

**Given** any filter matches zero rows
**When** the command runs
**Then** stdout prints an empty table (with header row only, or a "no matching rows" message — render choice documented); exit 0 (not 4 — the cache isn't empty, just the filtered view is).

### Story 4.3: `lcrc show --all` includes uninstalled models and outdated `backend_build`s

As Theop,
I want `lcrc show --all` to include cells for models no longer present on disk and cells for outdated `backend_build`s (which the default view hides, mirroring HTML behavior),
So that I can audit historical measurements after a `brew upgrade llama.cpp` or after deleting a model file (FR42).

**Acceptance Criteria:**

**Given** a cache with cells for model A (still on disk) and model B (file deleted)
**When** I run `lcrc show` (default)
**Then** only model A rows appear.

**Given** the same cache
**When** I run `lcrc show --all`
**Then** both model A and model B rows appear; model B rows are visually marked (e.g., a `(uninstalled)` annotation in the model column).

**Given** a cache with cells from `backend_build=llama.cpp-b3791+a1b2c3d` and `llama.cpp-b3850+e4f5a6b` (after a `brew upgrade`)
**When** I run `lcrc show` (default)
**Then** only cells matching the *current* `backend_build` appear.

**Given** the same cache and `lcrc show --all`
**When** the command runs
**Then** all cells appear; outdated-build cells are visually marked (e.g., `(outdated build)` annotation).

**Given** the HTML report's default-view behavior
**When** compared to `lcrc show` defaults
**Then** they match: the same cells are hidden in both surfaces; `--all` reveals the same hidden set in both surfaces.

### Story 4.4: `lcrc show --format json` with `schema_version`

As a script author (Theop, scripting lcrc into Makefiles or cron),
I want `lcrc show --format json` to emit stable, pipe-friendly JSON with a top-level `schema_version` field,
So that I can pipe the output into `jq`, parse it deterministically, and detect breaking schema changes (FR43, FR54).

**Acceptance Criteria:**

**Given** a populated cache
**When** I run `lcrc show --format json`
**Then** stdout receives valid JSON parseable by `jq` (e.g., `lcrc show --format json | jq '.rows[0]'` returns the top-ranked row); the JSON has a top-level object with at minimum `schema_version` (integer) and `rows` (array of cell-summary objects).

**Given** the JSON output schema
**When** I check `src/jsonout/schema.rs`
**Then** the schema is defined as serializable Rust types (serde-derived); the schema's structure is documented in the README.

**Given** `lcrc show --format json` and `lcrc show` are both run against the same cache
**When** I compare outputs
**Then** the rank order in the JSON `rows` array matches the rank order of the plain-text table (both rendered from the same `cache::query` helper).

**Given** a future minor version of lcrc adds a new optional field to a row object
**When** the JSON schema bumps from `schema_version=N` to still `N` (backward-compatible addition)
**Then** older `jq` scripts continue to work; new field is documented (FR54 backward compatibility within a major version).

**Given** a future major version makes a breaking schema change
**When** released
**Then** `schema_version` increments; CHANGELOG documents the break (FR54 majors bump on break).

**Given** `lcrc show --limit 0 --format json`
**When** the command runs
**Then** the JSON is `{"schema_version": N, "rows": []}` (empty array — not an error).

### Story 4.5: Lock-free reads concurrent with active `lcrc scan`

As Theop,
I want `lcrc show` (and Epic 5's `lcrc verify`) to be safely runnable while a `lcrc scan` is in progress in another terminal,
So that I can monitor cache contents in real time without blocking the scan or being blocked by it (FR53, NFR-R7).

**Acceptance Criteria:**

**Given** a `lcrc scan` is running and writing cells via SQLite WAL mode (per AR-7)
**When** I run `lcrc show` in a parallel terminal
**Then** `lcrc show` returns within its NFR-P7 budget (<500 ms for ≤1000 cells), reading whatever cells have been committed so far; the scan is not delayed or interrupted.

**Given** the same setup
**When** I run `lcrc show` repeatedly during the scan
**Then** each invocation returns the cells committed at that point; rows appear/update as the scan progresses.

**Given** `lcrc show` opens the SQLite file
**When** the connection is established
**Then** it uses `OpenFlags::SQLITE_OPEN_READ_ONLY` (per Story 4.1) — preventing accidental writes and avoiding any lock contention with the scan's writer.

**Given** a binding integration test in `tests/concurrency_lock.rs`
**When** CI runs it
**Then** the test spawns a long-running `lcrc scan` (with a slow fixture model) and concurrently issues `lcrc show` calls, asserting both succeed without deadlock or write-contention errors (precursor to Epic 5's verify-during-scan test).

**Given** a `lcrc show` is running against a cache mid-scan
**When** the scan writes a cell during the show's read
**Then** the show sees a consistent snapshot (SQLite WAL semantics); never observes a half-written row.

---

## Epic 5: `lcrc verify` — drift detection

**Goal:** Add the trust-audit surface. `lcrc verify --sample N` re-measures N randomly-sampled cached cells inside the same sandbox envelope and emits a numerical drift report (cached value, new value, delta, CI overlap per cell). Default behavior is **warn**, not invalidate — to act on drift, the user runs `lcrc scan`. Exit 5 when drift detected; `--format json` for machine-readable output. `machine_fingerprint` stability across macOS patch-level upgrades (FR30) is verified here since drift detection is its first user-facing surface.

### Story 5.1: `lcrc verify --sample N` re-measures sampled cells

As Theop,
I want `lcrc verify --sample N` to randomly select N cells from the cache, re-measure each one inside the sandbox using the same orchestrator + llama-server + harness path, and capture the new outcomes for comparison,
So that I have fresh measurements to compare against cached ones (FR28; PRD Journey 4 sets up Story 5.2's drift report).

**Acceptance Criteria:**

**Given** a populated cache and `lcrc verify --sample 5`
**When** the command runs
**Then** the verify orchestrator: (1) selects 5 cells uniformly at random from the eligible cell set, (2) for each selected cell, starts llama-server for `(model, params)` if not already up, spawns a per-task container with the workspace + sandbox envelope, runs the same task via mini-swe-agent, captures the new outcome (pass/fail + perf metrics), (3) holds the new outcomes in memory for Story 5.2's drift report.

**Given** the cache is read during sample selection
**When** the SQLite connection opens
**Then** it opens read-only (no cache writes from verify ever — per FR29 warn-not-invalidate); concurrent with active `lcrc scan` is supported (FR53; piggybacks on Story 4.5's WAL-mode pattern).

**Given** verify spawns its own per-task containers and llama-servers
**When** running concurrently with an active `lcrc scan` for the same model
**Then** both processes succeed; their llama-servers bind to different free ports (per Story 1.11 AC); their containers are independent (per-scan-id label per AR-15).

**Given** `--sample N` exceeds the number of cells in cache
**When** verify runs
**Then** it samples min(N, cell_count) cells; stderr notes the actual sample size.

**Given** `lcrc verify` is invoked with no `--sample` flag
**When** the command runs
**Then** the default sample size is 5 (per PRD §"`lcrc verify`" default).

**Given** a simulated macOS patch-level upgrade (test fixture mocks `sysctl` output before/after)
**When** verify computes the cell key for selected cells
**Then** the `machine_fingerprint` matches the cached fingerprint exactly; cells remain identifiable; re-measurement runs against the same cell PK (FR30 verified as a binding AC here).

**Given** verify completes its re-measurements
**When** I `SELECT count(*) FROM cells WHERE scan_timestamp > <verify_start_time>`
**Then** zero new cells were written (verify is non-destructive per NFR-R6).

**Given** verify is mid-flight (one re-measurement in progress, others queued)
**When** the user sends SIGINT (Ctrl-C)
**Then** verify exits 3 within ~1 s; the in-progress per-task container is torn down on best-effort basis (mirroring Story 1.12 + NFR-R8); any llama-server spawned by verify is terminated; no partial drift output is written to stdout. Verify is non-destructive (Story 5.3 read-only-open) so there is no cache state to roll back.

### Story 5.2: Numerical drift report (cached vs new, delta, CI overlap)

As Theop,
I want `lcrc verify` to print a numerical drift report — one row per re-measured cell showing cached value, new value, delta, and Wilson CI overlap — to stdout in plain-text fixed-width format,
So that I can interpret drift by reading numbers, not narratives (FR28; PRD acceptance #7 "interpretable, numerical, not narrative").

**Acceptance Criteria:**

**Given** verify completed re-measurement of N cells
**When** the drift report renders
**Then** stdout receives a fixed-width table with one row per re-measured cell; columns include `model`, `task_id`, `cached_pass`, `new_pass`, `delta` (e.g., `+0`, `-1`, `+1` for binary pass changes), `cached_perf` (e.g., tok/s), `new_perf`, `perf_delta_pct`, `ci_overlap` (yes/no for whether the new measurement falls within the cached Wilson CI).

**Given** zero cells drifted (every re-measurement matches cached within CI)
**When** the report renders
**Then** the table prints with all rows showing `ci_overlap=yes`; a summary line at the bottom reads `"No significant drift detected (5/5 cells within CI)."`.

**Given** at least one cell drifted (re-measurement falls outside cached Wilson CI OR pass-rate flipped)
**When** the report renders
**Then** the drifted row(s) are visually marked (e.g., `*` prefix or color when stderr is TTY-equivalent); the summary line reads e.g., `"Drift detected: 2/5 cells (qwen-3-coder/django-1234, llama-3-8b/numpy-9012)."`.

**Given** the report is plain text (default `--format text`)
**When** I pipe it to `less` or `grep`
**Then** the output is grep-friendly fixed-width (no carriage-return-overwrites, no color codes when piped).

**Given** the per-cell perf metrics include NULLs (graceful-degrade)
**When** the report renders
**Then** NULL values display as `-` or `N/A`; the row still parses; `perf_delta_pct` shows `-` for unmeasurable changes.

### Story 5.3: Default warn-not-invalidate + exit 5 trigger path

As Theop,
I want `lcrc verify` to default to **warn** on drift (cells are NOT invalidated; cache is unchanged) and to exit with code 5 when drift is detected,
So that I keep human-in-the-loop control over whether to re-measure (PRD Q1 resolution; FR29; FR45 exit 5 trigger path).

**Acceptance Criteria:**

**Given** verify completes and detects no drift
**When** the process exits
**Then** exit code is 0; the cache is unchanged.

**Given** verify completes and detects drift on at least one cell
**When** the process exits
**Then** exit code is 5; the cache is STILL unchanged (no cells invalidated, no cells overwritten).

**Given** drift was detected
**When** the report's footer renders
**Then** stderr (not stdout — keeps the report clean for parsing) prints actionable guidance: `"To re-measure affected models, run: lcrc scan --model <pattern>"` (substituted with the actual model patterns from drifted rows).

**Given** the architecture decision (PRD Q1 resolution: "user opts in to re-measurement")
**When** I check the codebase for any auto-invalidation logic in verify
**Then** there is none — `src/verify/` writes nothing to the cache; Story 5.1's read-only-open AC is the structural enforcement.

**Given** the user wants to act on drift
**When** they run `lcrc scan` (per the guidance message) against the affected models
**Then** Standard's structural re-measurement (per AR-10) takes care of it: `backend_build` likely changed; new cells get measured; old cells stay accessible via `lcrc show --all` (Story 4.3).

### Story 5.4: `lcrc verify --format json`

As a script author,
I want `lcrc verify --format json` to emit the same drift data as JSON with a top-level `schema_version` field, pipe-friendly to `jq`,
So that I can monitor cache integrity from cron / CI without parsing text tables (FR43, FR54).

**Acceptance Criteria:**

**Given** verify completes and `--format json` is set
**When** the command runs
**Then** stdout receives valid JSON with at minimum `{"schema_version": N, "summary": {...}, "drift": [...]}`; each `drift[i]` object has `model`, `task_id`, `cached_pass`, `new_pass`, `delta`, `cached_perf`, `new_perf`, `perf_delta_pct`, `ci_overlap` fields.

**Given** the JSON output schema
**When** I check `src/jsonout/schema.rs`
**Then** the verify schema is defined alongside Story 4.4's `show` schema as serializable Rust types; both share the same `schema_version` discipline (single global schema version per AR-28-style single-source-of-truth).

**Given** verify detects drift with `--format json`
**When** the command exits
**Then** exit code is still 5 (per Story 5.3); the JSON output is fully written to stdout *before* exit (no truncation when exit code is non-zero).

**Given** future minor lcrc versions add an optional field to the drift schema
**When** released
**Then** `schema_version` does NOT bump; older `jq` scripts continue to work (FR54).

**Given** `lcrc verify --format json | jq '.drift[].ci_overlap'`
**When** run after a scan
**Then** the output is a list of booleans (one per re-measured cell) — pipe-friendly machine-readable.

### Story 5.5: `lcrc verify --model <pattern>` filter

As Theop,
I want `lcrc verify --model <pattern>` to restrict the sample population to cells matching the model pattern (substring on name OR `model_sha` prefix),
So that after a `brew upgrade llama.cpp` I can verify drift on just the models I'm actively using (FR12; PRD Journey 4 spot-check pattern).

**Acceptance Criteria:**

**Given** a populated cache and `lcrc verify --model qwen --sample 3`
**When** the command runs
**Then** the sample of 3 cells is drawn only from cells whose model matches `qwen` (the filter applies before sampling, not after); the same filter helper from Story 3.4 is reused.

**Given** the filter matches zero cells
**When** verify runs
**Then** stderr prints a clear message ("no cells match pattern '<pattern>' for verification"); exit 0; no measurement runs.

**Given** the filter matches fewer cells than `--sample N`
**When** verify runs
**Then** verify samples min(N, matching_count); stderr notes the actual sample size.

**Given** `--model` and `--sample N` combine
**When** I check `src/cli/verify.rs`
**Then** clap-derive declares both flags with documented defaults; `--help` shows their interaction.

---

## Epic 6: Config, concurrency & CLI polish — safe for cron / CI / Makefile

**Goal:** Make lcrc production-ready as a scriptable CLI. TOML config at `$XDG_CONFIG_HOME/lcrc/config.toml` with figment-backed layered precedence (CLI flag > env var > TOML > built-in default). Config validation fails fast with line-pointing errors (exit 10). `scan.lock` enforces single-writer concurrency on `lcrc scan` (exit 12 with holding PID); `lcrc show`/`verify` remain lock-free. `paths.extra_model_dirs` extends discovery beyond the default llama.cpp cache. `lcrc --version` carries full self-attestation; `lcrc --help` and per-subcommand help are complete and polished.

### Story 6.1: TOML config schema + file loading with built-in defaults

As Theop,
I want lcrc to read an optional TOML config from `$XDG_CONFIG_HOME/lcrc/config.toml`, with every key defaulted in code so the file is never required,
So that I can override defaults when needed but never have to create a config file just to make lcrc work (FR49, AR-23).

**Acceptance Criteria:**

**Given** no config file exists at `$XDG_CONFIG_HOME/lcrc/config.toml`
**When** I run any lcrc command
**Then** lcrc starts cleanly with all built-in defaults applied; no error or warning about missing config.

**Given** a config file at the default path with valid TOML
**When** I inspect `src/config/schema.rs`
**Then** the `Config` struct mirrors the architecture-locked schema (AR-23): `[paths] cache_dir, report_dir, state_dir`; `[discovery] extra_model_dirs`; `[scan] default_depth, quick_task_timeout, standard_task_timeout, full_task_timeout, canary_task_timeout, server_startup_timeout`; `[runtime] docker_host`; `[backend] llama_server_path` — each with a documented default.

**Given** a partial config (only `[scan] default_depth = "standard"`)
**When** lcrc loads it
**Then** the explicit keys override defaults; all other keys keep their built-in defaults; no error.

**Given** the config file uses tilde expansion (e.g., `cache_dir = "~/custom/cache"`)
**When** lcrc loads it
**Then** `~` is expanded to the user's home directory consistently with `etcetera` XDG resolution.

**Given** a developer reads `src/config.rs`
**When** inspecting the loader
**Then** `config::load()` is the single function that reads the TOML file (per AR-28); no other module calls `toml::from_str` or reads the config path directly.

### Story 6.2: Env var + CLI flag layering on top of TOML

As Theop,
I want CLI flags > `LCRC_<SECTION>_<KEY>` env vars > TOML file > built-in defaults to be the resolved precedence (per FR50, AR-24), composed by figment in a single `config::load()` function,
So that I can override any setting at any layer without editing the TOML.

**Acceptance Criteria:**

**Given** TOML sets `[scan] default_depth = "quick"` and the env var `LCRC_SCAN_DEFAULT_DEPTH=standard` is set
**When** lcrc resolves config
**Then** the effective `default_depth` is `"standard"` (env wins over TOML).

**Given** the same env + a CLI flag `--depth full` (when present on the subcommand)
**When** lcrc resolves config
**Then** the effective depth is `"full"` (CLI wins over env).

**Given** `LCRC_DISCOVERY_EXTRA_MODEL_DIRS="/path/a:/path/b:/path/c"` (PATH-style colon-separated per AR-24)
**When** lcrc resolves discovery config
**Then** `extra_model_dirs` is `["/path/a", "/path/b", "/path/c"]`.

**Given** `config::load()` composes layers via figment
**When** I inspect the implementation
**Then** the layer order is: `Serialized::defaults(Config::default())`, then `Toml::file(toml_path)`, then `Env::prefixed("LCRC_").split("_")`, then CLI overrides applied separately from clap-derive (per AR-23 architecture sketch).

**Given** any module other than `src/config/`
**When** I grep for `std::env::var`
**Then** there are zero matches (per AR-28 single-source-of-truth — env reads happen only via `config::load`).

**Given** a unit test for the layering
**When** CI runs it
**Then** every layer's precedence is asserted (defaults vs TOML vs env vs CLI; combinations).

### Story 6.3: Config validation with line-pointing errors → exit 10

As Theop,
I want lcrc to validate the config file on startup and fail fast with a stderr message pointing at the offending line/key when a value is invalid,
So that I'm not stuck in a broken-config loop trying to figure out which key is wrong (FR51, FR45 exit 10 trigger path).

**Acceptance Criteria:**

**Given** a TOML file with a syntactically invalid line (e.g., unclosed quote)
**When** lcrc starts
**Then** stderr prints a clear error including the line number and column from the TOML parser's error message; exit 10.

**Given** a TOML file with a typo'd key (e.g., `[scan] default_dpeth = "quick"`)
**When** lcrc starts
**Then** stderr prints `"unknown key: [scan] default_dpeth"` (or similar) with the offending line; exit 10. (figment's deny_unknown_fields or equivalent).

**Given** a TOML file with a value out of range (e.g., `[scan] quick_task_timeout = -100`)
**When** lcrc starts
**Then** validation rejects it; stderr explains why (e.g., "must be a positive integer"); exit 10.

**Given** an env var with an invalid value (e.g., `LCRC_SCAN_DEFAULT_DEPTH=xyz` — not in `quick|standard|full`)
**When** lcrc starts
**Then** stderr explains the valid set; exit 10.

**Given** a CLI flag with an invalid value
**When** clap parses it
**Then** clap's own error message renders (per its conventions); exit 10 (mapped via `From<ClapError>` for the top-level `Error`).

**Given** valid configs at all layers
**When** lcrc starts
**Then** validation passes silently; no stderr noise.

### Story 6.4: `scan.lock` single-writer concurrency → exit 12 with holding PID

As Theop,
I want only one `lcrc scan` to be runnable at a time on a given machine (a second invocation exits cleanly with code 12 and the holding PID printed to stderr); `lcrc show` and `lcrc verify` remain lock-free,
So that I never accidentally corrupt the cache by running two scans in parallel (FR52, FR53, NFR-R7, FR45 exit 12 trigger path).

**Acceptance Criteria:**

**Given** no `lcrc scan` is running
**When** I run `lcrc scan`
**Then** the lock file at `$XDG_STATE_HOME/lcrc/scan.lock` is acquired (file content: the lcrc process PID); the scan proceeds.

**Given** `lcrc scan` is running with PID 12345
**When** I run `lcrc scan` in a second terminal
**Then** the second invocation exits 12 immediately; stderr prints `"another lcrc scan is in progress (PID 12345); exiting"`.

**Given** `lcrc scan` exits cleanly (any exit code 0/1/2/3/etc)
**When** the next `lcrc scan` runs
**Then** the lock is released (file removed or PID rewritten) and the new scan proceeds normally.

**Given** `lcrc scan` is killed via SIGKILL (no graceful cleanup)
**When** the next `lcrc scan` runs
**Then** lcrc detects the lock file's PID is no longer alive (e.g., `kill -0 PID` returns ESRCH), removes the stale lock, and proceeds. (Stale-lock recovery; documented in `src/scan/lock.rs`.)

**Given** `lcrc show` and `lcrc verify` are run while a scan holds the lock
**When** they execute
**Then** they ignore the lock entirely (read-only operations are lock-free per FR53); both succeed concurrent with the scan.

**Given** the lock implementation
**When** I inspect `src/scan/lock.rs`
**Then** it uses `fs2` or `fd-lock` for advisory locking on the file (per AR-4); no `sleep`-and-retry busy waits.

### Story 6.5: `paths.extra_model_dirs` config-driven discovery

As Theop,
I want `paths.extra_model_dirs` in the TOML config (or `LCRC_DISCOVERY_EXTRA_MODEL_DIRS` env) to extend model discovery beyond the default `~/.cache/llama.cpp/...`,
So that I can scan models in alternative locations (e.g., a network share, a project-local model cache) (FR11).

**Acceptance Criteria:**

**Given** `extra_model_dirs = ["/Volumes/models", "~/projects/lcrc-models"]` in the TOML config
**When** I run `lcrc scan`
**Then** `LlamaCppBackend::discover_models()` walks both extra directories in addition to `~/.cache/llama.cpp/`; discovered models from all locations are union'd into the candidate set.

**Given** the env var `LCRC_DISCOVERY_EXTRA_MODEL_DIRS="/path/a:/path/b"` is set
**When** lcrc resolves config
**Then** the colon-separated paths become `extra_model_dirs = ["/path/a", "/path/b"]` (PATH-style per AR-24).

**Given** an extra directory does not exist
**When** discovery walks it
**Then** stderr logs a warning at WARN level ("extra_model_dirs entry '/Volumes/models' not found; skipping"); discovery continues with the directories that do exist.

**Given** an extra directory contains GGUFs that duplicate `~/.cache/llama.cpp/` (same `model_sha`)
**When** discovery completes
**Then** the duplicate is detected by `model_sha` equality; only one `ModelRef` is kept; the cell key is unaffected (paths are not part of the cell key).

**Given** the extra directories store the model_sha computation of new files
**When** scanning many extra dirs first time
**Then** `LlamaCppBackend::discover_models()` doesn't re-hash files on every invocation; subsequent runs benefit from any caching layer (or model_sha is computed lazily before measurement, not during discovery — implementation choice documented).

### Story 6.6: Full `lcrc --version` self-attestation

As Theop,
I want `lcrc --version` to report the full attestation: lcrc semver + commit short, vendored mini-swe-agent version, vendored SWE-Bench Pro subset version, container image digest, and build commit hash,
So that any screenshot of `--version` paired with a screenshot of a report is sufficient to reconstruct the exact measurement environment (FR3 full, NFR-O4).

**Acceptance Criteria:**

**Given** a built lcrc binary
**When** I run `lcrc --version`
**Then** stdout receives output formatted like:
```
lcrc 0.1.0 (build a1b2c3d4)
  task source: swe-bench-pro 2026.04.30
  harness:     mini-swe-agent 1.2.3
  backend:     llama.cpp (auto-detected at runtime)
  container:   ghcr.io/<org>/lcrc-task@sha256:abc1234...
```

**Given** the version output
**When** I check the source of each field
**Then** lcrc semver + commit short come from `build.rs` / `env!("CARGO_PKG_VERSION")` + git commit; mini-swe-agent version comes from `image/requirements.txt` (or a build-time constant); SWE-Bench Pro subset version comes from `tasks/swe-bench-pro/version`; container digest comes from `src/constants.rs` per AR-13; build commit comes from `build.rs` capturing `git rev-parse --short HEAD`.

**Given** any of the constants is missing or empty (e.g., release built without commit info)
**When** the version renders
**Then** the missing field shows `"unknown"` rather than crashing; the rest renders normally.

**Given** the version field
**When** I run it cold
**Then** it returns in <200 ms (NFR-P7).

**Given** Story 1.4's placeholder `--version` output
**When** Epic 6 lands
**Then** the placeholder fields are replaced with the actual values; the format is stable (any future version-format change documented as a CHANGELOG note).

### Story 6.7: Full `lcrc --help` per-subcommand polish

As Theop,
I want `lcrc --help` and `lcrc <subcommand> --help` to be polished: clear descriptions, every flag documented with type + default + example, the README link rendered,
So that someone discovering lcrc for the first time can learn its surface from `--help` alone (FR4 full per-subcommand).

**Acceptance Criteria:**

**Given** `lcrc --help`
**When** I run it
**Then** stdout shows: program description (one paragraph), subcommand list with one-line descriptions for `scan`, `show`, `verify`, link to README (e.g., `"For details: https://github.com/<org>/lcrc"`).

**Given** `lcrc scan --help`
**When** I run it
**Then** stdout shows: subcommand description, every flag (`--depth`, `--model`, `--quiet`, `--report-path`) with type + default + brief description + an example invocation; exit-code summary at the bottom (per FR45).

**Given** `lcrc show --help`
**When** I run it
**Then** all of `--format`, `--model`, `--depth`, `--limit`, `--all` are documented similarly.

**Given** `lcrc verify --help`
**When** I run it
**Then** all of `--sample`, `--model`, `--format` are documented similarly.

**Given** the help text
**When** rendered
**Then** it returns in <200 ms (NFR-P7) and uses TTY-aware color when stderr/stdout are TTYs (per `is-terminal` from AR-4).

**Given** clap-derive is used everywhere
**When** I add or rename a flag in code
**Then** the `--help` output reflects the change automatically (no separate doc to keep in sync).

---

## Epic 7: Distribution, sandbox audit & calibration — v1 ship gate

**Goal:** Ship-gate epic. Homebrew formula (`brew install lcrc` works end-to-end on a clean Mac with `depends_on "podman" + "llama.cpp"`); GitHub Actions release workflow builds per-arch bottles and publishes the per-task container image to GHCR with digest pinning. Acceptance check #9 sandbox negative test runs the **full adversarial battery** as a binding test — every out-of-envelope attempt must fail at the container boundary (exit 2). Per-tier `*_task_timeout` values are **finally locked** based on data gathered from Epic 2's sanity check + ongoing Epic 3 runs. README, badge glossary, and JSON schema docs are published. SWE-Bench Pro redistribution license is confirmed (or fallback path documented). GHCR organization name is filled.

### Story 7.1: Homebrew formula with `depends_on` + caveats

As a friend Theop sent lcrc to,
I want `brew install lcrc` to work end-to-end on my clean Mac, pulling in Podman and llama.cpp as dependencies and printing the first-run setup steps in `brew info`,
So that getting started is one command + the documented setup in caveats (FR1, AR-6, NFR-I5).

**Acceptance Criteria:**

**Given** the formula at `homebrew/lcrc.rb`
**When** I inspect it
**Then** it declares: `desc`, `homepage`, `url` (pointing at a GitHub Release tarball), `sha256`, `license "Apache-2.0"`, `depends_on "podman"`, `depends_on "llama.cpp"`, `def install` placing the binary in `bin`, and `def caveats` with `podman machine init && podman machine start` instructions.

**Given** a clean Mac with Homebrew installed
**When** I run `brew install lcrc` (after the formula is published to a tap)
**Then** the install pulls Podman and llama.cpp if not present; the lcrc binary is placed in `$HOMEBREW_PREFIX/bin/lcrc`; `brew info lcrc` prints the caveats.

**Given** Podman is freshly installed via brew but `podman machine` not yet initialized
**When** I follow the caveats and run `podman machine init && podman machine start`, then `lcrc scan`
**Then** preflight passes (Story 1.9), and the scan proceeds end-to-end.

**Given** the formula references a published GitHub Release artifact
**When** I check the URL and SHA256
**Then** they match the actual release artifact (verified by Story 7.2's release workflow).

### Story 7.2: GitHub Actions release workflow

As a maintainer (Theop),
I want a `.github/workflows/release.yml` workflow that triggers on a `v*` tag, builds per-arch macOS Apple Silicon binaries, and drafts a GitHub Release with the binary attached,
So that cutting a release is a tag push, not a manual build dance (AR-5).

**Acceptance Criteria:**

**Given** a developer pushes a tag like `v0.1.0`
**When** the release workflow runs
**Then** it: (1) checks out the tagged commit, (2) runs `cargo build --release` on macOS Apple Silicon, (3) packages the binary as a tarball with a deterministic name (e.g., `lcrc-0.1.0-aarch64-apple-darwin.tar.gz`), (4) computes the SHA256, (5) drafts a GitHub Release with the tarball attached and the SHA256 in the release notes.

**Given** the release workflow gates
**When** I inspect it
**Then** it depends on `ci.yml` passing first (fmt + clippy + test gates from Story 1.2 + binding tests including Story 7.4 sandbox battery).

**Given** the release is drafted (not auto-published)
**When** I review it on GitHub
**Then** I can edit release notes manually and click "Publish" — semi-automated, not fully automated, so I get a human-in-loop review before the release goes public.

**Given** the workflow's permissions
**When** I inspect it
**Then** it requests only the minimum scopes needed (release write); no broader org permissions.

### Story 7.3: GHCR container image publish + GHCR org name filled

As a maintainer,
I want the same release workflow (or a sibling) to build the per-task container image from `image/Dockerfile`, tag it with the lcrc version, push it to `ghcr.io/<actual-org>/lcrc-task`, capture the digest, and have `src/constants.rs` reference that digest in the matching commit,
So that the image identity is reproducible and the AR-38 "fill GHCR org name" pre-v1 owed item is resolved (AR-13, AR-38).

**Acceptance Criteria:**

**Given** the release workflow runs on a tag push
**When** the image-publish step executes
**Then** it: (1) builds `image/Dockerfile`, (2) tags as `ghcr.io/<org>/lcrc-task:<lcrc-version>` AND `ghcr.io/<org>/lcrc-task:latest`, (3) pushes both tags, (4) captures the resulting `sha256:...` digest.

**Given** the digest is captured
**When** I inspect `src/constants.rs` for the matching commit
**Then** `CONTAINER_IMAGE_DIGEST` is set to the actual `sha256:...` value (no `<placeholder>` strings).

**Given** AR-38 (GHCR org name placeholder)
**When** I grep the codebase + Dockerfile + Homebrew formula + release workflow + README for `<org>`
**Then** there are zero matches — every reference is the actual org/user name.

**Given** the published image
**When** any user pulls it (e.g., via Story 1.10's image pull on first scan)
**Then** the digest matches `CONTAINER_IMAGE_DIGEST` exactly; lcrc refuses to use an image whose digest doesn't match (per AR-13 digest pinning).

**Given** the image-publish workflow's permissions
**When** I inspect it
**Then** it has `packages: write` scope to push to GHCR; the org-level GHCR settings allow lcrc-task as a public package.

### Story 7.4: Acceptance check #9 sandbox negative test — binding v1-ship gate

As a reviewer reading the lcrc repo (PRD Journey 5),
I want a binding integration test at `tests/sandbox_envelope.rs` that runs the full adversarial battery from acceptance check #9 — and CI fails the build if any probe escapes the sandbox,
So that the trust story is verifiable by inspection and v1 cannot ship if the sandbox has structural holes (AR-32, NFR-S1–S6, NFR-S2).

**Acceptance Criteria:**

**Given** `tests/sandbox_envelope.rs`
**When** I inspect it
**Then** it runs an adversarial task whose agent attempts the full battery from PRD acceptance #9 + AR-32: arbitrary host file reads (`cat /etc/passwd`, `find ~/`, `cat /Users/*/Documents/*`, `cat ~/.aws/credentials`, `cat ~/.ssh/id_rsa`), arbitrary outbound network (`curl https://example.com`, `nslookup google.com`, `nslookup` of arbitrary domains), sibling-task workspace enumeration (`ls /tmp/lcrc-task-*`, `cat /tmp/lcrc-task-*/output.txt`), credential env var probing (`env | grep -iE 'token|key|secret|password|aws|gh|openai|anthropic|hf'`).

**Given** the test runs
**When** the adversarial agent completes
**Then** every probe either (a) fails at the container boundary (read returns empty / network unreachable / env returns no matches), OR (b) reads the *image's* file content (Debian-slim's `/etc/passwd`, not the host's). The test asserts both.

**Given** the test asserts the run produced `Badge::SandboxViolation` for at least one detected violation
**When** the cell records
**Then** the badge is present.

**Given** the test asserts the lcrc process exit code
**When** the run completes
**Then** exit code is 2 (per FR45, NFR-S2).

**Given** CI runs `tests/sandbox_envelope.rs`
**When** any probe escapes (i.e., reads actual host content the test fixture marked as off-limits)
**Then** the test fails; CI blocks the build; the v1 release cannot be cut (release workflow per Story 7.2 depends on CI being green).

**Given** the test is a binding gate
**When** I check `.github/workflows/ci.yml`
**Then** the sandbox negative test is included in the test gate (not a separate optional suite); failure blocks PR merges and release cuts.

### Story 7.5: Calibration pass — final lock of `*_task_timeout` values

As Theop,
I want a calibration pass where I run `lcrc scan --depth quick`, `--depth standard`, and `--depth full` on the M1 Pro 32GB reference rig with my actual installed-model set, gather wall-clock data per task and per scan, and lock the final `quick_task_timeout`, `standard_task_timeout`, `full_task_timeout`, `canary_task_timeout`, `server_startup_timeout` values into the built-in defaults (Story 6.1's `Config::default()`),
So that the working-assumption defaults are replaced with empirically-grounded values before v1 ships (AR-37, AR-17).

**Acceptance Criteria:**

**Given** Epic 2's Story 2.17 sanity check has been run and produced wall-clock data for Quick
**When** I extend with Standard + Full runs
**Then** I have empirical per-task wall-clock distributions for all three depths on the reference rig.

**Given** the empirical data
**When** I choose final cap values
**Then** each cap is set to a generous-but-bounded value (e.g., 95th-percentile observed task wall-clock + 30% headroom); rationale is documented in a file like `docs/calibration-2026-XX.md` or a CHANGELOG entry.

**Given** the final cap values are determined
**When** I update the codebase
**Then** `Config::default()` (Story 6.1) reflects the new values; the README's "Performance" section quotes them; the values are pinned per release (not per-user-tunable beyond the existing TOML override).

**Given** Quick exceeds the 25-min ceiling on M1 Pro 32GB / 5-model
**When** the calibration pass surfaces it
**Then** the per-task cap is tightened OR the task selection (Story 2.6's static-ordering first-task pick) is revisited — Quick must remain Quick (per PRD §"Three-tier scan budget").

**Given** the calibration is documented
**When** v1 ships
**Then** the documented numbers + rationale are part of the release artifact (README or shipped docs); future calibration passes (per CHANGELOG) can compare.

### Story 7.6: README + badge glossary + JSON schema docs

As a new lcrc user (Theop or someone he shared the repo with),
I want the README to honestly describe scope, limitations, install steps, usage examples, and the badge glossary; and the JSON output schemas to be documented alongside,
So that I can use lcrc and audit its claims without reading the source (PRD §"Release Success — Honest README", FR54 schema docs).

**Acceptance Criteria:**

**Given** `README.md` at the repo root
**When** I read it top to bottom
**Then** it contains: (1) one-paragraph description matching the PRD's Executive Summary, (2) honest scope + limitations (v1 = macOS Apple Silicon only; llama.cpp only; Quick is screening not switch-decision; etc.), (3) install (`brew install lcrc`) + first-run setup (`podman machine init && start`), (4) usage examples for `lcrc scan`, `lcrc show`, `lcrc verify` (one example each), (5) the badge glossary (every variant in the `Badge` enum, kebab-case, with one-line meaning), (6) link to the JSON schema docs.

**Given** the badge glossary
**When** I cross-reference it against `src/report/badges.rs`
**Then** every enum variant is documented; no documented badges that don't exist; no enum variants that aren't documented (single source of truth maintained per AR-28).

**Given** JSON schema documentation
**When** I check `docs/json-schema.md` (or wherever it lives)
**Then** it documents the `lcrc show --format json` and `lcrc verify --format json` schemas — every field, type, optionality, and the `schema_version` versioning policy (FR54 backward compat within major).

**Given** the README claims about reproducibility and trust
**When** I cross-check against the actual cell schema (Story 1.7) + sandbox design (Story 1.10 + 2.7) + acceptance #9 test (Story 7.4)
**Then** every claim has a verifiable backing in the codebase or test suite (no over-promises).

**Given** PRD §"Release Success — no marketing or competitive-window urgency framing" (per project memory)
**When** I read the README
**Then** zero competitive-urgency framing exists ("be the first to ..." / "before X catches up" / etc.); positioning is honest scope-and-tradeoffs.

### Story 7.7: SWE-Bench Pro redistribution license confirmation + fallback path

As a maintainer,
I want to confirm Scale's redistribution terms for the SWE-Bench Pro curated subset before v1 release, AND to document the fallback contingency (per AR-19, AR-36) if vendoring is restricted,
So that v1 doesn't ship a license violation and lcrc has a graceful path if Pro becomes unusable mid-v1-lifecycle.

**Acceptance Criteria:**

**Given** Scale's published license + redistribution terms for SWE-Bench Pro
**When** I review them
**Then** I have a definitive answer: vendorable (case A) or restricted (case B). The decision is recorded in `docs/swe-bench-pro-license.md` (or equivalent) with the date + the license text version reviewed.

**Given** case A (vendorable)
**When** v1 ships
**Then** `tasks/swe-bench-pro/` is bundled in the release tarball; the README documents the license and attribution per Scale's terms.

**Given** case B (restricted)
**When** v1 ships
**Then** lcrc implements the install-time pull fallback per AR-19: on first run, lcrc downloads the SWE-Bench Pro subset from Scale's documented endpoint to `$XDG_DATA_HOME/lcrc/tasks/`; the version + hash are documented in `tasks/swe-bench-pro/manifest.json`; the README documents the dependency and the brittleness.

**Given** the AR-19 fallback contingency (Pro becomes unusable later — restricted or contaminated)
**When** documented in `docs/task-source-fallback.md`
**Then** it lists candidate alternative `TaskSource` impls (SWE-Bench Lite + Verified-cleanup, LiveCodeBench, Multi-SWE-Bench-mini) and the architectural slot (Story 2.3 trait) that swaps them in without rearchitecture.

**Given** v1 release
**When** the README is updated
**Then** it accurately reflects which case (A or B) is in effect and what the user needs to know.

### Story 7.8: Release-pipeline dry-run via v0.9.0-rc1

As a maintainer (Theop),
I want to cut a v0.9.0-rc1 release through the entire pipeline (release workflow → image publish → GHCR digest pin → tap formula update → `brew install lcrc` → smoke test on a clean Mac) before tagging v1.0.0,
So that the first end-to-end exercise of the release chain is a release candidate I can throw away, not the actual v1 cut where mistakes ship to friends.

**Acceptance Criteria:**

**Given** Stories 7.1, 7.2, 7.3, 7.4 are complete and CI is green
**When** I push tag `v0.9.0-rc1`
**Then** the release workflow runs end-to-end: builds the binary, packages the tarball, builds and pushes the per-task image to GHCR with a real digest, captures the digest, opens a PR against the tap repo with the updated formula, drafts the GitHub Release with the tarball + SHA256.

**Given** the dry-run completes
**When** I run `brew install lcrc` from the tap on a clean Mac (or a clean test VM)
**Then** the formula installs cleanly; `lcrc --version` reports `0.9.0-rc1` with all self-attestation fields populated (per Story 6.6); `lcrc scan` (after `podman machine init && start` per Story 1.9 caveats) runs end-to-end against my real installed-model set and produces a populated `latest.html`.

**Given** any pipeline failure during the dry-run
**When** I diagnose
**Then** the failure is repaired in the codebase (not papered over); a follow-up `v0.9.0-rc2` (or further) is cut and re-tested. v1.0.0 is tagged only after a fully-clean dry-run end-to-end.

**Given** the dry-run process is documented
**When** I update `docs/release-process.md`
**Then** the dry-run is recorded as a required ship-gate step for any future major version bump (v2.0.0, etc.); the document includes the rc-tag naming convention and the "repair, don't paper over" policy.

**Given** the rc1 release artifacts (tarball, image, formula PR)
**When** v1.0.0 is tagged after a clean dry-run
**Then** the rc1 artifacts are either (a) deleted (release deleted from GitHub, image untagged from GHCR via `gh release delete` + GHCR API, tap PR closed without merge), or (b) left in place as historical reference. Choice is the maintainer's; rationale is recorded in CHANGELOG.
