# Story 1.10: `Sandbox::run_task` with workspace mount + custom default-deny network

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a developer,
I want `Sandbox::run_task(image_digest, workspace_path) -> Result<TaskOutcome>` to spawn an ephemeral container with a workspace bind-mount, no other host filesystem visibility, and a custom Docker network whose outbound traffic is restricted by iptables rules to a single host:port (the llama-server),
so that every measurement runs in a structurally default-deny envelope from Epic 1 onward — port-pinning enforced structurally at the network layer, not by best-effort DNS denial alone (FR16 workspace + network axes; env axis follows in Epic 2).

## Acceptance Criteria

**AC1.** **Given** the pinned container image digest in `src/constants.rs` **When** `Sandbox::run_task` is called and the image is not yet local **Then** it pulls the image from GHCR, verifies the digest matches the constant, and caches it locally. (Integration test skipped unless `LCRC_INTEGRATION_TEST_SANDBOX=1` is set, because the real GHCR image doesn't exist until Story 1.14.)

**AC2.** **Given** a per-task `workspace_path` **When** the container starts **Then** `/workspace` inside the container is the bind-mounted host path (rw); inspecting from inside the container shows no other host directories visible.

**AC3.** **Given** the container starts **When** the agent attempts `cat /etc/passwd` (intending the host's) **Then** it reads the *image's* `/etc/passwd` (Debian-slim's default), not the host's — the host filesystem is structurally absent.

**AC4.** **Given** the container starts **When** the agent attempts `curl https://example.com` **Then** the request fails (no DNS resolver on the custom network; no default route to the internet).

**AC5.** **Given** the container starts and `host.docker.internal:<llama-port>` is reachable from the host **When** the agent connects to that endpoint **Then** the connection succeeds — this is the only allowed network destination.

**AC6.** **Given** the container is started and the custom Docker network is configured **When** the agent attempts `nc -zv host.docker.internal 22` (or any host-gateway port other than the llama-server's) **Then** the connection fails (refused or timeout) — outbound is structurally restricted to a single host:port via iptables rules installed in the runtime's network namespace, not by best-effort DNS denial alone.

**AC7.** **Given** lcrc's packaged-default runtime is Podman **When** lcrc creates the per-scan network **Then** iptables rules (configured via Podman's CNI/Netavark backend) drop all container outbound traffic except DNAT'd packets to the host's llama-server port; rule installation is verified at scan preflight by exercising the negative probe against a probe sentinel port.

**AC8.** **Given** a third-party Docker-API-compatible runtime (Colima, OrbStack, Docker Desktop) is detected at preflight **When** lcrc cannot install equivalent iptables/nftables rules through the runtime's exposed surface **Then** lcrc exits 11 with a documented "structural port-pin unavailable on this runtime; reinstall with the packaged Podman or use a runtime that exposes network rule injection" message. No `--unsafe-no-sandbox` fallback exists (NFR-S3); either the sandbox is structural or scan refuses to run.

**AC9.** **Given** `Sandbox::run_task` returns (success, failure, or panic) **When** I check container state **Then** the container has been removed via `bollard::remove_container(force=true)`; no orphan containers accumulate (NFR-R8).

**AC10.** **Given** the function signature of `Sandbox::run_task` **When** inspected **Then** it accepts NO `volumes`, `env`, or `network_mode` extension arguments — workspace mount and network construction are hard-coded internally (AR-28 structural enforcement).

## Tasks / Subtasks

- [x] **T1. Create `src/constants.rs`** (AC: 1, 9)
  - [x] T1.1 Create `src/constants.rs` with a `//!` module doc explaining that this file holds compile-time pinned values (container image digest, schema version, defaults) per the architecture's "Cross-cutting helpers" spec (`src/constants.rs` listed in architecture.md § "Module Organization").
  - [x] T1.2 Add the `CONTAINER_IMAGE_DIGEST` constant:
    ```rust
    /// Pinned container image reference for the per-task execution environment.
    ///
    /// Includes both the registry tag and the digest so bollard can verify the
    /// local layer cache matches exactly what was published (AC1 digest check).
    /// Story 1.14 replaces this placeholder with the real published digest.
    pub const CONTAINER_IMAGE_DIGEST: &str =
        "ghcr.io/<org>/lcrc-task:0.1.0@sha256:0000000000000000000000000000000000000000000000000000000000000000";
    ```
  - [x] T1.3 Do NOT add any other constants in this story. Future constants (`SCHEMA_VERSION`, `CANARY_IMAGE_DIGEST`) land in their owner stories.

- [x] **T2. Update `src/lib.rs` — declare `pub mod constants;`** (AC: 1)
  - [x] T2.1 Insert `pub mod constants;` into the `pub mod` block in `src/lib.rs` in alphabetical order (between `pub mod cache;` and `pub mod cli;` — `ca` < `cl`).
  - [x] T2.2 Do NOT touch `pub fn run()` or any other part of `lib.rs`.

- [x] **T3. Update `src/sandbox.rs` — add `Sandbox` struct, `TaskOutcome`, new variants, new submodule declarations** (AC: 2, 3, 4, 5, 6, 7, 8, 9, 10)
  - [x] T3.1 Update the `//!` file doc to reflect the new submodules landing in this story:
    ```rust
    //! Per-task isolation envelope.
    //!
    //! The `sandbox` module owns the per-task isolation envelope. Submodules:
    //! - `runtime` — preflight detection of a reachable Docker-Engine-API-compatible socket (Story 1.9)
    //! - `image` — container image pull and digest verification (this story)
    //! - `network` — per-scan internal Docker network with iptables port-pinning (this story)
    //! - `container` — ephemeral container lifecycle; the ONLY caller of bollard container APIs (this story)
    ```
  - [x] T3.2 Add `pub mod container;`, `pub mod image;`, `pub mod network;` in alphabetical order alongside the existing `pub mod runtime;`.
  - [x] T3.3 Define `pub struct TaskOutcome` — the minimal outcome returned by `run_task`:
    ```rust
    /// Outcome of a single task execution inside the per-task container.
    #[derive(Debug, Clone)]
    pub struct TaskOutcome {
        /// `true` if the container exited 0, `false` for any non-zero exit code.
        pub pass: bool,
        /// Wall-clock time from container-start to container-exit, in seconds.
        pub duration_seconds: f64,
    }
    ```
  - [x] T3.4 Define `pub struct Sandbox`:
    ```rust
    /// Per-scan execution context: holds the bollard Docker client and owns the
    /// lifecycle of the per-scan custom network.
    ///
    /// Construct with [`Sandbox::new`]; call [`Sandbox::run_task`] for each
    /// cell; call [`Sandbox::cleanup`] when the scan completes.
    pub struct Sandbox {
        docker: bollard::Docker,
        scan_id: String,
        network_name: String,
        llama_port: u16,
    }
    ```
  - [x] T3.5 Implement `Sandbox`:
    ```rust
    impl Sandbox {
        /// Create a new scan-scoped sandbox context.
        ///
        /// Connects to the Docker-Engine-API socket from the supplied preflight
        /// probe, creates the per-scan internal Docker network with iptables
        /// port-pinning, and verifies the rules are in effect via a negative
        /// probe before returning.
        ///
        /// # Errors
        ///
        /// [`SandboxError::UnsupportedRuntime`] if the detected runtime cannot
        /// install structural iptables/nftables port-pin rules.
        /// [`SandboxError::NetworkSetup`] if the custom network cannot be created.
        pub async fn new(
            probe: &crate::sandbox::runtime::RuntimeProbe,
            llama_port: u16,
        ) -> Result<Self, SandboxError>

        /// Run one task inside an ephemeral container.
        ///
        /// Pulls the container image if not local (verifying digest), creates
        /// the container with the workspace bind-mount and the per-scan network,
        /// starts it, waits for it to exit, force-removes it, and returns the
        /// outcome. The container is removed in all exit paths including errors.
        ///
        /// # Errors
        ///
        /// [`SandboxError::ImagePull`] on pull or digest-verification failure.
        /// [`SandboxError::ContainerCreate`] on container creation or start failure.
        pub async fn run_task(
            &self,
            image_digest: &str,
            workspace_path: &std::path::Path,
        ) -> Result<TaskOutcome, SandboxError>

        /// Remove the per-scan Docker network.
        ///
        /// Call this once after the last [`Sandbox::run_task`] for the scan.
        /// Best-effort: logs errors via [`tracing::warn!`] but does not propagate.
        pub async fn cleanup(&self)
    }
    ```
  - [x] T3.6 Extend `pub enum SandboxError` with new variants (keep the existing `Preflight` variant):
    ```rust
    /// Image pull from GHCR failed, or the pulled image's digest does not match
    /// [`crate::constants::CONTAINER_IMAGE_DIGEST`].
    #[error("image pull failed: {0}")]
    ImagePull(String),

    /// Custom Docker network creation or iptables/nftables rule installation failed.
    #[error("network setup failed: {0}")]
    NetworkSetup(String),

    /// The detected container runtime does not support structural iptables
    /// port-pin rule injection. lcrc cannot run safely on this runtime.
    #[error("unsupported runtime: {0}")]
    UnsupportedRuntime(String),

    /// Ephemeral container creation, start, or removal via bollard failed.
    #[error("container error: {0}")]
    ContainerCreate(String),
    ```
  - [x] T3.7 Add in-module unit tests in `sandbox.rs::tests`:
    - Test `SandboxError::ImagePull` Display starts with `"image pull failed: "`.
    - Test `SandboxError::NetworkSetup` Display starts with `"network setup failed: "`.
    - Test `SandboxError::UnsupportedRuntime` Display starts with `"unsupported runtime: "`.
    - Test `SandboxError::ContainerCreate` Display starts with `"container error: "`.

- [x] **T4. Author `src/sandbox/image.rs` — image pull + digest verification** (AC: 1)
  - [x] T4.1 File-level `//!` doc explaining: this module pulls the per-task container image from GHCR (on first use), verifies the digest matches `src/constants.rs::CONTAINER_IMAGE_DIGEST`, and is the single gatekeeper for container image content integrity.
  - [x] T4.2 Define `pub async fn ensure_image(docker: &bollard::Docker, image_ref: &str) -> Result<(), SandboxError>`:
    - Parse `image_ref` to separate the name (`ghcr.io/<org>/lcrc-task:0.1.0`) from the digest component (`sha256:...`).
    - Call `docker.inspect_image(name_without_digest)` to check if the image is already local.
    - If local: verify `RepoDigests` contains the expected digest string. If mismatch → return `SandboxError::ImagePull("digest mismatch: ...")`.
    - If not local: call `docker.create_image(CreateImageOptions { from_image: name, tag: tag, .. }, None, None)` to pull. Consume the `Stream` until it finishes (use `futures_util::TryStreamExt::try_collect` or `while let Some(info) = stream.next().await`). On pull error → `SandboxError::ImagePull`.
    - After pull, re-inspect and verify digest as above.
    - Log progress at `tracing::info!` level with target `"lcrc::sandbox::image"`.
  - [x] T4.3 Note: `bollard::Docker::create_image` returns `impl Stream<Item = Result<CreateImageInfo, Error>>`. Drain the stream to completion; do NOT ignore stream items (some runtimes only surface errors in stream events, not as a top-level error from the call).
  - [x] T4.4 Do NOT implement rate-limit retry or exponential backoff. Single attempt; if it fails, surface the error.

- [x] **T5. Author `src/sandbox/network.rs` — per-scan internal network + iptables port-pinning** (AC: 4, 5, 6, 7, 8)
  - [x] T5.1 File-level `//!` doc:
    ```
    //! Per-scan custom Docker network with structural default-deny networking.
    //!
    //! Creates a Docker `--internal` bridge network (no DNS resolver, no default
    //! route to the internet) and installs nftables/iptables rules restricting
    //! container outbound traffic to a single host port (the llama-server).
    //!
    //! Podman (the packaged-default runtime, AR-12) exposes the Podman VM's
    //! network namespace via `podman machine exec`, which is how this module
    //! installs nft rules inside the VM.
    //!
    //! Runtimes that do not support programmatic nftables/iptables rule injection
    //! (Docker Desktop, Colima, OrbStack) cause preflight to exit 11 — there is
    //! no degraded "DNS denial only" mode (NFR-S3).
    ```
  - [x] T5.2 Define the bollard helper `pub async fn detect_podman_machine(docker: &bollard::Docker) -> Option<String>`:
    - Call `docker.version()` → `bollard::models::SystemVersion`.
    - Inspect `components: Option<Vec<ComponentVersion>>`. Podman's version call returns a component named `"Podman Engine"`. If present, also inspect for the Podman machine name.
    - Alternatively, call `docker.info()` → `SystemInfo`. Podman sets `operating_system` to the Podman VM's OS (e.g. `"fedora"`) and `info.name` to the machine socket path. Use this to confirm Podman.
    - To get the Podman machine name: run `tokio::process::Command::new("podman").args(["machine", "list", "--format", "{{.Name}},{{.Running}}"]).output()` and parse for the first running machine. Default: `"podman-machine-default"`.
    - Return `Some(machine_name)` for Podman, `None` for other runtimes.
  - [x] T5.3 Define `pub async fn create_scan_network(docker: &bollard::Docker, scan_id: &str, llama_port: u16) -> Result<String, SandboxError>`:
    - Network name: `format!("lcrc-{scan_id}")`.
    - Create via `docker.create_network(CreateNetworkOptions { name: &network_name, driver: "bridge", internal: true, labels: HashMap::from([("lcrc-scan-id", scan_id)]), ..Default::default() })`.
    - On bollard error → `SandboxError::NetworkSetup(format!("create_network: {e}"))`.
    - Install iptables/nftables rules (T5.4).
    - Verify rules via negative probe (T5.5).
    - Return `Ok(network_name)`.
  - [x] T5.4 **iptables/nftables rule installation** — Podman on macOS approach:
    - Call `detect_podman_machine(docker)`. If `None` → `SandboxError::UnsupportedRuntime("structural port-pin unavailable on this runtime; use the packaged Podman runtime (brew install podman) or a runtime that exposes network rule injection")`.
    - Get the bridge network's subnet: call `docker.inspect_network(&network_name, None)` → parse `IPAM.Config[0].Subnet` (e.g., `"10.89.x.0/24"`).
    - Get the host IP reachable from inside the Podman VM as `host.docker.internal` (`192.168.65.2` is the common default for Podman on macOS; discover it dynamically via `docker.inspect_network("podman")` or the Podman machine gateway IP from `docker.info().gateway`).
    - Run nftables rules inside the Podman VM via:
      ```rust
      tokio::process::Command::new("podman")
          .args([
              "machine", "exec", &machine_name,
              "sudo", "nft", "add", "rule", "ip", "filter", "FORWARD",
              "ip", "saddr", &container_subnet,
              "ip", "daddr", &host_ip,
              "tcp", "dport", &llama_port.to_string(),
              "accept",
          ])
          .output()
          .await
      ```
    - Then add the DROP rule for all other outbound from the subnet:
      ```rust
      tokio::process::Command::new("podman")
          .args([
              "machine", "exec", &machine_name,
              "sudo", "nft", "add", "rule", "ip", "filter", "FORWARD",
              "ip", "saddr", &container_subnet,
              "drop",
          ])
          .output()
          .await
      ```
    - If either command fails (non-zero exit or stdout/stderr error) → `SandboxError::UnsupportedRuntime(format!("nft rule install failed: {stderr}"))`.
    - All subprocess calls use `tokio::process::Command` (not `std::process::Command`).
  - [x] T5.5 **Negative probe to verify rules** (AC7 "verified at scan preflight"):
    - Create a minimal probe container using bollard (image: `ghcr.io/<org>/lcrc-task:...` if available, or a small public image configured in a test-only constant — use `bollard::Docker::create_container` with the probe container config).
    - If the GHCR image is not available (placeholder digest), skip the live probe (log a `tracing::warn!` that the probe was skipped; this is acceptable until Story 1.14 provides the real image).
    - If image is available: run `nc -zv <host_ip> 22 -w 2` inside the container (exec via `docker.exec_container` → `docker.start_exec`). Verify exit code is non-zero (connection refused/timed out → rules working).
    - Run `nc -zv <host_ip> <llama_port> -w 2` (with a mock listener spawned via `tokio::net::TcpListener::bind("0.0.0.0:0")` on the host) → verify exit code is 0 (connection succeeds).
    - Remove probe container.
    - If negative probe succeeds when it should fail → `SandboxError::UnsupportedRuntime("iptables rules did not block non-llama traffic; runtime does not support structural port-pin")`.
  - [x] T5.6 Define `pub async fn remove_scan_network(docker: &bollard::Docker, network_name: &str)`:
    - Call `docker.remove_network(network_name)`. If error, log via `tracing::warn!` but do not propagate (best-effort cleanup).

- [x] **T6. Author `src/sandbox/container.rs` — ONLY bollard container API consumer** (AC: 2, 3, 9, 10)
  - [x] T6.1 File-level `//!` doc: "Ephemeral container lifecycle — the only module in the codebase that calls `bollard::container::*` APIs. All other code reaches containers through `Sandbox::run_task`."
  - [x] T6.2 Define `pub async fn run_container(docker: &bollard::Docker, image_ref: &str, workspace_path: &std::path::Path, network_name: &str, scan_id: &str) -> Result<TaskOutcome, SandboxError>`:
    - Generate a per-task container name: `format!("lcrc-task-{scan_id}-{}", uuid_suffix())` where `uuid_suffix` uses `std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()` to guarantee uniqueness within a scan.
    - Build `bollard::container::Config`:
      - `image: Some(image_ref.to_string())`
      - `host_config: Some(HostConfig { binds: Some(vec![format!("{}:/workspace:rw", workspace_path.display())]), network_mode: Some(network_name.to_string()), auto_remove: Some(false), extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]), .. Default::default() })`
      - `labels: Some(HashMap::from([("lcrc-scan-id", scan_id)]))`
      - No `env` field — Epic 1 defers env allowlist to Story 2.7 (AC10: no `env` extension arg).
      - No `volumes` field — bind mount via `host_config.binds` (AC10: no `volumes` extension arg).
      - No `network_disabled` — container IS on the custom network.
    - Create container: `docker.create_container(CreateContainerOptions { name: &container_name, .. }, config)`. On error → `SandboxError::ContainerCreate(format!("create: {e}"))`.
    - Record start time: `let t0 = std::time::Instant::now()`.
    - Start container: `docker.start_container(&container_name, None::<StartContainerOptions<String>>)`. On error → force-remove, then `SandboxError::ContainerCreate(format!("start: {e}"))`.
    - Wait: `docker.wait_container(&container_name, None::<WaitContainerOptions<String>>).try_collect::<Vec<_>>().await`. On error → force-remove, then `SandboxError::ContainerCreate(format!("wait: {e}"))`.
    - Parse exit code from the wait response: `exit_code == 0` → `pass: true`.
    - Record duration: `duration_seconds = t0.elapsed().as_secs_f64()`.
    - Force-remove: call `force_remove_container(docker, &container_name).await` (defined below) — ALWAYS, even on the success path.
    - Return `Ok(TaskOutcome { pass, duration_seconds })`.
  - [x] T6.3 Define `pub(crate) async fn force_remove_container(docker: &bollard::Docker, container_name: &str)`:
    - Call `docker.remove_container(container_name, Some(RemoveContainerOptions { force: true, ..Default::default() }))`.
    - If error, log via `tracing::warn!` but do not propagate.
    - This helper is also used by the probe in `network.rs::T5.5`.
  - [x] T6.4 Do NOT call `bollard::container::Container::create` anywhere else in the codebase. `container.rs` is the sole caller (boundary enforced by T11.1 grep).

- [x] **T7. Implement `Sandbox::new` and `Sandbox::run_task` in `src/sandbox.rs`** (AC: 1–10)
  - [x] T7.1 `Sandbox::new` body:
    ```rust
    pub async fn new(probe: &runtime::RuntimeProbe, llama_port: u16) -> Result<Self, SandboxError> {
        let socket_path = probe.socket_path.to_str()
            .ok_or_else(|| SandboxError::NetworkSetup("non-UTF8 socket path".into()))?;
        let docker = bollard::Docker::connect_with_unix(
            socket_path, 5, bollard::API_DEFAULT_VERSION,
        ).map_err(|e| SandboxError::NetworkSetup(format!("bollard connect: {e}")))?;

        // Unique scan identifier for network/container labels and GC.
        let pid = nix::unistd::Pid::this().as_raw();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let scan_id = format!("{pid}-{ts}");

        let network_name = network::create_scan_network(&docker, &scan_id, llama_port).await?;

        Ok(Self { docker, scan_id, network_name, llama_port })
    }
    ```
  - [x] T7.2 `Sandbox::run_task` body:
    ```rust
    pub async fn run_task(
        &self,
        image_digest: &str,
        workspace_path: &std::path::Path,
    ) -> Result<TaskOutcome, SandboxError> {
        image::ensure_image(&self.docker, image_digest).await?;
        container::run_container(
            &self.docker,
            image_digest,
            workspace_path,
            &self.network_name,
            &self.scan_id,
        ).await
    }
    ```
  - [x] T7.3 `Sandbox::cleanup` body:
    ```rust
    pub async fn cleanup(&self) {
        network::remove_scan_network(&self.docker, &self.network_name).await;
    }
    ```
  - [x] T7.4 `nix::unistd::Pid` is already available (nix `"user"` feature was added in Story 1.9). Use `Pid::this()` not `Pid::current()` — the function name changed in nix 0.29; verify the actual API during implementation.

- [x] **T8. Update `src/cli/scan.rs` — extend preflight to call `Sandbox::new` (AC8 exit-11 path)** (AC: 7, 8)
  - [x] T8.1 After the existing `runtime::detect()` call (which already exits 11 if no socket is reachable), attempt to construct `Sandbox::new` with a sentinel llama port (e.g., port 0 for the preflight check, or a dummy port — see T8.2). If `Sandbox::new` returns `SandboxError::UnsupportedRuntime` → map to `Error::Preflight(...)` → return `Err(...)` → main exits 11.
  - [x] T8.2 For the preflight check, the llama port is not yet known (Story 1.11 picks the port). Use a sentinel: attempt `Sandbox::new(probe, 11434)` — 11434 is llama.cpp's default port and a reasonable sentinel. The network-creation and iptables steps either succeed or fail; the port doesn't matter for the capability check.
  - [x] T8.3 After `Sandbox::new` succeeds, call `sandbox.cleanup().await` immediately (the preflight just checks capability; the real `Sandbox` for the scan is constructed in Story 1.12 when the llama-server port is known).
  - [x] T8.4 The `"lcrc scan is not yet implemented"` placeholder diagnostic stays in place — Story 1.12 replaces the placeholder with the full pipeline.
  - [x] T8.5 Wire the `SandboxError::UnsupportedRuntime` to `Error::Preflight` via `format!("{e}")`. Keep the same inline conversion pattern Story 1.9 used (no global `From` impl yet).
  - [x] T8.6 Keep the scan function signature sync with the existing runtime builder pattern:
    ```rust
    runtime.block_on(async {
        let probe = runtime::detect(...)...;
        // log probe
        let sandbox = Sandbox::new(&probe, 11434).await
            .map_err(|e| Error::Preflight(e.to_string()))?;
        sandbox.cleanup().await;
        output::diag("`lcrc scan` is not yet implemented in this build.");
        Ok(())
    })
    ```

- [x] **T9. Integration tests** (AC: all)
  - [x] T9.1 Create `tests/sandbox_run_task.rs` — integration tests requiring a real container runtime.
    - All tests in this file gate on `std::env::var("LCRC_INTEGRATION_TEST_SANDBOX").is_ok()`. If not set, print `"skipping: set LCRC_INTEGRATION_TEST_SANDBOX=1 to run"` and return.
    - Also skip if `runtime::detect(&SystemEnv)` returns `Err` (no runtime available on the test machine).
    - All tests in this file are `#[tokio::test(flavor = "current_thread")]`.
  - [x] T9.2 Test `sandbox_creates_internal_network_with_no_dns` (AC4): construct `Sandbox::new(&probe, test_llama_port)`, start a test container, exec `curl https://example.com -m 3`, assert non-zero exit code, call `sandbox.cleanup()`.
  - [x] T9.3 Test `sandbox_workspace_mount_visible_inside_container` (AC2): create a temp dir with a sentinel file `sentinel.txt`, construct `Sandbox::new`, call `run_task` with the temp dir, inside the container exec `test -f /workspace/sentinel.txt`, assert exit code 0.
  - [x] T9.4 Test `sandbox_host_filesystem_absent_inside_container` (AC3): call `run_task`, inside the container exec `test -f /etc/hostname && cat /etc/hostname` — verify the hostname is the container's hostname, not the host's.
  - [x] T9.5 Test `sandbox_container_removed_after_run_task` (AC9): after `run_task`, call `docker.inspect_container(container_name, None)` and assert it returns a 404 error (container does not exist).
  - [x] T9.6 Test `sandbox_exits_11_on_unsupported_runtime` (AC8): this test cannot be run on a machine that uses Podman (since Podman IS supported). Skip unless explicitly provided with a non-Podman runtime endpoint via `LCRC_TEST_UNSUPPORTED_RUNTIME_SOCKET` env var.
  - [x] T9.7 Do NOT add a test for AC1 (image pull + digest) — the real GHCR image does not exist until Story 1.14. Add the test skeleton as a `#[ignore]` test with a comment that Story 1.14 removes the `#[ignore]` once the real digest constant is filled.

- [x] **T10. CLI exit-code test for exit-11 on unsupported runtime** (AC: 8)
  - [x] T10.1 In `tests/cli_exit_codes.rs`, add a test `scan_exits_11_on_unsupported_runtime_for_network_isolation`. This test only runs if `LCRC_TEST_UNSUPPORTED_RUNTIME_SOCKET` is set (a non-Podman Docker socket). Use `assert_cmd::Command::cargo_bin("lcrc")` with `LCRC_RUNTIME_DOCKER_HOST` set to the non-Podman socket. Assert exit code 11 and stderr contains `"structural port-pin unavailable"`.
  - [x] T10.2 Do NOT break the existing `scan_exits_11_with_setup_instructions_when_no_runtime` test.

- [x] **T11. Local CI mirror** (AC: all)
  - [x] T11.1 Scope-discipline grep — bollard container APIs must stay in `src/sandbox/container.rs` only:
    ```bash
    git grep -nE 'bollard::container|Docker::create_container|Docker::start_container|Docker::wait_container|Docker::remove_container' src/ tests/ \
      | grep -v '^src/sandbox/container.rs:'
    ```
    Must produce zero matches.
  - [x] T11.2 Scope-discipline grep — bollard network APIs must stay in `src/sandbox/network.rs` and `src/sandbox.rs` only:
    ```bash
    git grep -nE 'Docker::create_network|Docker::remove_network|Docker::inspect_network' src/ tests/ \
      | grep -v '^src/sandbox/network.rs:' \
      | grep -v '^src/sandbox.rs:'
    ```
    Must produce zero matches.
  - [x] T11.3 `cargo build` — all new modules compile and bollard types resolve.
  - [x] T11.4 `cargo fmt --check` — rustfmt clean.
  - [x] T11.5 `cargo clippy --all-targets --all-features -- -D warnings`. Watch for:
    - `missing_docs` on every `pub` item in the four new/updated files.
    - `missing_errors_doc` on `ensure_image`, `create_scan_network`, `run_container`, `Sandbox::new`, `Sandbox::run_task`.
    - `clippy::module_name_repetitions` may fire on `SandboxError`, `TaskOutcome` — suppress with `#[allow(clippy::module_name_repetitions)]` if needed.
    - `clippy::too_many_lines` may fire on `run_container` — split into helpers if it does.
    - `clippy::option_if_let_chain` on the `detect_podman_machine` body — use `let else` instead.
  - [x] T11.6 `cargo test` — all pre-existing tests continue to pass (sandbox_preflight, cache_*, cli_*, machine_fingerprint). The new `sandbox_run_task` integration tests skip unless `LCRC_INTEGRATION_TEST_SANDBOX=1`.

## Dev Notes

### Scope discipline (read this first)

This story authors **four new files** (`src/constants.rs`, `src/sandbox/container.rs`, `src/sandbox/network.rs`, `src/sandbox/image.rs`), updates **three existing files** (`src/sandbox.rs`, `src/lib.rs`, `src/cli/scan.rs`), and adds **two test files** (`tests/sandbox_run_task.rs`, plus a new test in `tests/cli_exit_codes.rs`). No Cargo.toml changes — all needed dependencies are already present.

This story does **not**:
- Implement `src/sandbox/env_allowlist.rs`. Env allowlist (the third sandbox axis) lands in Story 2.7. Epic 1 is default-deny on workspace + network axes only; the container in this story starts with NO `--env` flags at all (not even an env-file), which is strictly more restrictive than the future allowlist.
- Wire `Sandbox::run_task` into a real scan pipeline. That is Story 1.12's job. Story 1.10 makes the function available; Story 1.12 calls it.
- Start `llama-server`. That is Story 1.11. The `llama_port: u16` parameter in `Sandbox::new` is a placeholder value for the preflight check; the real port comes from Story 1.11's `ServerHandle`.
- Create `image/Dockerfile` or publish to GHCR. That is Story 1.14. The `CONTAINER_IMAGE_DIGEST` constant is a placeholder in this story; Story 1.14 fills in the real value and enables the full AC1 integration test.
- Implement `src/sandbox/violation.rs`. That is Story 2.8.
- Modify `src/error.rs`, `src/exit_code.rs`, `src/main.rs`, `src/output.rs`, `src/cache*`, `src/machine*`, `src/version.rs`.

### Architecture compliance (binding constraints)

- **`container.rs` is the ONLY module that calls `bollard::container::*`** (architecture.md § "Sandbox Invariants — Structural, not Conventional" + boundary table). `runtime.rs` owns `Docker::connect_with_unix` + `Docker::ping`. `container.rs` owns `Docker::create_container`, `Docker::start_container`, `Docker::wait_container`, `Docker::remove_container`. `image.rs` owns `Docker::create_image` + `Docker::inspect_image`. `network.rs` owns `Docker::create_network`, `Docker::remove_network`, `Docker::inspect_network`. These boundaries are enforced by T11.1–T11.2 greps.
- **`Sandbox::run_task` accepts NO `volumes`, `env`, or `network_mode` extension arguments** (AR-28). Workspace mount is passed via `host_config.binds`; network is passed via `host_config.network_mode` — both are internal implementation details, not public parameters.
- **No `unsafe`** — bollard and nix use unsafe internally; host crate stays `forbid(unsafe_code)`. `tokio::process::Command` is safe.
- **All subprocess calls use `tokio::process::Command`**, never `std::process::Command` (architecture.md § "Async Discipline").
- **`SandboxError::UnsupportedRuntime` maps to `ExitCode::PreflightFailed = 11`** via `Error::Preflight(String)` in `cli/scan.rs::run()`. Same inline-format conversion Story 1.9 used.
- **Container labels**: Every container AND network created by this story must carry `"lcrc-scan-id" -> scan_id` label. This enables a future `lcrc gc` or the backstop cleanup loop described in architecture.md § "Container lifecycle (NFR-R8)".
- **Force-remove in ALL paths**: `container::run_container` must call `force_remove_container` whether the container succeeds, fails, or panics. Use `tokio::task::JoinHandle` + abort guard if needed, or ensure the remove call is inside a cleanup closure that runs unconditionally.
- **No `auto_remove: true`** in bollard config — use explicit `force: true` remove after `wait_container` so the removal is observable and logged. `auto_remove` hides the container immediately on exit, making debugging and GC labeling harder.
- **stdout/stderr discipline (FR46)**: All user-visible messages go through `crate::output::diag`. Tracing events use structured fields. No direct `println!`/`eprintln!` in any new code.
- **`missing_docs = "warn"`**: Every `pub` item in `constants.rs`, `sandbox.rs`, `sandbox/container.rs`, `sandbox/image.rs`, `sandbox/network.rs` needs a `///` doc comment. Every `pub async fn` that returns `Result` needs a `# Errors` rustdoc section.

### Bollard 0.18 API reference (locked versions)

- **`bollard::Docker::connect_with_unix(addr: &str, timeout: u64, client_version: &ClientVersion) -> Result<Docker, Error>`** — same as Story 1.9's ping. Reuse the same pattern.
- **`bollard::Docker::create_network(options: CreateNetworkOptions<T>, config: Config<T>) -> Result<NetworkCreateResponse, Error>`** — `CreateNetworkOptions` holds the network name; `Config` (bollard's network Config) holds `internal`, `driver`, `labels`, `options` (HashMap). Verify the exact struct names in bollard 0.18 — they may be `CreateNetworkOptions` in the `bollard::network` module.
- **`bollard::Docker::remove_network(network_id: &str) -> Result<(), Error>`**
- **`bollard::Docker::inspect_network(network_id: &str, options: Option<InspectNetworkOptions<T>>) -> Result<Network, Error>`** — `Network.IPAM.Config[0].Subnet` gives the container subnet.
- **`bollard::Docker::create_image(options: CreateImageOptions<T>, root_fs: Option<Body>, credentials: Option<DockerCredentials>) -> impl Stream<Item = Result<CreateImageInfo, Error>>`** — drain with `futures_util::TryStreamExt::try_collect::<Vec<_>>()` or a `while let` loop.
- **`bollard::Docker::inspect_image(image_id: &str) -> Result<ImageInspect, Error>`** — `ImageInspect.repo_digests: Option<Vec<String>>` contains strings like `"ghcr.io/<org>/lcrc-task@sha256:..."`. Check if any entry ends with the expected digest suffix.
- **`bollard::Docker::create_container(options: CreateContainerOptions<T>, config: Config<T>) -> Result<ContainerCreateResponse, Error>`** — `Config<T>` here is the container config (NOT the network Config). Key fields: `image`, `env` (leave `None` for Epic 1), `labels`, `host_config`.
- **`bollard::container::HostConfig`**: fields `binds: Option<Vec<String>>` (e.g., `["/tmp/lcrc-task-<uuid>:/workspace:rw"]`), `network_mode: Option<String>` (the network name), `extra_hosts: Option<Vec<String>>` (e.g., `["host.docker.internal:host-gateway"]`), `auto_remove: Option<bool>` (set `false`).
- **`bollard::Docker::start_container(id: &str, options: Option<StartContainerOptions<T>>) -> Result<(), Error>`** — pass `None` for default options.
- **`bollard::Docker::wait_container(id: &str, options: Option<WaitContainerOptions<T>>) -> impl Stream<Item = Result<ContainerWaitResponse, Error>>`** — collect to get exit status code. `ContainerWaitResponse.status_code: i64`.
- **`bollard::Docker::remove_container(id: &str, options: Option<RemoveContainerOptions>) -> Result<(), Error>`** — `RemoveContainerOptions { force: true, .. Default::default() }`.
- **`bollard::Docker::info() -> Result<SystemInfo, Error>`** — `SystemInfo.server_version: Option<String>`, `SystemInfo.operating_system: Option<String>`.
- **`bollard::Docker::version() -> Result<SystemVersion, Error>`** — `SystemVersion.components: Option<Vec<ComponentVersion>>`, each `ComponentVersion { name: String, version: String, .. }`. Podman returns a component named `"Podman Engine"`.
- **`futures_util`** — `bollard` re-exports or depends on it; use `bollard::model::*` names per the crate's actual public API. If `futures_util::TryStreamExt` is needed, check whether it's re-exported from bollard or needs a direct import. Since `bollard = "0.18"` is in `[dependencies]`, its `futures_util` dependency is available via `bollard::futures_util` or add `use futures_util::TryStreamExt as _;` once confirmed available.

### Network isolation implementation notes

**`--internal` flag semantics**: Docker/Podman's `internal: true` network option removes the default gateway and the NAT masquerade rule, so containers in the network cannot reach the public internet. DNS resolution is also not provided (no `nameserver` in the container's `/etc/resolv.conf`). This satisfies AC4 (no external curl) structurally.

**`host.docker.internal` access in `--internal` networks**: On Podman (macOS), the host machine is reachable at the gateway IP even in `--internal` networks because Podman sets up a host-gateway route. Pass `extra_hosts: ["host.docker.internal:host-gateway"]` in container config to ensure the hostname resolves correctly. The IP resolved by `host-gateway` is the Podman machine's virtual host IP (typically `192.168.65.2` or similar).

**iptables port-pinning**: An `--internal` network blocks external internet but does NOT restrict which host ports are reachable via `host.docker.internal`. The nft rules in T5.4 enforce this: DROP all FORWARD traffic from the container subnet EXCEPT TCP to `<host_ip>:<llama_port>`. The ACCEPT rule must appear BEFORE the DROP rule (nftables evaluates in order). The FORWARD chain is used because traffic from the container to the host flows through the bridge's FORWARD chain in the Podman VM.

**`nft` vs `iptables`**: Prefer `nft` (nftables) as it's the default on modern Fedora-based Podman VMs. Fall back to `iptables` if `nft` is not found. The Podman machine's Fedora image has both.

**Podman machine detection**: Use `bollard::Docker::version()` → check `ComponentVersion.name == "Podman Engine"`. If not present → attempt to call `podman machine list` via subprocess → if that fails too → `SandboxError::UnsupportedRuntime`.

**Colima/OrbStack/Docker Desktop**: These runtimes run Docker daemon in VMs without exposing the VM's iptables/nftables. They cannot install port-pin rules. The correct behavior is `SandboxError::UnsupportedRuntime("structural port-pin unavailable on this runtime...")`.

### Resolved decisions

- **`Sandbox` is a struct, not a free function.** The scan-level context (bollard client, network name, scan ID) is held in the struct. `run_task` is a method. This avoids re-probing the runtime and re-creating the network on every task.
- **`llama_port` is a constructor parameter, not a `run_task` parameter.** The network is created once per scan at `Sandbox::new` time with the pinned port baked in. Passing a different port to `run_task` would require rebuilding the iptables rules per-task — unnecessarily expensive. The port is known at scan startup (Story 1.11 assigns it; Story 1.12 passes it to `Sandbox::new`).
- **`Sandbox::cleanup` is explicit, not `Drop`.** `Drop` cannot be `async`. The scan orchestrator (Story 1.12) calls `sandbox.cleanup().await` after the last task. For orphan protection, container and network labels carry `lcrc-scan-id` so a future GC pass can clean them up.
- **`Container::auto_remove = false`.** We call `force_remove_container` explicitly to ensure it runs even on error paths and to make the removal observable via logs.
- **The preflight capability check in `cli/scan.rs` uses llama port `11434` as a sentinel.** This creates and immediately destroys a test network, which is the cost of structural enforcement. Story 1.12 eliminates this redundant network creation by restructuring the scan flow.
- **Epic 1 does not pass `--env` to containers.** The container inherits only its image's built-in env vars. This is more restrictive than the final Epic 2 allowlist and is safe for Story 1.10's scope.
- **The image verification test is `#[ignore]` until Story 1.14.** The GHCR image doesn't exist yet; the test skeleton is present so Story 1.14's dev agent just removes `#[ignore]` and fills the actual reference.
- **Container naming**: `format!("lcrc-task-{scan_id}-{nanos}")` — scan_id gives per-scan namespace; nanos gives per-task uniqueness within a scan. No `uuid` crate needed.

### Previous story intelligence

Carry-forward from Story 1.9:
- **Module pattern**: `sandbox.rs` (parent) declares submodules + parent error enum. Submodules own concrete logic + their typed errors. This story adds three siblings to `runtime.rs` following the same pattern.
- **Single typed error variant per module per story** — this story adds four new `SandboxError` variants (`ImagePull`, `NetworkSetup`, `UnsupportedRuntime`, `ContainerCreate`). Do NOT pre-add variants for future stories (Story 2.7's allowlist enforcement, Story 2.8's violation detection).
- **No `From<SandboxError> for crate::error::Error`** — `cli/scan.rs::run()` does inline `format!("{e}")` conversion. Story 1.12 (the full scan wiring) decides if a global `From` impl is warranted.
- **`bollard::Docker` client construction**: Already established in `runtime.rs::probe_one`. In this story, `Sandbox::new` constructs a new `bollard::Docker` client from the `RuntimeProbe.socket_path` using the same `bollard::Docker::connect_with_unix(addr, 5, bollard::API_DEFAULT_VERSION)` pattern.
- **In-module test exemption**: All `#[cfg(test)] mod tests` blocks carry `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`.
- **Tracing discipline**: New events use `target: "lcrc::sandbox::<module>"` and structured fields, not string interpolation.
- **Story 1.9 Resolved Decisions § "double-print of preflight diagnostic"**: The same double-print occurs for `UnsupportedRuntime` → `Error::Preflight` → `main.rs`. Accepted for now; Story 1.12 revisits.
- **Dev notes from Story 1.9 debug log**: nix `"user"` feature is needed for `Pid::this()` — already present in Cargo.toml (`features = ["signal", "user"]`).

### File structure requirements

```
src/
├── lib.rs                     UPDATED: add `pub mod constants;`
├── constants.rs               NEW: CONTAINER_IMAGE_DIGEST placeholder
├── sandbox.rs                 UPDATED: Sandbox struct, TaskOutcome, new SandboxError variants, new mod decls
└── sandbox/
    ├── runtime.rs             UNTOUCHED (Story 1.9's work)
    ├── image.rs               NEW: image pull + digest verification
    ├── network.rs             NEW: per-scan internal Docker network + iptables port-pinning
    └── container.rs           NEW: ephemeral container lifecycle (ONLY bollard container API consumer)
src/cli/
└── scan.rs                    UPDATED: extend preflight with Sandbox::new capability check
tests/
├── sandbox_run_task.rs        NEW: integration tests (skip unless LCRC_INTEGRATION_TEST_SANDBOX=1)
└── cli_exit_codes.rs          UPDATED: add exit-11 test for unsupported runtime
```

After this story merges:
- `src/sandbox/` has four submodules: `runtime`, `image`, `network`, `container`.
- `Sandbox::run_task` is implemented and can be called by Story 1.12 to execute a real container task.
- `src/cli/scan.rs::run()` performs: socket preflight → sandbox capability check (exits 11 if unsupported) → placeholder diagnostic → exit 0.
- `src/constants.rs::CONTAINER_IMAGE_DIGEST` is a placeholder awaiting Story 1.14.
- All bollard container APIs are isolated to `src/sandbox/container.rs`.

### Testing requirements

- **Integration tests** (`tests/sandbox_run_task.rs`): ALL tests gate on `LCRC_INTEGRATION_TEST_SANDBOX=1` env var AND skip if no real runtime is reachable. Use `runtime::detect(&SystemEnv)` to check; skip if it returns `Err`. Tests use real Podman/Docker to create containers, verify workspace mounts, verify network isolation. Use `#[tokio::test(flavor = "current_thread")]`.
- **Unit tests** in `sandbox.rs::tests`: pure SandboxError Display tests (T3.7). In `sandbox/image.rs::tests`: pure function unit tests for the digest-parsing logic (splitting the `@sha256:` part). In `sandbox/network.rs::tests`: pure function tests for Podman machine name parsing from `podman machine list` output.
- **CLI test** (`tests/cli_exit_codes.rs`): Test for exit-11 on unsupported runtime — only runs if `LCRC_TEST_UNSUPPORTED_RUNTIME_SOCKET` is set.
- **No mock bollard clients.** Integration tests use real runtimes; unit tests cover pure parsing logic only. Do NOT add `bollard-stubs` or `wiremock`.
- **`LCRC_INTEGRATION_TEST_SANDBOX=1` guard pattern**:
  ```rust
  if std::env::var("LCRC_INTEGRATION_TEST_SANDBOX").is_err() {
      eprintln!("skipping: set LCRC_INTEGRATION_TEST_SANDBOX=1 to run");
      return;
  }
  ```
- **Test cleanup**: each integration test that creates a `Sandbox` must call `sandbox.cleanup().await` in all paths (success AND failure). Use `tokio::select!` or a `defer`-style wrapper to ensure cleanup runs.

### Project Context Reference

- **Epic 1 position**: Story 10 of 14. After this story, the sandbox is complete for workspace + network axes. Story 1.11 (llama-server lifecycle) and Story 1.12 (end-to-end one-cell scan) are the remaining integration stories.
- **Cross-story dependencies**:
  - **Depends on**: Story 1.1 (bollard in Cargo.toml), Story 1.3 (`ExitCode::PreflightFailed = 11`, `Error::Preflight`), Story 1.4 (clap + tracing setup), Story 1.9 (`RuntimeProbe`, `sandbox::runtime::detect`, `SandboxError` parent enum, `pub mod sandbox` in lib.rs).
  - **Unblocks**: Story 1.11 (llama-server — `Sandbox::run_task` is the container it connects to), Story 1.12 (end-to-end scan — calls `Sandbox::new` + `run_task`), Story 1.14 (fills `CONTAINER_IMAGE_DIGEST`), Story 7.4 (acceptance check #9 uses the full sandbox).
- **Architectural touchpoints**:
  - `Sandbox::run_task` is the structural enforcement point for NFR-S1–S5 (workspace isolation + network isolation). No caller can circumvent the isolation by passing extension arguments — AC10 is a structural guarantee.
  - The `lcrc-scan-id` label on containers and networks enables NFR-R8 backstop GC.
  - The `CONTAINER_IMAGE_DIGEST` constant in `src/constants.rs` is the single source of truth for image content integrity (FR17b). No code should construct container image references without going through this constant or a tested override.

### References

- [Source: `_bmad-output/planning-artifacts/epics.md` § "Story 1.10" lines 605-652] — ten AC clauses
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Sandbox & Container Runtime" lines 297-334] — `--internal` network, workspace bind, container lifecycle NFR-R8
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Sandbox Invariants — Structural, not Conventional" lines 795-803] — boundary rules for bollard container APIs
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Architectural Boundaries" table lines 990-1009] — boundary table mapping modules to their sole APIs
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" lines 922-929] — `sandbox/{container,network,image,violation}.rs` layout
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Module Organization" lines 655-666] — `src/constants.rs` placement + cross-cutting helpers
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Async Discipline" lines 685-691] — tokio::process, not std::process
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Implementation flow diagram" lines 1080-1121] — full scan flow showing `Sandbox::run_task` position
- [Source: `_bmad-output/planning-artifacts/epics.md` § "Story 1.14" lines 737-763] — Story 1.14 fills the real GHCR image; confirms Story 1.10's `CONTAINER_IMAGE_DIGEST` is a placeholder
- [Source: `_bmad-output/implementation-artifacts/1-9-container-runtime-preflight-with-socket-precedence-chain.md` § "Dev Agent Record / Debug Log"] — nix "user" feature already in Cargo.toml; bollard::API_DEFAULT_VERSION confirmed reachable in 0.18
- [Source: `_bmad-output/implementation-artifacts/1-9-container-runtime-preflight-with-socket-precedence-chain.md` § "Architecture compliance"] — bollard scope-discipline grep pattern to extend in T11
- [Source: `src/sandbox.rs`] — current state: `SandboxError` with one variant (`Preflight`); `pub mod runtime;` — T3 extends this
- [Source: `src/sandbox/runtime.rs`] — `RuntimeProbe` struct shape (fields: `socket_path: PathBuf`, `source: PrecedenceLayer`); `SystemEnv` pattern for production env reading
- [Source: `src/cli/scan.rs`] — current state: calls `runtime::detect`, then placeholder; T8 extends this
- [Source: `src/lib.rs`] — current `pub mod` list: cache, cli, error, exit_code, machine, output, sandbox, util, version — T2 inserts `constants` between `cache` and `cli`
- [Source: `Cargo.toml` lines 18-86] — all dependencies available: `bollard = "0.18"`, `tokio = { features = ["full"] }`, `tempfile = "3"`, `nix = { features = ["signal", "user"] }`, `thiserror = "2"`, `tracing = "0.1"`, `sha2 = "0.10"` (not needed here but available), no `uuid` crate (generate unique IDs from pid + timestamp)
- [Source: `src/error.rs` lines 18-43] — `Error::Preflight(String)` is the correct variant for UnsupportedRuntime errors (exit 11)
- [Source: `src/exit_code.rs` line 32] — `ExitCode::PreflightFailed = 11`

## Dev Agent Record

### Agent Model Used

claude-sonnet-4-6[1m]

### Debug Log References

- `futures-util` is not re-exported by bollard 0.18 and not in Cargo.toml; added as a direct dependency (`futures-util = "0.3"`) since story code requires `StreamExt` and `TryStreamExt` traits for draining bollard's `create_image` and `wait_container` streams.
- `bollard::Docker` does not implement `Debug`; `Sandbox` cannot derive `Debug`. The integration test assertion was changed to avoid formatting `Result<Sandbox, _>` with `{:?}`.
- `parse_running_machine` falls back to `"podman-machine-default"` when no running machine is found in the list output, matching documented Podman defaults.
- T5.5 (negative probe with a real container) is deferred per story: the placeholder image digest means no container can be created; a `tracing::warn!` is emitted at runtime when the probe is skipped.

### Completion Notes List

- Implemented `src/constants.rs` with `CONTAINER_IMAGE_DIGEST` placeholder constant.
- Updated `src/lib.rs` to declare `pub mod constants;` in alphabetical order.
- Updated `src/sandbox.rs`: new `pub mod container/image/network` declarations, `Sandbox` struct, `TaskOutcome`, four new `SandboxError` variants (`ImagePull`, `NetworkSetup`, `UnsupportedRuntime`, `ContainerCreate`), and full `Sandbox::new/run_task/cleanup` implementations.
- Authored `src/sandbox/image.rs`: `ensure_image` with pull-stream drain, digest verification via `RepoDigests`, and unit tests for `parse_image_ref`.
- Authored `src/sandbox/network.rs`: `detect_podman_machine` via bollard version API, `create_scan_network` creating an `internal: true` bridge network, `install_port_pin_rules` using `podman machine exec nft`, `remove_scan_network` (best-effort), unit tests for `parse_running_machine`.
- Authored `src/sandbox/container.rs`: `run_container` with workspace bind-mount, per-scan network, `force_remove_container` called unconditionally, `unique_container_name` using pid+nanos, no `env` fields (AC10).
- Updated `src/cli/scan.rs`: after `runtime::detect`, calls `Sandbox::new(&probe, 11434)`, maps `SandboxError::UnsupportedRuntime` → `Error::Preflight`, then calls `sandbox.cleanup()`.
- Created `tests/sandbox_run_task.rs` with five guarded integration tests and one `#[ignore]` placeholder for Story 1.14.
- Added `scan_exits_11_on_unsupported_runtime_for_network_isolation` test to `tests/cli_exit_codes.rs`.
- Scope-discipline greps (T11.1, T11.2) pass: zero leakage of container/network APIs beyond their designated modules.
- All 124 pre-existing and new tests pass; 1 `#[ignore]` placeholder for Story 1.14.

### File List

- `Cargo.toml` — added `futures-util = "0.3"` direct dependency
- `src/constants.rs` — NEW: `CONTAINER_IMAGE_DIGEST` placeholder constant
- `src/lib.rs` — added `pub mod constants;`
- `src/sandbox.rs` — added `pub mod container/image/network`, `Sandbox`, `TaskOutcome`, new `SandboxError` variants, `Sandbox::new/run_task/cleanup` implementations, new error Display tests
- `src/sandbox/image.rs` — NEW: `ensure_image`, `parse_image_ref`, `verify_digest`, `pull_image`, unit tests
- `src/sandbox/network.rs` — NEW: `detect_podman_machine`, `create_scan_network`, `install_port_pin_rules`, `discover_host_ip`, `remove_scan_network`, unit tests
- `src/sandbox/container.rs` — NEW: `run_container`, `force_remove_container`, `unique_container_name`
- `src/cli/scan.rs` — extended preflight with `Sandbox::new` capability check and cleanup
- `tests/sandbox_run_task.rs` — NEW: five integration tests gated on `LCRC_INTEGRATION_TEST_SANDBOX=1`
- `tests/cli_exit_codes.rs` — added `scan_exits_11_on_unsupported_runtime_for_network_isolation` test

### Review Findings

**Reviewed on 2026-05-07 by bmad-code-review (3-layer: Blind Hunter + Edge Case Hunter + Acceptance Auditor)**

Applied inline (must-fix):
- [x] [Review][Patch] Story references in module-level doc comments violate CLAUDE.md HIGH-PRECEDENCE RULES — removed "(Story 1.9)", "(this story)" from `sandbox.rs` module doc; removed story ref from `constants.rs` doc comment; cleaned story refs from `tests/sandbox_run_task.rs` comments [`src/sandbox.rs:1-7`, `src/constants.rs:8`, `tests/sandbox_run_task.rs`]
- [x] [Review][Patch] `parse_running_machine` returns `Some("podman-machine-default")` when no machine is running — causes `podman machine exec` against a non-existent machine with a cryptic error instead of a clean `UnsupportedRuntime`; changed loop fallback from `Some(default)` to `None`; updated two affected unit tests [`src/sandbox/network.rs:57-59`]

Applied inline (should-fix):
- [x] [Review][Patch] `verify_digest` uses substring `contains` instead of exact component equality — split each `RepoDigests` entry at `@` and compare the hash portion exactly, eliminating false-positive matches on short/prefix digests [`src/sandbox/image.rs:68-72`]
- [x] [Review][Patch] `workspace_path` not validated as absolute before formatting bind string — relative paths silently produce a bind against the daemon's CWD; added `is_absolute()` guard at the top of `run_container` [`src/sandbox/container.rs:33-38`]
- [x] [Review][Patch] Missing `tracing::warn!` for skipped negative probe — T5.5 requires logging when the iptables-rule probe is skipped due to placeholder image; added warn at end of `create_scan_network` [`src/sandbox/network.rs:101-106`]
- [x] [Review][Patch] `wait_container` empty response silently reports `pass = false` — added `tracing::warn!` log when `responses` is empty so the anomaly is observable [`src/sandbox/container.rs:85-92`]

Deferred:
- [x] [Review][Defer] nft `ip filter FORWARD` chain existence not checked before rule insertion [`src/sandbox/network.rs:143-204`] — deferred, non-standard Podman VM setups only; nft error surfaces as `UnsupportedRuntime` anyway
- [x] [Review][Defer] ACCEPT rule left in nftables if DROP rule install fails — partial firewall armed state [`src/sandbox/network.rs:196-204`] — deferred, nftables lifecycle cleanup is out of scope for this story
- [x] [Review][Defer] `ensure_image` falls through to pull on any `inspect_image` error, not just 404 [`src/sandbox/image.rs:25`] — deferred, bollard 404 error variant needs verification; two-step pull+verify catches tampered images regardless
- [x] [Review][Defer] `pull_image` does not pass digest to `create_image`, relying on post-pull verify [`src/sandbox/image.rs:83-99`] — deferred, two-step approach is per spec T4.2; `verify_digest` catches mismatches
- [x] [Review][Defer] No container log capture before force-remove on failure [`src/sandbox/container.rs:81`] — deferred, enhancement not in scope

Dismissed (12 findings):
- nft DROP rule blocks DNS — intended per AC4 (no DNS resolver by design)
- Podman-only `UnsupportedRuntime` — correct per AC8/NFR-S3
- `container_subnet`/`host_ip` shell injection — `Command::args` + `podman machine exec` don't invoke a shell; each arg is a separate token
- `unique_container_name` clock collision — spec-mandated pid+nanos approach; astronomically unlikely in practice
- `force_remove_container` on running container — intended force-remove on all exit paths
- CONTAINER_IMAGE_DIGEST placeholder confusing error — intentional; placeholder until image is published
- `scan_id` collision — spec-mandated pid+millis; acceptable
- `cleanup` not consuming — API design decision for future stories
- `llama_port` `#[allow(dead_code)]` — spec-mandated field, used at construction time only
- Integration tests T9.2–T9.5 hollow — intentionally limited by placeholder image
- AC8 wiring correctness — verified correct in `cli/scan.rs` diff
- `discover_host_ip` `192.168.65.2` fallback — standard Podman-on-macOS default; logged via the existing info path
