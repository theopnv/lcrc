---
title: "Product Brief: lcrc"
status: "complete"
created: "2026-04-30"
updated: "2026-04-30"
inputs:
  - _bmad-output/brainstorming/brainstorming-session-2026-04-29.md
  - web research (2026-04-30): NeuralNoise harness-bench, SWE-Bench Pro / Scale SEAL, GuideLLM, Inspect AI status, Apple Silicon MoE shift
---

# Product Brief: lcrc

## Executive Summary

Local LLMs are now genuinely useful for agentic coding — MiniMax M2.5, Qwen3-Coder, GLM-5, DeepSeek V3.2 all clear 70%+ on SWE-Bench Verified, and the April 2026 release of MoE models like Qwen 3.6-35B-A3B runs at 3B-active speed on a $1,799 Mac Mini. The hardware and the model quality finally meet. But picking *which* model to run on *your* machine for *your* tasks is still a guessing game: Reddit lore, hand-rolled blog benchmarks, and "it depends" answers. Existing tools each cover a slice — Inspect AI runs evals but doesn't sweep configurations; Bench360 measures system performance but only on server-class NVIDIA stacks; mini-swe-agent is a harness, not a framework. None of them say "here's what your hardware should run."

**lcrc is an autonomous, personal benchmark database for local LLMs.** It scans the GGUF models you already have installed, runs a small but credible measurement battery against each, and gives you a single static HTML leaderboard: "for agentic coding on this machine, run *this* model with *these* settings." The database — the cache of measurements keyed on `(your machine, model SHA, backend build, params)` — *is* the product. Re-running tomorrow with a new model installed only measures what's new or stale. Incremental scans complete in under two hours; first scans depend on how many models are installed (plan for an overnight run if you have a full collection). v1 is built for one person (the author) running it on their own Mac. If it's useful to anyone else, that's a v1.1 conversation.

## The Problem

A local-LLM user today, picking a model for agentic coding, faces this loop:

1. Read three Reddit threads claiming different "best" models for their setup.
2. Pull a 14GB GGUF, discover their RAM-to-context-length math was wrong, watch the model swap to disk and crawl.
3. Try a different quantization. Discover the harness they're using doesn't enforce JSON schema and tool calls fail silently. Try a different backend. Discover Ollama unloaded the model mid-session.
4. Eyeball some output, conclude "it works I guess," default to it for a week, suspect a newer model would be better, return to step 1.

The community knows this is broken. Multiple 2026 benchmark authors migrated from Ollama to llama-swap specifically because of unattended-serving failures. r/LocalLLaMA is full of "Q8 vs Q4 confusion," "Gemma 4 enters repetition loops after ~11 tool calls," "model loaded fine but stuck in repetitive loops on agentic tasks." SWE-Bench Verified is now considered tainted (training-data leakage; OpenAI stopped publishing scores). Public leaderboards report numbers from someone else's hardware running someone else's harness on a tainted task set — useful as folklore, not as a "what should I run on my Mac" answer.

The cost of the status quo is hours-per-week of guesswork that produces no durable answer. You don't know if your default model is good. You don't know if last month's new release would be better. You re-do the comparison from scratch when something changes.

## The Solution

A CLI tool. One command, `lcrc scan`, on a fresh machine:

- Detects your installed GGUF models (reads llama.cpp's local cache).
- Filters out models that won't fit your RAM × context budget.
- For each remaining model, runs a **two-tier measurement battery**:
  - **Tier 1 (first-run, always):** ~3-5 SWE-Bench Pro tasks at one default quantization per model, scored by mini-swe-agent for agentic pass-rate; macOS-native perf metrics for tok/s, ttft, peak RSS, power. Sized so a typical 5-model scan finishes in roughly two hours (5 models × 5 tasks × ~5 min/task ≈ 125 min).
  - **Tier 2 (`lcrc deepen`, opt-in):** the full SWE-Bench Pro subset, expanded perf sampling, and additional quant/ctx variants. Plan for an overnight run.
- Writes results to a cache keyed on `(machine_fingerprint, model_sha, backend_build, params)`, where:
  - `machine_fingerprint` = chip generation + RAM size + GPU core count (durable across OS patch-level upgrades).
  - `params` = ctx length, sampler temperature, threads, `n_gpu_layers`. Everything else pinned to defaults in v1.
- Emits a static HTML report: a single self-contained file, opens in your browser, shows a leaderboard per task type with per-cell metrics inline, including "cached on `<date>` with `<backend_version>`."

The autonomy is in *what* gets measured, not *when*. Re-run after pulling a new model: only that model is measured. Re-run after a llama.cpp upgrade: the affected cells re-measure, the rest are reused. `lcrc deepen` opts into the full task set for users who want it; `lcrc verify` re-measures a sample of cached cells to catch drift. Every scan also re-runs a fixed **canary task** (one task with a known-good baseline, ~1 minute) so the report header can tell you up front: "infrastructure healthy — trust this run" or "canary failed — something in the harness or backend changed, treat this run with suspicion." After the first scan, every subsequent incremental scan is well under two hours.

The reports are also a teaching surface. Each cell carries **templated badges** explaining its rank — `ctx-limited`, `OOM-at-N`, `repetition-loop`, `tool-call-format-failure`, `thermal-throttled`, `low-confidence-CI` — so the user gets calibrated about what to even ask for over time. Becoming better at specifying configurations is co-equal value with the recommendation itself.

Three smaller benefits fall out of the cache architecture almost for free, worth naming as bonuses (not the headline):

- **Hardware-purchase decision support.** Because every cell is keyed on a real machine fingerprint, "what does this model actually do on an M2 Pro 32GB?" is now an answerable question rather than a YouTube video. Before spending $1,800-$5,000 on a Mac, you can see real numbers from comparable hardware.
- **Backend-regression detection.** The cache key includes `backend_build`, so lcrc naturally surfaces "llama.cpp build N+3 made model X 12% slower" — a recurring pain point with no current observability story.
- **Personal config archaeology.** The cache is longitudinal, not a snapshot. Six months in, you can ask "what was I running in February and why did I move off it?" — a journal of your own local-LLM practice.

## What Makes This Different

| Existing tool | What it does | What it's missing |
|---|---|---|
| **Inspect AI** | 200+ pre-built evals, agentic eval framework | No sweep / no perf orchestration; evaluates one config at a time |
| **Bench360** | System performance benchmarks | Server-class NVIDIA only; no Mac, no Ollama/llama.cpp, no agentic harness |
| **mini-swe-agent** | Minimal bash agentic harness | Just a harness; not a framework, not a reporter |
| **GuideLLM (Red Hat)** | RPS/concurrency sweep on OpenAI-compatible endpoints | No agentic tasks, no model-selection output |
| **NeuralNoise `harness-bench`** (April 28, 2026) | Manual 17-model × 5-harness × 16-task sweep on M3 Max | Single-author research artifact: hand-curated blog tables, no autonomy, no caching, no extensibility, dense and expert-only |

The novel contribution is the **combination**: autonomous orchestration (only measures what's missing or stale) + a cache-as-first-class artifact (your machine's measurements, durable across runs) + opinionated beginner-friendly defaults (one harness, one task source, one report — no "pick your eval framework" config screen). NeuralNoise proved harness-as-axis matters; lcrc is the productized form of that idea — and approachable for someone whose first interaction is `brew install lcrc && lcrc scan`, not reading a 4,000-word blog post.

The defensibility is **execution and UX**, not technical moat. Concretely: **one command produces output**, **no configuration screen**, **a single self-contained HTML file as the artifact**, **one harness and one task source** in v1. Inspect AI could plausibly add sweep orchestration in a 6-12 month window; the bet is that a Mac-first, opinionated, batteries-included tool wins the local tinkerer audience before the eval-framework incumbent does.

## Who This Serves

**Primary user (v1): the local-LLM tinkerer who runs models on their own Mac.** They have 4-20 GGUFs in `~/.cache/llama.cpp/`, they read r/LocalLLaMA, they have an opinion about Q4_K_M vs Q5_K_M, and they currently pick their daily-driver model by gut. They want to type one command and get a defensible "use this one for coding" answer scoped to *their* hardware. They will accept a CLI; they will not accept a configuration screen.

**Concretely:** the author is building this for himself. v1 success is "I trust the leaderboard enough to switch defaults based on what it says."

**Secondary user (acknowledged, deferred to v1.1+): the engineering lead picking a local stack for their team.** They want to codify their team's quality bar with custom evals reflecting their actual codebase, and re-run as new models drop. v1 does not serve this user — there is no custom-eval extension surface in v1. The brief acknowledges this user exists; the architecture should not paint itself into a corner that would block them in v1.1.

## Success Criteria

v1 is successful when **the author runs `lcrc scan` on their own Mac monthly, and the leaderboard is trustworthy enough to drive an actual default-model switch.**

Concrete signals:

- **Measurement loop works:** First run completes (any reasonable subset of installed models). Subsequent runs after a new model install measure only the new cell and complete in under two hours.
- **Cache is durable:** `lcrc verify` returns "no drift" on a sample of cached cells across at least two re-runs separated by a llama.cpp upgrade. The per-scan canary task passes consistently across runs (a failed canary triggers investigation rather than silent leaderboard updates).
- **Output is decision-grade:** At least one default-model switch happens because of what lcrc said, not in spite of it. (e.g., "I was running model X; lcrc shows model Y is meaningfully better for my coding tasks; I switched.")
- **Honesty holds:** Every leaderboard row displays (a) a Wilson-score confidence interval on the pass-rate, (b) the cache age and `backend_build` the cell was measured under, and (c) any failure-mode badges seen during the run. The v1 acceptance check includes opening a real report and visually confirming all three are present.

External adoption (GitHub stars, blog citations, community pickup) is **not** a v1 gate. If it happens organically that's information for v1.1; it is not the goal.

## v1 Scope

**In:**
- macOS Apple Silicon only.
- llama.cpp / `llama-server` only (single backend).
- One harness: mini_bash, wrapping mini-swe-agent.
- One task source: curated SWE-Bench Pro subset (~3-5 tasks per model, one default quant, on first-run; full set + multi-quant on `lcrc deepen`).
- macOS-native perf collection: tok/s, ttft, peak RSS, power, thermal.
- Cache keyed on `(machine_fingerprint, model_sha, backend_build, params)`. Cache lookup before measure.
- Commands: `lcrc scan`, `lcrc show`, `lcrc deepen`, `lcrc verify`.
- **Per-scan canary task** with a fixed known-good baseline: ~1 min, runs every `scan`, results displayed prominently in the report header. Detects infrastructure drift (harness, backend, OS) separately from model behavior change.
- **Wilson confidence intervals** on every leaderboard pass-rate (already noted in Success Criteria — listed here for scope completeness).
- Static self-contained HTML report; CLI ranking views. The HTML is **screenshot-friendly by default**: canonical machine-fingerprint header, scan date, lcrc version, and `backend_build` rendered prominently so a screenshot pasted into a Reddit/Discord thread is self-attesting.
- Empty-machine UX: text explainer + hardcoded ~3-5 small starter-model suggestions with exact `ollama pull` / download commands. No recommendation engine, no curation.

**Out (deferred to v1.1+ or v2):**
- Linux NVIDIA, Windows.
- Ollama (via llama-swap), vLLM, LM Studio backends.
- OpenCode and any second harness; **harness-as-axis comparison** (the docs/legacy/v0.md headline) is explicitly v2.
- Curated catalog with auto-discovery.
- Custom eval extension surface (the eng-lead persona feature).
- Pareto fronts and weighted-profile recommenders; user-tunable scoring.
- FastAPI server, React SPA, live progress UI.
- Background daemon + native notifications. ("Autonomous" in v1 means *what* gets measured is delta-aware; *when* is still CLI-triggered. README must be honest about this.)
- Inspect AI integration / interop.
- Community-shared benchmark dataset.

## Vision (12-24 months)

If v1 is genuinely useful to one person, the path is:

- **v1.1** — Linux NVIDIA tier-1 support. Ollama (via llama-swap) as a second backend. Background daemon + native notifications so re-measurement triggers automatically when the catalog changes (closes the F5 honesty gap).
- **v2** — Harness-as-axis returns: OpenCode, Aider, Claude Code as harness alternatives, with the apples-to-apples comparison nobody else does productively. Custom-eval extension surface unlocks the engineering-lead persona — teams codify their own task sets and run lcrc as a "what should our team run" decision support tool. Inspect AI interop for users on that stack. Pareto-front view for users who want to tune the score weighting.
- **v3** — Community-shared benchmark dataset: opt-in upload of `(machine_fingerprint, model, score, perf)` rows so users without the hardware can preview "what does this model do on a setup like mine?" before downloading. The personal database becomes a federated reference.

The shape that wins is the same shape across all three: **autonomous orchestration filling a structured cache that's the actual product**, with opinionated defaults that make the first run trivial. Everything past v1 is breadth, not a different thesis.

## Open Questions for the PRD

1. `lcrc verify` behavior: warn on drift vs. auto-invalidate? (Working assumption: warn; user opts in to re-measure.)
2. Cache storage shape: SQLite + JSON blobs (per docs/legacy/v0.md) or a flat JSON-per-cell layout? (Architecture decision.)
3. Hardcoded starter pack composition: 3 or 5 models, picked how? (Smallest credible set that runs on a low-end Mac.)
4. First-run battery: exact task count and selection criterion within SWE-Bench Pro.
5. SWE-Bench Pro access: licensing, redistribution, and stability of the curated subset — and the fallback plan if Pro becomes unavailable or visibly contaminated within v1's lifetime.
6. macOS perf-collection mechanism: `powermetrics` (sudo, per-call) vs. a one-time signed launchd helper vs. graceful-degrade-without-power. Trade is between first-run UX (no sudo prompts) and metric completeness.
7. Agentic-pass-rate scoring: pass@1 vs. pass@k semantics, per-task wall-clock cap, timeout-as-fail vs. timeout-as-skip. Drives every ranking, currently underspecified.
8. Cache staleness policy on `backend_build` change: invalidate every cell, or use a backend-build compatibility classifier (only invalidate when ABI/perf-relevant changes are detected)?
