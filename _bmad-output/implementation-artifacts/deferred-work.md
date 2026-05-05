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

## Deferred from: story 1.2 (2026-05-05)

- **Bump `actions/checkout@v4` → `@v5` (or current Node.js 24 major) in `.github/workflows/ci.yml`.** Hard deadline: **2026-09-16** (Node.js 20 removed from GitHub-hosted runner images). Soft deadline: **2026-06-02** (forced Node.js 24 default; `@v4` may still run via `ACTIONS_ALLOW_USE_UNSECURE_NODE_VERSION=true` opt-out, but that is a smell). Triggered by deprecation warning emitted on cold-cache run `25380632500`. Out of Story 1.2 scope because the story spec pins `@v4`. ~5-line PR; no code or test impact expected.
