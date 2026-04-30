---
stepsCompleted: ['step-01-init', 'step-02-discovery', 'step-02b-vision', 'step-02c-executive-summary', 'step-03-success', 'step-04-journeys', 'step-05-domain', 'step-06-innovation-skipped', 'step-07-project-type', 'step-08-scoping', 'step-09-functional', 'step-10-nonfunctional', 'step-11-polish', 'step-12-complete']
releaseMode: phased
inputDocuments:
  - _bmad-output/planning-artifacts/product-brief-lcrc.md
  - _bmad-output/planning-artifacts/product-brief-lcrc-distillate.md
  - _bmad-output/brainstorming/brainstorming-session-2026-04-29.md
documentCounts:
  briefs: 2
  research: 0
  brainstorming: 1
  projectDocs: 0
classification:
  projectType: cli_tool
  domain: scientific
  complexity: medium-high
  projectContext: greenfield
workflowType: 'prd'
---

# Product Requirements Document - lcrc

**Author:** Theop
**Date:** 2026-04-30

## Executive Summary

lcrc is an autonomous, personal benchmark database for local LLMs on macOS Apple Silicon. It scans the GGUF models a user has installed, runs a measurement battery against each on the user's actual hardware, and emits a single self-contained HTML leaderboard answering: *for agentic coding on this machine, run this model with these settings.* The cache of measurements — keyed on `(machine_fingerprint, model_sha, backend_build, params)` — **is the product**; autonomous orchestration is the mechanism that fills it. Re-running after a new model install measures only the new cell; re-running after a llama.cpp upgrade re-measures only what the backend change affects. Incremental scans complete in under two hours.

**Target user (v1):** the local-LLM tinkerer running `llama.cpp` on their own Mac with 4–20 GGUFs in `~/.cache/llama.cpp/`. Concretely, the author. v1 success is measured by the author trusting the leaderboard enough to switch their default coding model based on what it says. External adoption is not a v1 gate; lcrc is built to be open-sourced, not marketed.

**The problem:** picking which model to run on which hardware for which task is currently a folklore problem. Reddit threads, hand-rolled blog benchmarks, and "it depends" answers produce no durable result. Public leaderboards report numbers from someone else's hardware running someone else's harness on a SWE-Bench Verified task set now considered training-data tainted (OpenAI stopped publishing scores). Existing tools each cover one slice — Inspect AI runs evals but not perf or sweeps; Bench360 measures perf but only on server-class NVIDIA; mini-swe-agent is a harness, not a framework — and none answer "what should *this* machine run?" The cost is hours-per-week of guesswork that vanishes the next time something changes.

### What Makes This Special

The novel contribution is the **combination**, not any single component: cache-as-first-class-artifact + delta-aware autonomous orchestration + opinionated batteries-included Mac UX. Once the cache is the product, re-measurement becomes incremental for free, which is what makes the 15-minute Quick iteration target physically possible. Concretely: one command (`lcrc scan`) produces output, no configuration screen, a single self-contained HTML file as the artifact, one harness (mini_bash wrapping mini-swe-agent), one task source (curated SWE-Bench Pro subset), one backend (llama.cpp / `llama-server`) in v1.

Every leaderboard row carries (a) a Wilson-score confidence interval on the pass-rate, (b) the cache age and `backend_build` it was measured under, and (c) templated failure-mode badges (`ctx-limited`, `OOM-at-N`, `repetition-loop`, `tool-call-format-failure`, `thermal-throttled`, `low-confidence-CI`) explaining each rank. A per-scan canary task with a known-good baseline runs every scan and is rendered prominently in the report header — so infrastructure drift (harness, backend, OS) is detected separately from model behavior change. The HTML is screenshot-friendly by default: canonical machine-fingerprint header, scan date, lcrc version, and `backend_build` rendered prominently so a screenshot pasted into a thread is self-attesting.

**Design philosophy: two faces, one product.** Beginner default = preset (no configuration, no flags, opinionated answers); expert escape hatches = optional TOML config, depth/model/format flags, JSON output. Not a UX fork between modes — one product where the defaults are good enough that the v1 user never has to touch the escape hatches, but the escape hatches exist for the cases where they do. This shape threads through every CLI decision (`--depth quick` as the default; `lcrc scan` with no flags is the canonical first invocation; everything else is opt-in).

Differentiation is execution and shape: a Mac-first, opinionated, batteries-included tool for the tinkerer audience. That posture is orthogonal to what eval-framework-first products are designed to be, regardless of feature overlap over time.

## Project Classification

- **Project Type:** CLI tool. One command, no configuration screen. Aspirational entry point: `brew install lcrc && lcrc scan`. Subcommand catalog locked in the Functional Requirements step (working set: `lcrc scan` with `--depth quick|standard|full`, `lcrc show`, `lcrc verify`).
- **Domain:** Scientific / measurement framework for ML/AI artifacts. Concerns: reproducibility (cache key durability across OS patches and backend upgrades), validation methodology (Wilson-score CIs on small task counts, per-scan canary), accuracy metrics (agentic pass-rate semantics, currently underspecified — open question), computational requirements (RAM-fit gate, native macOS perf collection).
- **Complexity:** Medium-high. The CLI surface is small, but the measurement-methodology surface is unusually thick for a personal tool: machine-fingerprint stability, backend-build cache invalidation policy, harness-contamination risk (NeuralNoise's documented ~14% grader-peeking on OpenCode), SWE-Bench Pro licensing and redistribution, perf-collection privilege model on macOS. No regulatory or safety burden.
- **Project Context:** Greenfield. `v1.md` and `docs/legacy/v0.md` are superseded planning artifacts, not a brownfield system. v1 architecture must not paint into a corner that blocks the v1.1+ secondary persona (engineering lead with custom evals).

## Success Criteria

### User Success

v1 is successful when **the author runs `lcrc scan` on their own Mac and the leaderboard is trustworthy enough to drive an actual default-model switch.** Concretely:

- **The "aha" moment:** First time the author runs `lcrc scan` on a freshly installed model, sees the new row appear in the HTML report alongside the existing models with a Wilson CI tight enough to rank confidently, and either keeps or replaces their current default based on what the report says — not in spite of it.
- **Trust signal — the explicit switch:** At least one default-model switch (or one *deliberate non-switch*, where the author was tempted by a new model and lcrc said "no, your current default is still better") happens because of what lcrc reported. This is the falsifiable v1 outcome.
- **Workflow fit:** After scan completion, opening the HTML report is a one-double-click action; the report is readable without explanation; the author does not need to consult the CLI or re-run anything to interpret a row.
- **Honesty earns trust over time:** When a `backend_build` upgrade lands and `lcrc verify` flags drift on a sampled cell, the author re-runs and the new measurement reconciles with intuition (or the badge explains why it doesn't). The cache + canary mechanism makes drift legible, not invisible.

### Release Success

lcrc is built to be open-sourced, not marketed. There are no adoption, growth, or revenue targets. Release-readiness criteria stand in for the conventional "business success" slot:

- **Honest README:** scope and limitations stated up front. The "autonomous" framing is qualified explicitly — v1 autonomy is *what* gets measured (delta-aware), not *when* (still CLI-triggered). No competitive-window urgency framing in user-facing copy.
- **Reproducible install:** `brew install lcrc && lcrc scan` works end-to-end on a clean macOS Apple Silicon system that has at least one supported model installed. Empty-machine UX directs users to the hardcoded starter pack with exact download commands.
- **Self-attesting reports:** A user can paste an HTML screenshot into a thread and the canonical machine-fingerprint header, scan date, lcrc version, and `backend_build` are visible without scrolling — sufficient for someone else to know what hardware and toolchain produced the numbers.
- **External adoption is explicitly not a v1 gate.** GitHub stars, blog citations, and community pickup are information for v1.1+ planning if they happen, not goals.

### Technical Success

- **Cache key durability:** `(machine_fingerprint, model_sha, backend_build, params)` survives macOS patch-level upgrades without invalidation. `machine_fingerprint` = chip generation + RAM size + GPU core count. `model_sha` is **format-agnostic** (works for GGUF in v1, MLX or other formats in future) — no model-format constraint baked into the data model.
- **Measurement isolation (default-deny container-per-task sandbox):** Every per-task measurement runs inside an ephemeral container with **no host filesystem mounted** (only the per-task workspace bind-mounted read-write), **no network** (only a single allowed localhost route to the host's `llama-server` port), and **no host environment variables** (only a documented per-task allowlist of safe variables). Default-deny by structural construction — every host file path, network destination, and env var not explicitly admitted is non-existent from inside the container. The threat model is the measurement subject itself: an agentic LLM with bash tool access. No enumerated allowlist would survive its exploration; isolation must be structural. Container runtime (Colima, OrbStack, Docker Desktop, Lima, etc.) is an architecture-phase choice. The container image is pinned per lcrc release. v1 refuses to run if no supported container runtime is present at scan pre-flight time (no fallback; exit code `11`). This requirement is **not** optional.
- **Three-tier scan budget (breadth-first, cache-extending):** `lcrc scan` accepts `--depth quick|standard|full` (default = `quick`). All three tiers extend the same cache; tiers compose, not replace. Targets on the v1 reference rig (M1 Pro 32GB) for a typical 5-model installed set:
  - **Quick** (default): canary + 1 SWE-Bench Pro task per model, **target ~15 min, hard ceiling ~25 min.** Coarse leaderboard with wide Wilson CIs; every row carries `low-confidence-CI` by default. Designed for screening, not for default-switch decisions.
  - **Standard** (`--depth standard`): extends each model's cell to 3–5 tasks total. Target ~1.5–3 h. Tight enough for default-switch decisions.
  - **Full** (`--depth full`): full curated SWE-Bench Pro subset and additional quant/ctx variants. Overnight job.
  The 15-min Quick target is design-binding subject to architecture-phase validation: if measured wall-clock on M1 Pro 32GB exceeds 25 min for the typical 5-model case, the per-task wall-clock cap or the task selection from Pro is tightened before the Quick budget is loosened. Quick must remain Quick.
- **Streaming CLI feedback:** during a scan, the CLI updates live as each cell completes — per-model progress lines, an honest estimated-remaining clock, and a one-line summary when each cell finishes. The HTML report is regenerated to disk after every cell; the user refreshes the browser tab themselves. No server, no WebSocket, no SPA — just file writes the user reloads.
- **Discriminative task ordering:** the curated SWE-Bench Pro subset ships with a static "most-informative-first" ordering, picked once based on offline calibration against known model classes. Quick's single task per model is the first task in this ordering — chosen so a single cell carries maximum rank-signal. Adaptive re-calibration against the user's local cache is deferred to v1.1+.
- **Drift detection:** `lcrc verify` re-runs a configurable sample of cached cells and reports drift in machine-readable form. Default behavior is **warn**, not auto-invalidate; user opts in to re-measurement (resolves brief open Q1).
- **Canary discipline:** The per-scan canary task (single fixed task with known-good baseline, ~1 minute) runs every `scan` and the report header renders one of three states explicitly: `canary-pass` (trust this run), `canary-fail` (treat with suspicion, investigate before acting), `canary-skipped` (with reason). A failed canary does not block writing the report — but it must be impossible to miss when reading it.
- **Statistical honesty:** Every leaderboard row displays a Wilson-score confidence interval on its pass-rate. Wilson over normal-approximation is a deliberate choice given small Tier-1 task counts.
- **Failure-mode legibility:** Every leaderboard row can carry zero or more templated badges from a fixed enum: `ctx-limited`, `OOM-at-N`, `repetition-loop`, `tool-call-format-failure`, `thermal-throttled`, `low-confidence-CI`. Badges are templated, not LLM-generated prose.
- **Per-task wall-clock cap:** No single SWE-Bench Pro task can wedge a scan past its tier's budget. Capped tasks record a timeout-equivalent badge rather than blocking. Cap value is per-tier (tighter on Quick than on Full) and decided in architecture phase.

### Measurable Outcomes (v1 acceptance check)

A v1 build is "shippable" when, on the author's reference Mac (M1 Pro 32GB, macOS Apple Silicon), all of the following hold simultaneously:

1. **Quick budget:** `lcrc scan` (default `--depth quick`) on a 5-model installed set completes in **≤25 minutes wall-clock**, with a target of ~15 min. (If empirical measurement on M1 Pro 32GB exceeds 25 min, the per-task wall-clock cap or the task selection is tightened before the Quick budget is relaxed.)
2. **Standard extension:** `lcrc scan --depth standard` against the cache produced by step 1 extends each model's cell to 3–5 tasks within ~1.5–3 h, fills only missing cells (no re-measurement of cells already present), and produces a leaderboard whose rank order is stable enough that consecutive Standard runs on unchanged inputs agree on the top-3.
3. **Full extension:** `lcrc scan --depth full` completes overnight on the same cache and reports tightened CIs vs. Standard with no rank inversions on the top-3 unless explained by a templated badge.
4. **Streaming CLI feedback:** during scan execution at any depth, the CLI displays live per-cell completion and an estimated-remaining clock; the HTML report on disk is regenerated after every cell.
5. **Report contents:** the HTML report opens in a browser, renders the canonical header (machine fingerprint + scan date + lcrc version + `backend_build`) without scrolling, and every leaderboard row visually carries (a) a Wilson CI, (b) cache age + `backend_build`, (c) any templated badges seen during the run, (d) which depth tier produced the cell.
6. **Canary header:** the per-scan canary row in the report header shows `canary-pass` for a healthy run; a deliberately broken backend triggers `canary-fail` and the failure is impossible to miss when reading the report.
7. **Drift detection:** `lcrc verify --sample 3` re-runs three cached cells and the resulting drift report is interpretable (numerical, not narrative).
8. **CLI mirror:** `lcrc show` returns a plain-text leaderboard view in the terminal that ranks identically to the HTML report.
9. **Sandbox negative test (default-deny verification):** a deliberately constructed adversarial task whose agent attempts a battery of out-of-envelope operations — arbitrary host file reads (e.g., `cat /etc/passwd`, `find ~/ -name '*.json'`, `cat /Users/*/Documents/*`), arbitrary outbound network (e.g., `curl https://example.com`, DNS resolution of an arbitrary domain), enumeration of sibling task workspaces by guessed path, and reads of credential-bearing environment variables that should not exist (`env | grep -iE 'token|key|secret|password'`) — has **every** attempt fail at the container boundary. Each blocked attempt surfaces as a sandbox-violation badge or report-surfaced event; the run exits with code `2`. No "silent pass" path through the envelope.

## Product Scope

### MVP — Minimum Viable Product

Everything in the v1 In-Scope list from the product brief, restated for binding (with refinements from PRD discovery):

- **Platform:** macOS Apple Silicon only.
- **Backend (v1):** llama.cpp / `llama-server` only. Single binary; pinned version inside cache key. **MLX backend evaluated during architecture phase for low-effort inclusion in v1** — if the model-execution interface factors cleanly so adding MLX is a focused additive change (the harness-side OpenAI-compatible API surface is largely shared via `mlx_lm.server`), do it in v1; otherwise defer to v1.1. Model format follows from backend (GGUF in llama.cpp, MLX-format in `mlx_lm`); no format constraint is baked at the data-model level.
- **Model discovery:** read installed models from the cache directories of supported backends (`~/.cache/llama.cpp/...` for v1; MLX/LM Studio paths if MLX backend lands in v1). Filter by RAM × context-length budget before measuring. lcrc never downloads or curates.
- **Harness:** one — `mini_bash`, wrapping `mini-swe-agent` as a pinned subprocess. `mini-swe-agent` and SWE-Bench Pro are vendored or version-pinned so harness updates don't silently invalidate measurements.
- **Task source:** curated SWE-Bench Pro subset, shipped with a static "most-informative-first" task ordering (discriminative ordering, picked once via offline calibration).
- **Three-tier breadth-first scan** (`lcrc scan --depth quick|standard|full`, default `quick`):
  - **Quick** (default): canary + 1 SWE-Bench Pro task per model (the first task in the discriminative ordering). Target ~15 min on M1 Pro 32GB / 5-model collection; ceiling 25 min.
  - **Standard** (`--depth standard`): extends each model's cell to 3–5 tasks total. Target ~1.5–3 h.
  - **Full** (`--depth full`): full curated SWE-Bench Pro subset and additional quant/ctx variants. Overnight job.
  All three depths extend the same cache; consecutive depths only fill missing cells (no re-measurement). Default `lcrc scan` runs Quick and prints (non-interactively) the exact commands to extend further; no auto-prompt.
- **Per-scan canary task:** single task with known-good baseline, ~1 minute, runs at the start of every `lcrc scan` invocation regardless of depth. Result rendered prominently in report header.
- **Streaming CLI + on-disk report regeneration:** during scan, CLI updates live per-cell with an estimated-remaining clock; HTML report on disk is regenerated after every cell completes; user refreshes the browser tab themselves. No server, no SPA, no WebSocket.
- **Measurement isolation (default-deny container-per-task sandbox):** every per-task measurement runs inside an ephemeral container with no host filesystem mounted (only the per-task workspace bind-mounted), no network (only a constrained localhost route to `llama-server`), and a per-task env-var allowlist. Default-deny by structural construction. Container runtime (Colima, OrbStack, Docker Desktop, Lima, etc.) chosen in architecture phase; v1 refuses to run if no supported runtime is present (no fallback). Per-task container image is pinned per lcrc release.
- **Perf metrics (macOS-native):** tok/s, ttft, peak RSS, power, thermal. Privilege model TBD in architecture phase (open Q6 from brief).
- **Cache key:** `(machine_fingerprint, model_sha, backend_build, params)`. Lookup before measure. Cells are independent and depth-extending; storage shape and pruning policy decided in architecture phase (open Q2, Q8).
- **Commands:** working set is `lcrc scan` (with `--depth`), `lcrc show`, `lcrc verify`. `lcrc deepen` is **not** a separate command — its function is folded into `lcrc scan --depth full`. Final command surface and per-command intent locked in the Functional Requirements step.
- **Output:** single self-contained static HTML file + CLI ranking views. Wilson CIs and templated badges on every leaderboard row; rows are tagged with the depth tier that produced them.
- **Per-task wall-clock cap:** enforced per-tier (tighter on Quick than on Full); no task can wedge a scan past its tier's budget.
- **Run resumability:** Ctrl-C and crash-recovery resumability for `lcrc scan` at any depth — the cache keeps every cell that completed before the interrupt; next invocation resumes from missing cells (open Q9 from brief — flagged for binding requirement in PRD requirements step).

### Growth Features (Post-MVP, v1.1+)

- Linux NVIDIA tier-1 platform support.
- Ollama backend support via llama-swap.
- MLX backend (if not landed in v1).
- Background daemon + native macOS notifications — closes the "*when* is autonomous" gap so re-measurement triggers automatically when the catalog changes.
- Custom-eval extension surface — unlocks the deferred secondary persona (engineering lead codifying their team's quality bar). Architecture must not paint into a corner that blocks this.
- **Adaptive depth (Wilson-CI-driven early stop):** after each pass, look at pairwise CI overlaps; stop measuring more tasks against models whose ranks are already settled, concentrate further runs on contested rank pairs. Maximum information per minute. Architecture must not paint into a corner that blocks this — the cell-level cache structure already supports it.
- **Adaptive task-ordering re-calibration:** discriminative ordering re-tuned against the user's local cache as it grows (v1 ships static ordering only).
- "Indistinguishable cluster" leaderboard marking (above-and-beyond the Wilson CIs already in v1).
- Multi-run pass-rate / reliability-under-repeat metric, if eval-variance turns out to bite trust in practice.
- `lcrc gc` cache-pruning command for cells from uninstalled models / old backend builds.
- `lcrc doctor` pre-flight checks (low disk, missing helpers, llama-server health).
- Default `lcrc` (no args) wizard mode ("you have 2 new GGUFs since last run; measure them?").

### Vision (Future, v2+)

- **v2:** Second harness (OpenCode, Aider, Claude Code) — harness-as-axis comparison returns. vLLM and LM Studio backends. Inspect AI interop. Pareto-front view for users who want to tune score weighting. User-tunable scoring sliders.
- **v3:** Community-shared benchmark dataset — opt-in upload of `(machine_fingerprint, model, score, perf)` rows so users without the hardware can preview "what does this model do on a setup like mine?" before downloading. The personal database becomes a federated reference.

## User Journeys

The v1 product has one user persona: **the local-LLM tinkerer running models on their own Mac.** Concretely, the author. There is no admin, support, moderator, or API-consumer role in v1 — lcrc is a personal CLI that runs locally and writes to a local cache. The journeys below cover scenario diversity for this one persona, plus a final acknowledgement journey for the deferred secondary persona (engineering lead with custom evals — v1.1+).

### Journey 1 — First scan, Quick → Standard → switch

**Persona:** Theop. M1 Pro 32GB. Six GGUFs in `~/.cache/llama.cpp/`. Default daily-driver coding model is "I think it's a Qwen-Coder quant — can't remember which one." Just saw another Reddit thread arguing Q4 vs. Q5 and felt the familiar "I should probably check" itch.

**Opening:** Reads about lcrc, runs `brew install lcrc && lcrc scan` after dinner. CLI prints a one-screen pre-flight: "Scanning 6 installed models. Default depth = quick (canary + 1 task per model). Target ~15 min on this rig. Cells will be added to your cache; extend with `--depth standard` or `--depth full` later." Theop hits enter and watches.

**Rising action — Quick (~18 min):** lcrc runs the canary first (`canary-pass`), then walks each fitted model through 1 SWE-Bench Pro task (the most-discriminative-first task from the static ordering) inside the per-task sandbox. CLI streams updates per cell: model name, task name, completion time, pass/fail. Halfway through, Theop opens the HTML report on disk in a browser tab — sees three rows already filled, refreshes a few minutes later and watches more rows appear. Each row carries `low-confidence-CI` because it's a single-task signal.

**Climax:** Quick complete in 18 min. CLI prints: "Quick done. To tighten CIs for switch decisions: `lcrc scan --depth standard` (~2h)." Coarse leaderboard already shows Theop's current default is in the top 3 but with overlapping CIs. Worth tightening before switching.

**Resolution:** Theop runs `lcrc scan --depth standard` after dinner; goes to bed. Wakes up to the cache extended, the report regenerated, CIs tight enough to rank decisively. Top-3 holds but the rank order shuffled; one row carries a `repetition-loop` badge that explains why the loaded GLM variant ranked lower than expected. Theop switches default model based on Standard, not Quick. The trust-signal v1 success criterion is satisfied.

**Capability surface:** install path; `lcrc scan` default-quick UX; canary-first execution; model discovery from llama.cpp cache; RAM × context-length fit gate; per-task sandbox; static discriminative task ordering; streaming CLI updates with per-cell ETA; on-disk HTML regeneration after each cell; cache extension via `--depth standard` (no re-measurement of Quick cells); HTML report with canonical header, Wilson CIs, templated badges, canary status, depth-tier-per-row tagging; format-agnostic `model_sha` keying.

### Journey 2 — Empty-machine first run

**Persona:** A friend Theop shared lcrc with after his switch. Brand-new Mac Studio; no llama.cpp models installed yet.

**Opening:** Runs `lcrc scan` curious to try.

**Tension:** Scan immediately reports zero models found. Could feel like an error or a broken install.

**Resolution:** lcrc prints the empty-machine UX: a one-paragraph explainer ("lcrc measures models you already have — it doesn't download or curate") followed by the **hardcoded starter pack** of 3–5 very small models with exact `huggingface-cli download` / `llama.cpp` pull commands. Friend pulls the smallest, re-runs scan, sees a single-row leaderboard (and the canary-pass header). Single row isn't useful for ranking, but the experience didn't feel like a failure.

**Capability surface:** empty-machine UX (text explainer + fixed list, **not** a recommendation engine); copy-paste-ready download commands; graceful single-row report.

### Journey 3 — Incremental scan after pulling a new model

**Persona:** Theop, three weeks after first scan. Has been running with the leaderboard's recommendation. Sees a new MoE release land and pulls it.

**Opening:** Wants to know if the new model beats the current default. In the old folklore-driven world, this would mean hand-running a few prompts and forming an opinion.

**Action:** `lcrc scan`. No flags. lcrc detects exactly one new `model_sha` in the cache directory, skips every existing-model cell (cache hit on `(machine_fp, model_sha, backend_build, params)`), runs Quick against just the new model. Wall-clock: ~5 minutes (canary + one task for one model).

**Climax:** HTML report shows the new row alongside the existing leaderboard. Wilson CI is wide because it's one task — but it's clearly competitive on the coarse signal. Theop runs `lcrc scan --depth full --model qwen-3.6-35b-a3b` and lets it run overnight to tighten the CI before committing to a switch.

**Resolution:** Next morning, the full CI is tight and the rank is decisive. Theop either switches or doesn't, with a defensible reason either way.

**Capability surface:** cache-aware delta scan (the cache-as-product moment); per-model selectivity (`--model` flag scope); depth-tier-aware extension (Full extends without redoing Quick); CI tightening as a function of task count; the per-tier budget commitments (Quick ~5 min for N=1 new model; Full overnight for the same).

### Journey 4 — Drift caught by canary + verify

**Persona:** Theop, after `brew upgrade llama.cpp` lands a new build between scans.

**Opening:** Runs `lcrc scan` for an unrelated new model install.

**Tension:** Report header banner reads `canary-fail`. Same canary task that has passed in 6 prior scans now fails. The report still gets written — but the failure is visually impossible to miss.

**Action:** Theop runs `lcrc verify --sample 5` to spot-check whether cached cells drifted under the new backend. The output is a numerical drift report (not a narrative one); one cached cell has drifted 12 percentage points on the same task, the other four are within noise.

**Resolution:** Theop sees `backend_build` changed, decides the drift is real, and re-runs `lcrc scan --depth standard` against the affected models (cache lookup hits the old `backend_build`; the new `backend_build` invalidates those cells and re-measures). The drift was warned, not silently absorbed; trust holds.

**Capability surface:** per-scan canary as infrastructure-vs-model drift discriminator; `lcrc verify` with sampling; machine-readable drift output; cache key sensitivity to `backend_build`; default-warn-not-invalidate policy.

### Journey 5 — Sandbox protects the eval (the trust audit)

**Persona:** A reviewer reading the open-source repo, trying to understand whether the trust story holds.

**Opening:** Reads the README's claim that the leaderboard is trustworthy because every measurement runs inside a default-deny container. Reviewer is skeptical: enumerated allowlists are brittle; what actually stops the model from arbitrary host reads, network calls, or sibling-cell snooping?

**Action:** Runs the v1 acceptance test #9 (sandbox negative test). It executes a deliberately adversarial task whose agent attempts a battery: `cat /etc/passwd`, `find ~/ -name '*.json'`, `cat ~/.aws/credentials`, `curl https://example.com`, `nslookup google.com`, `cat /tmp/lcrc-task-*/output.txt` (sibling-workspace fishing), and `env | grep -iE 'token|key|secret|password'`.

**Climax:** Every attempt fails — not because lcrc enumerated the paths, but because none of them exist from inside the container. The report row carries sandbox-violation badges for each attempt; the run exits with code `2`. The reviewer looks at the container spec and verifies isolation directly: no host filesystem mounted, no network except the constrained localhost route to `llama-server`, no host env vars beyond a small documented allowlist.

**Resolution:** Reviewer concludes the trust story is verifiable by inspection of the container spec, not asserted by enumeration of blocked paths. Continues evaluating lcrc.

**Capability surface:** default-deny container-per-task envelope; pre-flight refusal on missing container runtime (exit 11); sandbox-violation badge / report event surface; pinned container image (per-cell metadata records image identifier); adversarial-task verification as part of the v1 acceptance check; trust verifiability by an outside reader inspecting the container spec.

### Journey 6 (acknowledged, NOT v1) — Engineering lead bringing custom evals

**Persona:** v1.1+ secondary user. An engineering lead at a small team that wants to codify their team's quality bar with custom SWE-style tasks reflecting their actual codebase, then re-run as new models drop.

**Why this journey is here:** v1 does not implement a custom-eval extension surface. But the v1 architecture must not paint into a corner that blocks this user in v1.1+. Concretely, the eval-task interface (currently bound to a curated SWE-Bench Pro subset) needs to be cleanly factorable so a v1.1 release can swap in or compose other task sources without rearchitecting the cache, the harness, or the sandbox envelope.

**Capability surface (constraint on v1 architecture, not a v1 feature):** task source is an isolated module with a small interface (load tasks, evaluate per-task pass/fail); cache key already accommodates additional task-source identifiers; sandbox envelope is task-source-agnostic.

### Journey Requirements Summary

The five v1 journeys collectively reveal the following capability areas, mapped to the binding scope from the brief and the refinements added in PRD discovery:

- **Install & first-run UX (J1, J2):** Homebrew formula, opinionated `lcrc scan` with no required flags, default depth = Quick (~15 min target), honest pre-flight, starter-pack-with-download-commands for the empty machine.
- **Measurement loop (J1, J3):** model discovery from llama.cpp cache (extensible to MLX/LM Studio), RAM × ctx fit gate, default-deny container-per-task sandbox (pre-flight refusal if no container runtime), mini_bash-wrapping-mini-swe-agent harness, three-tier breadth-first scan (Quick / Standard / Full) over the curated SWE-Bench Pro subset with static discriminative task ordering.
- **Cache-as-product (J3, J4):** delta-aware scan (cache key = `(machine_fp, model_sha, backend_build, params)`), per-model selectivity (`--model` flag), format-agnostic `model_sha`, depth-extending cells (Standard and Full extend without re-measuring Quick), `lcrc verify` to detect drift, default-warn-not-invalidate.
- **Streaming feedback (J1):** live per-cell CLI updates with estimated-remaining clock; HTML report on disk regenerated after every cell; user refreshes browser tab manually.
- **Reporting & honesty (J1, J3, J4, J5):** single self-contained HTML report, screenshot-friendly canonical header, plain-text `lcrc show` mirroring HTML rank, Wilson CIs on every row (every Quick row carries `low-confidence-CI` by default), depth-tier-per-row tagging, templated badges (fixed enum, no LLM-generated prose), per-scan canary in header with three explicit states, sandbox-violation surface.
- **Trust auditability (J5):** adversarial sandbox negative test as part of the v1 acceptance check, default-deny container-per-task envelope as a v1 binding requirement, container spec inspectable by reviewers (runtime architecture-decided; image pinned per release).
- **v1.1+ extensibility (J6):** task-source factoring, eval-task interface modularity, adaptive-depth (Wilson-CI-driven early stop) cell-level cache compatibility — constraints on v1 architecture, not v1 features.

## Domain-Specific Requirements

lcrc operates in the **scientific / measurement-framework** domain. There is **no regulatory or compliance burden** — no HIPAA, no PCI-DSS, no FDA, no GDPR data-subject rights to honor (lcrc runs locally and stores no third-party PII). The domain-specific weight sits entirely in measurement methodology, reproducibility, and eval-data integrity. A benchmark that gets these wrong is worse than no benchmark — it produces confidently wrong rankings that the user trusts.

### Reproducibility

A measurement is reproducible when re-running it on the same `(machine_fingerprint, model_sha, backend_build, params)` produces a result within statistical noise of the original. The v1 binding requirements:

- **Cache key completeness:** every input that can change the result is in the key. `machine_fingerprint` = chip generation + RAM + GPU core count (durable across OS patch upgrades). `model_sha` = content hash, format-agnostic. `backend_build` = pinned llama.cpp commit/version. `params` = ctx length, sampler temperature, threads, `n_gpu_layers`. Anything not in the key is pinned to a documented default.
- **Cell-level independence:** each `(model, task)` cell is independently keyed and stored. Depth tiers (Quick / Standard / Full) extend the cache by adding more `(model, task)` cells; they never replace existing cells. This is what makes `lcrc scan --depth standard` after `lcrc scan --depth quick` cheap, what makes Ctrl-C resumability automatic, and what leaves the door open for adaptive depth (v1.1+) without rearchitecting the cache.
- **Harness and task pinning:** `mini-swe-agent` and the curated SWE-Bench Pro subset are vendored or version-pinned. The pin is part of the lcrc release version, not the cache key — but the cache key's `backend_build` slot has an effective sibling for "harness/task version" that the architecture phase must decide how to represent (collapse into `backend_build`, add a fifth key dimension, or scope by lcrc release version).
- **Drift detection over time:** `lcrc verify` re-runs sampled cells and reports drift in machine-readable form. Default warn-not-invalidate (per Success Criteria). The per-scan canary task with a known-good baseline is the always-on smoke detector.

### Validation Methodology

- **Three-tier breadth-first scan with streaming reports** (decided this PRD round, see Success Criteria > Technical Success). Quick produces a coarse leaderboard in ~15 min; Standard tightens for default-switch decisions; Full provides tightest CIs. Cells are added to the same cache; the HTML report regenerates on disk after every cell.
- **Static discriminative task ordering** (decided this PRD round). The curated SWE-Bench Pro subset ships with a "most-informative-first" ordering picked once via offline calibration against known model classes. Quick's single task per model is task #1 in this ordering — chosen so a single cell carries maximum rank-signal. The fact that Quick produces a useful (if coarse) leaderboard at all depends on this choice; arbitrary task ordering would make Quick a coin-flip.
- **Pass-rate scoring (open question, must be resolved in architecture phase):** v1 working assumption is **pass@1** with a per-tier wall-clock cap (tighter on Quick, looser on Full). Architecture phase resolves: pass@1 vs. pass@k semantics, timeout-as-fail vs. timeout-as-skip, and how the per-task cap interacts with each tier's budget. (Brief open Q7.)
- **Confidence intervals:** Wilson-score CI on every leaderboard pass-rate. Wilson chosen over normal-approximation because Quick task counts are tiny (n=1) and Standard task counts are small (n=3–5) — normal-approx degenerates badly under those conditions. Quick rows always carry a `low-confidence-CI` badge by structural default; switching default models based on Quick alone is treated as user error in the README.
- **Adaptive depth — v1.1+ candidate, architecturally not blocked:** after each pass, look at pairwise CI overlaps and concentrate further runs only on contested rank pairs (stop measuring more tasks against models whose ranks are already settled). Maximum information per minute. Defer to v1.1 to keep v1 simple, but the cell-level cache structure (per Reproducibility above) supports it for free.
- **Reliability under repeat (deferred to v1.1+):** Single-run pass-rate is what v1 reports. Multi-run pass-rate (e.g., a model that scores 60% on a single run but 25% across 8 runs) is the next-frontier eval signal in 2026 — the brief flagged it as a v1.1+ candidate. The v1 PRD does not promise it; the architecture must not paint into a corner that makes adding it expensive.
- **Per-scan canary discipline:** the canary task uses a fixed, known-good baseline that has nothing to do with model-under-test ranking. It exists solely to detect infrastructure drift (harness regression, backend regression, OS change) so it can be distinguished from model behavior change. Three explicit states render in the report header: `canary-pass`, `canary-fail`, `canary-skipped`. Canary runs at the start of every `lcrc scan` invocation regardless of `--depth`.

### Eval-Data Integrity

This is the failure mode that has tainted SWE-Bench Verified and several other eval sets in 2025–2026. lcrc's defenses:

- **Use SWE-Bench Pro, not Verified.** The brief documented OpenAI's confirmation that Verified shows training-data leakage across every frontier model and that 59.4% of unsolved tasks have flawed tests. lcrc starts from the Pro subset, which is Scale-controlled and has not been declared tainted as of brief creation.
- **Default-deny container-per-task sandbox (binding requirement, see Success Criteria > Technical Success and FR16).** Every measurement runs inside an ephemeral container with no host filesystem mounted (only the per-task workspace), no network (only a constrained localhost route to `llama-server`), and a per-task env-var allowlist — default-deny by structural construction, not by enumerated policy. The model under test cannot read out-of-task host state, exfiltrate credentials, reach external hosts, or cross-contaminate sibling task workspaces, because none of those things exist from inside the container. The measurement subject *is* the threat model: an agentic LLM with bash tool access; no enumerated allowlist would survive its exploration. v1 acceptance check #9 is an adversarial-task verification that every out-of-envelope attempt fails at the container boundary.
- **Harness contamination, not just task contamination.** The brief documented NeuralNoise's finding that OpenCode demonstrates ~14% grader-peeking on its agentic harness. v1's choice of `mini_bash` wrapping `mini-swe-agent` is partially driven by avoiding this class of contamination; mini-swe-agent's minimal scaffolding is designed not to leak grader signals to the model. The architecture phase should document the audit-trail for this choice in case a future v1.1 considers adding OpenCode or another harness.
- **SWE-Bench Pro lifecycle risk (open question, must be resolved in architecture phase):** Pro is Scale-controlled. There is a non-zero probability that within v1's lifetime Pro becomes (a) inaccessible to lcrc due to licensing/redistribution constraints on the bundled subset, or (b) visibly contaminated like Verified. The architecture phase decides the fallback plan: pinned local snapshot of the curated subset, alternative task source, or graceful-degradation behavior if Pro becomes unusable. (Brief open Q5.)

### Computational & Resource Constraints

- **RAM × context-length fit gate:** before measuring, lcrc filters out models that won't fit in RAM at their default context length. RAM-sizing failures are the brief-documented #1 user complaint ("local AI is slow") — the fit gate is the v1 surface that addresses this, even though it's not a measurement methodology concern per se.
- **macOS-native perf collection privilege model (open question, must be resolved in architecture phase):** `powermetrics` requires sudo per call; a one-time signed launchd helper requires more setup but no per-call sudo prompts; graceful-degrade-without-power is the no-privilege option. The trade is between first-run UX cleanliness and metric completeness. (Brief open Q6.)

### Open Methodology Questions Carried to Architecture Phase

This PRD does not lock the following — the architecture phase resolves them:

1. **Pass@1 vs. pass@k semantics** + per-tier wall-clock cap behavior + timeout-as-fail vs. timeout-as-skip. (Brief Q7.)
2. **Cache staleness policy on `backend_build` change:** invalidate every cell vs. backend-build compatibility classifier (only invalidate when ABI/perf-relevant changes detected). (Brief Q8.)
3. **Harness/task version representation in the cache key.** (Carried forward from Reproducibility above.)
4. **SWE-Bench Pro lifecycle fallback plan.** (Brief Q5.)
5. **macOS perf-collection privilege model.** (Brief Q6.)
6. **Cache storage shape:** SQLite + JSON blobs vs. flat JSON-per-cell. (Brief Q2.)
7. **Run resumability** (Ctrl-C / OOM mid-task / crash recovery) — protocol decided in architecture phase, but cell-level independence (above) makes the trivial answer ("next invocation skips cells already in cache") work for free unless we want stronger guarantees. (Brief Q9.)

## CLI Tool Specific Requirements

### Project-Type Overview

lcrc is a non-interactive, scriptable CLI tool. The design defaults are: zero prompts, predictable exit codes, stdout-for-results / stderr-for-progress discipline, an explicit `--format` flag for output selection, and a layered config (CLI flags > env vars > optional TOML config file > built-in defaults). The CLI is the primary surface for v1; the HTML report is a downstream artifact written to disk during scan and read by the user manually. There is no daemon, no server, no SPA, no live-refresh mechanism beyond the user reloading the file.

The conventional `cli_tool` PRD sections `visual_design`, `ux_principles`, and `touch_interactions` are explicitly **not applicable** to lcrc and are skipped.

### Command Structure

The v1 command surface is **three subcommands plus standard meta-commands.** `lcrc deepen` from earlier brainstorming is folded into `lcrc scan --depth full`; `lcrc gc` and `lcrc doctor` are deferred to v1.1+. Shell completions deferred to v1.1.

#### `lcrc scan` — fill or extend the cache

**Purpose:** Run measurements against installed models. Always runs the canary task first; on `canary-fail`, completes the scan and writes the report (failure is impossible to miss in the header), but exits non-zero.

**Behavior:** Non-interactive. Streams per-cell completion to stderr with a per-model progress line and an honest estimated-remaining clock. After each cell completes, the HTML report on disk is regenerated and the cache is updated. Ctrl-C is safe — partial results are persisted; the next invocation resumes by skipping cells already in the cache.

**Flags:**
- `--depth quick|standard|full` (default: `quick`)
- `--model <pattern>` — restrict measurement to models matching `<pattern>` (substring match against model name or `model_sha` prefix)
- `--quiet` / `-q` — suppress per-cell streaming on stderr (results still written to disk)
- `--report-path <path>` — override the default HTML output path

**Exit codes:**
- `0` — scan completed cleanly; canary passed; no sandbox violations
- `1` — scan completed; canary failed (report written; treat with suspicion)
- `2` — scan completed; one or more sandbox-violation events occurred (report written)
- `3` — scan aborted by signal (Ctrl-C / SIGTERM); partial cache persisted
- `10` — configuration error
- `11` — pre-flight failure (no measurement attempted; e.g., **no supported container runtime present**, model directory unreadable, llama-server missing)
- `12` — concurrent scan in progress (lock-file held by another lcrc process; PID reported)

#### `lcrc show` — read the cache; render leaderboard

**Purpose:** Read-only view of cached measurements. Never writes to the cache or to the HTML report.

**Flags:**
- `--format text|json` (default: `text`)
- `--model <pattern>` — filter rows to matching models
- `--depth quick|standard|full` — filter to cells produced at a specific tier
- `--all` — include cells for uninstalled models or outdated `backend_build`s (default: hide; mirrors what the HTML report shows)
- `--limit N` — show only top-N rows

**Output:** text format is a fixed-width table sorted by the configured rank metric, written to stdout. JSON format is the same data with a `schema_version` field, written to stdout.

**Exit codes:**
- `0` — output written
- `4` — cache is empty (no scan has been run yet); message printed to stderr suggesting `lcrc scan`
- `10` — configuration error

#### `lcrc verify` — re-measure sampled cells; report drift

**Purpose:** Pick a sample of cached cells, re-measure them, and compare against the cached values. Default behavior is **warn**, not invalidate (per Success Criteria). To act on drift, the user runs `lcrc scan` again — there is no `--reinvalidate` flag.

**Flags:**
- `--sample N` — number of cells to re-measure (default: 5)
- `--model <pattern>` — restrict the sample to specific models
- `--format text|json` (default: `text`)

**Output:** text format is a numerical drift report (one row per re-measured cell, showing cached value, new value, delta, and CI overlap). JSON format is the same data with `schema_version`.

**Exit codes:**
- `0` — no significant drift detected
- `5` — drift detected on at least one cell (report written)
- `10` — configuration error

#### Meta-commands

- `lcrc --version` — prints `lcrc <semver>` plus pinned versions of `mini-swe-agent`, vendored SWE-Bench Pro subset, and the build commit hash. Exit 0.
- `lcrc --help` — usage summary, list of subcommands, link to README. Exit 0. `lcrc <subcommand> --help` shows per-subcommand usage.

### Output Formats

1. **Streaming CLI progress** (stderr, during `lcrc scan`): per-cell completion lines, per-model progress lines, estimated-remaining clock. Color via terminal-detection on stderr (color when TTY, plain otherwise). Suppressible with `--quiet`.
2. **HTML report** (file on disk, regenerated by `lcrc scan` after every cell): single self-contained file. Default location is `$XDG_DATA_HOME/lcrc/reports/latest.html` with timestamped historical files alongside (`report-2026-04-30T14-23-15.html`). Screenshot-friendly canonical header. The user opens this file in a browser and refreshes manually as the scan progresses.
3. **Plain-text leaderboard** (stdout, via `lcrc show`): fixed-width table format, sorted by rank. Mirrors the HTML rank exactly (acceptance check #8).
4. **JSON output** (stdout, via `lcrc show --format json` or `lcrc verify --format json`): stable schema with a top-level `schema_version` field. Backward-compatible additions only within a major version; breaking changes bump the version. Documented in the README.
5. **No log files in v1.** lcrc writes only to stderr during interactive use. Users redirect to a file themselves if they want logs (`lcrc scan 2> scan.log`). A v1.1+ `--log-file` flag could be added without breaking compatibility.

### Configuration Schema

Optional TOML config at `$XDG_CONFIG_HOME/lcrc/config.toml` (defaults to `~/.config/lcrc/config.toml`). Every key has a built-in default; the v1 user (the author) should never need to create this file. Layered precedence: CLI flag > env var > config file > built-in default.

**Sketch (architecture phase locks final keys):**

```toml
[paths]
# Cache directory; defaults to $XDG_DATA_HOME/lcrc/cache
cache_dir = "~/.local/share/lcrc/cache"
# HTML report output directory
report_dir = "~/.local/share/lcrc/reports"

[discovery]
# Additional model directories beyond the default llama.cpp cache (~/.cache/llama.cpp/...)
extra_model_dirs = []

[scan]
# Default depth when --depth is not specified on the CLI
default_depth = "quick"
# Per-tier per-task wall-clock caps (seconds) — exact values decided in architecture phase
quick_task_timeout = 600
standard_task_timeout = 900
full_task_timeout = 1800

[perf]
# Privilege model for macOS perf collection — chosen in architecture phase
# Values: "auto" | "powermetrics" | "launchd-helper" | "none"
collection_method = "auto"
```

**Env var overrides** follow the convention `LCRC_<SECTION>_<KEY>` (e.g., `LCRC_PATHS_CACHE_DIR`, `LCRC_SCAN_DEFAULT_DEPTH`). Single-section keys may use the shorter form (`LCRC_CACHE_DIR`) where unambiguous; final naming locked in architecture.

**Validation:** lcrc validates the config file on startup. Invalid keys or values fail fast with a helpful error message pointing at the offending line; exit code `10`.

### Scripting Support

- **Non-interactive by design.** No prompts anywhere. Safe to invoke from cron, CI, shell scripts, Makefiles.
- **Stable exit codes** as documented above. The exit-code table is part of the public interface; semver applies — no breaking changes within a major version.
- **stdout / stderr discipline.** `lcrc show` and `lcrc verify` write results to stdout; progress, diagnostics, and errors go to stderr. Pipe-friendly: `lcrc show --format json | jq '.rows[0]'` works as expected.
- **Stable JSON schemas** with `schema_version`. Schema documented alongside the README. Additions within a major version are backward-compatible (new optional fields); breaking changes bump the major.
- **Idempotency.** `lcrc scan` is safe to re-run; the cache prevents duplicate measurements within the same `(machine_fp, model_sha, backend_build, params)` cell. Re-running after partial completion (Ctrl-C, crash) resumes by skipping cells already in the cache — no `--resume` flag needed.
- **Concurrency control.** Only one `lcrc scan` invocation may run at a time. A lock file at `$XDG_STATE_HOME/lcrc/scan.lock` enforces this. Concurrent `scan` invocations exit immediately with code `12` and a stderr message identifying the holding PID. `lcrc show` and `lcrc verify` are read-only and lock-free; they may run concurrently with each other and with `lcrc scan` (showing the cache as it currently exists).
- **Quiet mode** (`--quiet` / `-q`) on `lcrc scan` suppresses per-cell streaming on stderr; the report still regenerates after every cell, results are still written, exit codes are unchanged. Useful for cron / CI / Makefile use.

## Project Scoping & Phased Development

### MVP Strategy & Philosophy

**MVP type:** problem-solving MVP. The minimum viable measurement loop that produces credibility-grade rankings for one user (the author) on one machine (M1 Pro 32GB). Not an experience MVP (no UX flourish required beyond the canonical HTML report), not a platform MVP (no ecosystem, no plugin surface), not a revenue MVP (no commerce surface, no marketing).

**Validation hypothesis:** the author switches default coding model (or makes a deliberate non-switch) based on what the report says. This is the single falsifiable signal v1 is built around. If the author runs lcrc, looks at the report, and either makes no decision or makes a decision lcrc didn't influence, v1 has missed.

**Fastest path to validated learning:** Quick tier (canary + 1 task per model) targets a 15-minute feedback loop. The author can scan, look, and form an opinion in one sitting — no overnight commitment required for the screening pass. Standard tier earns its complexity by being where the actual default-switch decision happens.

**What MVP explicitly does NOT include** (re-stated for binding; full list in Product Scope > Growth Features):
- No second backend (Ollama, MLX-if-not-low-effort, vLLM, LM Studio).
- No second harness (OpenCode, Aider, Claude Code).
- No second platform (Linux NVIDIA, Windows).
- No daemon, no notifications, no server, no SPA.
- No custom-eval extension surface for the deferred secondary persona.
- No adaptive depth, no multi-run reliability metric, no Pareto fronts.

The single-backend / single-harness / single-platform / single-task-source posture is what makes a solo-developer v1 tractable. Each "single" choice is documented with its lift-or-defer trigger so v1.1+ scoping has a clear runway.

### Resource Requirements

- **Team size:** one. The author is sole developer, sole tester, sole user. No designated PM, no QA, no SRE. The PRD itself was facilitated, not authored by another person.
- **Skill level:** intermediate (per BMM config). CS-literate, comfortable with TOML, Rust or Python (architecture phase decides), Apple Silicon developer tooling.
- **Time commitment:** **not committed.** lcrc has no shipping deadline. The no-marketing-posture decision means there is no "ship before competitor X" pressure (project memory: "lcrc — no marketing or urgency framing"). Effort estimation happens in the architecture phase, not here.
- **External dependencies under the team's control:** none. All v1 dependencies (llama.cpp, mini-swe-agent, SWE-Bench Pro subset, Homebrew formula) are external projects pinned by version. Loss of any single upstream is a real risk (see Risk Mitigation below); the team's mitigation surface is limited to vendoring and fallback planning.
- **Open-source release** is a v1 deliverable (Release Success criteria), not a v1 success gate. There is no community-management surface in v1; no expectation of contributions, issue triage, or PRs to handle. If they happen, that's information for v1.1+ scoping; if they don't, v1 is unchanged.

### Risk Mitigation Strategy

Three risk categories. Methodology and personal-utility risks are first-order; solo-developer risk shapes everything else.

**1. Methodology risks** — the failure mode where lcrc produces confidently wrong rankings the user trusts.
Mitigations (all binding in PRD):
- Default-deny container-per-task sandbox (acceptance check #9; Domain > Eval-Data Integrity); pre-flight refusal if no container runtime present.
- Per-scan canary task with three explicit header states (Domain > Validation Methodology).
- Wilson-score CIs on every leaderboard row; structural `low-confidence-CI` badge on every Quick row.
- Static discriminative task ordering (so Quick's single task is maximally signal-rich, not arbitrary).
- Cache key includes `backend_build`; `lcrc verify` re-measures and surfaces drift.
- mini_bash wrapping mini-swe-agent (avoids documented OpenCode grader-peeking, ~14% per NeuralNoise).
- Architecture-phase open questions (pass@1 semantics, backend-build invalidation policy, SWE-Bench Pro lifecycle fallback) are flagged and deferred to architecture, not silently assumed.

**2. Personal-utility risks** (replaces "market risks" — there is no market in scope) — the failure mode where lcrc is technically correct but the author stops using it because the loop isn't satisfying or the output isn't actionable.
Mitigations:
- 15-minute Quick budget (Technical Success target on M1 Pro 32GB).
- Streaming CLI updates + on-disk HTML regeneration (the user sees progress, not a black box).
- Tier discipline ("Quick screens, Standard switches, Full tightens") — each tier earns its wall-time honestly.
- Templated badges explain ranks without prose, so the user understands *why* without reading paragraphs.
- One-double-click report opening; no CLI re-invocation needed to interpret a row.
- Empty-machine UX directs to starter pack with copy-paste-ready download commands; failure-as-error mode avoided.

**3. Solo-developer risks** — the failure mode where lcrc consumes more energy than the author has, the project is abandoned, and the cache becomes a stale curiosity.
Mitigations:
- Aggressive scope cuts already locked: 1 backend, 1 harness, 1 platform, 1 task source. Each "single" defers a known v1.1+ feature, not a phantom one.
- Cell-level cache architecture is the structural foundation; everything else is layered features that can be implemented incrementally and shipped at any partial state. Even a 50%-complete v1 with only the cache + canary + Quick scan + HTML report is useful.
- Many architecture-phase open questions are explicitly punted to architecture, not pre-decided in PRD — buying the architect (also Theop) flexibility to pick the cheapest implementation per question.
- No team coordination overhead. No external deadlines. No marketing or community-management burden.
- **Upstream-dependency loss** (mini-swe-agent abandoned, SWE-Bench Pro restricted, llama.cpp ABI churn, etc.) is the corner case. Vendoring the harness and task subset is a v1 binding choice (Domain > Reproducibility) that buys runway. The SWE-Bench Pro lifecycle fallback is an architecture-phase open question; the loss-of-Pro contingency must produce a documented graceful-degradation behavior, not a v1 outage.

### Out-of-scope risks (explicitly NOT mitigated in v1)

- **Adoption risk:** there is no v1 adoption goal; non-adoption is not a failure.
- **Competitive risk:** no urgency framing (project memory). If Inspect AI or another framework adds sweep orchestration, lcrc's persona-fit posture is what matters; competitive-window thinking is not a v1 input.
- **Eval-variance flipping leaderboard between runs (F4 from brief pre-mortem):** acknowledged, accepted for v1, revisit if it bites.
- **"Autonomous"-bounded-by-CLI-invocation (F5 from brief pre-mortem):** accepted; README is honest. Daemon = v1.1+.

## Functional Requirements

This section is the binding capability contract for v1. Any capability not listed here will not exist in v1 unless explicitly added. UX/CLI design, architecture, and epic breakdown all trace back to this list.

### Installation & First Run

- **FR1:** User can install lcrc via Homebrew (`brew install lcrc`) on macOS Apple Silicon.
- **FR2:** User can run `lcrc scan` immediately after install with zero prior configuration; sensible defaults cover all required behavior.
- **FR3:** User can invoke `lcrc --version` to see the lcrc semver, the vendored mini-swe-agent version, the vendored SWE-Bench Pro subset version, and the build commit hash.
- **FR4:** User can invoke `lcrc --help` for a usage summary; per-subcommand help is available via `lcrc <subcommand> --help`.
- **FR5:** When no eligible models are detected on first run, user can see the empty-machine UX: a one-paragraph explainer plus a hardcoded starter pack of 3–5 small models with exact copy-paste-ready download commands.
- **FR6:** User can run `lcrc scan` on any installed-model set and see the canary's pass/fail/skipped state rendered prominently in the report header.

### Model Discovery & Eligibility

- **FR7:** System can detect installed models in the llama.cpp local cache directory (`~/.cache/llama.cpp/...`).
- **FR8:** System can compute a format-agnostic content hash (`model_sha`) for each detected model.
- **FR9:** System can filter detected models by RAM × default-context-length budget, excluding models that would not fit on the user's machine.
- **FR10:** User can see in the CLI output and the report which detected models were excluded by the fit gate and why (e.g., "RAM-budget exceeded at default ctx").
- **FR11:** System can extend model discovery to additional directories specified by configuration (`paths.extra_model_dirs` in `~/.config/lcrc/config.toml`).
- **FR12:** User can restrict any scan, show, or verify operation to a subset of models via `--model <pattern>` (substring match against model name or `model_sha` prefix).

### Measurement Execution

- **FR13:** System can run a canary task at the start of every `lcrc scan` invocation regardless of `--depth`.
- **FR14:** System can render the canary's outcome as one of `canary-pass`, `canary-fail`, or `canary-skipped` in the report header; `canary-fail` does not block the report from being written.
- **FR15:** System can execute SWE-Bench Pro tasks against each fit-eligible model via mini-swe-agent wrapped as a subprocess.
- **FR16:** System can run each per-task measurement inside a **default-deny isolation envelope** structurally implemented as a per-task ephemeral container. The container starts with: no host filesystem mounted (only the per-task workspace bind-mounted read-write), no network access (only a single allowed localhost route to the host's `llama-server` port), no host environment variables (only a documented per-task allowlist of safe variables). Every other host file path, network destination, and environment variable is non-existent from inside the container — blocked by structural construction, not by enumerated policy.
- **FR17:** System can record sandbox-violation events — any attempted access that the container blocks but the model still tried — as templated badges on the affected row and as report-surfaced events; sandbox violations cause `lcrc scan` to exit with code `2`.
- **FR17a:** System detects the presence of a supported container runtime at scan pre-flight time. If no supported runtime is available or running, `lcrc scan` exits with code `11` and prints setup instructions to stderr; no measurement is attempted. Container-runtime choice (Colima, OrbStack, Docker Desktop, Lima, etc.) is deferred to the architecture phase.
- **FR17b:** System pins the per-task container image (or image-build recipe) per lcrc release; the image identifier is recorded in cell metadata (per FR31) so a measurement is reproducible against the exact toolchain it ran under.
- **FR18:** System can collect macOS-native perf metrics — tok/s, ttft, peak RSS, power, thermal — for each measured cell; metrics that cannot be collected (e.g., due to perf-collection privilege model) are recorded as null/unavailable rather than blocking measurement.
- **FR19:** System can enforce a per-tier per-task wall-clock cap; capped tasks record a timeout-equivalent badge and do not block the scan from continuing.
- **FR20:** System can execute scans at three depths via `--depth quick|standard|full`; each successive depth extends the previous depth's cells with additional task measurements rather than replacing them.
- **FR21:** Quick depth runs the canary plus 1 SWE-Bench Pro task per model — specifically task #1 in the static "most-informative-first" task ordering shipped with the curated subset.
- **FR22:** Standard depth extends each model's cell to 3–5 tasks (Quick's task plus the next 2–4 in the static ordering).
- **FR23:** Full depth extends each model's cell to the full curated SWE-Bench Pro subset and adds quant/ctx variants beyond the default.

### Cache & Persistence

- **FR24:** System can key each measurement cell on `(machine_fingerprint, model_sha, backend_build, params)`, where `machine_fingerprint` = chip generation + RAM size + GPU core count, and `params` = ctx length, sampler temperature, threads, `n_gpu_layers`.
- **FR25:** System can store and retrieve each `(model, task)` cell independently; cells are the unit of caching, measurement, resumability, and depth extension.
- **FR26:** System can perform a cache lookup before measuring a cell; cells already present and matching the current cache key are not re-measured within a single scan or across scans.
- **FR27:** System can persist partial scan results such that Ctrl-C, OOM, or crash mid-scan does not lose completed cells; the next `lcrc scan` invocation resumes by skipping cells already in the cache. No `--resume` flag is required.
- **FR28:** User can run `lcrc verify --sample N` to re-measure N sampled cached cells and see a numerical drift report (cached value, new value, delta, CI overlap per cell).
- **FR29:** System defaults `lcrc verify` to warn on drift; cells are not invalidated unless the user re-runs `lcrc scan` against the affected models.
- **FR30:** System treats macOS patch-level upgrades as machine-fingerprint-stable (cells remain valid); `backend_build` changes invalidate affected cells per a policy decided in the architecture phase (open question carried to architecture).
- **FR31:** System can record per-cell metadata: depth tier that produced the cell, scan timestamp, `backend_build`, lcrc version, vendored harness/task version, perf metrics collected.

### Reporting

- **FR32:** System can render a single self-contained static HTML report file to disk; the file requires no external network access to view.
- **FR33:** System regenerates the HTML report on disk after every cell completes during a scan; the user refreshes the browser tab manually.
- **FR34:** System renders a canonical screenshot-friendly header on the HTML report containing, without scrolling: machine fingerprint, scan date, lcrc version, `backend_build`, canary state.
- **FR35:** System renders Wilson-score confidence intervals on every leaderboard pass-rate.
- **FR36:** System renders templated failure-mode badges on every applicable row from a fixed enum: `ctx-limited`, `OOM-at-N`, `repetition-loop`, `tool-call-format-failure`, `thermal-throttled`, `low-confidence-CI`, plus sandbox-violation tags. No LLM-generated prose explanations.
- **FR37:** System tags each leaderboard cell with the depth tier (Quick / Standard / Full) that produced it.
- **FR38:** Every Quick-tier row carries a `low-confidence-CI` badge by structural default to discourage default-switch decisions on Quick-only data.
- **FR39:** System writes the HTML report to a default location of `$XDG_DATA_HOME/lcrc/reports/latest.html` plus a timestamped historical file (`report-<ISO8601>.html`); user can override via `--report-path <path>` on `lcrc scan`.
- **FR40:** User can run `lcrc show` to see a plain-text leaderboard view in the terminal that ranks identically to the HTML report.
- **FR41:** User can filter `lcrc show` output via `--model <pattern>`, `--depth <tier>`, `--limit N`.
- **FR42:** User can include cells for uninstalled models or outdated `backend_build`s in `lcrc show` output via `--all` (default: hidden, mirroring HTML report behavior).
- **FR43:** User can request JSON output via `--format json` on `lcrc show` and `lcrc verify`; JSON outputs carry a top-level `schema_version` field. Default `--format` is `text`.

### CLI Surface, Configuration & Scripting

- **FR44:** System runs every command non-interactively; there are no interactive prompts on any subcommand at any depth.
- **FR45:** System exits with documented, semver-stable exit codes per subcommand: `0` success; `1` canary failed; `2` sandbox violations occurred; `3` scan aborted by signal; `4` cache empty (`lcrc show`); `5` drift detected (`lcrc verify`); `10` configuration error; `11` pre-flight failure; `12` concurrent `lcrc scan` in progress.
- **FR46:** System writes results (text or JSON) to stdout and progress, diagnostics, and errors to stderr; output streams are pipe-friendly (e.g., `lcrc show --format json | jq` works).
- **FR47:** System emits per-cell completion lines and an estimated-remaining clock to stderr during scan execution; stderr output uses color when stderr is a TTY and plain text otherwise.
- **FR48:** User can suppress per-cell streaming progress via `--quiet`/`-q` on `lcrc scan`; the report still regenerates after every cell, results still write to disk, exit codes are unchanged.
- **FR49:** System reads optional configuration from a TOML file at `$XDG_CONFIG_HOME/lcrc/config.toml`; every key has a documented default.
- **FR50:** System resolves configuration with layered precedence: CLI flag > environment variable > config file > built-in default.
- **FR51:** System validates the config file on startup; invalid keys or values fail fast with a stderr message pointing at the offending line, exit code `10`.
- **FR52:** System enforces single-writer concurrency on `lcrc scan` via a lock file at `$XDG_STATE_HOME/lcrc/scan.lock`; concurrent `scan` invocations exit immediately with code `12` and a stderr message identifying the holding PID.
- **FR53:** System allows `lcrc show` and `lcrc verify` to run concurrently with each other and with a running `lcrc scan` (read-only operations are lock-free).
- **FR54:** System exposes stable JSON output schemas with backward-compatible additions only within a major version; breaking schema changes bump the major.

## Non-Functional Requirements

NFRs specify how well lcrc must perform across quality attributes. Listed selectively: only categories that apply to lcrc's single-user, local-CLI, no-network shape. Scalability and Accessibility are explicitly **not** in scope for v1; growth and broad-audience deployment are not v1 concerns.

### Performance

Targets are stated against the v1 reference rig (M1 Pro 32GB, macOS Apple Silicon) with a typical installed-model count of ~5.

- **NFR-P1 (Quick scan budget):** A `lcrc scan --depth quick` invocation on a 5-model fit-eligible installed set completes in **≤25 minutes wall-clock**, with a target of **~15 minutes**. Container spin-up overhead is included in this budget.
- **NFR-P2 (Standard scan budget):** A `lcrc scan --depth standard` extending a Quick-populated cache for the same 5-model set completes in **~1.5–3 hours wall-clock**.
- **NFR-P3 (Full scan budget):** A `lcrc scan --depth full` for the same 5-model set completes overnight (≤12 hours wall-clock target on the reference rig).
- **NFR-P4 (Per-task cap, Quick):** No single SWE-Bench Pro task at Quick depth exceeds the cap value chosen in architecture phase (working assumption: 600 seconds). Capped tasks record a timeout-equivalent badge.
- **NFR-P5 (Cache lookup latency):** Cache-key lookup before measurement (per FR26) completes in **<100 ms** for a cache containing up to 10,000 cells.
- **NFR-P6 (Report regeneration latency):** HTML report regeneration after a cell completes (per FR33) finishes in **<2 seconds** for a cache containing up to 1,000 cells.
- **NFR-P7 (CLI startup latency):** `lcrc show` returns rendered output in **<500 ms** for a cache containing up to 1,000 cells. `lcrc --help` and `lcrc --version` return in **<200 ms**.
- **NFR-P8 (Streaming feedback responsiveness):** The CLI estimated-remaining clock during `lcrc scan` updates at least once every 10 seconds; per-cell completion lines appear within 1 second of the cell finishing.
- **NFR-P9 (Per-task container spin-up):** Container creation, workspace mount, and shutdown overhead per task is **<5 seconds** on the reference rig with the chosen runtime; overhead higher than that is treated as runtime-choice failure and re-evaluated in architecture.

### Reliability

- **NFR-R1 (Resumability):** A `lcrc scan` interrupted by Ctrl-C, OOM, host suspend/resume, or crash loses no completed cells. The next `lcrc scan` invocation resumes by skipping cells already in the cache, without user intervention or special flags.
- **NFR-R2 (Atomicity of cell writes):** A cell write is atomic from the cache's perspective: a partially-completed measurement either appears fully in the cache after success or does not appear at all. No half-written cells.
- **NFR-R3 (Cache durability across version upgrades):** A cache populated by lcrc version `X.Y.Z` is readable by version `X.Y.(Z+n)` and `X.(Y+n).0`. Major version upgrades may require explicit migration; lcrc must detect a too-old cache schema and exit with a clear error rather than silently misreading.
- **NFR-R4 (Graceful degradation — perf collection):** If perf metrics cannot be collected, affected metrics are recorded as null/unavailable per cell and the scan continues. Missing perf metrics never abort a scan.
- **NFR-R5 (Graceful degradation — model-server lifecycle):** `llama-server` startup failures, mid-task crashes, hangs, or unexpected exits are detected via timeout and surfaced as a templated badge on the affected cell. The scan continues with the next cell.
- **NFR-R6 (Idempotency):** Repeated `lcrc scan` invocations against an unchanged installed-model set + cache + `backend_build` produce no new measurements (cache hit on every cell). `lcrc verify --sample N` is non-destructive.
- **NFR-R7 (Concurrency safety):** A concurrent `lcrc scan` invocation never partially overlaps another; the lock file (per FR52) prevents both from progressing simultaneously. `lcrc show` and `lcrc verify` reads remain consistent during a concurrent scan.
- **NFR-R8 (Container teardown on abort):** When `lcrc scan` aborts (Ctrl-C, crash, OOM), any per-task container that was running is torn down by lcrc on best-effort basis; orphaned containers do not accumulate across scans. The container runtime's own cleanup mechanisms (e.g., labels for orphan detection) are used as a backstop.

### Security

The single security-relevant boundary is the **default-deny container-per-task isolation envelope** (FR16). The threat model is the measurement subject itself — an agentic LLM with bash tool access running inside the per-task container. Isolation must be structural; enumerated allowlists are insufficient against an exploring agent.

- **NFR-S1 (Default-deny by structural construction):** The per-task container starts with: no host filesystem mounted (only the per-task workspace bind-mounted), no network access except a single allowed localhost route to the host's `llama-server` port, and no host environment variables except a documented per-task allowlist of safe variables. Every other host file path, network destination, and environment variable is non-existent from inside the container — not blocked by policy, but absent by construction. There is no enumerated denylist; the model under test cannot read or reach what is not provided.
- **NFR-S2 (Sandbox failure visibility):** Any sandbox-violation event (an attempted access that the container blocks but the model still tried) surfaces as a templated badge on the affected row AND causes `lcrc scan` to exit with code `2`. There is no "silent pass" path through the envelope. Acceptance check #9 verifies this with an adversarial-task battery.
- **NFR-S3 (Container runtime is a hard dependency, no fallback):** lcrc requires a supported container runtime to be installed and running. At scan pre-flight (FR17a), if no supported runtime is detected, lcrc exits with code `11` and prints setup instructions; no measurement is attempted under any "weak isolation" mode. There is no `sandbox-exec`-only fallback, no `--unsafe-no-sandbox` flag, no degraded mode. The choice is run-with-isolation or don't run.
- **NFR-S4 (Network surface from inside container):** The container's network configuration permits exactly one outbound destination: the host's `llama-server` on a specific port (or its containerized equivalent). DNS, public-internet, host-other-port, and same-bridge other-container connectivity are blocked. No model-under-test can exfiltrate task state, credentials, or eval signals to an external host because no external host is reachable.
- **NFR-S5 (Environment variable scrubbing):** The per-task container receives only environment variables on a documented allowlist (e.g., `PATH`, `LANG`, task-specific test-runner config). Credential-bearing variables — `AWS_*`, `GH_*`, `GITHUB_TOKEN`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `HF_TOKEN`, and any other variables not explicitly on the allowlist — are not passed in. The allowlist is finite and documented; any variable not on it is absent inside the container.
- **NFR-S6 (Container image pinning):** The per-task container image (or image-build recipe) is pinned per lcrc release. The image identifier is recorded in cell metadata so a measurement is reproducible against the exact toolchain it ran under (per FR17b). Image content (base OS, language toolchains, test runners) is documented; reviewers can read the image spec to verify isolation directly.
- **NFR-S7 (No telemetry):** lcrc collects no usage telemetry, no crash reports, no anonymized usage statistics, no opt-in or opt-out telemetry of any kind in v1. Nothing leaves the user's machine. (Re-stated in Observability for emphasis.)

### Compatibility & Portability

- **NFR-C1 (Platform support, v1):** lcrc runs on macOS 12 Monterey or later on Apple Silicon (M1, M2, M3, M4 generations). Intel Mac and pre-Monterey macOS are explicitly unsupported.
- **NFR-C2 (Cache key stability across OS patches):** A `machine_fingerprint` computed before a macOS patch-level upgrade matches the `machine_fingerprint` computed after. Cache cells remain valid (per FR30).
- **NFR-C3 (Cache key stability across lcrc patch upgrades):** A cache populated by lcrc `X.Y.Z` reads correctly under lcrc `X.Y.(Z+n)`. Patch versions never invalidate caches.
- **NFR-C4 (External-binary version drift):** Vendored `mini-swe-agent` and SWE-Bench Pro subset versions are pinned by lcrc release. lcrc never silently accepts an unpinned version of either. Container image is similarly pinned (per NFR-S6).
- **NFR-C5 (Architecture extensibility constraint):** v1 architecture must not preclude Linux NVIDIA support in v1.1 — i.e., platform-specific code (perf collection, container-runtime selection, model-discovery paths) is factored cleanly such that Linux additions are additive, not architectural rewrites.

### Observability

- **NFR-O1 (Streaming feedback):** During `lcrc scan` execution, the CLI emits per-cell completion lines and a per-model progress line to stderr; the estimated-remaining clock updates at least every 10 seconds (per NFR-P8).
- **NFR-O2 (Disk-only state):** lcrc writes only to disk: cache, HTML reports, lock file, optional log file (if user redirects stderr). lcrc opens no network sockets except (a) localhost to `llama-server` and (b) the local container runtime's control socket, both for measurement orchestration.
- **NFR-O3 (No telemetry, hard line):** Per NFR-S7, lcrc has no telemetry, crash reporting, or usage analytics surface. This is non-negotiable in v1.
- **NFR-O4 (`lcrc --version` self-attestation):** `lcrc --version` reports lcrc semver, vendored mini-swe-agent version, vendored SWE-Bench Pro subset version, container image identifier (per NFR-S6), and build commit hash. Sufficient to reproduce a measurement environment from a screenshot or report.

### Integration

- **NFR-I1 (`llama-server` integration):** lcrc starts and manages `llama-server` instances per measurement (one server per cell or per model — architecture decides). The server runs on the host (not inside the per-task container, to avoid model-load overhead per task). Per-task containers connect to the server via a constrained localhost route (the only outbound route the container has, per NFR-S4). Server crashes, hangs, and unresponsive states are detected via documented timeouts and recovered per NFR-R5.
- **NFR-I2 (`mini-swe-agent` integration):** lcrc invokes `mini-swe-agent` as a vendored subprocess **inside the per-task container** (so the agent itself is also subject to the isolation envelope). lcrc captures the subprocess's exit code, stdout, stderr, and per-task pass/fail signal. Subprocess crashes surface as templated badges (per NFR-R5).
- **NFR-I3 (Perf-collection integration):** lcrc collects macOS perf metrics from the host (not from inside the container — perf metrics describe the host's resource utilization while the model runs). Mechanism (`powermetrics` with sudo, signed launchd helper, or graceful-degrade-without-power) is chosen in architecture phase. Failure to obtain privilege results in null perf metrics, never an aborted scan (per NFR-R4).
- **NFR-I4 (Container runtime integration):** lcrc detects the presence of a supported container runtime at scan pre-flight (per FR17a). Supported runtimes are documented; the architecture phase picks one or more. lcrc uses the runtime via its standard command-line / API surface; lcrc does not require root or privileged access to the runtime (rootless container support is preferred where the runtime offers it).
- **NFR-I5 (Homebrew distribution):** lcrc ships as a Homebrew formula such that `brew install lcrc` is the canonical install path. The formula `depends_on` the chosen container runtime so that `brew install lcrc` pulls in the runtime if it isn't already installed; the user is informed of the dependency at install time.
- **NFR-I6 (No required cloud or external service):** lcrc requires no external service, no API key, no auth flow, no remote endpoint to function in v1. The user's machine is the entire dependency graph (plus pre-installed `llama-server` and the container runtime).
