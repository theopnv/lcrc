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

## Deferred from: code review of 1-7-sqlite-schema-migrations-framework (2026-05-07)

- **Map `SQLITE_NOTADB` (path points at an existing non-SQLite file) to a dedicated `CacheError::CorruptDb` variant.** Currently `Connection::open` failures of this kind surface as a generic `CacheError::Open` whose `source` is `rusqlite::Error::SqliteFailure(SQLITE_NOTADB, ...)`. UX-side mapping decision belongs to Story 1.12 (CLI wiring of `Error::Preflight` → `ExitCode::PreflightFailed = 11`); pre-defining the variant here would create dead surface area until that consumer exists.
- **Distinguish a manually-poisoned negative or out-of-range on-disk `user_version` from a generic PRAGMA failure.** `read_user_version` (`src/cache/migrations.rs:135-138`) reads as `u32` directly; a negative value (manual edit, FS corruption) becomes `rusqlite::Error::InvalidColumnType` wrapped in `CacheError::Pragma`. A defensive `read i64 → validate non-negative → cast` path would emit a clearer diagnostic, but the corruption-recovery UX is out of v1 scope and arguably belongs in a future hardening story alongside `lcrc verify` (Epic 5).

## Deferred from: code review of 1-6-cache-key-helpers-in-src-cache-key-rs (2026-05-06)

- **No `Params::temp.is_finite()` validation in `params_hash`.** Non-finite `temp` values (NaN, ±∞) produce `KeyError::ParamsHashSerialize` (now correctly documented). Pre-validation would let consumers surface a clearer "invalid temp" diagnostic rather than a serializer error. Defer to Story 1.8 (the first consumer that decides UX policy on bad `Params`).
- **No DoS-guard on `model_sha`** (regular-file check, max-size cap, or `tokio::time::timeout`): hashing `/dev/zero`, a FIFO, or a FUSE-hung mount blocks indefinitely. Spec is explicit that source-side validation owns this; the orchestrator (Story 2.13) is the right wiring point.
- **`backend_build` separator collisions** — `name`/`semver`/`commit_short` containing `-` or `+` produce ambiguous output. Spec defers validation to `Backend::version()` (Story 2.1).
- **Additional `model_sha` error-path tests** (EISDIR, EACCES, mid-stream EIO, symlink-to-missing-target): single Display-substring assertion is the contract today; per-errno tests would belt-and-suspenders the variant carrying the symlink path vs. the resolved path. Low value vs. deferring to integration.
- **`serde_json/preserve_order` feature-unification static guard.** A future transitive enabling `preserve_order` would silently flip `Map` from `BTreeMap` to `IndexMap` and break canonical encoding. Pinned-digest test catches it with a generic message; an explicit build-time `cfg!`-static-assert would point future maintainers at the actual cause.
- **`#[non_exhaustive]` on `Params` / `BackendInfo` / `KeyError`.** Adding a field to `Params` silently invalidates every cached cell — pinned-digest test catches it generically. Schema-versioning / API-versioning policy is owned by Story 1.8 + NFR-R3 work, not by the primitive author.
- **`BufReader::with_capacity(64*1024, file)` vs. wrapping in default-8 KiB BufReader and reading into a 64 KiB user buffer.** Micro-perf, not correctness; streaming-vs-bulk equivalence test pins behavior. Pick up in a `bmad-quick-dev` pass if any future profiling flags it.

## Deferred from: code review of 1-5-machine-fingerprint-module (2026-05-06)

- **Multiple `gpu-core-count` lines on multi-AGX hosts (Mac Pro Ultra) silently picks first.** `parse_gpu_cores_from_ioreg` (`src/machine/apple_silicon.rs:122`) returns the first line that matches the quoted ioreg key. On a Mac Pro / Ultra with multiple IOAccelerator nodes, the wrong number could end up in the canonical fingerprint. Needs investigation on real hardware before a fix — first-match might already be the canonical SoC value depending on ioreg traversal order. If it isn't, the fix is to scan all matches and either de-dup or take max.
- **No timeout on subprocess execs in `run_capture`.** A hung `sysctl` / `ioreg` (e.g. fs-stalled binary, locked IORegistry) would hang `MachineFingerprint::detect()` indefinitely. `tokio::time::timeout` is already in the locked feature set; defensive wrapper is one helper away. Defensive only — these binaries do not hang in practice.
- **Boundary-input test gaps in `apple_silicon::tests`** (BOM, NBSP, embedded `\n`, leading `+`, `u64::MAX`+1, two `gpu-core-count` lines, substring-collision on a key whose name contains `gpu-core-count`). Production parsers reject these correctly today; tests would pin behavior against future regressions. Cheap to add when the next maintenance pass touches the file.

## Deferred from: story 1.2 (2026-05-05)

- **Bump `actions/checkout@v4` → `@v5` (or current Node.js 24 major) in `.github/workflows/ci.yml`.** Hard deadline: **2026-09-16** (Node.js 20 removed from GitHub-hosted runner images). Soft deadline: **2026-06-02** (forced Node.js 24 default; `@v4` may still run via `ACTIONS_ALLOW_USE_UNSECURE_NODE_VERSION=true` opt-out, but that is a smell). Triggered by deprecation warning emitted on cold-cache run `25380632500`. Out of Story 1.2 scope because the story spec pins `@v4`. ~5-line PR; no code or test impact expected.
