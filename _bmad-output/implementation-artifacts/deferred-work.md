# Deferred Work

Backlog of small, out-of-scope, or time-bound items discovered during story
implementation that are not story-sized on their own. Format follows BMad's
`bmad-code-review` / `bmad-quick-dev` convention: one `## Deferred from: <source> (<date>)`
heading per source, one bullet per item.

Items here should be picked up via `bmad-quick-dev` (small one-off changes) or
folded into the next relevant story / epic retrospective. Items with hard
external deadlines should additionally be tracked as GitHub issues with a
milestone to make the deadline enforceable outside BMad context.

---

## Deferred from: code review of 1-5-machine-fingerprint-module (2026-05-06)

- **Multiple `gpu-core-count` lines on multi-AGX hosts (Mac Pro Ultra) silently picks first.** `parse_gpu_cores_from_ioreg` (`src/machine/apple_silicon.rs:122`) returns the first line that matches the quoted ioreg key. On a Mac Pro / Ultra with multiple IOAccelerator nodes, the wrong number could end up in the canonical fingerprint. Needs investigation on real hardware before a fix — first-match might already be the canonical SoC value depending on ioreg traversal order. If it isn't, the fix is to scan all matches and either de-dup or take max.
- **No timeout on subprocess execs in `run_capture`.** A hung `sysctl` / `ioreg` (e.g. fs-stalled binary, locked IORegistry) would hang `MachineFingerprint::detect()` indefinitely. `tokio::time::timeout` is already in the locked feature set; defensive wrapper is one helper away. Defensive only — these binaries do not hang in practice.
- **Boundary-input test gaps in `apple_silicon::tests`** (BOM, NBSP, embedded `\n`, leading `+`, `u64::MAX`+1, two `gpu-core-count` lines, substring-collision on a key whose name contains `gpu-core-count`). Production parsers reject these correctly today; tests would pin behavior against future regressions. Cheap to add when the next maintenance pass touches the file.

## Deferred from: story 1.2 (2026-05-05)

- **Bump `actions/checkout@v4` → `@v5` (or current Node.js 24 major) in `.github/workflows/ci.yml`.** Hard deadline: **2026-09-16** (Node.js 20 removed from GitHub-hosted runner images). Soft deadline: **2026-06-02** (forced Node.js 24 default; `@v4` may still run via `ACTIONS_ALLOW_USE_UNSECURE_NODE_VERSION=true` opt-out, but that is a smell). Triggered by deprecation warning emitted on cold-cache run `25380632500`. Out of Story 1.2 scope because the story spec pins `@v4`. ~5-line PR; no code or test impact expected.
