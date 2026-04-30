---
stepsCompleted:
  - step-01-document-discovery
  - step-02-prd-analysis
  - step-03-epic-coverage-validation
  - step-04-ux-alignment
  - step-05-epic-quality-review
  - step-06-final-assessment
filesIncluded:
  prd: _bmad-output/planning-artifacts/prd.md
  architecture: _bmad-output/planning-artifacts/architecture.md
  epics: _bmad-output/planning-artifacts/epics.md
  ux: null
---

# Implementation Readiness Assessment Report

**Date:** 2026-04-30
**Project:** lcrc

## Document Inventory

| Type | File | Size | Status |
|------|------|------|--------|
| PRD | `prd.md` | 80.8K | confirmed |
| Architecture | `architecture.md` | 78.1K | confirmed |
| Epics & Stories | `epics.md` | 151.7K | confirmed |
| UX Design | — | — | intentionally skipped (no UI surface) |

**Notes:**
- No duplicate whole/sharded conflicts.
- UX intentionally skipped per user confirmation.
- Supporting artifacts present but out of scope for this assessment: `product-brief-lcrc.md`, `product-brief-lcrc-distillate.md`.

## PRD Analysis

### Functional Requirements

**Installation & First Run (FR1–FR6)**
- FR1: Install lcrc via Homebrew (`brew install lcrc`) on macOS Apple Silicon.
- FR2: `lcrc scan` runs immediately after install with zero prior configuration; sensible defaults cover all required behavior.
- FR3: `lcrc --version` shows lcrc semver, vendored mini-swe-agent version, vendored SWE-Bench Pro subset version, and build commit hash.
- FR4: `lcrc --help` and per-subcommand `lcrc <subcommand> --help` available.
- FR5: Empty-machine UX — one-paragraph explainer plus hardcoded starter pack of 3–5 small models with copy-paste-ready download commands when no eligible models detected.
- FR6: Canary pass/fail/skipped state rendered prominently in report header on every scan.

**Model Discovery & Eligibility (FR7–FR12)**
- FR7: Detect installed models in llama.cpp local cache directory (`~/.cache/llama.cpp/...`).
- FR8: Compute format-agnostic content hash (`model_sha`) for each detected model.
- FR9: Filter detected models by RAM × default-context-length budget; exclude models that would not fit.
- FR10: CLI output and report show which detected models were excluded by the fit gate and why.
- FR11: Extend model discovery to additional directories via `paths.extra_model_dirs` config.
- FR12: Restrict scan/show/verify to a model subset via `--model <pattern>` (substring match against model name or `model_sha` prefix).

**Measurement Execution (FR13–FR23)**
- FR13: Run a canary task at the start of every `lcrc scan` invocation regardless of `--depth`.
- FR14: Render canary outcome as `canary-pass`, `canary-fail`, or `canary-skipped`; `canary-fail` does not block the report from being written.
- FR15: Execute SWE-Bench Pro tasks against each fit-eligible model via mini-swe-agent wrapped as a subprocess.
- FR16: Each per-task measurement runs inside a default-deny isolation envelope (per-task ephemeral container): no host filesystem mounted (only per-task workspace bind-mounted RW), no network (only single localhost route to host's `llama-server` port), no host env vars (only documented per-task allowlist). Default-deny by structural construction, not enumerated policy.
- FR17: Record sandbox-violation events as templated badges and report-surfaced events; sandbox violations cause `lcrc scan` to exit code `2`.
- FR17a: Detect supported container runtime at scan pre-flight; if missing/not running, exit code `11` with stderr setup instructions; no measurement attempted. Runtime choice deferred to architecture.
- FR17b: Pin per-task container image (or build recipe) per lcrc release; image identifier recorded in cell metadata per FR31.
- FR18: Collect macOS-native perf metrics (tok/s, ttft, peak RSS, power, thermal) per cell; uncollectable metrics recorded as null/unavailable, never block measurement.
- FR19: Enforce per-tier per-task wall-clock cap; capped tasks record timeout-equivalent badge and do not block scan.
- FR20: Three depths via `--depth quick|standard|full`; each successive depth extends previous depth's cells, never replaces.
- FR21: Quick = canary + 1 SWE-Bench Pro task per model (task #1 in static "most-informative-first" ordering).
- FR22: Standard extends each model's cell to 3–5 tasks (Quick's task plus next 2–4 in static ordering).
- FR23: Full extends each model's cell to full curated SWE-Bench Pro subset and adds quant/ctx variants.

**Cache & Persistence (FR24–FR31)**
- FR24: Cache key = `(machine_fingerprint, model_sha, backend_build, params)`; `machine_fingerprint` = chip generation + RAM + GPU core count; `params` = ctx length, sampler temperature, threads, `n_gpu_layers`.
- FR25: Each `(model, task)` cell stored independently; cells are unit of caching, measurement, resumability, depth extension.
- FR26: Cache lookup before measuring; matching cells not re-measured within or across scans.
- FR27: Persist partial scan results so Ctrl-C / OOM / crash mid-scan loses no completed cells; next invocation auto-resumes (no `--resume` flag).
- FR28: `lcrc verify --sample N` re-measures N sampled cells; numerical drift report (cached value, new value, delta, CI overlap per cell).
- FR29: `lcrc verify` defaults to warn on drift; cells not invalidated unless user re-runs `lcrc scan`.
- FR30: macOS patch upgrades are machine-fingerprint-stable; `backend_build` changes invalidate per architecture-phase policy (open question).
- FR31: Per-cell metadata records: depth tier, scan timestamp, `backend_build`, lcrc version, vendored harness/task version, perf metrics.

**Reporting (FR32–FR43)**
- FR32: Single self-contained static HTML report file; no external network required to view.
- FR33: HTML report regenerated to disk after every cell completes; user refreshes browser tab manually.
- FR34: Canonical screenshot-friendly header without scrolling: machine fingerprint, scan date, lcrc version, `backend_build`, canary state.
- FR35: Wilson-score confidence intervals on every leaderboard pass-rate.
- FR36: Templated failure-mode badges from fixed enum: `ctx-limited`, `OOM-at-N`, `repetition-loop`, `tool-call-format-failure`, `thermal-throttled`, `low-confidence-CI`, plus sandbox-violation tags. No LLM-generated prose.
- FR37: Each leaderboard cell tagged with depth tier (Quick/Standard/Full) that produced it.
- FR38: Every Quick-tier row carries `low-confidence-CI` badge by structural default.
- FR39: HTML report default location `$XDG_DATA_HOME/lcrc/reports/latest.html` plus timestamped historical file `report-<ISO8601>.html`; overridable via `--report-path`.
- FR40: `lcrc show` plain-text leaderboard ranks identically to HTML report.
- FR41: `lcrc show` filters: `--model <pattern>`, `--depth <tier>`, `--limit N`.
- FR42: `--all` flag includes cells for uninstalled models or outdated `backend_build`s (default hidden, mirrors HTML).
- FR43: `--format json` on `lcrc show` and `lcrc verify`; JSON has top-level `schema_version`. Default `--format text`.

**CLI Surface, Configuration & Scripting (FR44–FR54)**
- FR44: Every command non-interactive; no prompts on any subcommand at any depth.
- FR45: Documented semver-stable exit codes per subcommand: `0` success; `1` canary failed; `2` sandbox violations; `3` scan aborted by signal; `4` cache empty (`show`); `5` drift detected (`verify`); `10` configuration error; `11` pre-flight failure; `12` concurrent `lcrc scan` in progress.
- FR46: Results (text/JSON) to stdout; progress, diagnostics, errors to stderr; pipe-friendly.
- FR47: Per-cell completion lines and estimated-remaining clock to stderr during scan; color when stderr is TTY, plain otherwise.
- FR48: `--quiet`/`-q` on `lcrc scan` suppresses per-cell streaming; report still regenerates, results still write, exit codes unchanged.
- FR49: Optional TOML config at `$XDG_CONFIG_HOME/lcrc/config.toml`; every key has documented default.
- FR50: Configuration precedence: CLI flag > env var > config file > built-in default.
- FR51: Validate config file on startup; invalid keys/values fail fast with stderr message pointing at offending line, exit code `10`.
- FR52: Single-writer concurrency on `lcrc scan` via lock file at `$XDG_STATE_HOME/lcrc/scan.lock`; concurrent invocations exit `12` with stderr message identifying holding PID.
- FR53: `lcrc show` and `lcrc verify` lock-free; may run concurrently with each other and with `lcrc scan`.
- FR54: Stable JSON output schemas; backward-compatible additions only within a major; breaking changes bump major.

**Total FRs:** 56 (FR1–FR54 with FR17a, FR17b sub-items).

### Non-Functional Requirements

**Performance (NFR-P1–NFR-P9), reference rig M1 Pro 32GB / ~5 installed models**
- NFR-P1: Quick scan ≤25 min wall-clock (target ~15 min); container spin-up included.
- NFR-P2: Standard scan extending Quick-populated cache ~1.5–3 h wall-clock.
- NFR-P3: Full scan ≤12 h wall-clock (overnight target).
- NFR-P4: No single SWE-Bench Pro task at Quick depth exceeds per-tier cap (working assumption 600s); capped tasks badged.
- NFR-P5: Cache-key lookup <100 ms for caches up to 10,000 cells.
- NFR-P6: HTML report regeneration <2 s for caches up to 1,000 cells.
- NFR-P7: `lcrc show` <500 ms for caches up to 1,000 cells; `--help`/`--version` <200 ms.
- NFR-P8: Estimated-remaining clock updates ≥ once every 10 s; per-cell completion lines within 1 s of cell finishing.
- NFR-P9: Container creation/mount/shutdown overhead per task <5 s on reference rig with chosen runtime.

**Reliability (NFR-R1–NFR-R8)**
- NFR-R1: Resumability — Ctrl-C/OOM/host suspend-resume/crash loses no completed cells; auto-resume without flags.
- NFR-R2: Atomic cell writes — cell either appears fully or not at all; no half-written cells.
- NFR-R3: Cache durability across version upgrades — patch and minor upgrades read existing caches; major upgrades require explicit migration; too-old schema detected with clear error.
- NFR-R4: Graceful degradation on perf collection — uncollectable metrics null/unavailable, never abort scan.
- NFR-R5: Graceful degradation on `llama-server` lifecycle — startup/crash/hang/unexpected exit detected via timeout, surfaced as templated badge; scan continues.
- NFR-R6: Idempotency — repeated scans on unchanged inputs produce no new measurements; `lcrc verify` non-destructive.
- NFR-R7: Concurrency safety — lock file prevents overlapping scans; `show`/`verify` reads consistent during a scan.
- NFR-R8: Container teardown on abort — best-effort teardown of running per-task containers; runtime cleanup mechanisms as backstop; no orphan accumulation.

**Security (NFR-S1–NFR-S7)**
- NFR-S1: Default-deny by structural construction — no host fs mounted (only per-task workspace), no network except localhost→llama-server, no host env vars except documented allowlist; no enumerated denylist.
- NFR-S2: Sandbox failure visibility — sandbox-violation badge on row + exit code `2`; no silent pass; acceptance check #9 verifies via adversarial battery.
- NFR-S3: Container runtime is hard dependency — no fallback, no `--unsafe-no-sandbox`; pre-flight refusal exits `11`.
- NFR-S4: Network surface — exactly one outbound destination (host `llama-server`); DNS, public internet, host-other-port, sibling-bridge other-container blocked.
- NFR-S5: Env var scrubbing — only documented allowlist (e.g., `PATH`, `LANG`, task test-runner config); credential vars (`AWS_*`, `GH_*`, `GITHUB_TOKEN`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `HF_TOKEN`, anything else not allowlisted) absent inside container.
- NFR-S6: Container image pinned per lcrc release; image identifier in cell metadata; image content (base OS, toolchains, runners) documented; reviewable by inspection.
- NFR-S7: No telemetry — zero usage telemetry, crash reports, anonymized stats, opt-in or opt-out, in v1.

**Compatibility & Portability (NFR-C1–NFR-C5)**
- NFR-C1: macOS 12 Monterey+ on Apple Silicon (M1–M4); Intel Mac and pre-Monterey explicitly unsupported.
- NFR-C2: `machine_fingerprint` stable across macOS patch upgrades; cells remain valid (per FR30).
- NFR-C3: Cache populated by lcrc `X.Y.Z` reads under `X.Y.(Z+n)`; patch versions never invalidate.
- NFR-C4: Vendored `mini-swe-agent`, SWE-Bench Pro subset, container image pinned by lcrc release; no silent acceptance of unpinned versions.
- NFR-C5: v1 architecture must not preclude Linux NVIDIA support in v1.1 — platform-specific code (perf, container runtime, model discovery) factored cleanly.

**Observability (NFR-O1–NFR-O4)**
- NFR-O1: Streaming feedback — per-cell completion lines and per-model progress lines to stderr; estimated-remaining clock ≥ every 10 s.
- NFR-O2: Disk-only state — only writes are cache, HTML reports, lock file, optional log via stderr redirect; only network sockets are localhost→llama-server and local container runtime control socket.
- NFR-O3: No telemetry, hard line; non-negotiable in v1 (re-states NFR-S7).
- NFR-O4: `lcrc --version` self-attestation reports lcrc semver, vendored mini-swe-agent version, SWE-Bench Pro subset version, container image identifier, build commit hash.

**Integration (NFR-I1–NFR-I6)**
- NFR-I1: `llama-server` integration — lcrc starts/manages servers per measurement (server per cell or per model — architecture decides); server runs on host (not in per-task container, to avoid model-load overhead per task); per-task containers connect via constrained localhost route; crash/hang detection via documented timeouts (NFR-R5).
- NFR-I2: `mini-swe-agent` integration — invoked as vendored subprocess inside per-task container (agent itself isolated); lcrc captures exit code, stdout, stderr, pass/fail signal; subprocess crashes badged (NFR-R5).
- NFR-I3: Perf-collection integration — metrics collected from host (not container); mechanism (`powermetrics`+sudo, signed launchd helper, or graceful-degrade-without-power) chosen in architecture; privilege failure → null metrics, never aborted scan (NFR-R4).
- NFR-I4: Container runtime integration — pre-flight detection (FR17a); supported runtimes documented; architecture picks one or more; uses standard CLI/API surface; rootless preferred where runtime offers it.
- NFR-I5: Homebrew distribution — `brew install lcrc` is canonical install path; formula `depends_on` chosen container runtime so it's pulled in if missing; user informed at install time.
- NFR-I6: No required cloud or external service — no API key, no auth, no remote endpoint; user's machine is entire dependency graph (plus `llama-server` and container runtime).

**Total NFRs:** 39 (P:9, R:8, S:7, C:5, O:4, I:6).

### Additional Requirements / Constraints

**Acceptance check items (Success Criteria > Measurable Outcomes 1–9)** — these are concrete v1 acceptance criteria the build must hit on M1 Pro 32GB:
- AC#1: Quick budget ≤25 min on 5-model set.
- AC#2: Standard extension fills only missing cells, top-3 stable across consecutive Standard runs on unchanged inputs.
- AC#3: Full extension reports tightened CIs vs Standard with no rank inversions on top-3 unless explained by templated badge.
- AC#4: Streaming CLI feedback during scan + on-disk HTML regenerated after every cell.
- AC#5: HTML report renders canonical header without scrolling and every row carries Wilson CI, cache age + `backend_build`, badges, depth tier.
- AC#6: Canary header impossible to miss on `canary-fail` (deliberate broken backend).
- AC#7: `lcrc verify --sample 3` produces interpretable numerical drift report.
- AC#8: `lcrc show` ranks identically to HTML report.
- AC#9: Sandbox negative test — adversarial battery (host fs reads, outbound network, sibling-workspace fishing, credential env vars) every attempt fails at container boundary; sandbox-violation badges + exit `2`.

**Architecture-phase open questions (deferred, not pre-decided):**
1. Pass@1 vs pass@k semantics + per-tier wall-clock cap behavior + timeout-as-fail vs timeout-as-skip.
2. Cache staleness policy on `backend_build` change (invalidate-all vs ABI-aware classifier).
3. Harness/task version representation in cache key (collapse into `backend_build`, fifth dimension, or scope by lcrc release).
4. SWE-Bench Pro lifecycle fallback plan.
5. macOS perf-collection privilege model.
6. Cache storage shape (SQLite + JSON blobs vs flat JSON-per-cell).
7. Run resumability protocol details (cell-level independence makes trivial answer work for free).

**v1.1+ extensibility constraints on v1 architecture (must not be blocked):**
- Custom-eval extension surface (engineering-lead persona, J6) — task source must be isolated module with small interface.
- Adaptive depth (Wilson-CI early stop) — cell-level cache structure already supports.
- Adaptive task-ordering re-calibration against local cache.
- Multi-run reliability metric.
- Linux NVIDIA tier-1 platform support (per NFR-C5).
- MLX backend (architecture phase decides whether low-effort enough for v1).
- Background daemon + native macOS notifications.

### PRD Completeness Assessment

**Strengths:**
- Functional scope is sharp and bound. 56 FRs with clear ownership of every CLI surface, every output format, every exit code.
- NFRs include concrete numeric targets (latency, wall-clock budgets) tied to a named reference rig, not vague "performant" language.
- Security section is unusually rigorous for a personal tool; threat model is named (the measurement subject itself), and isolation is structural rather than enumerated.
- Acceptance criteria (#1–#9) are testable and directly map to FRs/NFRs.
- Open questions are explicitly carried to architecture rather than silently assumed.

**Watch items for downstream coverage check:**
- FR17a/FR17b are sub-items not numbered as distinct FRs — epics/stories must not lose them. Look for explicit coverage in the epic breakdown.
- Architecture-phase open questions (esp. #1 pass-rate semantics, #2 backend-build invalidation) cascade into FRs (FR19, FR30) — verify the architecture document resolved them, otherwise stories will inherit unresolved decisions.
- v1.1+ extensibility constraints are real architecture obligations (NFR-C5 + Journey 6 capability surface) — verify the architecture and epics treat them as binding, not aspirational.
- AC#9 (sandbox negative test) is a concrete adversarial test — must appear in the implementation plan, likely as a dedicated story.

## Epic Coverage Validation

### Epic FR Coverage Extracted

The epics document at lines 240–298 contains an explicit "FR Coverage Map" table; per-epic headers also list "FRs covered". Below is the consolidation cross-checked against both surfaces.

### Coverage Matrix

| FR | PRD scope (short) | Epic Coverage | Status |
|---|---|---|---|
| FR1 | `brew install lcrc` on macOS Apple Silicon | Epic 7 (Story 7.1) | ✓ Covered |
| FR2 | `lcrc scan` works with zero config | Epic 1 (skeleton) → Epic 2 (real-model scan) | ✓ Covered |
| FR3 | `lcrc --version` self-attestation | Epic 1 (stub Story 1.4) → Epic 6 (Story 6.6 full) | ✓ Covered |
| FR4 | `lcrc --help` per-subcommand | Epic 1 (Story 1.4 skeleton) → Epic 6 (Story 6.7) | ✓ Covered |
| FR5 | Empty-machine UX with starter pack | Epic 2 (Story 2.14) | ✓ Covered |
| FR6 | Canary state in report header | Epic 2 (Story 2.5) | ✓ Covered |
| FR7 | llama.cpp cache discovery | Epic 2 (Story 2.1) | ✓ Covered |
| FR8 | format-agnostic `model_sha` | Epic 1 (Story 1.6) → Epic 2 (at scale) | ✓ Covered |
| FR9 | RAM × ctx fit gate | Epic 2 (Story 2.2) | ✓ Covered |
| FR10 | exclusion visibility | Epic 2 (Story 2.2) | ✓ Covered |
| FR11 | `paths.extra_model_dirs` discovery | Epic 6 (Story 6.5) | ✓ Covered |
| FR12 | `--model <pattern>` filter | Epic 3 (Story 3.4); reused by Stories 4.2, 5.5 | ✓ Covered |
| FR13 | canary runs every scan | Epic 2 (Story 2.5) | ✓ Covered |
| FR14 | 3-state header rendering | Epic 2 (Story 2.5) | ✓ Covered |
| FR15 | mini-swe-agent subprocess execution | Epic 1 (Story 1.12 hardcoded) → Epic 2 (Story 2.6 per-model) | ✓ Covered |
| FR16 | default-deny container envelope | Epic 1 (Story 1.10 workspace + custom network) → Epic 2 (Story 2.7 env allowlist) | ✓ Covered |
| FR17 | sandbox-violation events + exit 2 | Epic 2 (Story 2.8) | ✓ Covered |
| FR17a | container runtime preflight + exit 11 | Epic 1 (Story 1.9) | ✓ Covered |
| FR17b | container image digest pinning + cell metadata | Epic 1 (Stories 1.10, 1.14) | ✓ Covered |
| FR18 | macOS-native perf metrics graceful-degrade | Epic 2 (Story 2.10) | ✓ Covered |
| FR19 | per-tier wall-clock cap + `task-timeout` badge | Epic 2 (Story 2.9) | ✓ Covered |
| FR20 | `--depth quick|standard|full` | Epic 1 (flag accepted) → Epic 2 (Quick) → Epic 3 (Stories 3.1, 3.2) | ✓ Covered |
| FR21 | Quick = canary + 1 task/model from static order | Epic 2 (Story 2.6) | ✓ Covered |
| FR22 | Standard = 3–5 tasks | Epic 3 (Story 3.1) | ✓ Covered |
| FR23 | Full = full subset + quant/ctx variants | Epic 3 (Stories 3.2, 3.3) | ✓ Covered |
| FR24 | 7-dim cell PK | Epic 1 (Story 1.7) | ✓ Covered |
| FR25 | independent cell storage | Epic 1 (Story 1.7) | ✓ Covered |
| FR26 | lookup-before-measure | Epic 1 (Story 1.8) → Epic 3 (user-visible cache extension) | ✓ Covered |
| FR27 | resumability without `--resume` | Epic 1 (Story 1.8 atomic) → Epic 2 (Story 2.15 end-to-end) | ✓ Covered |
| FR28 | `lcrc verify --sample N` numerical drift report | Epic 5 (Stories 5.1, 5.2) | ✓ Covered |
| FR29 | warn-not-invalidate default | Epic 5 (Story 5.3) | ✓ Covered |
| FR30 | machine_fingerprint stable across OS patches | Epic 5 (Story 5.1 AC) — leverages Story 1.5 | ✓ Covered |
| FR31 | per-cell metadata | Epic 1 (Story 1.7 base) → Epic 2 (Story 2.10 perf fields) | ✓ Covered |
| FR32 | self-contained HTML report | Epic 1 (Story 1.13 one-row) → Epic 2 (Story 2.12 full) | ✓ Covered |
| FR33 | regenerate after every cell | Epic 1 (Story 1.13) | ✓ Covered |
| FR34 | canonical screenshot-friendly header | Epic 2 (Story 2.12) | ✓ Covered |
| FR35 | Wilson-score CIs on every row | Epic 2 (Story 2.11) | ✓ Covered |
| FR36 | templated badge enum (10 variants) | Epic 2 (Story 2.4 enum + Story 2.18 server/thermal) → Epic 3 (Story 3.6 model-behavior badges) → Epic 7 (Story 7.4 audit) | ✓ Covered |
| FR37 | depth-tier tag per cell | Epic 2 (Story 2.11) | ✓ Covered |
| FR38 | structural `low-confidence-CI` on Quick rows | Epic 2 (Story 2.11) | ✓ Covered |
| FR39 | report path defaults + `--report-path` override + timestamped historical | Epic 1 (Story 1.13 default path) → Epic 3 (Story 3.5) | ✓ Covered |
| FR40 | `lcrc show` plain-text mirror | Epic 4 (Story 4.1) | ✓ Covered |
| FR41 | show filters `--model`, `--depth`, `--limit` | Epic 4 (Story 4.2) | ✓ Covered |
| FR42 | `--all` for uninstalled / outdated builds | Epic 4 (Story 4.3) | ✓ Covered |
| FR43 | `--format json` with `schema_version` | Epic 4 (Story 4.4 show), Epic 5 (Story 5.4 verify) | ✓ Covered |
| FR44 | non-interactive | Epic 1 (no prompts in any story; CLI is clap-derive only) | ✓ Covered |
| FR45 | full exit-code enum + trigger paths | Epic 1 (Story 1.3 full enum + paths 0/3/11) → Epic 2 (paths 1, 2) → Epic 4 (path 4) → Epic 5 (path 5) → Epic 6 (paths 10, 12) | ✓ Covered |
| FR46 | stdout/stderr discipline | Epic 1 (Story 1.3 `src/output.rs` discipline) | ✓ Covered |
| FR47 | streaming per-cell + ETA on stderr | Epic 2 (Story 2.13) | ✓ Covered |
| FR48 | `--quiet`/`-q` suppresses streaming | Epic 2 (Story 2.13) | ✓ Covered |
| FR49 | optional TOML config with documented defaults | Epic 6 (Story 6.1) | ✓ Covered |
| FR50 | layered precedence CLI > env > TOML > defaults | Epic 6 (Story 6.2) | ✓ Covered |
| FR51 | config validation → exit 10 | Epic 6 (Story 6.3) | ✓ Covered |
| FR52 | `scan.lock` single-writer + exit 12 | Epic 6 (Story 6.4) | ✓ Covered |
| FR53 | `lcrc show`/`verify` lock-free + concurrent with scan | Epic 4 (Story 4.5) | ✓ Covered |
| FR54 | stable JSON schemas with `schema_version` | Epic 4 (Story 4.4), Epic 5 (Story 5.4) | ✓ Covered |

### Missing Requirements

None. All 56 FRs (FR1–FR54 plus FR17a, FR17b sub-items) trace to at least one epic, and most trace to a specific story with explicit ACs.

### Coverage Statistics

- **Total PRD FRs:** 56 (FR1–FR54 with FR17a, FR17b)
- **FRs covered in epics:** 56
- **Coverage percentage:** **100%**
- **Epic distribution:** Epic 1 (16 FRs partial/full), Epic 2 (22 FRs partial/full), Epic 3 (5 FRs), Epic 4 (6 FRs), Epic 5 (4 FRs), Epic 6 (8 FRs), Epic 7 (2 FRs). Many FRs span multiple epics by design (tracer-bullet vertical slices).

### Coverage Quality Notes

- The epics document carries its own FR Coverage Map at lines 240–298 — a **strong signal of intentional traceability discipline**. The map and the per-epic "FRs covered" lines are consistent.
- **FR36 (badge enum) coverage map says "Epic 2 → 7"** but Story 3.6 in Epic 3 also contributes (model-behavior badges: `ctx-limited`, `oom-at-n`, `repetition-loop`, `tool-call-format-failure`). Per-epic header for Epic 3 lists FR36 implicitly via Story 3.6. Minor doc-consistency nit; not a coverage gap.
- **FR44 (non-interactive)** isn't explicitly enumerated in any epic's "FRs covered" line but is a pervasive design constraint enforced via `clap-derive` everywhere; acceptable structural coverage.
- FR17a and FR17b are correctly preserved as distinct sub-items in both PRD and epics — the early concern that they might get lost is unfounded.

## UX Alignment Assessment

### UX Document Status

**Not Found — intentionally skipped.**

### Rationale

- PRD §"Project-Type Overview" (line 295) explicitly states: "The conventional `cli_tool` PRD sections `visual_design`, `ux_principles`, and `touch_interactions` are explicitly **not applicable** to lcrc and are skipped."
- Epics document overview (line 16) repeats: "(UX Design document intentionally absent: lcrc is a CLI tool with no UI surface.)"
- Epics §"UX Design Requirements" (line 234) also marks: "_Not applicable — lcrc is a CLI tool with no UI surface._"
- User confirmed intentional skip in step 1.

### What stands in for UX in a CLI-only product

The PRD and architecture jointly specify the CLI's "user-facing surface" through:
- **CLI structure (PRD §"Command Structure"):** subcommands, flags, exit codes, output formats — all enumerated.
- **HTML report (PRD §"Output Formats" + FR32–FR39):** canonical screenshot-friendly header, Wilson CIs, templated badges, depth-tier tags. The HTML is a downstream artifact, not an interactive UI; user reloads the file manually (no SPA, no WebSocket, no server).
- **Streaming CLI feedback (FR47, NFR-O1, NFR-P8):** per-cell completion lines + estimated-remaining clock on stderr.
- **Five user journeys in the PRD** (lines 152–227) function as UX scenario coverage: First scan → Standard → switch (J1), Empty-machine first run (J2), Incremental scan after pulling a new model (J3), Drift caught by canary + verify (J4), Sandbox protects the eval (J5), and the acknowledged-but-deferred J6 (engineering-lead custom evals).

### Alignment Issues

None. The PRD's CLI/output specification is aligned with what the architecture supports (Tokio async runtime + clap-derive + askama HTML templating + indicatif streaming + bollard container API) and with what the epics implement (Stories 1.4, 1.13, 2.4, 2.11, 2.12, 2.13, 2.14, 4.1, 4.4, 6.7).

### Warnings

- **Empty-machine UX (FR5)** is the closest thing to a "first impression" surface. Epic 2 / Story 2.14 covers it with explicit ACs (one-paragraph explainer + hardcoded starter pack of 3–5 small models with copy-paste-ready download commands). No gap.
- **HTML report visual quality** is described in PRD/epics in terms of contents (header fields, badges, CI rendering) but no visual mockup or wireframe exists. Acceptable for v1 — askama templates are reviewable in code, and Story 4.1 binds the plain-text mirror identical-rank invariant which is the closest thing to a visual regression check. No action required, just noted: if v1.1+ adds chart-style visualizations, a UX pass would be warranted at that point.

## Epic Quality Review

### Per-Epic Compliance Summary

| Epic | Stories | User-value epic title | Independent / no fwd deps | Tracer-bullet vertical slice | AC Given/When/Then | Verdict |
|---|---|---|---|---|---|---|
| 1: Integration spine — one cell, one row, end-to-end | 14 (1.1–1.14) | ✓ "one cell end-to-end demoable" | ✓ stands alone | ✓ cache + sandbox + harness + server + report all wired in this epic | ✓ uniform | **Pass** |
| 2: Quick scan — real models, real leaderboard | 18 (2.1–2.18) | ✓ "coarse leaderboard, real models" | ✓ requires only Epic 1 | ✓ extends spine to multi-model with full sandbox | ✓ uniform | **Pass** |
| 3: Standard & Full depths — cache extension proves cache-as-product | 6 (3.1–3.6) | ✓ "Standard for switch, Full for tightness" | ✓ requires only Epic 2; Standard/Full ACs explicitly note they also work from empty cache | ✓ depth-tier extension visible | ✓ uniform | **Pass** |
| 4: `lcrc show` — read-only leaderboard view | 5 (4.1–4.5) | ✓ "terminal-side leaderboard" | ✓ requires only Epic 1's cache + Epic 2's CI computation | ✓ read-side surface end-to-end | ✓ uniform | **Pass** |
| 5: `lcrc verify` — drift detection | 5 (5.1–5.5) | ✓ "trust-audit surface" | ✓ reuses Epic 1's sandbox + harness path | ✓ verify orchestration + report end-to-end | ✓ uniform | **Pass** |
| 6: Config, concurrency & CLI polish — safe for cron / CI / Makefile | 7 (6.1–6.7) | ✓ "production-ready scriptable CLI" | ✓ layers TOML/env/lock onto existing CLI surface | ✓ config layer + lock + help polish all in this epic | ✓ uniform | **Pass** |
| 7: Distribution, sandbox audit & calibration — v1 ship gate | 8 (7.1–7.8) | ✓ "ship gate" | ✓ depends on Epic 2 + 3 *outputs* (correct direction); no future-epic reference | ✓ Homebrew + GHCR + adversarial battery + RC dry-run all in this epic | ✓ uniform | **Pass** |

**Total stories:** 63 across 7 epics.

### Tracer-Bullet Discipline (matches user's recorded preference)

- The epics document declares "tracer-bullet vertical slices" as its design principle in frontmatter (line 8) and overview (line 18). The user's memory `feedback_tracer_bullet_epics.md` records the same expectation.
- Cross-checked: every epic is a thin end-to-end vertical slice, not a horizontal layer. There is **no** "Epic 1: build the cache layer", "Epic 2: build the sandbox", etc. Epic 1 already touches all 6 architectural layers (cache, sandbox, harness, llama-server, HTML report, CLI) for one hardcoded measurement.
- Each epic ships at least one demoable surface: Epic 1 = one-row HTML on disk; Epic 2 = leaderboard from real installed models; Epic 3 = cache-extension between depth tiers; Epic 4 = terminal `show`; Epic 5 = drift report; Epic 6 = scriptable cron-safe CLI; Epic 7 = `brew install` works on a clean Mac.

### Greenfield Project Indicators

- ✓ Story 1.1: initial project scaffold with locked workspace lints
- ✓ Story 1.2: CI workflow gates (fmt + clippy + tests) at the start
- ✓ Story 1.4: dev tooling (clap-derive + tracing subscriber) early
- ✓ Story 7.2/7.3: release pipeline + container image publish

### Forward-Dependency Audit

Spot-checked all 63 stories' ACs and "depends on" language. No story references a *later* story's output as a precondition. Where a story references work elsewhere, it's always (a) an earlier story in the same or prior epic, or (b) an explicit forward-looking note that does not block the current story.

Examples of acceptable forward-looking notes (not violations):
- Story 2.4 declares the full 10-variant Badge enum but notes which attachments land later (Stories 2.18 and 3.6). The enum + rendering pipeline are complete in Story 2.4; subsequent attachments are additive.
- Story 1.3 declares the full 9-variant `ExitCode` enum from day one; trigger paths fill in across later epics. Same pattern — contract locked early, paths wire incrementally.
- Story 1.14 publishes a bootstrap container image manually; Story 7.3 takes over the publish via release workflow. Story 1.14 is self-sufficient.

### AC Quality Spot Check

Every story I read uses proper Given/When/Then BDD structure. ACs are concrete and testable: file paths, exit codes (with FR45 cross-references), latency budgets (with NFR-P cross-references), specific commands and expected outputs. Examples:
- Story 1.7 ACs cite the exact 7-dim PK and the metadata column list.
- Story 1.10 ACs include negative tests (`cat /etc/passwd`, `curl https://example.com`, `nc -zv host.docker.internal 22` on non-llama-server port) — strong adversarial framing in the integration test design.
- Story 2.11 ACs include a test-vector requirement for the Wilson formula.

### Database/Schema Creation Timing

The architecture chose a single SQLite cells table with the full PK + metadata columns from Story 1.7. This appears to violate the "create tables when needed" guideline in the literal sense (some columns like `power_watts` and Epic 3's badge fields land before they're populated), but is the intentional cache architecture: cell-level independence (FR25) requires the universal schema upfront. **Not a defect** — it's the architecture's correct read of the requirements.

### Findings

#### 🔴 Critical Violations

**None.**

#### 🟠 Major Issues

**None.**

#### 🟡 Minor Concerns

1. **Epic 1 (14 stories) and Epic 2 (18 stories) are large.** Justified by the role of Epic 1 (integration spine touching every layer) and Epic 2 (the v1 Quick experience plus 2 early-detection hardening stories + 1 badge story added in second-pass review). Worth being aware of for planning velocity but not a structural defect.
2. **Story 7.5 (calibration) depends on Story 2.17 + Epic 3 runs producing wall-clock data.** Backward dependency in the correct direction (Epic 7 reads from Epics 2/3), but Story 7.5 cannot be completed in isolation if Epic 2 hasn't shipped. This is a valid cross-epic data dependency, not a forward dependency.
3. **FR Coverage Map line for FR36 says "Epic 2 → 7"** but Story 3.6 (Epic 3) is a substantive contributor (attaches 4 of 10 badges). One-line doc inconsistency in the coverage map; Story 3.6's text is correct. Recommend updating the map line to "Epic 2 → 3 → 7" for consistency, but not blocking.
4. **Story 7.7 (SWE-Bench Pro license confirmation)** is a research-and-decision deliverable rather than a code story. The story documents both "vendorable" and "restricted" implementation paths cleanly, but Theop will need to make and record a real legal call before tagging v1.0.0. Architecture provides the fallback (TaskSource trait), so the design is unblocked either way.

## Summary and Recommendations

### Overall Readiness Status

**READY** — proceed to Phase 4 (sprint planning + implementation).

### Evidence Supporting "Ready"

- **FR coverage:** 56/56 = 100% (FR1–FR54 plus FR17a, FR17b sub-items, all traceable to specific epics/stories with explicit ACs).
- **NFR coverage:** All 39 NFRs (Performance, Reliability, Security, Compatibility, Observability, Integration) are reflected in story ACs, e.g., NFR-S1 → Stories 1.10 + 2.7; NFR-P1 → Story 2.17 budget sanity check + Story 7.5 calibration; NFR-R1 → Stories 1.8 + 2.15.
- **Acceptance criteria #1–#9** from PRD §"Measurable Outcomes" all map to binding tests in epics — most notably AC#9 (sandbox negative test) → Story 7.4 with Story 2.16 as early-detection smoke test.
- **Architecture-locked decisions** (AR-1 through AR-38) are all reflected in epic/story content; Cargo deps, exit-code enum discipline, single-source-of-truth modules (`output.rs`, `cache/key.rs`, `sandbox/container.rs`) all show up in Story 1.x ACs.
- **Design discipline** matches user's recorded preference: every epic is a tracer-bullet vertical slice through all integration layers.
- **Open questions** that the PRD deferred to architecture (pass@1 semantics, backend-build invalidation policy, harness/task version representation, SWE-Bench Pro lifecycle, perf-collection privilege, cache storage shape, resumability protocol) are all resolved in the architecture document and reflected in stories — no unresolved decisions cascading into implementation.
- **AC quality** is consistently high: Given/When/Then BDD format throughout, with concrete file paths, exit codes, and latency budgets cross-referenced to FR/NFR numbers.

### Critical Issues Requiring Immediate Action

**None.** No 🔴 critical, no 🟠 major findings.

### Minor Items to Track (not blocking)

1. **FR Coverage Map line for FR36** (epics.md ~line 279) reads "Epic 2 → 7"; should also include Epic 3 (Story 3.6 attaches 4 of the 10 badges). One-line doc fix in the table.
2. **Story 7.7** (SWE-Bench Pro license confirmation) needs a real legal call from Theop before tagging v1.0.0. Architecture provides the fallback path either way; the decision unblocks distribution but doesn't block earlier epics.
3. **Story 7.5** (final `*_task_timeout` calibration) reads data from Story 2.17 + Epic 3 runs. Sequencing-aware: don't try to complete Story 7.5 before Epic 2's sanity check has produced numbers on the M1 Pro 32GB rig.
4. **Epic 1 has 14 stories, Epic 2 has 18.** Both are large but justified (Epic 1 = integration spine touching all layers; Epic 2 = the v1 Quick experience plus hardening stories). Worth being aware of for sprint sizing — they may want to be planned as 2 sprints each rather than 1.

### Recommended Next Steps

1. **Run `bmad-sprint-planning`** in a fresh context window to produce the sprint-status file the story cycle (`bmad-create-story` → `bmad-dev-story` → `bmad-code-review`) iterates against. This is the gate that opens Phase 4.
2. **Optional but recommended:** Apply the FR36 coverage-map doc fix (Minor #1) before starting sprint planning so traceability stays clean.
3. **Optional:** Schedule a research step on the SWE-Bench Pro license question (Minor #2) so the answer is in hand before Epic 7 sprint planning, not blocking it on the day.
4. **Begin implementation with Epic 1 Story 1.1** (project scaffold). The integration spine establishes every layer's interlock; Stories 1.2–1.14 follow in order before Epic 2 starts.

### Final Note

This assessment identified 0 critical issues, 0 major issues, and 4 minor concerns across requirements coverage, traceability, and sprint-planning sequencing. The artifacts are unusually well-aligned: PRD → architecture → epics → stories carry consistent terminology, explicit FR/NFR cross-references, and a coherent tracer-bullet design discipline. Proceed to Phase 4.

---

**Assessor:** Claude (bmad-check-implementation-readiness)
**Date:** 2026-04-30
**Project:** lcrc
