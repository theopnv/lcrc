# Story 1.12: End-to-end one-cell scan (no HTML yet)

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As Theop,
I want `lcrc scan` (with the model path read from `LCRC_DEV_MODEL_PATH` for Epic 1) to run the canary task against that model inside the sandbox, capture pass/fail, and persist exactly one cell to the cache,
so that I can verify every layer of the integration spine works end-to-end before we invest in elaboration.

## Acceptance Criteria

**AC1.** **Given** a working preflight, `LCRC_DEV_MODEL_PATH` set to a real GGUF, and an empty cache **When** I run `lcrc scan` **Then** it executes the full pipeline: (1) preflight succeeds, (2) machine fingerprint detected, (3) model_sha computed from the GGUF file, (4) llama-server starts on a free port, (5) cell key computed via `cache::key`, (6) cache lookup misses, (7) temp workspace created and canary spec copied in, (8) `Sandbox::run_task` spawns the container with workspace + network envelope, (9) mini-swe-agent runs the canary task inside the container against `host.docker.internal:<llama-port>`, (10) outcome (pass/fail + duration) captured, (11) cell written atomically with all metadata, (12) container + server torn down, (13) process exits 0.

**AC2.** **Given** the cell was written **When** I inspect the SQLite file with `SELECT * FROM cells` **Then** there is exactly one row with all seven PK columns populated and all metadata present: `depth_tier='quick'`, `scan_timestamp` is valid RFC 3339 UTC, `container_image_id` matches `crate::constants::CONTAINER_IMAGE_DIGEST`, `lcrc_version` matches the binary's `crate::version::LCRC_VERSION`.

**AC3.** **Given** a completed first scan **When** I run `lcrc scan` a second time with no changes **Then** preflight succeeds, cache lookup hits (FR26), no measurement runs, no llama-server spawned, exit 0 — fully idempotent (NFR-R6).

**AC4.** **Given** Ctrl-C is sent during the in-container measurement **When** the SIGINT handler fires **Then** no cell is written for the in-progress task, the sandbox network is removed (best-effort), the llama-server process is terminated via `ServerHandle::drop`, exit 3 (`AbortedBySignal`).

**AC5.** **Given** preflight fails (no container runtime reachable) **When** I run `lcrc scan` **Then** exit 11 with setup instructions; no llama-server spawned; no measurement attempted; no SQLite file created.

## Tasks / Subtasks

- [ ] **T1. Create `tasks/swe-bench-pro/` vendored task directory** (AC: 1, 2)
  - [ ] T1.1 Create `tasks/swe-bench-pro/version` containing the string `"0.0.1-canary-only"`. This is `task_subset_version` in the cell PK.
  - [ ] T1.2 Create `tasks/swe-bench-pro/canary/spec.json` with a minimal SWE-bench task spec that mini-swe-agent can execute. Use the format:
    ```json
    {
      "task_id": "swe-bench-pro:canary",
      "repo": "princeton-nlp/SWE-bench",
      "instance_id": "canary-001",
      "base_commit": "HEAD",
      "problem_statement": "Write a file named `result.txt` in the current directory containing exactly the text `canary-pass`.",
      "hints_text": "",
      "created_at": "2026-01-01T00:00:00Z",
      "version": "0.0.1",
      "FAIL_TO_PASS": ["test_canary_result"],
      "PASS_TO_PASS": []
    }
    ```
    **Note:** The exact spec format is confirmed when Story 1.14 finalises the mini-swe-agent version and Dockerfile. This spec should be updated in tandem with Story 1.14.
  - [ ] T1.3 Create `tasks/swe-bench-pro/canary/baseline.json` — the expected outcome for a passing run:
    ```json
    {
      "task_id": "swe-bench-pro:canary",
      "expected_pass": true,
      "description": "Canary task always passes on a correctly installed mini-swe-agent"
    }
    ```

- [ ] **T2. Extend `src/scan.rs` — add new submodules** (AC: all)
  - [ ] T2.1 Add the following module declarations to `src/scan.rs` in alphabetical order (current: `pub mod server_lifecycle;`; add `canary`, `orchestrator`, `signal` before it):
    ```rust
    pub mod canary;
    pub mod orchestrator;
    pub mod server_lifecycle;
    pub mod signal;
    ```
  - [ ] T2.2 Update the file-level `//!` doc on `src/scan.rs` to list the four submodules.

- [ ] **T3. Implement `src/scan/signal.rs` — SIGINT handler** (AC: 4)
  - [ ] T3.1 Add file-level doc:
    ```rust
    //! SIGINT / Ctrl-C detection for the scan lifecycle.
    //!
    //! Exposes a single `wait_for_sigint()` future that resolves once
    //! `tokio::signal::ctrl_c()` fires. The scan orchestrator races this
    //! against the measurement future via `tokio::select!`.
    ```
  - [ ] T3.2 Implement:
    ```rust
    /// Resolves once the process receives SIGINT (Ctrl-C).
    ///
    /// Designed to be `tokio::select!`-ed against the scan future.
    /// The select arm that returns `Err(crate::error::Error::AbortedBySignal)`
    /// handles exit-code 3 at the `cli/scan.rs` call site.
    pub async fn wait_for_sigint() {
        tokio::signal::ctrl_c()
            .await
            .unwrap_or_default();
    }
    ```
    Using `.unwrap_or_default()` converts `Err(io::Error)` (e.g. when signal handler is not supported) to `()`, which is fine — if ctrl_c setup fails, we treat it as if the signal already fired (conservative).

- [ ] **T4. Implement `src/scan/canary.rs` — canary task loader** (AC: 1, 2)
  - [ ] T4.1 Add file-level doc:
    ```rust
    //! Canary task: stable identifier and workspace setup.
    //!
    //! The canary task is the single task run in Epic 1's integration spine.
    //! It uses a vendored spec from `tasks/swe-bench-pro/canary/spec.json`
    //! that mini-swe-agent executes inside the per-task container.
    ```
  - [ ] T4.2 Define constants:
    ```rust
    /// Stable task identifier for the canary cell PK.
    pub const CANARY_TASK_ID: &str = "swe-bench-pro:canary";

    /// Path to the vendored canary spec relative to the crate root.
    const CANARY_SPEC_PATH: &str = "tasks/swe-bench-pro/canary/spec.json";

    /// Path to the vendored task-subset version file relative to crate root.
    const TASK_SUBSET_VERSION_PATH: &str = "tasks/swe-bench-pro/version";
    ```
  - [ ] T4.3 Implement `task_subset_version()`:
    ```rust
    /// Read the vendored task-subset version string.
    ///
    /// Returns the contents of `tasks/swe-bench-pro/version` trimmed of
    /// whitespace. On read failure (file missing in dev build), returns the
    /// literal `"unknown"`.
    ///
    /// # Errors
    ///
    /// Returns `Err` wrapping a `std::io::Error` if the file exists but
    /// cannot be read (permissions, I/O failure).
    pub async fn task_subset_version() -> Result<String, std::io::Error> {
        match tokio::fs::read_to_string(TASK_SUBSET_VERSION_PATH).await {
            Ok(s) => Ok(s.trim().to_owned()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("unknown".to_owned()),
            Err(e) => Err(e),
        }
    }
    ```
  - [ ] T4.4 Implement `setup_workspace(dir: &Path) -> Result<(), std::io::Error>`:
    ```rust
    /// Copy the canary spec into a task workspace directory.
    ///
    /// The workspace directory must already exist (caller creates it via
    /// `tempfile::TempDir`). After this call, `dir/spec.json` contains the
    /// canary spec that mini-swe-agent reads from `/workspace/spec.json`
    /// inside the container.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the spec file cannot be read or written.
    pub async fn setup_workspace(dir: &std::path::Path) -> Result<(), std::io::Error> {
        let spec = tokio::fs::read_to_string(CANARY_SPEC_PATH).await?;
        tokio::fs::write(dir.join("spec.json"), spec).await
    }
    ```
    **Note:** `tokio::fs` — not `std::fs` — per async discipline (AR-3).

- [ ] **T5. Implement `src/scan/orchestrator.rs` — one-cell scan pipeline** (AC: 1, 2, 3, 4)
  - [ ] T5.1 Add file-level doc:
    ```rust
    //! One-cell scan orchestrator for Epic 1's integration spine.
    //!
    //! `run()` executes the full pipeline for a single hardcoded canary
    //! measurement: preflight → fingerprint → model_sha → cache miss → server
    //! start → workspace → sandbox → cell write → teardown.
    //!
    //! Model path comes from `LCRC_DEV_MODEL_PATH`. Epic 2 replaces the
    //! hardcoded path with real model discovery.
    ```
  - [ ] T5.2 Implement `detect_backend_build()` private helper:
    ```rust
    /// Detect the llama-server version string for `backend_build`.
    ///
    /// Runs `llama-server --version`, parses the output, and returns a
    /// canonical `BackendInfo`. If the binary is absent or the output format
    /// is unrecognised, returns a sentinel `"llama.cpp-unknown+unknown"` so
    /// the cache key degrades gracefully rather than blocking the scan.
    async fn detect_backend_build() -> crate::cache::key::BackendInfo {
        let out = tokio::process::Command::new("llama-server")
            .arg("--version")
            .output()
            .await;

        let Ok(output) = out else {
            return crate::cache::key::BackendInfo {
                name: "llama.cpp".into(),
                semver: "unknown".into(),
                commit_short: "unknown".into(),
            };
        };

        // llama-server --version typically prints:
        //   "version: b3791 (a1b2c3d4)\nbuilt with ..."
        // Extract build number and commit short.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let text = if stdout.trim().is_empty() { &stderr } else { &stdout };

        parse_backend_build(text)
    }

    fn parse_backend_build(text: &str) -> crate::cache::key::BackendInfo {
        // Look for "version: b<N> (<commit>)" in the first line.
        let first_line = text.lines().next().unwrap_or("");
        let semver = first_line
            .split_whitespace()
            .find(|t| t.starts_with('b') && t[1..].chars().all(char::is_numeric))
            .map(str::to_owned)
            .unwrap_or_else(|| "unknown".to_owned());

        let commit_short = first_line
            .find('(')
            .and_then(|i| text[i + 1..].find(')').map(|j| text[i + 1..i + 1 + j].to_owned()))
            .unwrap_or_else(|| "unknown".to_owned());

        crate::cache::key::BackendInfo {
            name: "llama.cpp".into(),
            semver,
            commit_short,
        }
    }
    ```
  - [ ] T5.3 Implement `run(runtime_probe: crate::sandbox::runtime::RuntimeProbe) -> Result<(), crate::error::Error>`:
    ```rust
    /// Execute the one-cell scan pipeline.
    ///
    /// # Errors
    ///
    /// - [`crate::error::Error::Preflight`] for model path missing, machine
    ///   fingerprint failure, or sandbox setup failure.
    /// - [`crate::error::Error::AbortedBySignal`] if SIGINT fires during measurement.
    /// - [`crate::error::Error::Other`] for cache I/O errors.
    pub async fn run(probe: crate::sandbox::runtime::RuntimeProbe) -> Result<(), crate::error::Error> {
        // --- Step 1: Model path ---
        let model_path_str = std::env::var("LCRC_DEV_MODEL_PATH")
            .map_err(|_| crate::error::Error::Preflight(
                "LCRC_DEV_MODEL_PATH not set; set it to a GGUF file path for Epic 1 scans".into()
            ))?;
        let model_path = std::path::PathBuf::from(&model_path_str);
        if !model_path.is_file() {
            return Err(crate::error::Error::Preflight(
                format!("LCRC_DEV_MODEL_PATH='{}' is not a readable file", model_path.display())
            ));
        }

        // --- Step 2: Machine fingerprint ---
        let fingerprint = crate::machine::MachineFingerprint::detect()
            .await
            .map_err(|e| crate::error::Error::Preflight(format!("machine fingerprint: {e}")))?;
        tracing::info!(
            target: "lcrc::scan::orchestrator",
            fingerprint = fingerprint.as_str(),
            "machine fingerprint detected",
        );

        // --- Step 3: Backend build ---
        let backend_info = detect_backend_build().await;
        let backend_build = crate::cache::key::backend_build(&backend_info);
        tracing::info!(
            target: "lcrc::scan::orchestrator",
            backend_build = %backend_build,
            "backend build detected",
        );

        // --- Step 4: Compute model_sha (streaming SHA-256 of GGUF file) ---
        let model_sha = crate::cache::key::model_sha(&model_path)
            .await
            .map_err(|e| crate::error::Error::Preflight(format!("model_sha: {e}")))?;
        tracing::info!(
            target: "lcrc::scan::orchestrator",
            model_sha = %model_sha,
            model_path = %model_path.display(),
            "model SHA-256 computed",
        );

        // --- Step 5: Build cell key ---
        let server_params = crate::scan::server_lifecycle::Params { ctx: 4096 };
        let key_params = crate::cache::key::Params {
            ctx: server_params.ctx,
            temp: 0.0_f32,
            threads: 0,
            n_gpu_layers: 0,
        };
        let params_hash = crate::cache::key::params_hash(&key_params)
            .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("params_hash: {e}")))?;

        let task_subset_version = crate::scan::canary::task_subset_version()
            .await
            .map_err(|e| crate::error::Error::Preflight(format!("task_subset_version: {e}")))?;

        let cell_key = crate::cache::cell::CellKey {
            machine_fingerprint: crate::cache::key::machine_fingerprint(&fingerprint),
            model_sha,
            backend_build: backend_build.clone(),
            params_hash,
            task_id: crate::scan::canary::CANARY_TASK_ID.to_owned(),
            harness_version: crate::version::HARNESS_VERSION.to_owned(),
            task_subset_version,
        };

        // --- Step 6: Open / initialise cache ---
        let cache_dir = etcetera::base_strategy::choose_base_strategy()
            .map_err(|e| crate::error::Error::Preflight(format!("XDG base dirs: {e}")))?
            .data_dir()
            .join("lcrc");
        tokio::fs::create_dir_all(&cache_dir)
            .await
            .map_err(|e| crate::error::Error::Preflight(format!("create cache dir: {e}")))?;
        let db_path = cache_dir.join("lcrc.db");
        let cache = tokio::task::spawn_blocking({
            let p = db_path.clone();
            move || crate::cache::migrations::open(&p)
        })
        .await
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("spawn_blocking join: {e}")))?
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("cache open: {e}")))?;

        // --- Step 7: Cache lookup (FR26) ---
        let existing = tokio::task::spawn_blocking({
            let c = cache.clone();
            let k = cell_key.clone();
            move || c.lookup_cell(&k)
        })
        .await
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("spawn_blocking join: {e}")))?
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("cache lookup: {e}")))?;

        if existing.is_some() {
            crate::output::diag("lcrc scan: cache hit — no measurement needed (AC3 idempotency).");
            tracing::info!(
                target: "lcrc::scan::orchestrator",
                task_id = crate::scan::canary::CANARY_TASK_ID,
                "cache hit; skipping measurement",
            );
            return Ok(());
        }
        tracing::info!(
            target: "lcrc::scan::orchestrator",
            task_id = crate::scan::canary::CANARY_TASK_ID,
            "cache miss; proceeding to measure",
        );

        // --- Steps 8-12: Measurement (raced against SIGINT) ---
        tokio::select! {
            result = measure_and_persist(
                probe,
                model_path,
                server_params,
                cell_key,
                cache,
            ) => result,

            () = crate::scan::signal::wait_for_sigint() => {
                // Container teardown is best-effort: the sandbox cleanup future runs
                // when the measurement branch is dropped, but async drops are not
                // guaranteed. The scan-id label on the network and containers enables
                // backstop GC in a future story.
                tracing::warn!(
                    target: "lcrc::scan::orchestrator",
                    "SIGINT received; aborting scan without writing cell",
                );
                Err(crate::error::Error::AbortedBySignal)
            }
        }
    }
    ```
  - [ ] T5.4 Implement `measure_and_persist(...)` private async helper:
    ```rust
    async fn measure_and_persist(
        probe: crate::sandbox::runtime::RuntimeProbe,
        model_path: std::path::PathBuf,
        server_params: crate::scan::server_lifecycle::Params,
        cell_key: crate::cache::cell::CellKey,
        cache: crate::cache::cell::Cache,
    ) -> Result<(), crate::error::Error> {
        // Start llama-server — drop order matters: sandbox must drop first
        // so its network cleanup runs before the server socket closes.
        let server = crate::scan::server_lifecycle::LlamaServer::new();
        let handle = server
            .start(&model_path, &server_params)
            .await
            .map_err(|e| crate::error::Error::Preflight(format!("llama-server startup: {e}")))?;

        tracing::info!(
            target: "lcrc::scan::orchestrator",
            port = handle.port(),
            "llama-server ready",
        );

        // Create sandbox — uses handle.port() as the pinned llama port
        let sandbox = crate::sandbox::Sandbox::new(&probe, handle.port())
            .await
            .map_err(|e| crate::error::Error::Preflight(format!("sandbox setup: {e}")))?;

        // Prepare workspace with canary spec
        let workspace_dir = tempfile::TempDir::new()
            .map_err(|e| crate::error::Error::Preflight(format!("temp workspace: {e}")))?;
        crate::scan::canary::setup_workspace(workspace_dir.path())
            .await
            .map_err(|e| crate::error::Error::Preflight(format!("canary workspace: {e}")))?;

        // Run task in container
        crate::output::diag(&format!(
            "lcrc scan: running canary task (this may take up to 120 s)…"
        ));
        let outcome = sandbox
            .run_task(
                crate::constants::CONTAINER_IMAGE_DIGEST,
                workspace_dir.path(),
            )
            .await
            .map_err(|e| crate::error::Error::Preflight(format!("run_task: {e}")))?;

        tracing::info!(
            target: "lcrc::scan::orchestrator",
            pass = outcome.pass,
            duration_seconds = outcome.duration_seconds,
            "task completed",
        );

        // Sandbox cleanup (best-effort) — must happen before server drop
        sandbox.cleanup().await;

        // Construct cell for persistence
        let scan_timestamp = crate::util::rfc3339_now();
        let cell = crate::cache::cell::Cell {
            key: cell_key,
            container_image_id: crate::constants::CONTAINER_IMAGE_DIGEST.to_owned(),
            lcrc_version: crate::version::LCRC_VERSION.to_owned(),
            depth_tier: "quick".to_owned(),
            scan_timestamp,
            pass: outcome.pass,
            duration_seconds: Some(outcome.duration_seconds),
            tokens_per_sec: None,
            ttft_seconds: None,
            peak_rss_bytes: None,
            power_watts: None,
            thermal_state: None,
            badges: vec![],
        };

        // Write cell atomically (NFR-R2)
        tokio::task::spawn_blocking(move || cache.write_cell(&cell))
            .await
            .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("spawn_blocking join: {e}")))?
            .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("cache write: {e}")))?;

        crate::output::diag(&format!(
            "lcrc scan: done — pass={}, duration={:.1}s",
            outcome.pass,
            outcome.duration_seconds,
        ));
        tracing::info!(
            target: "lcrc::scan::orchestrator",
            pass = outcome.pass,
            "cell written; scan complete",
        );

        // `handle` (ServerHandle) drops here: SIGTERM + SIGKILL via Drop impl
        Ok(())
    }
    ```
    **Drop ordering is important:**
    - `workspace_dir` (TempDir) drops here → temp directory cleaned up
    - `handle` (ServerHandle) drops here → SIGTERM to llama-server
    - `sandbox` was explicitly cleaned up above via `cleanup()` (its `cleanup()` is best-effort;
      the sandbox struct has no Drop impl so this is explicit)

- [ ] **T6. Add `crate::util::rfc3339_now()` helper in `src/util/tracing.rs`** (AC: 2)
  - [ ] T6.1 `src/util/tracing.rs` currently exists but there is no `src/util/time.rs` yet (architecture specifies it as future). For Story 1.12, add the timestamp helper directly to `src/util/tracing.rs` (same file), since `tracing.rs` is already a catch-all util module in this sprint:
    ```rust
    /// Return the current UTC time formatted as RFC 3339 with millisecond precision.
    ///
    /// Format: `"2026-04-30T14:23:15.412Z"` — the `Z` suffix is literal
    /// (not `+00:00`) per the timestamp-format pattern.
    #[must_use]
    pub fn rfc3339_now() -> String {
        use time::format_description::well_known::Rfc3339;
        use time::OffsetDateTime;
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00.000Z".to_owned())
    }
    ```
    **Note:** `time` crate is already in `Cargo.toml` with `features = ["formatting", "macros"]`.

    Make this function pub at crate root by adding to `src/util/tracing.rs` (re-export or add inline). Reference it in the orchestrator as `crate::util::rfc3339_now()`. Ensure `src/util/tracing.rs` re-exports or defines the `rfc3339_now` function at pub(crate) visibility and that `src/util/` exports it as `pub use tracing::rfc3339_now` or directly.
    
    Simpler approach: add `pub fn rfc3339_now()` directly to `src/util/tracing.rs`. Then call it as `crate::util::rfc3339_now()` if util re-exports it, or create a new small `src/util.rs` module that re-exports. Check the existing `src/util/` structure and follow whatever pattern is already there.

- [ ] **T7. Update `src/cli/scan.rs` — call orchestrator** (AC: 1, 3, 4, 5)
  - [ ] T7.1 Replace the existing `run()` body with:
    ```rust
    /// Entry point for `lcrc scan`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Preflight`] when the container-runtime
    /// preflight detects no reachable Docker-Engine-API-compatible socket,
    /// when `LCRC_DEV_MODEL_PATH` is unset, or when sandbox setup fails.
    /// Returns [`crate::error::Error::AbortedBySignal`] on Ctrl-C.
    pub fn run() -> Result<(), crate::error::Error> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| crate::error::Error::Preflight(format!("tokio runtime init: {e}")))?;
        runtime.block_on(async {
            match crate::sandbox::runtime::detect(&crate::sandbox::runtime::SystemEnv).await {
                Ok(probe) => {
                    tracing::info!(
                        target: "lcrc::sandbox::runtime",
                        socket_path = %probe.socket_path.display(),
                        source = probe.source.name(),
                        "detected container runtime",
                    );
                    crate::scan::orchestrator::run(probe).await
                }
                Err(err) => Err(crate::error::Error::Preflight(err.to_string())),
            }
        })
    }
    ```
    **Change `new_current_thread()` → `new_multi_thread()`:** The orchestrator uses `spawn_blocking` (for rusqlite sync calls) which requires a multi-thread runtime. `current_thread` silently deadlocks when `spawn_blocking` futures are awaited.

  - [ ] T7.2 Update the existing unit test `run_returns_preflight_error_when_no_runtime` — it still applies. No changes to the test needed; the test exercises the preflight path which now delegates correctly.

- [ ] **T8. Integration test `tests/scan_e2e.rs`** (AC: 1, 2, 3, 4, 5)
  - [ ] T8.1 Gate on three env vars:
    ```rust
    fn gate() -> Option<std::path::PathBuf> {
        if std::env::var("LCRC_INTEGRATION_TEST_SCAN").is_err() {
            eprintln!(
                "skipping: set LCRC_INTEGRATION_TEST_SCAN=1 \
                 LCRC_DEV_MODEL_PATH=<gguf> to run"
            );
            return None;
        }
        let path = std::env::var("LCRC_DEV_MODEL_PATH").ok()?;
        Some(std::path::PathBuf::from(path))
    }
    ```
  - [ ] T8.2 Test `scan_cache_miss_then_hit` (AC1, AC2, AC3): call the binary twice. First run → cache miss → cell written. Second run → cache hit → no measurement. Use `assert_cmd` + `predicates` for exit-code assertions.
  - [ ] T8.3 Test `scan_exit_11_on_no_runtime` (AC5): with no socket reachable, assert binary exits 11. Mirrors the existing `run_returns_preflight_error_when_no_runtime` unit test but at the binary level.
  - [ ] T8.4 All integration tests carry `#[ignore]` unless `LCRC_INTEGRATION_TEST_SCAN=1` is set (managed by the gate function above + `return` pattern).
  - [ ] T8.5 Tests use `tempfile::TempDir` for an isolated cache directory, passing it via `LCRC_PATHS_CACHE_DIR` env var (once config is wired — for Epic 1, the test may manually set this or accept the default XDG dir).

- [ ] **T9. Local CI mirror** (AC: all)
  - [ ] T9.1 `cargo build` — all new modules compile; `etcetera`, `time`, `tokio::signal`, `tempfile` all resolve.
  - [ ] T9.2 `cargo fmt --check` — rustfmt clean.
  - [ ] T9.3 `cargo clippy --all-targets --all-features -- -D warnings`. Watch for:
    - `missing_docs` on every `pub` item in new modules.
    - `clippy::module_name_repetitions` on any error types in `scan/`.
    - `clippy::used_underscore_binding` on `_` prefixed variables kept around for drop ordering.
    - `clippy::cast_possible_truncation` — no casts needed here; note if any appear.
  - [ ] T9.4 `cargo test` — all pre-existing tests pass. New integration tests skip unless `LCRC_INTEGRATION_TEST_SCAN=1`.
  - [ ] T9.5 Scope check — no module outside `src/scan/server_lifecycle.rs` spawns `llama-server`:
    ```bash
    git grep -nE 'Command::new\("llama' src/ tests/ \
      | grep -v '^src/scan/server_lifecycle.rs:' \
      | grep -v '^src/scan/orchestrator.rs:'  # orchestrator uses it for version detection only
    ```
    The orchestrator calls `llama-server --version` for `detect_backend_build()`. This is NOT a lifecycle spawn — it's a one-shot version probe. It's acceptable to have this one additional reference in `orchestrator.rs`.

## Dev Notes

### Scope discipline (read this first)

This story wires together the modules built in Stories 1.5–1.11. It creates **four new files** and updates two existing ones. No new crate dependencies are added.

**This story does:**
- Wire the end-to-end scan pipeline in `src/scan/orchestrator.rs`
- Add SIGINT handling in `src/scan/signal.rs`
- Add canary task loading in `src/scan/canary.rs`
- Create minimal `tasks/swe-bench-pro/canary/` vendored fixtures
- Replace the stub in `src/cli/scan.rs` with a real orchestrator call

**This story does NOT:**
- Implement HTML report rendering — that is Story 1.13
- Implement model discovery (`~/.cache/llama.cpp/`) — that is Story 2.1
- Build or publish the container image — that is Story 1.14
- Implement the `Backend` trait or `LlamaCppBackend` — that is Story 2.1
- Implement the `TaskSource` trait — that is Story 2.3
- Implement scan lock (`scan.lock`) — that is Story 6.4
- Implement config loading (`figment`/TOML) — that is Story 6.1
- Implement progress streaming (`indicatif`) — that is Story 2.13
- Add the `ScanError` variant to `src/error.rs` — the existing variants cover this story's error paths

### Critical dependency: container image (Story 1.14)

The `CONTAINER_IMAGE_DIGEST` constant in `src/constants.rs` is currently a placeholder `"sha256:0000..."`. `Sandbox::run_task` calls `image::ensure_image` which will fail with `ImagePull` until a real image is published by Story 1.14.

**The code written here is correct; the integration test requires Story 1.14 to have run.** The unit tests and `cargo test` still pass without the image.

### Architecture compliance (binding constraints)

- **`#[tokio::main(flavor = "multi_thread")]` in `main.rs`** — already set. The `cli/scan.rs::run()` uses `Builder::new_multi_thread()` to build its own runtime for the same reason: `spawn_blocking` requires the multi-thread scheduler (per AR-3, single runtime, no `block_on` inside async).
- **All I/O via tokio** — `tokio::fs::read_to_string`, `tokio::fs::write`, `tokio::fs::create_dir_all` in canary.rs and orchestrator.rs. `rusqlite` is sync-only; wrap with `spawn_blocking` (done above).
- **stdout/stderr discipline (FR46)** — only `crate::output::diag` for user-visible progress lines. No `println!`/`eprintln!` in new modules.
- **`missing_docs = "warn"`** — every `pub` item needs `///` doc comment.
- **No `unsafe`** — `nix` signal calls are safe via `ServerHandle::drop` (Story 1.11). `tokio::signal::ctrl_c()` is fully safe.
- **Single source of truth for cache keys** — always call `cache::key::model_sha`, `cache::key::params_hash`, `cache::key::backend_build`, `cache::key::machine_fingerprint`. Never compute inline.
- **`CacheError::DuplicateCell` is an upstream bug** — the `lookup_cell` before `write_cell` invariant means a duplicate is a programming error. Map it to `Error::Other(anyhow!)` with a loud message.

### rusqlite + tokio: `spawn_blocking` pattern

`rusqlite::Connection` is `!Send` — it cannot be `await`-ed across threads. Wrap every rusqlite call in `tokio::task::spawn_blocking`:

```rust
let result = tokio::task::spawn_blocking(move || {
    cache.lookup_cell(&key)
}).await??;
```

The double `?` unwraps: outer `?` for `JoinError` (panic in blocking task), inner `?` for the `CacheError`. The `Cache` struct from `cache::cell` must be `Send` for this to compile. Verify that `Cache` wraps a `Connection` behind an `Arc<Mutex<Connection>>` or similar. If not, open a new connection per blocking call using `crate::cache::migrations::open`.

**Check `src/cache/cell.rs` for `Cache`'s actual definition and `Send`-ness before writing the orchestrator.** If `Cache` is not `Send`, open a fresh connection in each `spawn_blocking` block using `crate::cache::migrations::open(&db_path)`.

### `etcetera` API for XDG paths

```rust
use etcetera::base_strategy::{BaseStrategy, choose_base_strategy};
let strategy = choose_base_strategy()
    .map_err(|e| Error::Preflight(format!("XDG: {e}")))?;
let cache_dir = strategy.data_dir().join("lcrc");
```

`etcetera::base_strategy::choose_base_strategy()` is the canonical function. It is **not** async. Call it outside `spawn_blocking` (it doesn't do I/O).

### `tokio::select!` semantics for SIGINT

```rust
tokio::select! {
    result = measure_and_persist(...) => result,
    () = wait_for_sigint() => Err(Error::AbortedBySignal),
}
```

When `wait_for_sigint()` wins the race:
- `measure_and_persist` future is **dropped** (cancelled).
- `sandbox.cleanup()` inside the dropped future **does NOT run** — async code in a dropped future halts at the current `.await` point.
- `handle` (ServerHandle) **Drop runs synchronously** because `Drop` is synchronous and Rust guarantees it on any drop (including cancelled futures) — but ONLY if the `ServerHandle` was already constructed. If SIGINT fires before `handle` is constructed, the server was never started.
- The per-scan Docker network is NOT cleaned up by the cancelled future's `sandbox.cleanup()`. This is accepted in Epic 1; the backstop GC (`lcrc-scan-id` label) handles it.

Document this in a comment in `orchestrator.rs` near the `select!`.

### `cache::key::machine_fingerprint` API

```rust
let fp_string = crate::cache::key::machine_fingerprint(&fingerprint);
// Returns "M1Pro-32GB-14gpu" — the canonical string for the PK column.
```

`MachineFingerprint` → `String` conversion: check `src/cache/key.rs` for the `machine_fingerprint` function signature. It likely takes `&MachineFingerprint` and returns `String`. Verify before writing the orchestrator.

### Library/framework requirements (use these exact crates)

- **`tokio::signal::ctrl_c()`** — SIGINT detection. In `tokio = { features = ["full"] }` — available without additional deps.
- **`etcetera::base_strategy`** — XDG paths. Already in `Cargo.toml` (`etcetera = "0.10"`).
- **`tempfile::TempDir`** — ephemeral workspace. Already in `Cargo.toml` (`tempfile = "3"`).
- **`time` crate** — RFC 3339 timestamp. Already in `Cargo.toml` (`time = { version = "0.3", features = ["formatting", "macros"] }`).
- **Do NOT add** `chrono`, `tokio-util`, `uuid`, or any new crate. All required dependencies are present.

### `etcetera` version 0.10 API note

- `etcetera::base_strategy::choose_base_strategy()` returns `Result<impl BaseStrategy, etcetera::HomeDirError>`.
- `strategy.data_dir()` returns `std::path::PathBuf`.
- On macOS, `data_dir()` resolves to `~/Library/Application Support` (XDG-like) or `~/.local/share` depending on the strategy. Either is correct for `lcrc.db`.

### File structure requirements

```
src/
├── lib.rs                           (unchanged — pub mod scan; already present)
├── cli/
│   └── scan.rs                      UPDATED: replace stub with orchestrator call
├── scan.rs                          UPDATED: add 3 new pub mod declarations
└── scan/
    ├── canary.rs                    NEW: canary task ID + workspace setup
    ├── orchestrator.rs              NEW: one-cell scan pipeline
    ├── server_lifecycle.rs          (unchanged — built in Story 1.11)
    └── signal.rs                    NEW: SIGINT handler

tasks/
└── swe-bench-pro/
    ├── version                      NEW: "0.0.1-canary-only"
    └── canary/
        ├── spec.json                NEW: canary task spec (format TBC with Story 1.14)
        └── baseline.json            NEW: known-good baseline

tests/
└── scan_e2e.rs                      NEW: end-to-end integration test (gated on env var)
```

After this story:
- `lcrc scan` executes the full pipeline when `LCRC_DEV_MODEL_PATH` is set and a container runtime is running.
- Story 1.13 adds HTML report rendering without touching `orchestrator.rs` (it adds a `report::render_html()` call after `cache.write_cell`).
- Story 1.14 publishes the container image that makes the integration test pass end-to-end.

### Cross-story interaction

- **Depends on**: Story 1.5 (`MachineFingerprint::detect`), Story 1.6 (`cache::key::*`), Story 1.7 (cache schema + `migrations::open`), Story 1.8 (`cache::cell::Cache::write_cell`, `lookup_cell`), Story 1.9 (runtime preflight), Story 1.10 (`Sandbox::new`, `run_task`, `cleanup`), Story 1.11 (`LlamaServer::start`, `ServerHandle`).
- **Unblocks**: Story 1.13 (HTML rendering — adds a `render_html` call in the orchestrator), Story 1.14 (publishes the image that makes `run_task` succeed).

### Previous story intelligence

Carry-forward from Story 1.11:

- **`spawn_blocking` for sync SQLite calls**: Story 1.11 did not hit this because `server_lifecycle.rs` is async-only. Story 1.12 IS the first caller of `cache::cell::Cache`. Check if `Cache` is `Send` before assuming it can cross `spawn_blocking` boundaries.
- **`current_thread` → `multi_thread`**: `spawn_blocking` panics at runtime in a `current_thread` tokio runtime. `cli/scan.rs` currently uses `Builder::new_current_thread()` — **this must be changed to `new_multi_thread()`** (see T7.1).
- **`#[allow(clippy::cast_possible_wrap)]` pattern**: used in Story 1.11 for PID casts. If similar casts arise in orchestrator code, apply the same pattern.
- **Tracing discipline**: new events use `target: "lcrc::scan::orchestrator"`, `target: "lcrc::scan::canary"`, `target: "lcrc::scan::signal"` with structured fields.
- **Review finding from 1.11**: "Planning artifact refs in comments" — do not reference Story numbers, Epic numbers, or AC codes in source code comments.
- **`Drop` ordering matters for resource cleanup**: make sure `sandbox` is cleaned up before `handle` (ServerHandle) drops. Use explicit `sandbox.cleanup().await` before the function returns, not relying on drop order alone (since async types don't implement `async Drop`).

### Git history intelligence

Recent commits show:
- `src/scan/server_lifecycle.rs` was patched post-review to add tracing instrumentation and fix non-UTF-8 path handling (using `model_path.to_string_lossy()` was found to silently corrupt non-UTF-8 paths — Story 1.11 review patch addressed this).
- `Sandbox::new` now takes `llama_port: u16` — passed from `handle.port()` (confirmed in Story 1.10).
- All integration tests use `#[tokio::test(flavor = "current_thread")]` (Story 1.11 pattern). For the scan E2E test, use `#[tokio::test(flavor = "multi_thread")]` since it calls `spawn_blocking`.

### Project Context Reference

- **Epic 1 position**: Story 12 of 14. Stories 1.13 (HTML report) and 1.14 (container image) are the final two.
- **NFR coverage**:
  - NFR-R1: Ctrl-C → exit 3 → no cell written → next scan re-measures (AC4).
  - NFR-R2: Atomic cell write via single SQLite transaction (cache::cell::write_cell, tested in Story 1.8).
  - NFR-R6: Idempotent scan — cache hit on second run (AC3).
  - NFR-R8: Best-effort container cleanup on abort (SIGINT path — sandbox.cleanup not guaranteed in dropped future; documented).
  - FR26: Lookup before measure implemented (AC3 second-run idempotency).
  - FR27: Atomic cell write = resumability unit (Ctrl-C loses only the in-progress cell).
  - FR45: exit codes 0 (success), 3 (SIGINT), 11 (preflight fail) wired. Trigger paths for 0, 3, 11 complete after this story.

### References

- [Source: `_bmad-output/planning-artifacts/epics.md` § "Story 1.12"] — five AC clauses and user story
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Data Flow — One Scan Cycle"] — orchestrator step sequence
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Run resumability protocol"] — SIGINT teardown order
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Async Discipline"] — `spawn_blocking` for sync crates, multi-thread runtime
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure"] — new file placements
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Architectural Boundaries"] — sole-module invariants
- [Source: `_bmad-output/implementation-artifacts/1-11-llama-server-lifecycle.md` § "Cross-story interaction: Story 1.12 dependency"] — `ServerHandle::port()` → `Sandbox::new(llama_port)` connection point
- [Source: `_bmad-output/implementation-artifacts/1-11-llama-server-lifecycle.md` § "Dev Agent Record / Debug Log"] — `Duration::from_mins(1)` lint, import ordering, confirmed dep list
- [Source: `src/cli/scan.rs`] — current stub replaced by T7.1; existing unit test preserved
- [Source: `src/scan.rs`] — current: `pub mod server_lifecycle;` — add 3 more
- [Source: `src/sandbox.rs`] — `Sandbox::new(probe, llama_port)`, `run_task(image_digest, workspace)`, `cleanup()`
- [Source: `src/cache/cell.rs`] — `Cache`, `Cell`, `CellKey` types
- [Source: `src/cache/key.rs`] — `model_sha`, `params_hash`, `backend_build`, `machine_fingerprint`, `BackendInfo`, `Params`
- [Source: `src/constants.rs`] — `CONTAINER_IMAGE_DIGEST` (placeholder; used as `container_image_id` in cell)
- [Source: `src/version.rs`] — `LCRC_VERSION`, `HARNESS_VERSION`
- [Source: `src/error.rs`] — `Error::AbortedBySignal`, `Error::Preflight`, `Error::Other`
- [Source: `src/machine.rs`] — `MachineFingerprint::detect() -> Result<_, FingerprintError>`
- [Source: `Cargo.toml`] — all deps present: `tokio="1"` (full), `etcetera="0.10"`, `tempfile="3"`, `time="0.3"` (formatting+macros), `anyhow="1"`, `thiserror="2"`, `tracing="0.1"`

## Dev Agent Record

### Agent Model Used

_to be filled by dev agent_

### Debug Log References

### Completion Notes List

### File List
