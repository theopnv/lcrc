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

## Deferred from: code review of 1-11-llama-server-lifecycle (2026-05-07)

- **TOCTOU race between `allocate_free_port` listener drop and llama-server bind.** `allocate_free_port` binds to port 0, records the port, then drops the listener before llama-server binds. Another process (or concurrent call) can claim that port in the window. Acknowledged in dev notes as an accepted trade-off; fixing it would require passing the bound fd to llama-server (not supported by its CLI) or a retry loop in `start()`.
- **Synchronous `std::thread::sleep(500ms)` in `ServerHandle::Drop` stalls the async executor.** In `current_thread` flavor (the test runtime), the 500ms blocking sleep freezes all other futures. Accepted in spec: async `Drop` is impossible in Rust, and llama-server needs a brief window after SIGTERM to flush in-flight writes. Revisit if executor stall becomes observable in production (multi-threaded runtime steals one worker thread per in-flight drop).

## Deferred from: code review of 1-10-sandbox-run-task-with-workspace-mount-custom-default-deny-network (2026-05-07)

- **nft `ip filter FORWARD` chain existence not checked before rule installation.** `install_port_pin_rules` runs `nft add rule ip filter FORWARD ...` without first verifying the table/chain exists. On a fresh or non-standard Podman VM, the command may fail with a generic `nft rule install failed` error; error surfaces as `UnsupportedRuntime` either way. Proper fix would add an existence check and potentially create the chain, but requires knowing the full nft schema of the target VM.
- **ACCEPT rule left in nftables if DROP rule install fails.** If the ACCEPT rule installs successfully but the DROP rule then fails, `create_scan_network` returns `UnsupportedRuntime` while the ACCEPT rule persists in nftables. Subsequent runs will accumulate stale ACCEPT rules. Correct fix requires cleanup of the ACCEPT rule on DROP failure.
- **`ensure_image` falls through to pull on any `inspect_image` error, not just 404.** If the daemon is unavailable or returns a permission error, the code tries to pull instead of surfacing the real error. Correct fix requires matching on `bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }` (or similar) — bollard error variant needs verification in the installed version.
- **`pull_image` does not pass digest to `create_image`.** The image is fetched by tag only; a re-tagged image could be pulled. Post-pull `verify_digest` catches mismatches, so correctness is preserved, but the defense-in-depth of pulling by digest is missing.
- **No container log capture before force-remove on failure.** When `run_container` fails or exits non-zero, `force_remove_container` is called immediately with no attempt to retrieve container logs via `docker.logs(...)`. All diagnostic output is permanently lost. Enhancement deferred to a future story that wires the full scan observability pipeline.

## Deferred from: code review of 1-9-container-runtime-preflight-with-socket-precedence-chain (2026-05-07)

- **Double-print of preflight diagnostic to stderr.** `scan::run` prints the `format_no_runtime_reachable` block via `output::diag`, then `main.rs` re-prints with the "error: preflight failed: " prefix. Explicit design decision in spec; Story 1.12 (first full scan wiring) is the revisit point.
- **`try_exists` returns true for non-socket filesystem entries.** A regular file or directory at a socket path causes `ConnectFailed` instead of the more accurate `SocketFileMissing`. Adding a socket-type check (`FileTypeExt::is_socket()`) would improve the diagnostic for a vanishingly rare edge case. Pre-existing trade-off per spec T3.11.
- **`try_exists` I/O errors (e.g. permission denied) silently become `SocketFileMissing`.** `unwrap_or(false)` is the spec-chosen behaviour; the user sees "socket file missing" rather than "permission denied". Improving accuracy would require propagating `io::Error` through the `SocketFileMissing` path.
- **Integration tests lack a Podman socket guard.** `podman_default_socket_path()` is `pub(crate)`, so `tests/sandbox_preflight.rs` cannot call it directly to compute the skip condition. Tests that require all layers to fail may panic on a developer machine running Podman without Docker.

## Deferred from: code review of 1-8-cache-cell-write-read-api-with-atomic-semantics (2026-05-07)

- **`Option<f64>` perf fields silently round-trip `NaN` / `±Infinity`.** `Cell::{duration_seconds, tokens_per_sec, ttft_seconds, power_watts}` are `Option<f64>`; SQLite's `REAL` accepts non-finite values, but they break `PartialEq` round-trip equality (`NaN != NaN`). The cache primitive trusts its inputs per spec; the perf collector (Story 2.10) is the right validation point, surfacing non-finite as `None` ("graceful degrade") at the producer layer.
- **Empty-string PK components are accepted as distinct row identities.** `TEXT NOT NULL` is satisfied by `""`. A buggy upstream that lets an empty `model_sha` / `task_id` through pollutes the table with a "ghost" cell that collides for every other empty-string caller. Producer-layer validation per spec; owner is Story 1.6's `cache::key::*` helpers (already constrained to lowercase-hex digests for `model_sha` / `params_hash`) plus Story 2.6's task-loader.
- **`scan_timestamp: String` is free-form TEXT — no RFC 3339 validation at write or read.** Doc-comment promises RFC 3339 but downstream queries that assume lexicographic = chronological will silently misorder rows that drift from the convention. Owner: `util::time` helper (lands with Story 1.12 when the first producer needs it).
- **No `CHECK (pass IN (0, 1))` constraint at the SQL layer.** Cooperates with the read-side "defensive non-zero check" (`pass_int != 0`) — a `2` written by an external tool round-trips as `1`. A CHECK constraint at the schema layer would belt-and-suspender, but the schema constants live in Story 1.7's `src/cache/schema.rs::CELLS_DDL_V1`; an additive migration (`CELLS_DDL_V2`) is the right vehicle, owner TBD.
- **`Cache::open` is not idempotent under concurrent first-time creation by two processes against a fresh path.** Two `Cache::open` calls racing on a fresh path can both read `user_version = 0` and attempt the same DDL transaction. Engine-level mutual exclusion is provided by Story 6.4's `scan.lock` upstream of any `Cache::open` call; the cache layer correctly does not re-implement application-level locking. If a future use case opens `Cache` outside the scan-lock scope, this becomes a real concern.
- **100 ms perf assertion in `lookup_*_at_10k_cells_under_100ms_nfr_p5` may flake on shared CI runners.** The lookup is sub-ms on indexed PK; the risk is from CI noise, not the assertion. Spec T4.8 documents the escape hatch: "if CI runs the populate phase >30 s in practice, switch to a single explicit transaction wrapping all 10 K INSERTs (the LOOKUP is the perf measurement, the populate is fixture setup)." Pick up only if observed flake rate exceeds tolerance.

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
