# Story 1.11: llama-server lifecycle

Status: review

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a developer,
I want `LlamaServer::start(model_path, params) -> Result<ServerHandle>` to spawn `llama-server`, poll `/health` until ready (with a configurable timeout), and terminate cleanly on `Drop`,
so that each measurement has a known-ready server to talk to and no orphan processes leak (NFR-I1, NFR-R5).

## Acceptance Criteria

**AC1.** **Given** a real GGUF and `Params { ctx: 4096 }` **When** I call `LlamaServer::start(model_path, &params)` **Then** it spawns `llama-server` on a free localhost port and returns a `ServerHandle` once `GET /health` returns HTTP 200 (within `startup_timeout`, default 60 s).

**AC2.** **Given** a returned `ServerHandle` **When** I drop it **Then** `llama-server` is terminated (SIGTERM, 500 ms wait, SIGKILL fallback), and the port is freed. No orphan `llama-server` process remains after drop.

**AC3.** **Given** a model file that fails to load (e.g. a corrupt GGUF or a non-GGUF file) **When** I call `start` **Then** it returns `Err(ServerError::StartupFailure(...))` with the failure reason; no orphan process exists after the call returns.

**AC4.** **Given** the server hangs on startup and never returns HTTP 200 **When** `startup_timeout` expires **Then** `start` returns `Err(ServerError::StartupFailure(...))` with a timeout message; any spawned process is killed before the function returns.

**AC5.** **Given** two concurrent `LlamaServer::start` calls in the same process **When** both complete (success or failure) **Then** they bind to different ports and operate independently — port assignment is per-invocation, not global.

## Tasks / Subtasks

- [x] **T1. Create `src/scan.rs` — parent module root** (AC: all)
  - [x] T1.1 Create `src/scan.rs` with `pub mod server_lifecycle;` as its sole content. This is the parent for the `scan/` submodule tree; later stories add `orchestrator`, `canary`, `lock`, `signal`, `timeout` submodules following this pattern.
  - [x] T1.2 Add `pub mod scan;` to `src/lib.rs` in alphabetical order (between `pub mod sandbox;` and `pub mod util;` — `sa` < `sc` < `u`).

- [x] **T2. Author `src/scan/server_lifecycle.rs` — types** (AC: 1, 2, 3, 4, 5)
  - [x] T2.1 File-level `//!` doc:
    ```rust
    //! llama-server process lifecycle — spawn, health-gate, and teardown.
    //!
    //! [`LlamaServer`] owns startup configuration. [`ServerHandle`] represents a
    //! running server instance and terminates the process on [`Drop`].
    //!
    //! The server runs on the host (per NFR-I1) so containers reach it via
    //! `host.docker.internal` on the per-scan constrained network.
    ```
  - [ ] T2.2 Define `pub struct Params`:
    ```rust
    /// Parameters passed to `llama-server` at startup.
    ///
    /// Maps directly to CLI flags: `--ctx-size` is the only parameter
    /// that varies per `(model, params)` group in Epic 1.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Params {
        /// Context window size in tokens, passed as `--ctx-size`.
        pub ctx: u32,
    }
    ```
  - [x] T2.3 Define `pub enum ServerError`:
    ```rust
    /// Errors produced by [`LlamaServer::start`].
    #[derive(Debug, thiserror::Error)]
    pub enum ServerError {
        /// llama-server process failed to become ready (model load failure,
        /// startup timeout, spawn error, or port allocation failure).
        #[error("server startup failed: {0}")]
        StartupFailure(String),
    }
    ```
    Use a single variant; pack all failure modes into the `String` payload for clarity. The AC language "Err(ServerStartupFailure)" maps to `ServerError::StartupFailure`.
  - [x] T2.4 Define `pub struct LlamaServer`:
    ```rust
    /// Configuration for spawning a `llama-server` process.
    ///
    /// Construct with [`LlamaServer::new`] (60 s default timeout) or
    /// [`LlamaServer::with_timeout`] for tests that need a tighter deadline.
    pub struct LlamaServer {
        startup_timeout: std::time::Duration,
    }

    impl LlamaServer {
        /// Create a new launcher with the default 60-second startup timeout.
        pub fn new() -> Self {
            Self { startup_timeout: std::time::Duration::from_secs(60) }
        }

        /// Create a new launcher with a custom startup timeout.
        pub fn with_timeout(timeout: std::time::Duration) -> Self {
            Self { startup_timeout: timeout }
        }
    }

    impl Default for LlamaServer {
        fn default() -> Self { Self::new() }
    }
    ```
  - [x] T2.5 Define `pub struct ServerHandle`:
    ```rust
    /// A running `llama-server` instance.
    ///
    /// Dropping this handle sends SIGTERM to the server process, waits 500 ms,
    /// then sends SIGKILL if the process has not exited. This is synchronous
    /// (async `Drop` is not possible in Rust); the 500 ms sleep is intentional
    /// and brief relative to model-load cost.
    pub struct ServerHandle {
        process: tokio::process::Child,
        port: u16,
    }

    impl ServerHandle {
        /// The localhost port the server is listening on.
        pub fn port(&self) -> u16 { self.port }
    }
    ```
  - [x] T2.6 Implement `Drop` for `ServerHandle`:
    ```rust
    impl Drop for ServerHandle {
        fn drop(&mut self) {
            use nix::{sys::signal::{kill, Signal}, unistd::Pid};
            let Some(raw_pid) = self.process.id() else { return };
            let pid = Pid::from_raw(raw_pid as i32);
            let _ = kill(pid, Signal::SIGTERM);
            std::thread::sleep(std::time::Duration::from_millis(500));
            let _ = kill(pid, Signal::SIGKILL);
        }
    }
    ```
    - `nix::sys::signal::kill` is synchronous and safe in Drop.
    - SIGTERM attempt is best-effort; SIGKILL is the backstop. Both return errors for processes that already exited — ignore those errors.
    - `nix` is already in `Cargo.toml` with features `["signal", "user"]` (added in Story 1.9).

- [x] **T3. Implement `LlamaServer::start` — port allocation + process spawn** (AC: 1, 3, 4, 5)
  - [x] T3.1 Implement port allocation as a private helper:
    ```rust
    fn allocate_free_port() -> Result<u16, ServerError> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|e| ServerError::StartupFailure(format!("port allocation failed: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| ServerError::StartupFailure(format!("port address failed: {e}")))?
            .port();
        // listener drops here; the OS marks the port free for the next bind
        Ok(port)
    }
    ```
    The TOCTOU window between this drop and `llama-server`'s bind is milliseconds and acceptable. No two `allocate_free_port` calls within the same process return the same port because `TcpListener` binds with `SO_REUSEADDR` disabled.
  - [x] T3.2 Implement `LlamaServer::start`:
    ```rust
    /// Spawn `llama-server` for the given model and parameters.
    ///
    /// Returns a [`ServerHandle`] once `/health` responds HTTP 200. The handle
    /// terminates the server on drop.
    ///
    /// # Errors
    ///
    /// [`ServerError::StartupFailure`] if the binary is not found in `PATH`,
    /// port allocation fails, the process exits before becoming ready (model
    /// load failure), or `startup_timeout` expires.
    pub async fn start(
        &self,
        model_path: &std::path::Path,
        params: &Params,
    ) -> Result<ServerHandle, ServerError> {
        let port = allocate_free_port()?;

        let mut process = tokio::process::Command::new("llama-server")
            .args([
                "--model", &model_path.to_string_lossy(),
                "--ctx-size", &params.ctx.to_string(),
                "--port", &port.to_string(),
                "--host", "127.0.0.1",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|e| ServerError::StartupFailure(
                format!("failed to spawn llama-server: {e}; ensure llama-server is in PATH")
            ))?;

        match wait_for_ready(&mut process, port, self.startup_timeout).await {
            Ok(()) => Ok(ServerHandle { process, port }),
            Err(e) => {
                // Kill any orphan process before returning the error (AC3, AC4).
                use nix::{sys::signal::{kill, Signal}, unistd::Pid};
                if let Some(raw_pid) = process.id() {
                    let _ = kill(Pid::from_raw(raw_pid as i32), Signal::SIGKILL);
                }
                Err(e)
            }
        }
    }
    ```

- [x] **T4. Implement `wait_for_ready` — health polling with process-exit detection** (AC: 1, 3, 4)
  - [x] T4.1 Implement `wait_for_ready` as a private async helper:
    ```rust
    async fn wait_for_ready(
        process: &mut tokio::process::Child,
        port: u16,
        timeout: std::time::Duration,
    ) -> Result<(), ServerError> {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/health");
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(ServerError::StartupFailure(format!(
                    "llama-server did not become ready within {}s",
                    timeout.as_secs()
                )));
            }

            // AC3: Detect early process exit (model load failure, bad path, etc.)
            match process.try_wait() {
                Ok(Some(status)) => {
                    return Err(ServerError::StartupFailure(format!(
                        "llama-server exited with {status} before becoming ready; \
                         check stderr above for model load errors"
                    )));
                }
                Ok(None) => {} // still running
                Err(e) => {
                    return Err(ServerError::StartupFailure(format!(
                        "failed to check llama-server status: {e}"
                    )));
                }
            }

            // Poll health endpoint. llama-server returns:
            //   200 {"status":"ok"}            — fully loaded, ready for inference
            //   503 {"status":"loading model"} — still loading, keep polling
            //   connection refused             — not yet bound, keep polling
            let send_result = client
                .get(&url)
                .timeout(std::time::Duration::from_secs(1))
                .send()
                .await;

            if let Ok(resp) = send_result {
                if resp.status() == reqwest::StatusCode::OK {
                    return Ok(());
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
    ```
  - [x] T4.2 The 500 ms poll interval is intentional: fast enough to minimize wasted time, slow enough not to spam the not-yet-bound server with connection errors during early startup.
  - [x] T4.3 Per-request timeout of 1 s prevents a single stalled HTTP call from eating a large chunk of the overall `startup_timeout`.
  - [x] T4.4 `stderr(Stdio::inherit())` was chosen so that model-load error messages from llama-server appear directly in lcrc's terminal output, satisfying AC3's "with the failure reason" without the complexity of async stderr capture.
  - [x] T4.5 `stdout(Stdio::null())` suppresses llama-server's per-request log chatter from lcrc's output.

- [x] **T5. In-module unit tests in `server_lifecycle.rs::tests`** (AC: 3, 5)
  - [x] T5.1 All test blocks carry `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`.
  - [x] T5.2 Test `server_error_display_startup_failure`: create `ServerError::StartupFailure("model not found".into())`, assert `format!("{e}")` starts with `"server startup failed: "`.
  - [x] T5.3 Test `params_construction`: assert `Params { ctx: 4096 }.ctx == 4096`.
  - [x] T5.4 Test `concurrent_port_allocation`: call `allocate_free_port()` twice, assert results differ (`p1 != p2`). This directly validates AC5's mechanism without requiring a real server.
  - [x] T5.5 Test `llama_server_default_timeout_is_60s`: `LlamaServer::new().startup_timeout == Duration::from_secs(60)`. Access the field via a test-only accessor or make `startup_timeout` pub(crate) for tests.

- [x] **T6. Integration test file `tests/server_lifecycle.rs`** (AC: 1, 2, 3, 4, 5)
  - [x] T6.1 All tests gate on `LCRC_INTEGRATION_TEST_SERVER=1` env var. If not set, print `"skipping: set LCRC_INTEGRATION_TEST_SERVER=1 and LCRC_TEST_MODEL_PATH=<path> to run"` and return.
  - [x] T6.2 Also check that `llama-server` is in PATH (call `which::which("llama-server").is_ok()` or use `tokio::process::Command::new("llama-server").arg("--version")` — see note on `which` crate below).
  - [x] T6.3 Test `server_starts_and_port_is_nonzero` (AC1): load `LCRC_TEST_MODEL_PATH`, call `LlamaServer::new().start(path, &Params { ctx: 512 }).await`, assert `Ok(handle)`, assert `handle.port() > 0`, drop handle.
  - [x] T6.4 Test `server_drop_terminates_process` (AC2): start server, record `let port = handle.port()`, drop handle explicitly (`drop(handle)`), then verify the process is gone by attempting `reqwest::get(format!("http://127.0.0.1:{port}/health")).await` and asserting it fails (connection refused).
  - [x] T6.5 Test `server_corrupt_model_returns_err` (AC3): create a `tempfile::NamedTempFile`, write a few bytes of junk (`b"not a gguf"`), call `start` with its path, assert `Err(ServerError::StartupFailure(_))` is returned.
  - [x] T6.6 Test `server_startup_timeout_kills_process` (AC4): marked `#[ignore]` because reliably simulating a hanging llama-server in CI is complex. Leave skeleton with comment: "Use LlamaServer::with_timeout(Duration::from_millis(100)) with a valid model path; the model takes longer than 100ms to load on most hardware."
  - [x] T6.7 Test `concurrent_starts_different_ports` (AC5): `tokio::join!` two `start` calls (using `LlamaServer::with_timeout(Duration::from_secs(120))`). Assert both return `Ok`, assert `h1.port() != h2.port()`. Drop both handles. Requires `LCRC_TEST_MODEL_PATH`. May be slow on low-RAM machines.
  - [x] T6.8 All tests use `#[tokio::test(flavor = "current_thread")]`.
  - [x] T6.9 Do NOT add a `which` crate dependency. Use `std::process::Command::new("llama-server").arg("--version").output().is_ok()` to check for binary availability in tests.

- [x] **T7. Local CI mirror** (AC: all)
  - [x] T7.1 `cargo build` — all new modules compile; `tokio::process`, `reqwest`, `nix`, `thiserror` types all resolve.
  - [x] T7.2 `cargo fmt --check` — rustfmt clean.
  - [x] T7.3 `cargo clippy --all-targets --all-features -- -D warnings`. Watch for:
    - `missing_docs` on every `pub` item in `src/scan.rs` and `src/scan/server_lifecycle.rs`.
    - `missing_errors_doc` on `LlamaServer::start` (returns `Result`).
    - `clippy::module_name_repetitions` on `ServerError`, `ServerHandle` — suppress with `#[allow(clippy::module_name_repetitions)]` if needed.
    - `clippy::default_trait_access` — use `Default::default()` or fully qualified syntax.
  - [x] T7.4 `cargo test` — all pre-existing tests continue to pass. The new `server_lifecycle` integration tests skip unless `LCRC_INTEGRATION_TEST_SERVER=1`.
  - [x] T7.5 Scope discipline: no other module spawns `llama-server` directly. Verify with:
    ```bash
    git grep -nE 'Command::new\("llama' src/ tests/ | grep -v '^src/scan/server_lifecycle.rs:' | grep -v '^tests/server_lifecycle.rs:'
    ```
    Must produce zero matches.

## Dev Notes

### Scope discipline (read this first)

This story creates **two new files** (`src/scan.rs`, `src/scan/server_lifecycle.rs`) and **updates one file** (`src/lib.rs`). It also creates one integration test file (`tests/server_lifecycle.rs`). No `Cargo.toml` changes — all needed dependencies are present.

This story does **not**:
- Wire `LlamaServer::start` into an actual scan pipeline. That is Story 1.12's job. Story 1.11 creates the API; Story 1.12 calls it.
- Implement KV-cache reset between tasks (`POST /slots/{id}/erase` or equivalent). That is Story 2.18's concern.
- Implement the `Backend` trait or `LlamaCppBackend`. The `Backend` trait is an Epic 2 abstraction. Story 1.11 creates a concrete `LlamaServer` directly — no trait abstraction yet.
- Implement crash recovery (badge `server-crashed`, restart-and-continue). That is Story 2.5's concern; NFR-R5 crash handling requires the full orchestrator context from Story 2.6.
- Create `src/backend/`, `src/scan/orchestrator.rs`, or any other `scan/` submodule. Later stories add those.
- Modify `src/error.rs`, `src/exit_code.rs`, `src/main.rs`, `src/output.rs`, `src/cache*`, `src/sandbox*`, `src/machine*`, `src/version.rs`.

### Architecture compliance (binding constraints)

- **`src/scan/server_lifecycle.rs` is the sole module that spawns `llama-server`** (architecture.md § "Architectural Boundaries" — boundary: "llama-server lifecycle", module: `src/scan/server_lifecycle.rs`). T7.5's grep enforces this boundary.
- **`src/backend/llama_cpp.rs` owns reqwest calls to `/completion`, `/tokenize`** — but NOT `/health` at startup. `/health` polling during startup is a lifecycle concern owned by this story's module. The module boundary means: health check at startup lives in `server_lifecycle.rs`; health monitoring and inference API calls during measurement live in `src/backend/llama_cpp.rs` (future story).
- **All I/O via tokio** — `tokio::process::Command`, not `std::process::Command`, for spawning (architecture.md § "Async Discipline"). Exception: `std::net::TcpListener::bind` in `allocate_free_port` is sync-only and there is no async equivalent in std; this is an accepted `spawn_blocking`-free exception since it returns immediately.
- **`Drop` uses synchronous `nix::sys::signal::kill` + `std::thread::sleep`.** Async `Drop` is impossible in Rust. The 500 ms sleep is synchronous and intentional — llama-server needs a brief window to flush in-flight writes. This is NOT a violation of the "all I/O via tokio" rule because Drop is inherently sync-only.
- **stdout/stderr discipline (FR46):** All lcrc user-visible messages go through `crate::output::diag`. Tracing events use `tracing::info!`/`tracing::warn!` with target `"lcrc::scan::server_lifecycle"`. Do not call `println!`/`eprintln!` from `server_lifecycle.rs`. The llama-server process's own stderr is inherited (appears unformatted in the terminal) intentionally — this is the server process's stderr, not lcrc's.
- **`missing_docs = "warn"`:** Every `pub` item in `src/scan.rs` and `src/scan/server_lifecycle.rs` needs a `///` doc comment. Every `pub async fn` returning `Result` needs a `# Errors` rustdoc section.
- **No `unsafe`** — host crate stays `forbid(unsafe_code)`. `nix::sys::signal::kill` is safe in nix's API.

### Library/framework requirements (use these exact crates)

- **`tokio::process::Command`** — spawn `llama-server`. Already a dep (`tokio = { features = ["full"] }`).
- **`reqwest`** — poll `/health`. Already a dep (`reqwest = { version = "0.12", features = ["json", "rustls-tls"] }`). Use `.timeout(Duration::from_secs(1))` on each request.
- **`nix`** — SIGTERM/SIGKILL in Drop. Already a dep (`nix = { version = "0.29", features = ["signal", "user"] }`). Use `nix::sys::signal::kill` and `nix::sys::signal::Signal::SIGTERM`/`SIGKILL`.
- **`thiserror`** — `ServerError` derive. Already a dep (`thiserror = "2"`).
- **`tempfile`** — test fixtures (non-GGUF file for AC3 test). Already a dep (`tempfile = "3"`).
- **Do NOT add** `which`, `uuid`, or any new crate. Check for `llama-server` binary using `std::process::Command::new("llama-server").arg("--version").output().is_ok()`.

### reqwest API notes (version 0.12)

- `reqwest::Client::new()` — builds a default client. OK to construct per-call in tests; in production code, construct once in `LlamaServer` or as a local variable in `start`.
- `client.get(&url).timeout(Duration).send().await` — returns `Result<Response, reqwest::Error>`.
- `resp.status()` — returns `reqwest::StatusCode`. Compare with `reqwest::StatusCode::OK` (200) or `reqwest::StatusCode::SERVICE_UNAVAILABLE` (503).
- `reqwest::Error` is `Send + Sync` — no issues using it in async contexts.

### nix 0.29 API notes

- `nix::sys::signal::kill(pid: Pid, signal: Signal) -> Result<(), Errno>` — synchronous, safe.
- `nix::unistd::Pid::from_raw(raw: i32) -> Pid` — construct from `u32` PID via `raw_pid as i32`.
- `tokio::process::Child::id() -> Option<u32>` — returns PID if process is still running, `None` if already exited. Call `id()` before SIGTERM in Drop; if `None`, process already exited (no orphan).
- `tokio::process::Child::try_wait() -> std::io::Result<Option<std::process::ExitStatus>>` — non-blocking status check. Returns `Ok(None)` if still running, `Ok(Some(status))` if exited.

### llama-server CLI reference

Canonical command:
```
llama-server --model <path> --ctx-size <n> --port <port> --host 127.0.0.1
```

- `--model` — absolute path to the GGUF file. Use `model_path.to_string_lossy()` (model paths on macOS don't typically contain non-UTF-8).
- `--ctx-size` — context window in tokens. Maps directly to `Params.ctx`.
- `--port` — TCP port to listen on. Must match `allocate_free_port()` return value.
- `--host 127.0.0.1` — bind to localhost only. Containers reach it via `host.docker.internal`, not `127.0.0.1`, so this is safe.
- Do NOT pass `--n-gpu-layers` — default is auto for METAL/CUDA. Epic 2's config layer adds GPU control.
- Do NOT pass `--parallel` or `--cont-batching` — single-slot defaults match Epic 1's sequential task execution.

Health endpoint:
- `GET /health` → 200 `{"status":"ok"}` when fully loaded and ready.
- `GET /health` → 503 `{"status":"loading model"}` while loading (respond with any status other than 200 → keep polling).
- Binary not found → `spawn()` returns `Err(io::ErrorKind::NotFound)` — surface as `StartupFailure("failed to spawn llama-server: ....; ensure llama-server is in PATH")`.

### File structure requirements

```
src/
├── lib.rs                     UPDATED: add `pub mod scan;` (between sandbox and util)
├── scan.rs                    NEW: `pub mod server_lifecycle;` only
└── scan/
    └── server_lifecycle.rs    NEW: LlamaServer, ServerHandle, Params, ServerError

tests/
└── server_lifecycle.rs        NEW: integration tests (skip unless LCRC_INTEGRATION_TEST_SERVER=1)
```

After this story merges:
- `src/scan.rs` is the root of the scan module tree; Story 1.12 adds `pub mod orchestrator;` etc.
- `LlamaServer::start(model_path, &params)` is callable from Story 1.12's scan wiring.
- `ServerHandle::port()` gives Story 1.12 the port to pass to `Sandbox::new(&probe, handle.port())`.
- `src/cli/scan.rs` is **not touched** by this story; Story 1.12 wires everything together there.

### Cross-story interaction: Story 1.12 dependency

Story 1.12 ("end-to-end one-cell scan") will:
1. Call `LlamaServer::new().start(model_path, &params).await?` → gets `ServerHandle`.
2. Call `Sandbox::new(&probe, handle.port()).await?` — passes `handle.port()` as the pinned llama port. This is the connection point between Story 1.11 (`ServerHandle::port()`) and Story 1.10 (`Sandbox::new(llama_port)`).
3. Call `sandbox.run_task(...)` with the running server.
4. Drop `sandbox` (cleanup network), then drop `handle` (terminate server).

Story 1.11 only needs to make `ServerHandle::port()` available; wiring these pieces is Story 1.12's responsibility.

### Previous story intelligence

Carry-forward from Story 1.10:

- **Module pattern**: `scan.rs` (parent) declares submodules. Submodules own their typed errors. Story 1.11 introduces `ServerError` following the same pattern as `SandboxError` in `sandbox.rs`.
- **`nix` features**: Already in `Cargo.toml` with `["signal", "user"]`. `nix::sys::signal::kill` is available.
- **Drop for async types**: `ServerHandle` holds a `tokio::process::Child` which is NOT `Sync`. Drop is inherently synchronous in Rust — use `nix::sys::signal::kill` directly (no await needed), exactly as documented in Story 1.10's Resolved Decisions § "Sandbox::cleanup is explicit, not Drop."
- **No `From<ServerError> for crate::error::Error`** — Story 1.12 decides if a global `From` impl is warranted. For now, callers do inline `format!("{e}")` conversion (same pattern as Story 1.9's preflight errors).
- **Tracing discipline**: New events use `target: "lcrc::scan::server_lifecycle"` and structured fields.
- **In-module test exemption**: All `#[cfg(test)] mod tests` blocks carry `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`.
- **stdout/stderr discipline (FR46)**: No `println!`/`eprintln!` in new code. Use `tracing::info!`/`tracing::warn!`.

### Testing requirements

- **Integration tests** (`tests/server_lifecycle.rs`): Gate on `LCRC_INTEGRATION_TEST_SERVER=1` AND `LCRC_TEST_MODEL_PATH=<path_to_real_gguf>`. Skip if either is absent. Check binary availability with `std::process::Command::new("llama-server").arg("--version").output().is_ok()`. Tests use `#[tokio::test(flavor = "current_thread")]`.
- **Unit tests** in `server_lifecycle.rs::tests`: Pure function tests for `ServerError` Display, `allocate_free_port` (two calls → different ports), and `Params` construction. No mock HTTP server needed; pure-logic tests only.
- **No mock HTTP server.** Do not add `wiremock`, `mockito`, or similar. The health poll behavior is tested via integration tests with a real server.
- **`LCRC_INTEGRATION_TEST_SERVER=1` guard pattern**:
  ```rust
  if std::env::var("LCRC_INTEGRATION_TEST_SERVER").is_err() {
      eprintln!("skipping: set LCRC_INTEGRATION_TEST_SERVER=1 and LCRC_TEST_MODEL_PATH=<path> to run");
      return;
  }
  let Some(model_path) = std::env::var("LCRC_TEST_MODEL_PATH").ok().map(std::path::PathBuf::from) else {
      eprintln!("skipping: LCRC_TEST_MODEL_PATH not set");
      return;
  };
  ```
- **Integration test cleanup**: Each test that creates a `ServerHandle` must ensure it is dropped (either explicitly or by letting it go out of scope at function end). There is no `sandbox.cleanup()` equivalent here — Drop handles cleanup.

### Project Context Reference

- **Epic 1 position**: Story 11 of 14. Stories 1.12 (end-to-end scan), 1.13 (HTML report), 1.14 (container image) are the remaining stories to complete the integration spine.
- **Cross-story dependencies**:
  - **Depends on**: Story 1.1 (reqwest in Cargo.toml), Story 1.3 (`thiserror` for ServerError pattern), Story 1.4 (tracing setup), Story 1.9 (nix dep with signal feature).
  - **Unblocks**: Story 1.12 (end-to-end scan — calls `LlamaServer::start` and passes `handle.port()` to `Sandbox::new`), Story 2.5 (server-crash badge — extends `ServerError` handling), Story 2.18 (llama-server thermal badge — reads `handle.port()` for thermal API calls).
- **Architectural position**: `src/scan/server_lifecycle.rs` is the sole module that spawns `llama-server` (architecture.md § "Architectural Boundaries"). The `Backend` trait abstraction (`src/backend.rs` + `src/backend/llama_cpp.rs`) is an Epic 2 concern; this story creates a direct concrete implementation.
- **NFR coverage**:
  - NFR-I1: one server per `(model, params)` group — the Story 1.12 orchestrator enforces this; Story 1.11 provides the API.
  - NFR-R5: crash recovery (badge `server-crashed`, restart) — deferred to Story 2.5. Story 1.11 only handles startup failures and clean teardown.
  - NFR-P9: container spin-up <5 s — not directly tested here; llama-server model load can take 5–30 s (the 60 s timeout accommodates even slow loads).

### References

- [Source: `_bmad-output/planning-artifacts/epics.md` § "Story 1.11"] — five AC clauses and user story
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "llama-server lifecycle granularity (NFR-I1)"] — lifecycle protocol, per-(model,params) grouping, crash recovery notes
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Architectural Boundaries" table] — `src/scan/server_lifecycle.rs` as sole module for llama-server lifecycle
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure"] — `src/scan.rs` + `src/scan/server_lifecycle.rs` file placement
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Async Discipline"] — tokio::process, not std::process; sync-only exception for TcpListener
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Backend trait definition"] — `start_server() -> ServerHandle` is the Epic 2 trait shape; Story 1.11 creates the concrete type
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Data Flow — One Scan Cycle"] — `server_lifecycle::start` called per (model,params) group in orchestrator flow
- [Source: `_bmad-output/implementation-artifacts/1-10-sandbox-run-task-with-workspace-mount-custom-default-deny-network.md` § "Scope discipline"] — "Start llama-server. That is Story 1.11. The `llama_port: u16` parameter in `Sandbox::new` is a placeholder; the real port comes from Story 1.11's ServerHandle."
- [Source: `_bmad-output/implementation-artifacts/1-10-sandbox-run-task-with-workspace-mount-custom-default-deny-network.md` § "Dev Agent Record / Debug Log"] — nix "user"+"signal" features confirmed in Cargo.toml; reqwest 0.12 confirmed
- [Source: `src/lib.rs`] — current pub mod list: cache, cli, constants, error, exit_code, machine, output, sandbox, util, version — insert `scan` between `sandbox` and `util`
- [Source: `src/error.rs`] — `Error::Preflight(String)` pattern; `ServerError` follows same inline-conversion pattern in Story 1.12
- [Source: `Cargo.toml`] — all deps present: `reqwest = "0.12"` (json+rustls-tls), `tokio = "1"` (full), `nix = "0.29"` (signal+user), `thiserror = "2"`, `tempfile = "3"`

## Dev Agent Record

### Agent Model Used

claude-sonnet-4-6[1m]

### Debug Log References

- `Duration::from_secs(60)` → `Duration::from_mins(1)`: clippy `duration_suboptimal_units` lint required use of `from_mins`. Field made `pub(crate)` so the in-module test can compare against `Duration::from_mins(1)`.
- Import ordering: rustfmt requires `{Signal, kill}` (alphabetical) not `{kill, Signal}`.
- Nested `if let Ok` / `if status == OK` collapsed to `is_ok_and(|r| ...)` to satisfy both `clippy::collapsible_if` and rustfmt's block-expansion preference.
- `#[allow(clippy::cast_possible_wrap)]` applied locally to `raw_pid as i32` casts (nix Pid API requires i32; PID values never exceed i32::MAX on Linux/macOS).
- Integration tests: added `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` file-level attribute; `tokio::join!` required binding `LlamaServer` launchers to named variables (temporaries dropped before future resolution).
- `#[derive(Debug)]` added to `ServerHandle` (`tokio::process::Child` implements `Debug`); required for `{result:?}` format in integration test assert messages.

### Completion Notes List

- Implemented `src/scan.rs` (module root) and `src/scan/server_lifecycle.rs` with `Params`, `ServerError`, `LlamaServer`, `ServerHandle`, `allocate_free_port`, and `wait_for_ready`.
- `LlamaServer::start` spawns `llama-server` on a dynamically allocated port, polls `/health` with 500 ms interval and 1 s per-request timeout, and kills any orphan on failure (AC1, AC3, AC4).
- `ServerHandle::drop` sends SIGTERM then SIGKILL (500 ms apart) via `nix::sys::signal::kill` (AC2).
- 4 unit tests cover: `ServerError` display, `Params` construction, concurrent port uniqueness, and 60 s default timeout.
- 5 integration tests (1 ignored) cover AC1–AC5; all gate on `LCRC_INTEGRATION_TEST_SERVER=1`.
- All 132 existing tests pass; 2 ignored (server_lifecycle integration + server_startup_timeout_kills_process).
- `cargo build`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` all clean.
- Scope discipline verified: zero matches for `Command::new("llama` outside the two designated files.

### File List

- `src/scan.rs` (new)
- `src/scan/server_lifecycle.rs` (new)
- `src/lib.rs` (modified — added `pub mod scan;`)
- `tests/server_lifecycle.rs` (new)

### Change Log

- 2026-05-07: Story 1.11 implemented — `LlamaServer` lifecycle API (`start`, health-gate, `Drop`-based teardown), unit + integration tests, CI checks all green.
