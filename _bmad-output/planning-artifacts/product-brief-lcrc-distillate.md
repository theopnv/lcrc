---
title: "Product Brief Distillate: lcrc"
type: llm-distillate
source: "product-brief-lcrc.md"
created: "2026-04-30"
purpose: "Token-efficient context for downstream PRD creation"
---

# lcrc — Product Brief Distillate

## The Reframe (history & rationale)

- **docs/legacy/v0.md framing (legacy):** "benchmark sweep tool that recommends" with Pareto fronts, FastAPI + SPA, multi-backend, multi-harness, ~24 milestones.
- **Brainstorming reframe (2026-04-29):** "**autonomous, personal benchmark database for local LLMs**." The cache *is* the product; autonomous orchestration is the mechanism that fills it. Both are co-equal — cache without orchestration = "another results JSON"; orchestration without cache = "another benchmark runner."
- **docs/legacy/v0.md is now legacy.** Theop has flagged it as superseded. All planning input flows from `_bmad-output/`. Don't pull design decisions from docs/legacy/v0.md without checking the brainstorming session first.
- **Reframe shrunk milestones from ~24 to ~10–12.** Cut: catalog (M6, M7), harness layer (M10, M11), Pareto reporter (M15 → simpler ranker), server (M17, M18), SPA (M19–M22).

## Vocabulary Shifts (rename across CLI, schema, docs)

| docs/legacy/v0.md word | v1 word | Reason |
|---|---|---|
| `sweep` | `run` | Theop flagged "sweep" as confusing jargon; `run` is plain |
| `recommendation report` | `measurement report` / "your top picks for `<task>`" | "Recommendation" oversells; v1 measures and ranks, doesn't infer |
| `Pareto-optimal cell` | `leaderboard row` | Pareto cut from v1; single weighted score per task |
| `tiered dropout` | "fill the cache efficiently" | Same algorithm, plainer name |
| `weekly profile` | `delta run` | Cadence doesn't matter — what matters is "measure what's missing" |
| `lcrc discover` | `lcrc scan` | "Discover" implied catalog browsing (cut); `scan` matches the new hardware-introspection role |
| `lcrc report` | `lcrc show` | Shorter, matches CLI conventions |
| (new) | `lcrc deepen` | Tier-2 opt-in to expand from minimal first-run battery to full SWE-Bench Pro subset |
| (new) | `lcrc verify` | Re-runs sample of cached cells to detect drift; warns by default |

## v1 Scope — IN

- **Platform:** macOS Apple Silicon ONLY (Linux NVIDIA + Windows = v1.1+).
- **Backend:** llama.cpp / `llama-server` ONLY (Ollama via llama-swap = v1.1; vLLM/LM Studio = v2).
- **Harness:** ONE — `mini_bash`, **wrapping mini-swe-agent** (decided; not internal reimplementation). Aligns with Scale SEAL's reference methodology.
- **Task source:** curated SWE-Bench Pro subset. ~3-5 tasks per model, one default quant, on first-run (Tier 1). Full set + multi-quant on `lcrc deepen` (Tier 2).
- **Scoring:** agentic pass-rate (mini-swe-agent's pass/fail). Wilson-score CI displayed on every leaderboard row. Pass@1 is working assumption — not yet locked (open Q7).
- **Perf metrics (macOS-native):** tok/s, ttft, peak RSS, power, thermal. Implementation mechanism TBD (open Q6: powermetrics-with-sudo vs signed launchd helper vs graceful-degrade).
- **Cache key:** `(machine_fingerprint, model_sha, backend_build, params)`.
  - `machine_fingerprint` = chip generation + RAM + GPU core count (durable across OS patch upgrades).
  - `params` = ctx length, sampler temperature, threads, `n_gpu_layers`. Everything else pinned to defaults.
- **Per-scan canary task:** ONE task with a known-good baseline, ~1 min, runs every `scan`. Result shown prominently in report header. Detects infrastructure drift (harness, backend, OS) separately from model behavior change.
- **Output:** single self-contained static HTML file. Screenshot-friendly by default — canonical machine-fingerprint header, scan date, lcrc version, `backend_build` rendered prominently for organic distribution via Reddit/Discord screenshots.
- **Cell explanations:** **templated badges** ONLY, not freeform/LLM-generated prose. Examples: `ctx-limited`, `OOM-at-N`, `repetition-loop`, `tool-call-format-failure`, `thermal-throttled`, `low-confidence-CI`.
- **Empty-machine UX:** text explainer + hardcoded ~3-5 small starter-model suggestions with exact `ollama pull` / download commands. NOT a recommendation engine — fixed list.
- **Iteration loop budget:** **<2h for incremental scans** (1 new model added). First-run is honestly an overnight job for a full collection — brief explicitly does NOT promise <2h for first scan.

## v1 Scope — OUT (with deferral target)

| Feature | Deferred to | Reason |
|---|---|---|
| Linux NVIDIA, Windows | v1.1 | F7 defuse: cross-platform doubles maintenance |
| Ollama backend | v1.1 | F8 defuse: backend lifecycle bugs (Ollama unloading, llama-swap config quirks) |
| vLLM, LM Studio backends | v2 | Adjacent to v1.1 backend expansion |
| Second harness (OpenCode etc.) | v2 | F1 defuse only requires ONE harness for agentic signal |
| **Harness-as-axis comparison** (docs/legacy/v0.md headline) | v2 | Brainstorming explicitly cut; single-harness only in v1 |
| Curated catalog with auto-discovery | v2 | R3 modified-accept: read installed GGUFs only in v1 |
| Custom eval extension surface | v1.1+ | The eng-lead persona feature; deferred to keep v1 tinkerer-first |
| Pareto fronts, weighted-profile recommenders | v2 | R5 accepted: single weighted score / leaderboard row |
| User-tunable scoring sliders | v2 | Same |
| FastAPI server, React SPA, live progress UI | v2 | R6 accepted: CLI + static HTML only |
| Background daemon + native notifications | v1.1+ | F5 acknowledged: "autonomous" in v1 = *what* is delta-aware; *when* is still CLI-triggered |
| Inspect AI integration / interop | v1.1+ | Deliberate decoupling; defends 6-12 month competitive window if AISI adds sweeping |
| Community-shared benchmark dataset | v3 | Federation across personal databases |
| "Indistinguishable cluster" leaderboard marking | v1.1+ | Nice-to-have UX polish; CIs in v1 carry the basic info |
| Falsifiable trust pre-commitment in success criteria | (skipped) | Theop chose qualitative success; revisit if v1 doesn't drive a default switch |

## Pre-Mortem Risks — Defuses Committed for v1

| ID | Risk | Defuse |
|---|---|---|
| F1 | Capability scores ≠ agentic usefulness | Keep ONE minimal agentic harness in v1 (mini_bash wrapping mini-swe-agent) — at least one source of agentic signal in the leaderboard |
| F2 | Cache invalidation eats trust | `lcrc verify` command re-runs N% of cached cells, reports drift. Cache key includes `(machine_fp, model_sha, backend_build, params)`. Reports show "cached on `<date>` with `<backend_version>`" inline. Plus per-scan canary in report header. |
| F3 | First run too long, breaks <2h promise | Two-tier battery: Tier 1 = small first-run (3-5 tasks, 1 quant); Tier 2 = `lcrc deepen` for full coverage. Brief explicitly scopes <2h to incremental scans only. |
| F6 | Empty-machine UX feels like an error | Hardcoded 3-5 starter models with exact download commands. Not curation — fixed list of very small (~0.8B class) models that run on any hardware. |
| F7 | Cross-platform doubles maintenance | macOS Apple Silicon ONLY for v1; Linux NVIDIA = v1.1 |
| F8 | Backend lifecycle bugs (Ollama unloading, llama-swap config) | llama.cpp via `llama-server` ONLY in v1; Ollama = v1.1 |

## Pre-Mortem Risks — Acknowledged but NOT solved in v1

- **F4 (eval CI variance flips leaderboard between runs):** Accept for v1. Wilson CIs displayed on every row mitigate misreading; cluster-marking deferred to v1.1. If eval variance bites in practice, revisit.
- **F5 (no daemon → "autonomous" is bounded by user invocation):** Accept for v1. README must be honest: "autonomous" in v1 means *what* gets measured (delta-aware), not *when* (still CLI-triggered). Daemon = v1.1+.

## Rejected Ideas (don't re-propose; they were considered and cut)

- **R1 radical form (predict quality without running, from community data):** Rejected for v1. Soft form (predict-skip via hardware-fit gate + cache as predictor) accepted. Quality prediction needs community data lcrc doesn't have yet.
- **R3 strict (curated catalog with "approved" models):** Rejected. Catalog is a *discovery* surface, not a curation gatekeeper. Modified to: v1 reads installed GGUFs only; catalog/discovery is a v2 idea. lcrc never downloads or curates.
- **R4 (multi-harness in v1):** Rejected. Keep one harness; harness comparison (the docs/legacy/v0.md headline novelty) is explicitly v2.
- **R5 (Pareto front per task):** Rejected. Single weighted score per task, leaderboard row format. Pareto = v2.
- **R6 (FastAPI + SPA in v1):** Rejected. CLI + static HTML opened in browser. Server + SPA = v2.
- **OpenCode as v1 harness:** Cut. docs/legacy/v0.md had OpenCode + mini_bash. Brainstorming + web research surfaced OpenCode contamination (~14% grader-peeking per NeuralNoise) and complexity reasons to defer.
- **Inspect AI as v1 eval framework:** Rejected. Brief decided to roll our own minimal runner + bundle SWE-Bench Pro tasks directly. Reasons: (a) avoid framework lock-in, (b) defends competitive window if AISI adds sweep orchestration, (c) keeps v1 batteries-included and beginner-friendly. Inspect interop = v1.1+ if useful.
- **Bench360 as v1 dependency:** Rejected by physics. Bench360 is server-class NVIDIA only; doesn't run on Mac. v1 implements equivalent system metrics natively (powermetrics et al), inspired by Bench360's methodology.
- **Falsifiable trust pre-commitment in success criteria:** User chose qualitative success ("trust enough to switch defaults") over Skeptic's suggested formal pre-commitment. Revisit if v1 ships and the qualitative criterion proves too soft.

## Requirements Hints (from user statements + research)

- **First-time UX:** `brew install lcrc && lcrc scan` is the aspirational one-liner. Implies Homebrew formula at v1.
- **No configuration screen.** Tinkerers will accept a CLI; will not accept a config wizard. Defaults must be opinionated and good.
- **Self-attesting reports.** HTML must be screenshot-friendly because tinkerers will paste them in Reddit/Discord to settle arguments — that's the organic distribution channel.
- **macOS perf collection privilege model is undecided** (open Q6). If sudo is required, ask once at install (e.g., signed launchd helper) — never on `scan`. Or graceful-degrade-without-power if no helper.
- **Watchdog / failure-mode UX** is unspecified in brief but raised by DX critic — model OOM, llama-server crash/hang, SWE-Bench Pro task hang, Ctrl-C resumability, low-disk pre-flight. PRD should address per-task wall-clock cap, run resumability, and `lcrc doctor` pre-flight.
- **Cache pruning / `lcrc gc`** is not in brief but DX critic flagged it. After 10+ runs, the cache could accumulate cells for uninstalled models / old backend builds. Worth scoping in PRD.
- **Default `lcrc` (no args)** could route to a context-aware wizard ("you have 2 new GGUFs since last run; measure them?"). Not in brief; DX critic suggested. Worth considering for Workflow Amnesia mitigation.
- **Backend-build compatibility classifier** (open Q8): full invalidate-on-upgrade is correct but expensive. PRD should decide whether to apply heuristics (only invalidate on commits flagged perf/ABI-relevant).
- **Variance reporting beyond CIs:** "tied cluster marking" is v1.1+ but worth implementing in a way that makes adding it cheap.

## Technical Context

- **Target hardware sweet spot (April 2026):** Mac Mini M4 Pro 48GB at $1,799 — runs Qwen 3.6-35B-A3B (35B MoE, 3B-active) at 3B-class speed. This is the spec the v1 starter pack and Tier 1 task budget should be tuned for.
- **Storage shape (open Q2):** SQLite + JSON blobs (per docs/legacy/v0.md) vs flat JSON-per-cell. Architecture decision; brainstorming punted.
- **Model discovery:** lcrc reads `~/.cache/llama.cpp/...` for installed GGUFs. Does NOT read Ollama's blob store in v1 (Ollama = v1.1). Should also handle LM Studio's `~/.cache/lm-studio/models/` if trivial — open question.
- **mini-swe-agent:** wrapped as subprocess (decided). Pin a specific version inside the cache key so harness updates don't silently invalidate measurements. Vendor or pin both mini-swe-agent and SWE-Bench Pro.
- **SWE-Bench Pro licensing/redistribution (open Q5):** Pro is Scale-controlled. May require auth, may have redistribution restrictions for the curated subset bundled with lcrc. Need a licensing check + fallback if Pro becomes unavailable or visibly contaminated within v1's lifetime.
- **Confidence-interval method:** Wilson score (chosen over normal-approximation because of small task counts in Tier 1).

## Audience — Detailed

- **Primary v1 user (the author, Theop):** Local-LLM tinkerer with 4-20 GGUFs in `~/.cache/llama.cpp/`. Reads r/LocalLLaMA. Has opinions about Q4_K_M vs Q5_K_M. Picks daily-driver model by gut. CS-literate, TOML-OK. Mac-first. v1 success = "I trust the leaderboard enough to switch defaults based on what it says." External adoption is NOT a v1 gate.
- **Secondary v1.1+ user (engineering lead picking team stack):** Wants to codify team's quality bar with custom evals reflecting their actual codebase. Wants to re-run as new models drop. Brief acknowledges them; v1 architecture must NOT paint into a corner that blocks them in v1.1+ (i.e., the eval-task interface should be cleanly factorable for extension).
- **"Two faces, one product" archetype** decision from brainstorming: beginner UX = preset; expert UX = full config. Single product, not a UX fork. v1 is beginner-default — expert escape hatches added as needed.
- **Framing posture: beginner-friendly throughout** — vs. NeuralNoise harness-bench's research-artifact density. Threads through tone, scope justification, and audience section.

## Competitive Intelligence (April 2026)

- **NeuralNoise `harness-bench` (April 28, 2026 — published 2 days before brief):** Closest existing artifact. 17 model-quants × 5 harnesses (Aider/Claude Code/OpenCode/Pi/Qwen CLI) × 16 SE tasks = 1,360 runs on a single M3 Max via llama-swap. Per-cell pass rate + seconds/task. Hand-curated blog tables, not productized. **Directly contradicts docs/legacy/v0.md's "nobody covers harness comparison + sweep + local" claim** — but lcrc cut harness comparison from v1, partially defusing the threat. lcrc differentiates on: productization, autonomy, caching, beginner-friendly UX, single-machine/installed-models scope. URL: http://www.neuralnoise.com/2026/harness-bench-wip/ — read in full before architecture work; cite explicitly in README.
- **Inspect AI (UK AISI):** 200+ pre-built evals + Ollama/llama.cpp/vLLM providers. Confirmed via April 2026 search: NO sweep / perf orchestration as of now. **Most likely incumbent to add it.** AISI has the resources. **6-12 month plausible defensibility window** for lcrc's local-first, opinionated, batteries-included Mac UX before Inspect potentially closes the gap. lcrc's avoidance of Inspect framework dependency in v1 is deliberate — preserves that window.
- **Bench360:** Server-class only (vLLM/SGLang/TGI/LMDeploy on NVIDIA). Doesn't run on Mac. lcrc takes inspiration for system-metrics methodology, not code.
- **mini-swe-agent:** Just a harness, used by Vals AI as their fair-comparison baseline scaffold. Scale SEAL canonical methodology pairs it with SWE-Bench Pro. lcrc bundles this same pairing — aligns with leading methodology.
- **GuideLLM (Red Hat / vLLM ecosystem):** RPS/concurrency sweep on OpenAI-compatible endpoints. JSON/YAML/CSV output. No agentic harness, no quality eval, no model-selection output. Orthogonal — production deployment perf.
- **AkitaOnRails llm-coding-benchmark (April 2026):** Open-source repo benchmarking commercial + OSS LLMs through OpenCode (codex exec for GPT-5.4). Single harness, no sweeps, no recommendation. Establishes "OpenCode as eval harness" pattern lcrc was planning to adopt — but lcrc cut OpenCode from v1.
- **qMeter (ASPLOS 2026), trtllm-bench:** Auto-sweep frameworks for server-class deployment (TensorRT-LLM, H100/A100). Academic / NVIDIA-tied. Proves "auto-sweep" is recognized 2026 pattern in cloud tier — orthogonal to lcrc's consumer-hardware niche.

## Market Signals (April 2026)

- **SWE-Bench Pro is current gold standard for agentic-coding evals.** Scale SEAL leaderboard is reference. Acknowledges scaffold/harness choice swings results materially (e.g., Grok 4 self-reports 72-75%, vals.ai's SWE-agent scaffold gets 58.6%).
- **SWE-Bench Verified is now considered tainted.** OpenAI confirmed every frontier model shows training-data leakage; 59.4% of unsolved tasks have flawed tests; OpenAI stopped reporting Verified scores. **Strengthens lcrc's "your machine, your data" positioning + the eng-lead "bring your own eval" v1.1+ pitch.**
- **Open-weight agentic-coding model quality crossed "usable" threshold in Q1 2026:** MiniMax M2.5 (80.2%), Qwen3-Coder-Next (70.6%, 3B active), GLM-5/Kimi K2.5/DeepSeek V3.2 (73-78%) on SWE-Bench Verified.
- **Apple Silicon MoE inflection:** Qwen 3.6-35B-A3B (released April 16 2026) — 35B MoE, 3B active, runs at 3B-class speed on Mac Mini M4 Pro 48GB at $1,799. Consumer hardware sweet spot expanded; lcrc timing is well-aligned.
- **llama-swap has emerged as de facto unattended-serving layer in early 2026,** displacing Ollama for benchmarking workloads. Multiple 2026 benchmark authors migrated from Ollama for reasons F8 captures (mid-session model unloading, flaky `keep_alive`, broken bf16, per-request num_ctx negotiation).
- **Reliability-under-repeat is the new eval frontier:** cited pattern is 60% single-run → 25% over 8 runs. v1 shows CIs but not multi-run pass-rate. Worth flagging in PRD as a v1.1 candidate so lcrc isn't dismissed as "just another single-shot benchmark."
- **Gartner-style projection cited in 2026 coverage:** >40% of agentic-AI projects will be cancelled by end of 2027 due to inadequate eval. Eval rigor is becoming a board-level concern, not just a hobbyist concern. Validates the eng-lead persona and timing.

## Common Local-LLM Pain Points (verbatim from r/LocalLLaMA / blogs / web research)

- **Q8 vs Q4 confusion is widespread.** Empirical finding from harness-bench: "Q8 is a slight net regression vs Q4 at sub-50B scale, and strictly slower." Sweep tools are uniquely positioned to surface this.
- **Backend choice has bigger practical impact than quantization for tool-calling reliability:** mlx_lm doesn't enforce JSON schema; llama.cpp grammar-constrained sampling causes infinite loops on long contexts; mlx-lm v0.30.6 lacks tool parsers for Mistral/Devstral. Users discover these the hard way after multi-GB downloads.
- **RAM-sizing failure is the #1 user complaint.** Users routinely load models exceeding the "60% of unified memory" rule and report "local AI is slow." lcrc's RAM-fit filter directly addresses this.
- **Repetition-loop failures are common:** "Gemma 4 enters repetition loops after ~11 tool calls"; "GLM-4.7 as chat model gets stuck"; "model loaded fine but stuck in repetitive loops on agentic tasks." Single-shot benchmarks miss these — multi-step harness eval catches them. Templated badge `repetition-loop` is the v1 surface for this.
- **Ollama unattended-benchmarking failures** documented across 2026: mid-session model unloading, flaky `keep_alive: 0`, broken bf16, per-request `num_ctx` negotiation. F8 defuse (llama.cpp/llama-server only in v1) is well-justified by these.
- **Subagent delegation is empirically a non-pattern in 2026** — forced delegation experiments produced equal quality at higher cost/wall time across Claude Code, OpenCode, Codex. lcrc's monolithic-orchestrator design is on solid ground; no need to over-architect.
- **Opus 4.7 produces measurably worse code under Claude Code's 6-11M cache-read context vs OpenCode's 210K** (AkitaOnRails). Concrete evidence harness/context choices materially affect quality.

## Open Questions for the PRD (carried from brief, with notes)

1. **`lcrc verify` behavior:** warn on drift vs auto-invalidate? Working assumption: warn; user opts in to re-measure. (Brainstorming open Q3.)
2. **Cache storage shape:** SQLite + JSON blobs (per docs/legacy/v0.md) vs flat JSON-per-cell. Architecture decision. (Brainstorming open Q4.)
3. **Hardcoded starter pack composition:** 3 or 5 models, picked how? Smallest credible set that runs on a low-end Mac. (Brainstorming open Q5.)
4. **First-run battery exact composition:** task count and selection criterion within SWE-Bench Pro to hit <2h budget on slowest expected installed model. (Brainstorming open Q2; brief-stage shrunk to 3-5 tasks/model.)
5. **SWE-Bench Pro access:** licensing, redistribution rights for the curated bundled subset, stability over v1's lifetime, fallback plan if Pro becomes unavailable or contaminated.
6. **macOS perf-collection mechanism:** `powermetrics` (sudo per-call) vs one-time signed launchd helper vs graceful-degrade-without-power. UX trade vs metric completeness.
7. **Agentic-pass-rate scoring semantics:** pass@1 vs pass@k, per-task wall-clock cap, timeout-as-fail vs timeout-as-skip. Drives every ranking, currently underspecified.
8. **Cache staleness policy on `backend_build` change:** invalidate every cell vs backend-build compatibility classifier (only invalidate when ABI/perf-relevant changes detected).
9. **Run resumability** (Ctrl-C, OOM mid-task, crash recovery): does in-progress measurement persist? Does next `scan` resume or restart?
10. **Per-task wall-clock cap** (DX critic): without this a single wedged SWE-Bench Pro task can blow the budget catastrophically.
11. **`lcrc doctor` pre-flight checks** (DX critic): low disk, missing helpers, llama-server health, expired tokens — should be a v1 surface or a v1.1 add?
12. **`lcrc gc` cache pruning** (DX critic): cells for uninstalled models / old backend builds accumulate over months. Default `lcrc show` to currently-installed only with `--all` for full history?
13. **Default `lcrc` (no args) wizard mode** (DX critic): "You have 2 new GGUFs since last run. Measure them? [Y/n]" — primary entry point, named subcommands for power use?
14. **LM Studio model discovery:** read `~/.cache/lm-studio/models/` in v1 if trivial, or strictly llama.cpp cache only?
15. **Variance reporting beyond CIs:** "indistinguishable cluster" marking deferred to v1.1 — but is multi-run pass-rate (the reliability-under-repeat metric) needed earlier given competitive trends?

## Action Plan (from brainstorming, post-brief)

1. ✅ **Product brief written** (this artifact + this distillate).
2. **PRD creation** — invoke `bmad-create-prd`. Pass both files as input. Resolve open questions 1-15 above (or punt the architectural ones to step 3).
3. **Architecture design** — invoke `bmad-create-architecture`. Redesign around cache-as-product. Resolve open Q2 (storage shape) here.
4. **Archive `docs/legacy/v0.md`** as historical reference (don't delete — useful for traceability; but DO NOT use as input to PRD/architecture).
5. **Validate <2h iteration claim** with concrete arithmetic on Tier 1 budget for a typical 5-model setup, before architecture is locked.
