---
stepsCompleted: [1, 2, 3, 4]
inputDocuments: ['docs/legacy/v0.md']
session_topic: 'lcrc — local agentic-LLM benchmarking framework: surface alternate framings, adjacent problems, and unstated assumptions before commitment'
session_goals: 'Stress-test the v1 plan from first principles; expose baked assumptions; explore alternate problem framings and adjacent problems worth pursuing instead'
selected_approach: 'ai-recommended'
techniques_used: ['first_principles', 'assumption_reversal', 'five_whys_reframe', 'pre_mortem']
ideas_generated: ['reframe_to_autonomous_personal_benchmark_db', 'rename_sweep_to_run', 'cut_catalog_for_v1', 'cut_pareto_for_v1', 'cut_server_and_spa_for_v1', 'cut_multibackend_for_v1', 'cut_multiplatform_for_v1', 'keep_one_harness_minibash', 'add_lcrc_verify_command', 'add_two_tier_battery_first_run_then_deepen', 'add_hardcoded_starter_pack_for_empty_machine', 'cache_as_first_class_keyed_on_machine_model_backend_params']
context_file: 'docs/legacy/v0.md'
focus_layers: ['premise', 'framing', 'mechanism']
primary_output: 'a reframe — alternate ways to describe what lcrc actually is'
session_active: false
workflow_completed: true
recommended_next_skill: 'bmad-product-brief'
---

# Brainstorming Session Results

**Facilitator:** Theop
**Date:** 2026-04-29

## Session Overview

**Topic:** lcrc — local agentic-LLM benchmarking framework
**Goals:** Take the project from the start. Surface alternate framings, adjacent problems, and assumptions baked into docs/legacy/v0.md without the user noticing.

### Context Guidance

docs/legacy/v0.md is a detailed v1 design doc for a Python framework that autonomously sweeps local agentic LLMs across (model × backend × harness × quant × KV cache × context) and produces a Pareto-optimal recommendation report. Already-resolved design decisions: tiered dropout sweeps, SQLite + JSON, FastAPI + React SPA, Inspect AI as the eval engine, llama.cpp + Ollama as v1 backends, OpenCode + a built-in mini-bash as v1 harnesses, baked use-case profiles for recommendations.

The user explicitly wants to be challenged on this — not validated. Goal is generative exploration, not implementation planning.

### Session Setup

**Approach:** AI-Recommended sequence
**Layers to attack:** premise, framing, mechanism (all three)
**Primary output target:** a reframe — one or two alternate ways to describe what lcrc actually is

**Technique sequence:**

1. **First Principles** — strip the premise to atomic claims and examine which are actually load-bearing truth vs. inherited habit
2. **Assumption Reversal** — flip each baked design decision and ask "what if the opposite were true / better?"
3. **Five Whys → Reframe** — chase the user's *real* job-to-be-done; surface candidate reframes
4. **Pre-mortem** — fast-forward 12 months: lcrc shipped and failed. Why?

---

## Decisions locked in during this session

- **"sweep" → "run"** everywhere. A run is composed of cells; cells contain trials.
- **One product, two faces** (beginner + expert). Beginner is assumed CS-literate (TOML-OK). Beginner UX = preset; expert UX = full config.
- **R1 accepted (soft form):** predict-skip cells via the hardware-fit gate + use the cache as a first-class predictor. Don't measure what we already know or what won't fit. (Radical R1 — predicting *quality* without running — needs community data and is deferred.)
- **R3 rejected:** the catalog is necessary. Users don't know what to download. The catalog is a *discovery* surface, not a list of "approved" models.
- **R4 accepted:** no harness in v1. Measure the model (perf + completion-quality only). Harness comparison = v2.
- **R5 accepted:** drop Pareto front. Single weighted score per task; leaderboard, not Pareto chart.
- **R6 accepted:** no FastAPI server, no React SPA in v1. CLI + static HTML report opened in browser.
- **Iteration loop target: < 2 hours** (hard constraint after the first run).
- **Reports also educate** — the user gets better at specifying configs over time. This is co-equal value with the recommendation itself.

## v1 shape after these cuts

> *A CLI tool. Maintains a curated + auto-discoverable catalog of local LLMs. On a fresh machine, scans hardware, runs a measurement battery (perf + completion quality) on a fit-filtered subset, caches results keyed on `(machine_fp, model_sha, backend_build, params)`, and emits a static HTML report. On rerun: only measures cells not in the cache or whose inputs changed (e.g., new models in the catalog). No harness, no server, no SPA, no Pareto. < 2h iteration loop after first run.*

Milestone count drops from ~24 to ~10–12. M17–M22 (server + WS + frontend) gone. M10–M11 (harnesses) deferred. M15 (Pareto) → simpler ranker.

---

## Technique 1: First Principles (results)

10 atomic claims surfaced; 4 reds, 4 yellows, 2 greens. Reds (#4 sweep-as-only-shape, #6 stable recommendation, #7 user trust, #10 hardware-cost-of-sweep) all got reframed or de-burdened in user responses.

## Technique 2: Assumption Reversal (results)

Six reversals proposed. R1 (soft form), R4, R5, R6 accepted. R3 came back as **modified-accept** after the catalog-is-discovery probe: **no curated catalog in v1**. lcrc reads what the user already has installed (`~/.ollama/models`, `~/.cache/llama.cpp/...`). The empty-machine UX is a text explainer + a few starter-model suggestions. Catalog + auto-discovery moves to a v2 idea.

This keeps lcrc a **pure measurement framework**. It does not curate, recommend-to-install, or download.

## Technique 3: Five Whys → Reframe (results)

### Reframe (the headline output of this session)

> **lcrc = an autonomous, personal benchmark database for local LLMs.**
>
> The *database* is the artifact: a record of what your hardware can do, scoped to your installed models, kept current without you babysitting it.
>
> *Autonomous orchestration* is the mechanism: you point it at your machine, it figures out which experiments to run, runs them in the background, and tells you when there's something new to look at.
>
> Both are co-equal. The cache without autonomy is "another results JSON." Autonomy without the cache is "another benchmark runner." lcrc is the pair.

### Vocabulary shift

| docs/legacy/v0.md word | Now |
|---|---|
| "sweep" | "run" |
| "recommendation report" | "measurement report" / "your top picks for `<task>`" view |
| "Pareto-optimal cell" | "leaderboard row" |
| "tiered dropout" | "fill the cache efficiently" (same algorithm) |
| "weekly profile" | "delta run" — measure what's missing |
| `lcrc discover` | `lcrc scan` |
| `lcrc report` | `lcrc show` |

## v1 shape after Technique 2 (revised)

> *A CLI tool. Detects local model installations. Runs a measurement battery (perf + completion quality) on installed models that fit the user's hardware. Caches results keyed on `(machine_fp, model_sha, backend_build, params)`. Emits a static HTML report. Empty-machine: explainer + suggested models to pull. New install detected on rerun → measure only that. < 2h iteration loop.*

**Modules cut from docs/legacy/v0.md:** `catalog/` (M6, M7), harness layer (M10, M11, plus harness-aware bits of M8/M9), reporting/Pareto (M15 → simpler ranker), server (M17, M18), frontend (M19–M22), packaging-with-SPA bits of M24. v1 milestone count: ~10 instead of 24.

**Modules added or reframed:**
- **Installed-model inspector** (new): scans Ollama/llama.cpp caches; computes model SHA for cache-keying.
- **Cache-as-first-class** (was: storage as log): lookup before measure; results survive machine FP, backend build, model SHA changes.
- **Static HTML report** (was: server + SPA): single self-contained HTML with embedded charts. Opens locally.

## Technique 4: Pre-mortem (results — locked defuses)

| ID | Risk | Defuse — committed for v1 |
|---|---|---|
| **F1** | Capability scores ≠ agentic usefulness | **Keep one minimal harness in v1.** mini_bash style; possibly wrapping [SWE-agent/mini-swe-agent](https://github.com/SWE-agent/mini-swe-agent) as upstream. Provides at least one source of agentic signal so the leaderboard isn't pure-completion-only. |
| **F2** | Cache invalidation eats trust | **`lcrc verify` command.** Re-runs N% of cached cells, reports drift. Cache key includes `(machine_fp, model_sha, backend_build, params)`. Reports show "cached on `<date>` with `<backend_version>`" inline. |
| **F3** | First run is too long, breaks <2h promise | **Two-tier battery: first-run + deepen.** First-run = small problem set, single quant/ctx, fast partial database. `lcrc deepen` adds the rest later for users who want it. |
| **F6** | Empty-machine UX feels like an error | **Hardcoded starter pack** (3–5 very small models, e.g. ~0.8B class, runs on any hardware). Print exact `ollama pull` / download commands. Not a recommendation engine — just a fixed list. |
| **F7** | Cross-platform doubles maintenance | **macOS Apple Silicon only for v1.** Linux NVIDIA = v1.1. |
| **F8** | Backend lifecycle bugs (Ollama unloading, llama-swap config) | **llama.cpp via llama-server only** for v1. Ollama = v1.1. |

### Pre-mortem risks acknowledged but not actively defused in v1

- **F4** (capability eval CI variance flips the leaderboard between runs) — accept for v1; revisit if it bites. Could mitigate later by displaying confidence intervals + "stable rankings" view.
- **F5** (static HTML report not reopened; autonomy is bounded by user invocation) — accept that v1 autonomy is in *what gets measured* (delta-aware), not *when* (still CLI-triggered). A background daemon + native notifications is v1.1+. Worth being honest about this in the v1 README so the "autonomous" framing isn't oversold.

## Final v1 scope (consolidated)

> **lcrc v1 = autonomous-but-CLI-invoked personal benchmark database.**
>
> Reads installed GGUF models from local llama.cpp cache. macOS Apple Silicon only. llama.cpp/llama-server only. One harness: mini_bash (possibly wrapping mini-swe-agent). Two-tier measurement battery (first-run = fast partial, `lcrc deepen` = full). Cache keyed on `(machine_fp, model_sha, backend_build, params)`. `lcrc verify` re-checks drift. `lcrc scan` fills gaps for newly installed models. Empty-machine: hardcoded starter-pack hint with `ollama pull` commands. Output: static HTML report opened locally; CLI ranking views.
>
> **Out of v1, on the v2/future-ideas list:** Ollama (via llama-swap) and other backends; OpenCode and other harnesses; Linux NVIDIA + Windows; harness × model comparison; curated catalog with auto-discovery; community-shared benchmark dataset; daemon + notifications; live progress UI / FastAPI server / React SPA; Pareto fronts and weighted-profile recommendations; user-weighted recommendation sliders.

## Session output summary

- **Reframe:** lcrc is an autonomous, personal benchmark database for local LLMs. The cache *is* the product; autonomous orchestration is the mechanism that fills it.
- **Vocabulary:** "sweep" → "run"; "Pareto-optimal cell" → "leaderboard row"; commands cleaned (`scan`, `show`, `deepen`, `verify`).
- **Cuts:** catalog module, harness layer (mostly), Pareto reporter, FastAPI server, React SPA, multi-backend, multi-platform — all deferred. Milestone count: ~10 vs original ~24.
- **Defuses committed:** F1 (one harness), F2 (verify command), F3 (two-tier battery), F6 (starter pack), F7 (single platform), F8 (single backend).
- **Risks acknowledged:** F4 (eval variance) and F5 (bounded autonomy) — accept, revisit later.

## Open questions for the next round (not for this session)

1. Does mini_bash get implemented internally, or do we wrap [mini-swe-agent](https://github.com/SWE-agent/mini-swe-agent) as a subprocess?
2. What exactly is "first-run battery" — how few problems is too few to be useful?
3. Does `lcrc verify` invalidate on drift, or just warn? (Probably warn; user opts in to re-measure.)
4. What's the actual storage shape of the cache — still SQLite + JSON blobs as in docs/legacy/v0.md, or simpler (a JSON file per cell)?
5. Is the hardcoded starter pack 3 or 5 models? Picked how?

---

## Idea Organization and Prioritization

### Thematic Organization

**Theme 1 — The Reframe (headline output)**
- lcrc = autonomous, personal benchmark database for local LLMs
- The cache *is* the product; autonomous orchestration is the mechanism that fills it
- Vocabulary cleanup: "sweep" → "run"; Pareto → leaderboard; commands → `scan`/`show`/`deepen`/`verify`

**Theme 2 — Aggressive scope cuts for v1**
- No catalog (read installed GGUFs only)
- No multi-backend (llama.cpp only)
- No multi-platform (macOS Apple Silicon only)
- No second harness (mini_bash only)
- No Pareto / weighted-profile recommender (single-score leaderboard)
- No FastAPI server, no React SPA (static HTML report)

**Theme 3 — Risk defuses baked into v1**
- F1 → keep mini_bash so there's at least one source of agentic signal
- F2 → `lcrc verify` command to re-check cached cells for drift
- F3 → two-tier battery (fast first-run + opt-in `lcrc deepen`)
- F6 → hardcoded ~3–5-model starter pack (very small models, runs anywhere) for the empty-machine UX

**Theme 4 — Risks acknowledged but accepted for v1**
- F4 (eval CI variance flips leaderboard between runs) — revisit later
- F5 (no daemon → "autonomous" is bounded by user invocation) — be honest in README; daemon = v1.1+

**Theme 5 — Future versions (deferred, not lost)**
- v1.1: Ollama (via llama-swap), Linux NVIDIA, daemon + native notifications
- v2: OpenCode and other harnesses, harness × model comparison, curated catalog with auto-discovery, weighted-profile recommender, FastAPI + SPA
- v3: community-shared benchmark dataset

### Prioritization

**Top priority for v1 commitment (do these next, in order):**

1. **Write a Product Brief** that locks in the reframe (Theme 1) and the v1 scope (Themes 2 + 3). This is the immediate next BMad artifact.
2. **Decide the 5 open questions** listed above before architecture work begins.
3. **Validate the < 2h iteration claim with a back-of-napkin time budget** for the first-run battery (Theme 3, F3) — make sure the design target is achievable with the cuts.

**Quick wins (lightweight follow-ups):**

- Pick the 3–5 starter-pack models (Theme 3, F6) and validate they actually run on a low-end Mac.
- Pick mini_bash internal-impl vs. mini-swe-agent wrapper (Open Q1).

**Breakthrough concepts to develop further:**

- The cache as a first-class predictor (soft-R1) — only measure cells we don't already have. This is what makes the < 2h target plausible and is the core differentiator vs. "another benchmark CLI."
- The reframe of autonomy as "what to measure" (delta-aware) rather than "when to invoke" (daemon-driven). v1 is honest about being CLI-invoked; the autonomy story is real but bounded.

### Action Plan — immediate next steps

| # | Action | Owner | Dependency |
|---|---|---|---|
| 1 | Invoke `bmad-product-brief` skill with this brainstorming doc as input. Lock the reframe + v1 scope into a product brief. | Theop | this session |
| 2 | Resolve open questions 1–5 during product-brief creation (or punt to PRD). | Theop | (1) |
| 3 | After product brief: invoke `bmad-create-prd` for binding requirements. | Theop | (1) |
| 4 | After PRD: invoke `bmad-create-architecture` to redesign around the cache-as-product. | Theop | (3) |
| 5 | Archive `docs/legacy/v0.md` as a reference doc (don't delete — useful for traceability). | Theop | — |

## Session Summary and Insights

### Key Achievements

- Surfaced a **reframe** of lcrc from "benchmark sweep tool that recommends" → "**autonomous personal benchmark database that fills its own gaps**." The cache became the product; orchestration became the mechanism.
- Cut **~14 of ~24 milestones** from docs/legacy/v0.md by deferring catalog, second harness, second backend, second platform, Pareto, server, and SPA.
- Killed jargon-debt: **"sweep" → "run"** across all CLI/schema/docs.
- Identified **6 specific risks** via pre-mortem and committed defuses for the 4 most damaging.
- Acknowledged 2 risks honestly without trying to solve them in v1 (capability variance, autonomy-bounded-by-user-invocation).

### Session Reflections

- The user's most productive moments were the explicit "this word confuses me" callouts (e.g., "sweep") and the "I haven't sized the cost of a bad guess" admissions. Those are the seams where assumptions live.
- docs/legacy/v0.md was a hybrid product-brief + design-doc + milestone-plan. The brainstorming exposed that the *product brief* layer was the weakest — the design and milestones were over-engineered for an underspecified product. The BMad workflow's discipline (brief → PRD → architecture) would have caught this earlier.
- The "two faces, one product" archetype decision avoided a UX-fork trap that docs/legacy/v0.md was sliding toward without naming it.

### Next BMad skill to invoke

**`bmad-product-brief`** — pass this session document as the input artifact. The brief should lock the reframe (Theme 1), the v1 scope cuts (Theme 2), and the committed defuses (Theme 3). Open questions and future-version ideas (Themes 4–5) belong in the brief's "out of scope" / "future considerations" sections.

After the product brief: `bmad-create-prd`, then `bmad-create-architecture`. The original docs/legacy/v0.md becomes a reference / historical artifact, not the basis for further design.






