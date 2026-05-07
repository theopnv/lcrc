//! llama-server process lifecycle — spawn, health-gate, and teardown.
//!
//! [`LlamaServer`] owns startup configuration. [`ServerHandle`] represents a
//! running server instance and terminates the process on [`Drop`].
//!
//! The server runs on the host (per NFR-I1) so containers reach it via
//! `host.docker.internal` on the per-scan constrained network.

/// Parameters passed to `llama-server` at startup.
///
/// Maps directly to CLI flags: `--ctx-size` is the only parameter
/// that varies per `(model, params)` group in Epic 1.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    /// Context window size in tokens, passed as `--ctx-size`.
    pub ctx: u32,
}

/// Errors produced by [`LlamaServer::start`].
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// llama-server process failed to become ready (model load failure,
    /// startup timeout, spawn error, or port allocation failure).
    #[error("server startup failed: {0}")]
    StartupFailure(String),
}

/// Configuration for spawning a `llama-server` process.
///
/// Construct with [`LlamaServer::new`] (60 s default timeout) or
/// [`LlamaServer::with_timeout`] for tests that need a tighter deadline.
pub struct LlamaServer {
    pub(crate) startup_timeout: std::time::Duration,
}

impl LlamaServer {
    /// Create a new launcher with the default 60-second startup timeout.
    #[must_use]
    pub fn new() -> Self {
        Self {
            startup_timeout: std::time::Duration::from_mins(1),
        }
    }

    /// Create a new launcher with a custom startup timeout.
    #[must_use]
    pub fn with_timeout(timeout: std::time::Duration) -> Self {
        Self {
            startup_timeout: timeout,
        }
    }

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
                "--model",
                &model_path.to_string_lossy(),
                "--ctx-size",
                &params.ctx.to_string(),
                "--port",
                &port.to_string(),
                "--host",
                "127.0.0.1",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|e| {
                ServerError::StartupFailure(format!(
                    "failed to spawn llama-server: {e}; ensure llama-server is in PATH"
                ))
            })?;

        match wait_for_ready(&mut process, port, self.startup_timeout).await {
            Ok(()) => Ok(ServerHandle { process, port }),
            Err(e) => {
                use nix::{
                    sys::signal::{Signal, kill},
                    unistd::Pid,
                };
                if let Some(raw_pid) = process.id() {
                    #[allow(clippy::cast_possible_wrap)]
                    let _ = kill(Pid::from_raw(raw_pid as i32), Signal::SIGKILL);
                }
                Err(e)
            }
        }
    }
}

impl Default for LlamaServer {
    fn default() -> Self {
        Self::new()
    }
}

/// A running `llama-server` instance.
///
/// Dropping this handle sends SIGTERM to the server process, waits 500 ms,
/// then sends SIGKILL if the process has not exited. This is synchronous
/// (async `Drop` is not possible in Rust); the 500 ms sleep is intentional
/// and brief relative to model-load cost.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct ServerHandle {
    process: tokio::process::Child,
    port: u16,
}

impl ServerHandle {
    /// The localhost port the server is listening on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        use nix::{
            sys::signal::{Signal, kill},
            unistd::Pid,
        };
        let Some(raw_pid) = self.process.id() else {
            return;
        };
        #[allow(clippy::cast_possible_wrap)]
        let pid = Pid::from_raw(raw_pid as i32);
        let _ = kill(pid, Signal::SIGTERM);
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = kill(pid, Signal::SIGKILL);
    }
}

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

        match process.try_wait() {
            Ok(Some(status)) => {
                return Err(ServerError::StartupFailure(format!(
                    "llama-server exited with {status} before becoming ready; \
                     check stderr above for model load errors"
                )));
            }
            Ok(None) => {}
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

        if send_result.is_ok_and(|r| r.status() == reqwest::StatusCode::OK) {
            return Ok(());
        }

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn server_error_display_startup_failure() {
        let e = ServerError::StartupFailure("model not found".into());
        let msg = format!("{e}");
        assert!(
            msg.starts_with("server startup failed: "),
            "unexpected display: {msg}"
        );
    }

    #[test]
    fn params_construction() {
        assert_eq!(Params { ctx: 4096 }.ctx, 4096);
    }

    #[test]
    fn concurrent_port_allocation() {
        let p1 = allocate_free_port().unwrap();
        let p2 = allocate_free_port().unwrap();
        assert_ne!(p1, p2, "two allocations returned the same port");
    }

    #[test]
    fn llama_server_default_timeout_is_60s() {
        let server = LlamaServer::new();
        assert_eq!(server.startup_timeout, Duration::from_mins(1));
    }
}
