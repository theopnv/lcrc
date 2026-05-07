# Story 1.9: Container runtime preflight with socket precedence chain

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As Theop,
I want `lcrc scan` to fail fast with helpful setup instructions if no container runtime is reachable,
so that I'm not stuck debugging a cryptic Docker socket error (FR17a, NFR-S3).

## Acceptance Criteria

**AC1.** **Given** a Mac with no container runtime installed (simulated in test by ensuring all four candidate sockets in the precedence chain — `LCRC_RUNTIME_DOCKER_HOST`, `DOCKER_HOST`, `/var/run/docker.sock`, the Podman default per-uid socket — are absent or unreachable) **When** I run `lcrc scan` **Then** the process exits with code 11 (`ExitCode::PreflightFailed`) and stderr contains the literal substrings `brew install podman`, `podman machine init`, and `podman machine start`. The setup-instructions block is the *only* user-facing diagnostic on this path; nothing else is printed to stdout.

**AC2.** **Given** a Mac with Podman installed but the machine not started (simulated in test by configuring the precedence chain to point at a path whose Unix socket file does not exist — i.e. the runtime is "installed-but-not-running" state) **When** I run `lcrc scan` **Then** the process exits with code 11 and stderr contains the literal substring `podman machine start` (the start-the-machine instruction is part of the same setup-instructions block as AC1; the AC pin is that the message *includes* the start instruction, not that it omits the install instruction). The single-message-covers-both-modes design is locked: lcrc does not distinguish "not installed" from "installed-but-not-started" in the user-facing copy because the user remediation is the same superset of commands.

**AC3.** **Given** a reachable Docker-Engine-API-compatible Unix socket at one of the precedence-chain paths (simulated in test by spawning a `tokio::net::UnixListener` that responds to a single `GET /_ping` request with `HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK`) **When** the `runtime::detect()` function is called with the precedence chain pointing at that socket path **Then** it returns `Ok(RuntimeProbe { socket_path, source })` where `socket_path` is the resolved absolute path that succeeded and `source` records which precedence-chain layer matched (one of `LcrcRuntimeDockerHost`, `DockerHost`, `DefaultDockerSock`, `PodmanDefaultSock`).

**AC4.** **Given** the env var `LCRC_RUNTIME_DOCKER_HOST` is set to a reachable socket AND `DOCKER_HOST` is set to a *different* reachable socket AND both `/var/run/docker.sock` and the Podman default socket are also reachable (all four mocked in test via four `tokio::net::UnixListener` instances) **When** `runtime::detect()` is called **Then** it probes `LCRC_RUNTIME_DOCKER_HOST` first, succeeds there, and returns `RuntimeProbe { source: PrecedenceLayer::LcrcRuntimeDockerHost, .. }` without ever opening connections to the other three sockets (verified by asserting that the other three `UnixListener`s recorded zero accepted connections).

**AC5.** **Given** the precedence chain is `LCRC_RUNTIME_DOCKER_HOST` → `DOCKER_HOST` → `/var/run/docker.sock` → Podman default socket AND every probe in the chain fails (all four sockets unreachable) **When** `runtime::detect()` is called **Then** it returns `Err(PreflightError::NoRuntimeReachable { attempts })` where `attempts` is a `Vec<ProbeAttempt>` of length **4**, recording each layer's `source: PrecedenceLayer`, `socket_path: PathBuf` (the resolved candidate path it tried, even if the env var was unset → uses the documented fallback path), and `failure: ProbeFailure` (one of `EnvVarUnset`, `SocketFileMissing`, `ConnectFailed { source: std::io::Error }`, or `PingFailed { source: bollard::errors::Error }`). The four attempts appear in precedence order so the diagnostic / log message can replay the chain top-to-bottom.

**AC6.** **Given** `runtime::detect()` succeeds via socket `X` **When** the scan continues **Then** lcrc emits a single `tracing::info!` event at the `lcrc::sandbox::runtime` target with the structured fields `socket_path = "<path>"` and `source = "<precedence-layer-name>"`, rendered through the existing tracing subscriber to stderr in the form `INFO lcrc::sandbox::runtime: detected container runtime at <path> source=<layer-name>` (subscriber's default fmt layer; field rendering is what `tracing-subscriber`'s `fmt::Layer` produces, not a hand-formatted string). Verified by running `lcrc scan` against a mock-listener socket with `RUST_LOG=info` and asserting stderr contains the substring `detected container runtime at` and the path string.

## Tasks / Subtasks

- [ ] **T1. Update `src/lib.rs` — declare `pub mod sandbox;`** (AC: 1, 2, 3, 4, 5, 6)
  - [ ] T1.1 Insert `pub mod sandbox;` into the existing `pub mod` block in `src/lib.rs:5-12` in alphabetical order (between `pub mod output;` and `pub mod util;` → resulting order: `cache`, `cli`, `error`, `exit_code`, `machine`, `output`, `sandbox`, `util`, `version`).
  - [ ] T1.2 Do NOT touch the `pub fn run()` body. CLI dispatch stays where it is; the only `lib.rs` change in this story is the module declaration. Same rule Stories 1.5 / 1.6 / 1.7 / 1.8 followed.
  - [ ] T1.3 Do NOT add re-exports (`pub use sandbox::runtime::detect;` etc.). Callers reach into the path explicitly: `lcrc::sandbox::runtime::detect(...)`. Re-export policy is an Epic 6 polish concern.

- [ ] **T2. Author `src/sandbox.rs` — module-root file with doc + `SandboxError` parent enum** (AC: 1, 2, 5)
  - [ ] T2.1 File-level `//!` doc that mirrors the architecture's two-level split (architecture.md § "Complete Project Directory Structure" lines 922-929): the `sandbox` module owns the per-task isolation envelope; submodules are `runtime` (this story — preflight detection of a reachable Docker-Engine-API-compatible socket per FR17a) plus future submodules (`container`, `network`, `env_allowlist`, `image`, `violation`) that land in their owner stories. List only the submodules that exist *now* (`runtime`); do not pre-list future submodules — same scope discipline Stories 1.7 / 1.8 used for `cache.rs`.
  - [ ] T2.2 Declare `pub mod runtime;` (alphabetical-trivial — only one submodule in this story).
  - [ ] T2.3 Define `pub enum SandboxError` as the **module-level** error type for sandbox concerns. Story 1.9 owns exactly one variant — `Preflight(#[from] runtime::PreflightError)`:
    ```rust
    /// Errors crossing the [`crate::sandbox`] module boundary.
    ///
    /// One variant per concrete failure mode the sandbox layer surfaces.
    /// Adding a variant is a public-API change; downstream code that
    /// `match`-es on this enum must be updated in the same change.
    #[derive(Debug, thiserror::Error)]
    pub enum SandboxError {
        /// Preflight probe of the container runtime socket precedence chain
        /// failed to reach any compatible runtime. Maps eventually to
        /// [`crate::exit_code::ExitCode::PreflightFailed`] when surfaced from a
        /// CLI command (boundary mapping owned by the consumer story).
        #[error("preflight failed: {0}")]
        Preflight(#[from] runtime::PreflightError),
    }
    ```
    - **One variant only.** Other sandbox failures (`ContainerCreate`, `ImagePull`, `NetworkSetup`, `Violation`) land in their owner stories (1.10, 1.14, 2.7, 2.8). Pre-adding them creates dead surface area. Same rule Stories 1.5 / 1.6 / 1.7 / 1.8 followed for `MachineFingerprintError` / `KeyError` / `CacheError`.
    - **`#[from]`** auto-derives the `From<runtime::PreflightError> for SandboxError` impl so `?` works inside future callers without manual wrapping.
    - **Display string starts with `"preflight failed: "`.** The format mirrors `error::Error::Preflight`'s rendering pattern (`src/error.rs:21-22`) so when this error eventually flows through `Error::Preflight(String)` (Story 1.12 wires the boundary), the user sees one consistent prefix — not double-prefixed `"preflight failed: preflight failed: ..."`. The wrapping at the consumer (Story 1.12) will be `Error::Preflight(format!("{sandbox_err}"))` — i.e. the `Display` of `SandboxError::Preflight(...)` *already* contains the `"preflight failed: "` prefix from this Display, so the consumer formats with `{sandbox_err}` not `format!("preflight failed: {sandbox_err}")`. Documented here so the consumer story does not double-prefix.
  - [ ] T2.4 Do NOT define `From<SandboxError> for crate::error::Error` in this story. Same rule Stories 1.5 / 1.6 / 1.7 / 1.8 followed: boundary mapping is the consumer's choice. Story 1.12 (the first end-to-end `lcrc scan` wiring) decides the conversion. **Exception:** see T5 — `cli/scan.rs` *is* updated in this story (the AC contract requires `lcrc scan` to exit 11), but it does the boundary conversion inline (`format!("{e}").into()` → `Error::Preflight`) rather than via a global `From` impl, so the `From` decision stays deferred.
  - [ ] T2.5 In-module unit tests for `SandboxError` Display behavior:
    ```rust
    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    mod tests {
        use super::SandboxError;
        use super::runtime::{PreflightError, ProbeAttempt, PrecedenceLayer, ProbeFailure};
        use std::path::PathBuf;

        #[test]
        fn display_passes_preflight_error_through_with_single_prefix() {
            let attempts = vec![ProbeAttempt {
                source: PrecedenceLayer::DefaultDockerSock,
                socket_path: PathBuf::from("/var/run/docker.sock"),
                failure: ProbeFailure::SocketFileMissing,
            }];
            let err = SandboxError::Preflight(PreflightError::NoRuntimeReachable { attempts });
            let rendered = err.to_string();
            assert!(rendered.starts_with("preflight failed: "));
            assert_eq!(rendered.matches("preflight failed:").count(), 1);
        }
    }
    ```
    - Pins the no-double-prefix invariant from T2.3.
    - Uses the `#[allow(...)]` attribute set the codebase already standardizes on (`src/cache.rs`, `src/cache/cell.rs`, `src/cache/migrations.rs` all repeat this exemption).

- [ ] **T3. Author `src/sandbox/runtime.rs` — precedence chain + probe** (AC: 3, 4, 5)
  - [ ] T3.1 File-level `//!` doc:
    ```text
    //! Container-runtime preflight (FR17a, NFR-S3).
    //!
    //! Probes the precedence chain
    //!   1. `LCRC_RUNTIME_DOCKER_HOST` env var
    //!   2. `DOCKER_HOST` env var
    //!   3. `/var/run/docker.sock` (Docker Desktop / Colima / OrbStack default)
    //!   4. Podman default per-uid socket (`$XDG_RUNTIME_DIR/podman/podman.sock`
    //!      with macOS fallback to `~/.local/share/containers/podman/...`)
    //!
    //! The first layer whose socket accepts a `bollard` `/_ping` round-trip
    //! wins. If every layer fails the function returns
    //! [`PreflightError::NoRuntimeReachable`] carrying the per-layer failure
    //! reasons in precedence order.
    //!
    //! No `--unsafe-no-sandbox` fallback exists per NFR-S3 — the sandbox is
    //! structural or the scan refuses to run.
    ```
  - [ ] T3.2 Define `pub enum PrecedenceLayer` — the four-variant enum naming each layer:
    ```rust
    /// Identifies which layer of the precedence chain a probe attempt
    /// targeted. Variant order matches probe order.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum PrecedenceLayer {
        /// `LCRC_RUNTIME_DOCKER_HOST` env var (lcrc-specific override).
        LcrcRuntimeDockerHost,
        /// `DOCKER_HOST` env var (Docker convention; `bollard` reads it natively
        /// when callers use `connect_with_local_defaults`, but we read it
        /// explicitly to keep the precedence-chain fully under our control).
        DockerHost,
        /// Hardcoded `/var/run/docker.sock` path.
        DefaultDockerSock,
        /// Podman default per-uid socket (XDG-resolved at probe time).
        PodmanDefaultSock,
    }

    impl PrecedenceLayer {
        /// Stable display name used in [`tracing`] structured fields and
        /// the `NoRuntimeReachable` Display rendering.
        #[must_use]
        pub const fn name(self) -> &'static str {
            match self {
                Self::LcrcRuntimeDockerHost => "lcrc_runtime_docker_host",
                Self::DockerHost => "docker_host",
                Self::DefaultDockerSock => "default_docker_sock",
                Self::PodmanDefaultSock => "podman_default_sock",
            }
        }
    }
    ```
    - **`Copy + Clone + PartialEq + Eq + Hash`**: pure tag-shaped enum, four nullary variants — derive everything cheap and useful. Tests that build expected-attempt lists construct these by value.
    - **`name()` returns `&'static str`**: stable snake_case identifiers used by structured tracing fields *and* by setup-instructions test assertions. Locked here so Display-template edits don't drift the field name silently.
  - [ ] T3.3 Define `pub enum ProbeFailure` — the four reasons a single layer's probe can fail:
    ```rust
    /// Concrete failure mode for a single layer of the precedence chain.
    /// Variants are ordered by where the probe stopped: env-resolution
    /// failure, then filesystem check, then connect, then ping.
    #[derive(Debug, thiserror::Error)]
    pub enum ProbeFailure {
        /// The env-var-backed layer (`LCRC_RUNTIME_DOCKER_HOST` or `DOCKER_HOST`)
        /// is unset. Path-based layers cannot produce this variant.
        #[error("env var unset")]
        EnvVarUnset,
        /// Resolved socket path does not exist (or is not a Unix socket).
        #[error("socket file missing")]
        SocketFileMissing,
        /// Connect to the Unix socket failed at the OS layer (permission
        /// denied, ECONNREFUSED, EHOSTUNREACH).
        #[error("connect failed: {source}")]
        ConnectFailed {
            /// Underlying I/O error returned by the bollard / tokio connector.
            #[source]
            source: std::io::Error,
        },
        /// Connect succeeded but the `/_ping` request did not return a
        /// 2xx response — the listener is not a Docker-Engine-API server.
        #[error("ping failed: {source}")]
        PingFailed {
            /// Underlying bollard error (HTTP status, parse failure, …).
            #[source]
            source: bollard::errors::Error,
        },
    }
    ```
    - **Four variants, no more, no less.** Discriminating between "socket missing" and "connect failed" is what lets the diagnostic tell the user whether they need to *install* a runtime or *start* one. Combining them into a single "Unreachable" variant would lose that signal.
    - **`#[source]` on the inner errors** preserves the cause chain for `tracing`'s `?error` instrumentation. The Display rendering shows the source's Display message (`{source}`) — already user-friendly for `std::io::Error` and `bollard::errors::Error`.
    - **`bollard::errors::Error` typed dependency** comes via Cargo.toml line 42 — `bollard = "0.18"`; no new dep.
  - [ ] T3.4 Define `pub struct ProbeAttempt`:
    ```rust
    /// Per-layer record of a probe attempt. One [`ProbeAttempt`] is recorded
    /// per layer in the precedence chain, even on the layer that ultimately
    /// succeeds (where it is *not* surfaced — the success path returns
    /// [`RuntimeProbe`] instead of attempts). On total failure
    /// [`PreflightError::NoRuntimeReachable`] carries one [`ProbeAttempt`]
    /// per layer in precedence order.
    #[derive(Debug)]
    pub struct ProbeAttempt {
        /// Which layer of the precedence chain this attempt targeted.
        pub source: PrecedenceLayer,
        /// Resolved candidate socket path. Always populated, even when
        /// the env-var-backed layer is `EnvVarUnset` (in which case it
        /// records the *would-have-been* path — `PathBuf::new()` for env
        /// layers since there is no path to record; see Resolved decisions).
        pub socket_path: std::path::PathBuf,
        /// Concrete failure reason.
        pub failure: ProbeFailure,
    }
    ```
    - **Field order**: `source` first so log lines / Debug renderings group by layer. `socket_path` second (the *what* of the probe). `failure` last (the *why*).
    - **`socket_path` for `EnvVarUnset`**: `PathBuf::new()` (empty path). The Display template (T3.7) elides the path when `failure == EnvVarUnset` so the user sees `"LCRC_RUNTIME_DOCKER_HOST: env var unset"` and not `"LCRC_RUNTIME_DOCKER_HOST: env var unset (path: )"`. See Resolved decisions § "EnvVarUnset path representation".
    - **`Debug` derive only**, no `PartialEq` — `ProbeFailure::ConnectFailed { source: std::io::Error }` and `PingFailed { source: bollard::errors::Error }` carry inner errors that do not implement `Eq`. Tests that need to inspect attempts use `match` patterns + field-by-field assertions, not `assert_eq!(attempt, ...)`. Same pattern Story 1.5's `MachineFingerprint` decided for nullable diagnostic fields.
  - [ ] T3.5 Define `pub struct RuntimeProbe`:
    ```rust
    /// Result of a successful preflight probe. Contains the resolved socket
    /// path and which precedence-chain layer provided it. Future stories
    /// (1.10) take a `&RuntimeProbe` to construct the `bollard::Docker`
    /// client without re-probing.
    #[derive(Debug, Clone)]
    pub struct RuntimeProbe {
        /// Absolute socket path that responded successfully to `/_ping`.
        pub socket_path: std::path::PathBuf,
        /// Layer of the precedence chain that produced [`Self::socket_path`].
        pub source: PrecedenceLayer,
    }
    ```
    - **`Clone` derive**: `PathBuf` and `PrecedenceLayer` are both `Clone`. Cloning is cheap (one `Vec<u8>` clone, one `Copy`); future callers may want to keep a copy of the probe alongside passing one into a constructor.
    - **No `Display` impl on `RuntimeProbe`.** The success-path log line is built at the call site via `tracing::info!(socket_path = ?probe.socket_path, source = probe.source.name(), "detected container runtime")`, not via `RuntimeProbe::Display`. Putting Display on the value type would invite drift between the log-line copy and the success message; structured tracing fields are the canonical surface.
  - [ ] T3.6 Define `pub enum PreflightError`:
    ```rust
    /// Errors returned by [`detect`].
    #[derive(Debug, thiserror::Error)]
    pub enum PreflightError {
        /// Every layer of the precedence chain failed. Carries one
        /// [`ProbeAttempt`] per layer in precedence order so the
        /// diagnostic can replay the chain top-to-bottom.
        #[error("{}", crate::sandbox::runtime::format_no_runtime_reachable(attempts))]
        NoRuntimeReachable {
            /// Per-layer failure records, in precedence order. Always exactly
            /// four entries (one per [`PrecedenceLayer`] variant).
            attempts: Vec<ProbeAttempt>,
        },
    }
    ```
    - **One variant only** — `NoRuntimeReachable`. Future preflight categories (`UnsupportedRuntime`, `NetworkRulesUnavailable` per Story 1.10) land in their owner story.
    - **Display via free function `format_no_runtime_reachable(&[ProbeAttempt]) -> String`.** Inline-formatting a multi-line message inside a `#[error("...")]` literal is unreadable; the free function is private to the module and emits the locked format. See T3.7.
  - [ ] T3.7 Implement `pub(crate) fn format_no_runtime_reachable(attempts: &[ProbeAttempt]) -> String`. The locked output format (a single string with `\n`-separated lines that the consumer prints to stderr verbatim):
    ```text
    no container runtime reachable. lcrc tried (in order):
      1. LCRC_RUNTIME_DOCKER_HOST: <reason>
      2. DOCKER_HOST: <reason>
      3. /var/run/docker.sock: <reason>
      4. <podman-default-path>: <reason>

    To install Podman (the recommended runtime):
      brew install podman
      podman machine init
      podman machine start

    Then re-run `lcrc scan`.
    ```
    - **Numbering** is hardcoded `1.`–`4.` matching the four layers in precedence order.
    - **Layer label rendering**:
      - `PrecedenceLayer::LcrcRuntimeDockerHost` → `"LCRC_RUNTIME_DOCKER_HOST"` (the env-var name as it appears in user docs)
      - `PrecedenceLayer::DockerHost` → `"DOCKER_HOST"`
      - `PrecedenceLayer::DefaultDockerSock` → the resolved path string (always `"/var/run/docker.sock"`)
      - `PrecedenceLayer::PodmanDefaultSock` → the resolved path string (varies; see T3.10)
      - Why path-as-label for the path layers and env-var-as-label for env layers: the user is mapped back to the same string they would set or check. Mixing label kinds across rows is the locked design — the user reads "lcrc tried `LCRC_RUNTIME_DOCKER_HOST` and got X, then `/var/run/docker.sock` and got Y" which matches their mental model better than a uniform "layer 1 / layer 2 / ..." enumeration.
    - **`<reason>` rendering** is `attempt.failure.to_string()` (from the `thiserror`-generated `Display`). For `EnvVarUnset` → `"env var unset"`, etc. Tests pin this contract.
    - **Setup instructions block is verbatim, hardcoded.** No conditional branching ("if Podman installed, suggest `start`; if not, suggest `install`") — the user-facing copy treats the install + init + start commands as one inseparable block. Same single-message-covers-both-modes design AC2 pins. Locked here so the AC1 / AC2 substring assertions hold.
    - Implementation skeleton (the dev-story implementer fills in the `match` arms + uses `std::fmt::Write` against a `String`):
      ```rust
      pub(crate) fn format_no_runtime_reachable(attempts: &[ProbeAttempt]) -> String {
          use std::fmt::Write as _;
          let mut s = String::with_capacity(512);
          let _ = writeln!(s, "no container runtime reachable. lcrc tried (in order):");
          for (i, a) in attempts.iter().enumerate() {
              let label = match a.source {
                  PrecedenceLayer::LcrcRuntimeDockerHost => "LCRC_RUNTIME_DOCKER_HOST".to_string(),
                  PrecedenceLayer::DockerHost => "DOCKER_HOST".to_string(),
                  PrecedenceLayer::DefaultDockerSock | PrecedenceLayer::PodmanDefaultSock => {
                      a.socket_path.display().to_string()
                  }
              };
              let _ = writeln!(s, "  {}. {}: {}", i + 1, label, a.failure);
          }
          s.push('\n');
          let _ = writeln!(s, "To install Podman (the recommended runtime):");
          let _ = writeln!(s, "  brew install podman");
          let _ = writeln!(s, "  podman machine init");
          let _ = writeln!(s, "  podman machine start");
          s.push('\n');
          let _ = writeln!(s, "Then re-run `lcrc scan`.");
          s
      }
      ```
    - **Why `let _ = writeln!(...)`**: writes to a `String` are infallible — `String` implements `std::fmt::Write` infallibly. The `let _ =` discards the always-`Ok` result and dodges `clippy::unused_must_use` without an `unwrap` (forbidden) or `expect` (forbidden). Same pattern other modules use; if clippy still fires, switch to `s.write_fmt(format_args!(...)).ok();`.
  - [ ] T3.8 Implement the env-source seam: `pub trait EnvSource: Send + Sync`:
    ```rust
    /// Abstraction over the environment so tests can drive the precedence
    /// chain deterministically without mutating process-global state.
    /// Production callers pass [`SystemEnv`]; tests pass an in-memory map.
    pub trait EnvSource: Send + Sync {
        /// Returns `Some(value)` if the named env var is set and non-empty,
        /// `None` otherwise. Empty-string values are treated as unset
        /// (matches POSIX shell `${VAR:?}` semantics — an empty `DOCKER_HOST`
        /// is a misconfiguration, not a deliberate path).
        fn get(&self, name: &str) -> Option<String>;
    }

    /// Production [`EnvSource`] backed by [`std::env::var`].
    #[derive(Debug, Default, Clone, Copy)]
    pub struct SystemEnv;

    impl EnvSource for SystemEnv {
        fn get(&self, name: &str) -> Option<String> {
            std::env::var(name).ok().filter(|v| !v.is_empty())
        }
    }
    ```
    - **Why a trait, not a function pointer or closure**: a trait with one method is the minimum-surface seam. Future preflight extensions (timeout, retry config) become trait methods or sibling traits. Closures lose this extensibility.
    - **`Send + Sync`**: the trait object will be passed across the tokio runtime boundary inside `detect`. No threading concern in v1 (the runtime is current-thread for now), but the bound is free to add and avoids a re-export later.
    - **Empty-string filter**: an empty env var (`DOCKER_HOST=`) is functionally indistinguishable from unset for our purposes. Documented above; same convention POSIX shell tools use.
  - [ ] T3.9 Implement the candidate-list builder: `pub fn candidate_chain(env: &dyn EnvSource) -> Vec<(PrecedenceLayer, std::path::PathBuf, ProbeFailure)>`. **No** — actually, build a different shape. We need the resolved path AND knowledge that it came from an unset env. Refactor: `pub(crate) fn resolve_candidates(env: &dyn EnvSource) -> [Candidate; 4]` returning a fixed-size array. Each `Candidate` is:
    ```rust
    /// Internal: a single layer's pre-resolution result. The `path` is
    /// `Some` when the layer produced a path (env var set, or hardcoded
    /// path layer); `None` when the env-var layer is unset.
    #[derive(Debug)]
    pub(crate) struct Candidate {
        pub source: PrecedenceLayer,
        pub path: Option<std::path::PathBuf>,
    }

    pub(crate) fn resolve_candidates(env: &dyn EnvSource) -> [Candidate; 4] {
        [
            Candidate {
                source: PrecedenceLayer::LcrcRuntimeDockerHost,
                path: env.get("LCRC_RUNTIME_DOCKER_HOST").map(strip_unix_prefix).map(std::path::PathBuf::from),
            },
            Candidate {
                source: PrecedenceLayer::DockerHost,
                path: env.get("DOCKER_HOST").map(strip_unix_prefix).map(std::path::PathBuf::from),
            },
            Candidate {
                source: PrecedenceLayer::DefaultDockerSock,
                path: Some(std::path::PathBuf::from("/var/run/docker.sock")),
            },
            Candidate {
                source: PrecedenceLayer::PodmanDefaultSock,
                path: Some(podman_default_socket_path()),
            },
        ]
    }
    ```
    - **`strip_unix_prefix(s: String) -> String`**: helper that strips a `unix://` scheme prefix if present. `DOCKER_HOST` values are conventionally `unix:///var/run/docker.sock`; lcrc accepts the raw path or the `unix://` URL. TCP-form `DOCKER_HOST` values (`tcp://...`) are out of v1 scope — see Resolved decisions § "TCP DOCKER_HOST out of scope". Implementation:
      ```rust
      fn strip_unix_prefix(s: String) -> String {
          s.strip_prefix("unix://").map_or(s.clone(), str::to_string)
      }
      ```
      Or use the borrow-then-allocate-once pattern (clippy-clean). Tests pin both inputs (`"/path"` → `"/path"`, `"unix:///path"` → `"/path"`).
    - **Fixed-size `[Candidate; 4]` array**: forces a compile-time guarantee that exactly four layers are probed, in this order. Adding a layer is a deliberate, type-level change. Same "exhaustive enum + fixed-array" guard pattern Stories 1.3's `ExitCode::variant_set_is_exhaustive` test uses for the enum variants.
  - [ ] T3.10 Implement `pub(crate) fn podman_default_socket_path() -> std::path::PathBuf` — XDG-resolves the Podman per-uid socket:
    - Order:
      1. `$XDG_RUNTIME_DIR/podman/podman.sock` if `XDG_RUNTIME_DIR` is set
      2. macOS fallback: `~/.local/share/containers/podman/machine/qemu/podman.sock` (the path Podman 4.x machine creates on macOS — verify against installed Podman during dev-story)
      3. Linux fallback: `/run/user/<uid>/podman/podman.sock` constructed via `nix::unistd::Uid::current().as_raw()`
    - Implementation skeleton:
      ```rust
      pub(crate) fn podman_default_socket_path() -> std::path::PathBuf {
          if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
              return std::path::PathBuf::from(xdg).join("podman").join("podman.sock");
          }
          // macOS doesn't always set XDG_RUNTIME_DIR; the Podman machine
          // socket lives under the user's home dir. The exact subpath has
          // changed across Podman versions; the path below matches Podman
          // 4.x; if the dev-story implementer finds a different path on
          // their installed Podman, update both this function and T3.9's
          // fixed array contract.
          if cfg!(target_os = "macos") {
              if let Some(home) = std::env::var_os("HOME") {
                  return std::path::PathBuf::from(home)
                      .join(".local/share/containers/podman/machine/qemu/podman.sock");
              }
          }
          // Linux fallback.
          let uid = nix::unistd::Uid::current().as_raw();
          std::path::PathBuf::from(format!("/run/user/{uid}/podman/podman.sock"))
      }
      ```
    - **Direct `std::env::var_os` usage** is allowed here even though architecture.md § "Layered Config Loading" line 758 forbids `std::env::var` outside `config::`. The forbidden case is *config* env vars (`LCRC_*`); platform-discovery env vars (`XDG_RUNTIME_DIR`, `HOME`) are infrastructure-discovery, not user-tunable configuration. Same exemption Story 1.5 used for `sysctl` shell-out. **Exception is documented in the WHY-comment at the call site.**
    - **`nix::unistd::Uid::current()`** is the locked Cargo.toml dep (line 56 — `nix = { version = "0.29", features = ["signal"] }`). Verify the `signal` feature includes the `unistd` module; if not, **expand the feature set in Cargo.toml** to include `user` (the feature gate for `unistd::Uid` in nix 0.29). This is the one Cargo.toml change Story 1.9 may need; document the change in the Dev Notes File List.
  - [ ] T3.11 Implement `async fn probe_one(candidate: &Candidate) -> Result<(), ProbeFailure>` — single-layer probe:
    1. If `candidate.path.is_none()` → `return Err(ProbeFailure::EnvVarUnset)`. (Path-based layers always have `Some(path)`; this branch only fires for unset env-backed layers.)
    2. `let path = candidate.path.as_ref().unwrap();` — the `unwrap` is correct because step 1 ruled out `None`. Wait — `unwrap_used` is `deny`. Use `let Some(path) = candidate.path.as_ref() else { return Err(ProbeFailure::EnvVarUnset); };` instead. The let-else pattern is stable since 1.65 and avoids the lint.
    3. Filesystem existence check: `if !tokio::fs::try_exists(path).await.unwrap_or(false) { return Err(ProbeFailure::SocketFileMissing); }`. (`unwrap_or(false)` is acceptable — it's a `Result::unwrap_or`, not `Option::unwrap` — and matches the conservative "if we can't tell, assume missing" semantics.)
    4. Construct a `bollard::Docker` client pinned to this socket: `let docker = bollard::Docker::connect_with_unix(path.to_str().ok_or_else(|| ProbeFailure::ConnectFailed { source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "non-utf8 socket path") })?, 5, bollard::API_DEFAULT_VERSION).map_err(|e| ProbeFailure::ConnectFailed { source: bollard_to_io_error(e) })?;` — see § "bollard error → io::Error mapping" in Resolved decisions.
       - Wait: in bollard 0.18, the constructor is `Docker::connect_with_unix(addr, timeout, client_version)` returning `Result<Docker, bollard::errors::Error>`. `addr` is `&str`. Map a `bollard::errors::Error` into `ProbeFailure::ConnectFailed { source: io_error_from_bollard(e) }` — see helper below. **Verify the exact 0.18 API surface during dev-story** and adjust if the constructor name has shifted (e.g. `connect_with_socket`).
    5. `docker.ping().await.map_err(|source| ProbeFailure::PingFailed { source })?;` — the `/ _ping` round-trip. Ping returning `Ok` is the canonical "this is a real Docker Engine API server" signal — bollard issues `GET /_ping` and expects a `200 OK` with body `OK`. Any non-2xx response, parse failure, or timeout surfaces as `bollard::errors::Error`.
    6. `Ok(())`.
    - **`pub(crate)` not `pub`**: only `detect` calls `probe_one`. Tests in `tests/sandbox_preflight.rs` exercise `detect` (the public API), not `probe_one` directly. In-module unit tests can call it as `super::probe_one(...)` from within `mod tests`.
    - **bollard error → io::Error helper** for the `connect_with_unix` failure path. Implementation:
      ```rust
      fn io_error_from_bollard(e: bollard::errors::Error) -> std::io::Error {
          // bollard::errors::Error wraps multiple kinds, none of which
          // satisfy `Into<io::Error>` directly. Stringify the source for
          // the error message; the precedence chain uses this for the
          // "connect failed: <reason>" rendering (T3.7).
          std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
      }
      ```
      The `ConnectFailed` variant carries `std::io::Error` because the user-visible reason at this layer is "could not establish a Unix-socket connection to the runtime"; bollard's lower-level error chain is replayed via the `to_string()` call. Keeping `std::io::Error` rather than `bollard::errors::Error` for `ConnectFailed` lets the same variant cover both pre-bollard (`tokio::fs::try_exists` failure surfaces as `SocketFileMissing`) and bollard-layer connect failures uniformly.
  - [ ] T3.12 Implement `pub async fn detect(env: &dyn EnvSource) -> Result<RuntimeProbe, PreflightError>`:
    ```rust
    /// Probe the four-layer precedence chain in order. Returns the first
    /// layer whose Unix socket accepts a Docker-Engine-API `/_ping`
    /// round-trip, or [`PreflightError::NoRuntimeReachable`] if every
    /// layer fails.
    ///
    /// `env` is the source for `LCRC_RUNTIME_DOCKER_HOST` and `DOCKER_HOST`
    /// — production callers pass [`SystemEnv`]; tests inject an in-memory
    /// implementation to exercise the precedence chain deterministically.
    ///
    /// # Errors
    ///
    /// [`PreflightError::NoRuntimeReachable`] when every layer's probe
    /// fails. The error carries one [`ProbeAttempt`] per layer in
    /// precedence order.
    pub async fn detect(env: &dyn EnvSource) -> Result<RuntimeProbe, PreflightError> {
        let candidates = resolve_candidates(env);
        let mut attempts = Vec::with_capacity(4);
        for cand in candidates {
            match probe_one(&cand).await {
                Ok(()) => {
                    let path = cand.path.unwrap_or_default();
                    return Ok(RuntimeProbe { socket_path: path, source: cand.source });
                }
                Err(failure) => {
                    attempts.push(ProbeAttempt {
                        source: cand.source,
                        socket_path: cand.path.unwrap_or_default(),
                        failure,
                    });
                }
            }
        }
        Err(PreflightError::NoRuntimeReachable { attempts })
    }
    ```
    - **Sequential probe, no parallelism**: the precedence chain is *ordered* — we MUST stop at the first success even if a later layer would also succeed. The AC4 contract pins this ("returns LcrcRuntimeDockerHost without ever opening connections to the other three sockets"). Parallel probing would either need to track which started first (unnecessary complexity) or violate the no-extra-connection contract.
    - **Returns at first success**: the loop's `return Ok(...)` short-circuits, leaving any remaining (un-probed) layers untouched. This is what AC4 verifies.
    - **`cand.path.unwrap_or_default()` on the `EnvVarUnset` failure path**: produces an empty `PathBuf`. The Display rendering (T3.7) elides the path on `EnvVarUnset` so the user does not see a misleading empty path. `unwrap_or_default()` is a `Option::unwrap_or_default`, not `Option::unwrap` — clippy-clean.
    - **`cand.path.unwrap_or_default()` on the success path**: cannot fire because a successful probe means `probe_one` got past the `EnvVarUnset` early-return → `path` was `Some`. The branch is defensive but unreachable in practice; chosen over `expect("path is Some on success path")` to keep the function `unwrap`/`expect`-free. A code-review-time `// SAFETY: probe_one returns Ok only when path was Some (see step 1)` comment would be planning-meta, so the helper stays bare.
  - [ ] T3.13 In-module unit tests for `runtime`:
    ```rust
    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    mod tests {
        use super::*;
        use std::collections::HashMap;
        use std::sync::Mutex;

        /// Test-only [`EnvSource`] backed by an in-memory map. Mutex inner
        /// type so the trait's `&self` shape composes with mutation in tests.
        #[derive(Debug, Default)]
        struct MapEnv(Mutex<HashMap<String, String>>);

        impl MapEnv {
            fn with(pairs: &[(&str, &str)]) -> Self {
                let mut m = HashMap::new();
                for (k, v) in pairs {
                    m.insert((*k).to_string(), (*v).to_string());
                }
                Self(Mutex::new(m))
            }
        }

        impl EnvSource for MapEnv {
            fn get(&self, name: &str) -> Option<String> {
                self.0.lock().unwrap().get(name).cloned().filter(|v| !v.is_empty())
            }
        }

        #[test]
        fn precedence_layer_name_is_stable_snake_case() {
            assert_eq!(PrecedenceLayer::LcrcRuntimeDockerHost.name(), "lcrc_runtime_docker_host");
            assert_eq!(PrecedenceLayer::DockerHost.name(), "docker_host");
            assert_eq!(PrecedenceLayer::DefaultDockerSock.name(), "default_docker_sock");
            assert_eq!(PrecedenceLayer::PodmanDefaultSock.name(), "podman_default_sock");
        }

        #[test]
        fn strip_unix_prefix_strips_scheme_when_present() {
            assert_eq!(strip_unix_prefix("/var/run/docker.sock".into()), "/var/run/docker.sock");
            assert_eq!(strip_unix_prefix("unix:///var/run/docker.sock".into()), "/var/run/docker.sock");
            assert_eq!(strip_unix_prefix("".into()), "");
        }

        #[test]
        fn resolve_candidates_uses_precedence_order_and_drops_empty_env() {
            let env = MapEnv::with(&[("LCRC_RUNTIME_DOCKER_HOST", "/tmp/lcrc.sock"), ("DOCKER_HOST", "")]);
            let cands = resolve_candidates(&env);
            assert_eq!(cands[0].source, PrecedenceLayer::LcrcRuntimeDockerHost);
            assert_eq!(cands[0].path.as_deref(), Some(std::path::Path::new("/tmp/lcrc.sock")));
            assert_eq!(cands[1].source, PrecedenceLayer::DockerHost);
            assert_eq!(cands[1].path, None);
            assert_eq!(cands[2].source, PrecedenceLayer::DefaultDockerSock);
            assert_eq!(cands[2].path.as_deref(), Some(std::path::Path::new("/var/run/docker.sock")));
            assert_eq!(cands[3].source, PrecedenceLayer::PodmanDefaultSock);
            assert!(cands[3].path.is_some());
        }

        #[test]
        fn resolve_candidates_strips_unix_prefix_from_docker_host() {
            let env = MapEnv::with(&[("DOCKER_HOST", "unix:///var/run/colima.sock")]);
            let cands = resolve_candidates(&env);
            assert_eq!(cands[1].path.as_deref(), Some(std::path::Path::new("/var/run/colima.sock")));
        }

        #[test]
        fn format_no_runtime_reachable_contains_setup_instructions() {
            let attempts = vec![
                ProbeAttempt {
                    source: PrecedenceLayer::LcrcRuntimeDockerHost,
                    socket_path: std::path::PathBuf::new(),
                    failure: ProbeFailure::EnvVarUnset,
                },
                ProbeAttempt {
                    source: PrecedenceLayer::DockerHost,
                    socket_path: std::path::PathBuf::new(),
                    failure: ProbeFailure::EnvVarUnset,
                },
                ProbeAttempt {
                    source: PrecedenceLayer::DefaultDockerSock,
                    socket_path: std::path::PathBuf::from("/var/run/docker.sock"),
                    failure: ProbeFailure::SocketFileMissing,
                },
                ProbeAttempt {
                    source: PrecedenceLayer::PodmanDefaultSock,
                    socket_path: std::path::PathBuf::from("/some/podman.sock"),
                    failure: ProbeFailure::SocketFileMissing,
                },
            ];
            let s = format_no_runtime_reachable(&attempts);
            assert!(s.contains("brew install podman"));
            assert!(s.contains("podman machine init"));
            assert!(s.contains("podman machine start"));
            assert!(s.contains("LCRC_RUNTIME_DOCKER_HOST"));
            assert!(s.contains("DOCKER_HOST"));
            assert!(s.contains("/var/run/docker.sock"));
            assert!(s.contains("/some/podman.sock"));
            assert!(s.contains("env var unset"));
            assert!(s.contains("socket file missing"));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn detect_returns_no_runtime_reachable_when_chain_empty() {
            // All four layers point at non-existent paths.
            let env = MapEnv::with(&[]);  // env layers unset
            // The path layers fall back to /var/run/docker.sock and the
            // podman-default — neither exists in a sandboxed test env.
            // (If the test machine does have a runtime running, this test
            // is skipped via the cfg below.)
            // Skip when /var/run/docker.sock exists locally — we cannot
            // make the path layer fail without filesystem manipulation
            // beyond test scope.
            if std::path::Path::new("/var/run/docker.sock").exists() {
                eprintln!("skipping: /var/run/docker.sock exists on this machine");
                return;
            }
            let result = detect(&env).await;
            match result {
                Err(PreflightError::NoRuntimeReachable { attempts }) => {
                    assert_eq!(attempts.len(), 4);
                    assert_eq!(attempts[0].source, PrecedenceLayer::LcrcRuntimeDockerHost);
                    assert!(matches!(attempts[0].failure, ProbeFailure::EnvVarUnset));
                    assert_eq!(attempts[1].source, PrecedenceLayer::DockerHost);
                    assert!(matches!(attempts[1].failure, ProbeFailure::EnvVarUnset));
                    assert_eq!(attempts[2].source, PrecedenceLayer::DefaultDockerSock);
                    assert!(matches!(attempts[2].failure, ProbeFailure::SocketFileMissing));
                    assert_eq!(attempts[3].source, PrecedenceLayer::PodmanDefaultSock);
                    // Last layer might be SocketFileMissing or ConnectFailed
                    // depending on whether the podman default path exists.
                    assert!(matches!(
                        attempts[3].failure,
                        ProbeFailure::SocketFileMissing | ProbeFailure::ConnectFailed { .. }
                    ));
                }
                other => panic!("expected NoRuntimeReachable, got {other:?}"),
            }
        }
    }
    ```
    - **Why `tokio::test(flavor = "current_thread")`**: `detect` is async; tests need a runtime. `current_thread` is enough — the test makes a single sequential call and does not benefit from multi-threaded scheduling. Matches Story 1.7 / 1.8 conventions where async tests stayed minimal.
    - **Skip-on-real-runtime guard**: the test machine may actually have `/var/run/docker.sock`. We skip rather than fail in that case — a real runtime is not a *test* environment failure. Same pattern Story 1.5 used (`apple_silicon::tests` skips on non-arm64).
    - **Tests that need a *successful* probe** (AC3, AC4) are integration-tested in `tests/sandbox_preflight.rs` (T4) using `tokio::net::UnixListener` + a hand-rolled HTTP/1.1 `/_ping` responder. The in-module unit suite stays focused on the pure-function paths (precedence resolution, label rendering, error chain construction).
  - [ ] T3.14 Do NOT define a `Display` impl for `RuntimeProbe`, `ProbeAttempt`, `Candidate`, or `EnvSource`. The Display surface this story owns is on `PreflightError` (via `format_no_runtime_reachable`) and on `ProbeFailure` + `SandboxError` (via thiserror's `#[error]`). Pre-adding Display to value types creates two paths to format the same data, with the inevitable drift.
  - [ ] T3.15 Do NOT add a `bollard::API_DEFAULT_VERSION` re-export at the crate root. The constant lives in `bollard`; consumers in this story reach it via the `bollard::` path explicitly. Story 1.10 (the first heavy bollard consumer) decides the re-export policy.
  - [ ] T3.16 Do NOT add `From<bollard::errors::Error> for ProbeFailure`. Each call site that produces a `ProbeFailure::PingFailed` does so explicitly via `.map_err(|source| ProbeFailure::PingFailed { source })`. A blanket `From` impl would lose the discriminator between `PingFailed` (post-connect) and `ConnectFailed` (pre-connect, mapped via a different path). Same pattern Story 1.8 used to forbid `From<rusqlite::Error> for CacheError`.

- [ ] **T4. Author `tests/sandbox_preflight.rs` — integration tests with mock Unix listeners** (AC: 3, 4, 5)
  - [ ] T4.1 New file `tests/sandbox_preflight.rs`. Standard integration-test crate (separate compilation unit; sees `lcrc::*` only via the public API). Standard exemption attribute at file top: `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`. Use `#[tokio::test(flavor = "current_thread")]` — every test in this file is async because `detect` is async.
  - [ ] T4.2 Imports:
    ```rust
    use lcrc::sandbox::runtime::{
        detect, EnvSource, PrecedenceLayer, PreflightError, ProbeFailure,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;
    use tokio::task::JoinHandle;
    ```
  - [ ] T4.3 Helper `MapEnv` (copy of T3.13's helper, lifted into this crate — integration crates cannot see `src/sandbox/runtime.rs`'s `tests` module):
    ```rust
    #[derive(Debug, Default)]
    struct MapEnv(Mutex<HashMap<String, String>>);

    impl MapEnv {
        fn with(pairs: &[(&str, &str)]) -> Self {
            let mut m = HashMap::new();
            for (k, v) in pairs {
                m.insert((*k).to_string(), (*v).to_string());
            }
            Self(Mutex::new(m))
        }
    }

    impl EnvSource for MapEnv {
        fn get(&self, name: &str) -> Option<String> {
            self.0.lock().unwrap().get(name).cloned().filter(|v| !v.is_empty())
        }
    }
    ```
  - [ ] T4.4 Helper `spawn_mock_docker(path: PathBuf, accept_count: Arc<AtomicUsize>) -> JoinHandle<()>` — spawns a `UnixListener` on `path` that accepts incoming connections, reads the HTTP request, and replies with a canned `/_ping` response. Implementation:
    ```rust
    /// Minimal HTTP/1.1 handler that mimics `/_ping`. Returns immediately
    /// after writing the response; does not implement keep-alive.
    async fn handle_one(mut stream: tokio::net::UnixStream) {
        let mut buf = vec![0_u8; 4096];
        let _ = stream.read(&mut buf).await; // best-effort: ignore parse errors
        const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\
                                  Content-Type: text/plain\r\n\
                                  Content-Length: 2\r\n\
                                  Api-Version: 1.43\r\n\
                                  \r\nOK";
        let _ = stream.write_all(RESPONSE).await;
        let _ = stream.shutdown().await;
    }

    fn spawn_mock_docker(path: PathBuf, accept_count: Arc<AtomicUsize>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let listener = UnixListener::bind(&path)
                .expect("UnixListener bind must succeed in tests");
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        accept_count.fetch_add(1, Ordering::SeqCst);
                        tokio::spawn(handle_one(stream));
                    }
                    Err(_) => break,
                }
            }
        })
    }
    ```
    - **Why an `Arc<AtomicUsize>`** counter: AC4 verifies that the chain stops at the first successful layer — the unprobed layers' counters stay at 0. The counter is the canonical "did the probe touch this socket" signal.
    - **`accept` may run forever**: the listener loop is unbounded; the test drops the `JoinHandle` (or aborts it explicitly) when the test scope ends. `tokio::test` cleans up its runtime, which drops outstanding tasks. If a test hangs waiting on a listener, the abort-on-drop path is broken — investigate; do not paper over with `tokio::time::timeout`.
    - **Header `Api-Version: 1.43`**: bollard's `Docker::ping()` reads this header to negotiate the API version. The exact value matters less than its presence — `1.43` is recent-stable Docker; verify against bollard 0.18's actual ping handler during dev-story.
    - **Best-effort error swallowing in `handle_one`**: the mock is fixture code; bollard's connection state on the other end determines correctness. If bollard's `ping()` succeeds against this canned response, the test passes; if it fails (because of a header mismatch or HTTP/1.1 vs HTTP/1.0 quirk), iterate on `RESPONSE` until ping succeeds. The fixture's job is to look enough like a Docker daemon for ping to return `Ok` — nothing more.
  - [ ] T4.5 Test `successful_probe_via_lcrc_runtime_docker_host`:
    ```rust
    #[tokio::test(flavor = "current_thread")]
    async fn successful_probe_via_lcrc_runtime_docker_host() {
        let dir = TempDir::new().unwrap();
        let sock = dir.path().join("lcrc.sock");
        let counter = Arc::new(AtomicUsize::new(0));
        let _h = spawn_mock_docker(sock.clone(), counter.clone());
        // Give the listener a tick to bind.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let env = MapEnv::with(&[("LCRC_RUNTIME_DOCKER_HOST", sock.to_str().unwrap())]);
        let probe = detect(&env).await.unwrap();
        assert_eq!(probe.source, PrecedenceLayer::LcrcRuntimeDockerHost);
        assert_eq!(probe.socket_path, sock);
        assert!(counter.load(Ordering::SeqCst) >= 1);
    }
    ```
    - **AC3 verification, layer 1**: the `LCRC_RUNTIME_DOCKER_HOST` half. Same shape as AC3's general case but pins layer 1 specifically.
    - **Why the `sleep(50ms)`**: `tokio::spawn` schedules the listener task; without yielding, `detect` may try to connect before `UnixListener::bind` returns. 50 ms is generous; if it flakes, switch to a `tokio::sync::oneshot` ready-signal from the listener.
  - [ ] T4.6 Test `successful_probe_via_docker_host_when_lcrc_unset`:
    ```rust
    #[tokio::test(flavor = "current_thread")]
    async fn successful_probe_via_docker_host_when_lcrc_unset() {
        let dir = TempDir::new().unwrap();
        let sock = dir.path().join("docker.sock");
        let counter = Arc::new(AtomicUsize::new(0));
        let _h = spawn_mock_docker(sock.clone(), counter.clone());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let env = MapEnv::with(&[("DOCKER_HOST", &format!("unix://{}", sock.display()))]);
        let probe = detect(&env).await.unwrap();
        assert_eq!(probe.source, PrecedenceLayer::DockerHost);
        assert_eq!(probe.socket_path, sock);
        assert!(counter.load(Ordering::SeqCst) >= 1);
    }
    ```
    - **AC3 verification, layer 2** + the `unix://` prefix-stripping check from T3.9. One test covers both.
  - [ ] T4.7 Test `precedence_chain_stops_at_first_success_AC4`:
    ```rust
    #[tokio::test(flavor = "current_thread")]
    async fn precedence_chain_stops_at_first_success_AC4() {
        let dir = TempDir::new().unwrap();
        let sock1 = dir.path().join("lcrc.sock");
        let sock2 = dir.path().join("docker.sock");
        let c1 = Arc::new(AtomicUsize::new(0));
        let c2 = Arc::new(AtomicUsize::new(0));
        let _h1 = spawn_mock_docker(sock1.clone(), c1.clone());
        let _h2 = spawn_mock_docker(sock2.clone(), c2.clone());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let env = MapEnv::with(&[
            ("LCRC_RUNTIME_DOCKER_HOST", sock1.to_str().unwrap()),
            ("DOCKER_HOST", sock2.to_str().unwrap()),
        ]);
        let probe = detect(&env).await.unwrap();
        // First layer wins.
        assert_eq!(probe.source, PrecedenceLayer::LcrcRuntimeDockerHost);
        assert_eq!(probe.socket_path, sock1);
        // First layer was probed at least once.
        assert!(c1.load(Ordering::SeqCst) >= 1, "LCRC layer was not probed");
        // Second (and later) layers were never opened.
        assert_eq!(c2.load(Ordering::SeqCst), 0, "DOCKER_HOST layer was probed despite earlier success");
    }
    ```
    - **AC4 direct verification**. The DefaultDockerSock + PodmanDefaultSock layers can't be tested for "no probe" without filesystem manipulation; the AC's two-layer test (LCRC + DOCKER_HOST) is the load-bearing assertion. The implicit guarantee for the further layers follows from the same loop — once the test pins that the loop short-circuits at layer 1, the pattern holds for layers 3 and 4 by inspection.
  - [ ] T4.8 Test `no_runtime_reachable_returns_four_attempts_in_order_AC5`:
    ```rust
    #[tokio::test(flavor = "current_thread")]
    async fn no_runtime_reachable_returns_four_attempts_in_order_AC5() {
        let dir = TempDir::new().unwrap();
        // Point env layers at paths that don't exist (no mock listener bound).
        let env = MapEnv::with(&[
            ("LCRC_RUNTIME_DOCKER_HOST", dir.path().join("nope1.sock").to_str().unwrap()),
            ("DOCKER_HOST", dir.path().join("nope2.sock").to_str().unwrap()),
        ]);
        // If the host has /var/run/docker.sock or a Podman socket alive,
        // the test cannot deterministically force NoRuntimeReachable. Skip
        // with an explanatory message rather than fail.
        if std::path::Path::new("/var/run/docker.sock").exists() {
            eprintln!("skipping: /var/run/docker.sock exists on this machine");
            return;
        }
        let err = detect(&env).await.unwrap_err();
        match err {
            PreflightError::NoRuntimeReachable { attempts } => {
                assert_eq!(attempts.len(), 4, "expected 4 attempts, got {}", attempts.len());
                // Order: LcrcRuntimeDockerHost, DockerHost, DefaultDockerSock, PodmanDefaultSock.
                assert_eq!(attempts[0].source, PrecedenceLayer::LcrcRuntimeDockerHost);
                assert!(matches!(attempts[0].failure, ProbeFailure::SocketFileMissing));
                assert_eq!(attempts[1].source, PrecedenceLayer::DockerHost);
                assert!(matches!(attempts[1].failure, ProbeFailure::SocketFileMissing));
                assert_eq!(attempts[2].source, PrecedenceLayer::DefaultDockerSock);
                assert!(matches!(attempts[2].failure, ProbeFailure::SocketFileMissing));
                assert_eq!(attempts[3].source, PrecedenceLayer::PodmanDefaultSock);
                // Podman path may be SocketFileMissing or ConnectFailed.
                assert!(matches!(
                    attempts[3].failure,
                    ProbeFailure::SocketFileMissing | ProbeFailure::ConnectFailed { .. }
                ));
            }
            other => panic!("expected NoRuntimeReachable, got {other:?}"),
        }
    }
    ```
    - **AC5 direct verification**. Variants for layers 0/1 are `SocketFileMissing` (not `EnvVarUnset`) because the env vars are *set* — to nonexistent paths. The `EnvVarUnset` path is exercised in the unit tests (T3.13 `detect_returns_no_runtime_reachable_when_chain_empty`).
  - [ ] T4.9 Test `env_var_set_to_empty_string_treated_as_unset`:
    ```rust
    #[tokio::test(flavor = "current_thread")]
    async fn env_var_set_to_empty_string_treated_as_unset() {
        let env = MapEnv::with(&[
            ("LCRC_RUNTIME_DOCKER_HOST", ""),
            ("DOCKER_HOST", ""),
        ]);
        if std::path::Path::new("/var/run/docker.sock").exists() {
            eprintln!("skipping: /var/run/docker.sock exists on this machine");
            return;
        }
        let err = detect(&env).await.unwrap_err();
        match err {
            PreflightError::NoRuntimeReachable { attempts } => {
                assert!(matches!(attempts[0].failure, ProbeFailure::EnvVarUnset));
                assert!(matches!(attempts[1].failure, ProbeFailure::EnvVarUnset));
            }
            other => panic!("expected NoRuntimeReachable, got {other:?}"),
        }
    }
    ```
    - **POSIX-shell-empty-string semantics check** (T3.8). Pins that empty-string env vars are treated as unset, not as the empty path string `""`.
  - [ ] T4.10 Do NOT spawn the `lcrc` binary in this test (no `assert_cmd::Command::cargo_bin("lcrc")`). The end-to-end CLI exit-11 contract is verified separately in `tests/cli_exit_codes.rs` (T6) — that's where `assert_cmd` is the right tool. This crate exercises the typed Rust API surface directly.
  - [ ] T4.11 Do NOT add a test that exercises the *real* host runtime (e.g. opening `/var/run/docker.sock` if it exists and asserting success). Real-runtime tests are environment-dependent and would flake on CI runners that don't have Docker installed. The `tests/sandbox_envelope.rs` integration suite (Story 1.10 / Story 7.4) is the right home for runtime-real tests, and it gates v1 ship — not Story 1.9's preflight primitive.
  - [ ] T4.12 Do NOT use `serial_test` or any cross-test serialization crate. Each test creates its own `TempDir` and `MapEnv`; none mutate process-global state. The standard parallel test execution is correct.

- [ ] **T5. Update `src/cli/scan.rs` — wire preflight + setup-instructions output** (AC: 1, 2, 6)
  - [ ] T5.1 Replace the current placeholder `pub fn run() -> Result<(), crate::error::Error>` body. The new body:
    1. Spins up a current-thread tokio runtime (`tokio::runtime::Builder::new_current_thread().enable_all().build()?` — or equivalent error-mapped via `Error::Other(anyhow::Error)` for the build failure path).
    2. Inside `runtime.block_on(async { ... })`, calls `lcrc::sandbox::runtime::detect(&lcrc::sandbox::runtime::SystemEnv).await`.
    3. On `Ok(probe)`: emits `tracing::info!(target: "lcrc::sandbox::runtime", socket_path = %probe.socket_path.display(), source = probe.source.name(), "detected container runtime")`. Then continues with the existing placeholder behavior: `crate::output::diag("`lcrc scan` is not yet implemented in this build.");` + `Ok(())`. The end-to-end scan pipeline (image pull → server start → container run → cell write → HTML render) is wired in Story 1.12; for now, scan does the preflight and stops.
    4. On `Err(PreflightError::NoRuntimeReachable { .. })`:
       a. Prints the rendered `format_no_runtime_reachable(...)` block to stderr via `crate::output::diag(&err.to_string())`. The Display already includes the layer-by-layer breakdown + setup instructions; one print covers both AC1 (substring `brew install podman`) and the full diagnostic.
       b. Returns `Err(crate::error::Error::Preflight(err.to_string()))`. The `to_string()` carries the same Display message; `main.rs` then prints `error: preflight failed: <full block>` *and* maps to `ExitCode::PreflightFailed`. The user sees the full diagnostic twice (once from `output::diag`, once from `main.rs::lcrc::output::diag(format!("error: {e}"))`). **This is a known double-print** — see Resolved decisions § "double-print of preflight diagnostic".
  - [ ] T5.2 Skeleton:
    ```rust
    //! Module exists so `lcrc scan --help` works — clap-derive emits the
    //! per-subcommand help from the `Subcommand` enum's
    //! `#[command(about = ...)]`. The `run` body wires preflight (Story 1.9);
    //! the rest of the scan pipeline lands in Story 1.12.

    /// Entry point for `lcrc scan`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Preflight`] when the container-runtime
    /// preflight (FR17a) detects no reachable Docker-Engine-API-compatible
    /// socket.
    pub fn run() -> Result<(), crate::error::Error> {
        let runtime = tokio::runtime::Builder::new_current_thread()
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
                    crate::output::diag("`lcrc scan` is not yet implemented in this build.");
                    Ok(())
                }
                Err(err) => {
                    crate::output::diag(&err.to_string());
                    Err(crate::error::Error::Preflight(err.to_string()))
                }
            }
        })
    }
    ```
    - **Why a current-thread runtime built in-place**: `cli::dispatch` is sync, and Stories 1.5 / 1.6 / 1.7 / 1.8 all kept the CLI surface sync. Adding `#[tokio::main]` to `main.rs` would force the entire dispatch chain async — a larger refactor than this story's scope. The current-thread runtime built locally is the minimum-viable bridge: scoped to scan's lifetime, dropped at the end of scan, leaves the rest of the CLI untouched.
    - **`enable_all()`**: enables both the IO and time drivers. `tokio::time::sleep` (used by tests but not by production preflight code) and `tokio::net::UnixStream` (used by bollard) require the IO + time drivers respectively. Cheap to enable both; saves a future "why is my timer not firing" debug session.
    - **`Error::Preflight(format!("tokio runtime init: {e}"))`** for the runtime-build failure path. This is the only path in `scan::run` that can fail before `detect` even runs; mapping it to `Preflight` over-classifies but is acceptable (the user sees `error: preflight failed: tokio runtime init: ...` and knows something below the runtime layer broke). Story 1.12 may decide to introduce a more specific variant; this story does not.
    - **`tracing::info!` field renderings**: `socket_path = %probe.socket_path.display()` uses the `%`-prefix to invoke `Display` (paths render via `display()`); `source = probe.source.name()` is a `&'static str`. The default `tracing-subscriber` `fmt::Layer` (Story 1.4 install) renders fields after the message: `INFO lcrc::sandbox::runtime: detected container runtime socket_path=/var/run/docker.sock source=default_docker_sock`. The `target:` override pins the event's module path so subscribers can filter by `lcrc::sandbox::runtime`.
  - [ ] T5.3 Add to the existing in-module `mod tests` of `src/cli/scan.rs`: a smoke test that `run()` returns `Err(Error::Preflight(...))` when no runtime is reachable. Skip if `/var/run/docker.sock` exists (same pattern T3.13 / T4.8 used). Note: this test uses `SystemEnv` against the real process env; tests that need to inject a `MapEnv` go through `runtime::detect` directly (covered by T3 / T4).
    ```rust
    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    mod tests {
        use super::run;
        use crate::error::Error;

        #[test]
        fn run_returns_preflight_error_when_no_runtime() {
            // Skip in any environment that has a real runtime — the test
            // can only assert the negative path. Same skip pattern as
            // sandbox/runtime.rs and tests/sandbox_preflight.rs.
            if std::path::Path::new("/var/run/docker.sock").exists() {
                eprintln!("skipping: /var/run/docker.sock exists on this machine");
                return;
            }
            // Force env vars to point at non-existent paths so the
            // env-backed layers fail with SocketFileMissing rather than
            // accidentally using the host's real DOCKER_HOST.
            //
            // SAFETY: tests are single-threaded enough that this
            // env mutation is acceptable; the standard `serial_test`
            // alternative is overkill for one test. If a future test
            // adds parallel env mutations, switch to MapEnv via the
            // detect() entry point.
            // SAFETY: the test runs on a single thread for this binary;
            // `set_var` + `remove_var` are unsafe in multi-threaded
            // contexts (Rust 2024) but the test does not spawn threads
            // before calling them.
            // To stay clippy-clean and unsafe-clean, this test instead
            // unsets the two env vars only if they are set, and lives
            // with the consequence: if a developer's shell has
            // DOCKER_HOST set to a real reachable runtime, this test
            // is in the same skip bucket.
            //
            // Two-line guard:
            if std::env::var("DOCKER_HOST").is_ok()
                || std::env::var("LCRC_RUNTIME_DOCKER_HOST").is_ok()
            {
                eprintln!("skipping: DOCKER_HOST or LCRC_RUNTIME_DOCKER_HOST set in env");
                return;
            }
            let result = run();
            match result {
                Err(Error::Preflight(msg)) => {
                    assert!(msg.contains("brew install podman"),
                        "expected setup instructions in error message, got: {msg}");
                }
                other => panic!("expected Err(Preflight), got {other:?}"),
            }
        }
    }
    ```
    - **No `set_var` / `remove_var`**: Rust 2024 marks `std::env::set_var` and `remove_var` as `unsafe fn` because they race with reads in other threads. The crate's `unsafe_code = "forbid"` lint rejects them outright. The test instead *checks* whether the env vars are set and skips if so; the trade-off is documented inline.
  - [ ] T5.4 Do NOT update `src/cli.rs` (the parent `cli` module file). The dispatch chain is unchanged; only `cli/scan.rs::run` body changes.
  - [ ] T5.5 Do NOT touch `src/main.rs`. The error-rendering call site (`output::diag(&format!("error: {e}"))` followed by `e.exit_code()`) already handles `Error::Preflight` correctly via the existing `error::Error::exit_code` exhaustive match (`src/error.rs:62`). No change needed.

- [ ] **T6. Update `tests/cli_exit_codes.rs` — add an exit-11 scenario test** (AC: 1, 2)
  - [ ] T6.1 Append a new test `scan_exits_11_with_setup_instructions_when_no_runtime`:
    ```rust
    #[test]
    fn scan_exits_11_with_setup_instructions_when_no_runtime() {
        // Skip when the host has a real runtime (CI Mac runners may not).
        if std::path::Path::new("/var/run/docker.sock").exists() {
            eprintln!("skipping: /var/run/docker.sock exists on this machine");
            return;
        }
        // Skip when env redirects to a real runtime.
        if std::env::var("DOCKER_HOST").is_ok()
            || std::env::var("LCRC_RUNTIME_DOCKER_HOST").is_ok()
        {
            eprintln!("skipping: DOCKER_HOST or LCRC_RUNTIME_DOCKER_HOST set");
            return;
        }
        let assert = Command::cargo_bin("lcrc")
            .unwrap()
            .arg("scan")
            .env_remove("DOCKER_HOST")
            .env_remove("LCRC_RUNTIME_DOCKER_HOST")
            .assert()
            .code(ExitCode::PreflightFailed.as_i32());
        let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
        // AC1 substring contract: setup-instructions block must appear.
        assert!(stderr.contains("brew install podman"),
            "stderr missing `brew install podman`: {stderr}");
        // AC2 substring contract: start-the-machine instruction must appear.
        assert!(stderr.contains("podman machine init"),
            "stderr missing `podman machine init`: {stderr}");
        assert!(stderr.contains("podman machine start"),
            "stderr missing `podman machine start`: {stderr}");
    }
    ```
    - **`env_remove("DOCKER_HOST")` + `env_remove("LCRC_RUNTIME_DOCKER_HOST")`**: belt-and-suspender on the env-skip guard. `assert_cmd::Command::env_remove` strips the var from the spawned child's env *before* the binary starts, so even if the developer's shell has it set, the spawned `lcrc scan` sees them unset.
    - **`assert.get_output().stderr.clone()`**: `assert_cmd::Assert::get_output` returns the captured `Output`; `.stderr` is the captured stderr bytes. We check substrings on the `String` rendering. AC1 + AC2 are both substring contracts → one stderr capture covers both.
    - **AC1's "stdout is empty" sub-contract**: the AC text says "the setup-instructions block is the only user-facing diagnostic on this path; nothing else is printed to stdout." We can additionally assert `assert.get_output().stdout.is_empty()`. Add this assertion if clippy-clean and idiomatic.
  - [ ] T6.2 Do NOT modify the existing `ok_path_exits_0` or `exit_code_enum_full_contract` tests. They cover separate contracts.
  - [ ] T6.3 Do NOT add a test for the *successful* scan path (`scan_exits_0_when_runtime_reachable`). Successful scan is Story 1.12's contract; this story's scan command is "preflight + placeholder" — the success path is "preflight passes → print placeholder → exit 0", which is not a binding contract until Story 1.12 wires the rest of the pipeline. A test asserting exit-0-on-real-runtime would either be skipped on most CI runners or be wrong once Story 1.12 makes the placeholder go away.

- [ ] **T7. Cargo.toml verification** (AC: all)
  - [ ] T7.1 Run `cargo build` and observe whether `Cargo.lock` changes. Bollard 0.18 is already locked (line 42). `tokio = { version = "1", features = ["full"] }` (line 35) covers `UnixListener`, `UnixStream`, `time::sleep`, `runtime::Builder`, `task::spawn`, `task::JoinHandle`, `io::AsyncReadExt`, `io::AsyncWriteExt` — all needed by tests + production. `tempfile = "3"` (line 50) is locked. `nix = { version = "0.29", features = ["signal"] }` (line 56) — verify the `unistd::Uid::current()` API is reachable under the `signal` feature. If not (likely needs the `user` feature), bump the feature list:
    ```toml
    nix = { version = "0.29", features = ["signal", "user"] }
    ```
    This is the **only** Cargo.toml change Story 1.9 may need; document the change in the File List + Dev Notes if it lands.
  - [ ] T7.2 Do NOT add `bollard`-related sub-features beyond the default. The default `bollard` feature set (since 0.18) already includes the Unix-socket connector; no `chrono` / `ssl` / `time` extension needed for ping.
  - [ ] T7.3 Do NOT add `serial_test` or any test-serialization crate. Story 1.9's tests do not mutate process-global state (env, working dir, signal handlers).
  - [ ] T7.4 If `Cargo.lock` changes for any reason other than the documented `nix` feature bump, investigate before pushing. An unintended re-resolve signals an accidental dep-tree change worth understanding.

- [ ] **T8. Local CI mirror** (AC: all)
  - [ ] T8.1 `cargo build` — confirms the new module compiles and the wire-up in `cli/scan.rs` typechecks against `error::Error`.
  - [ ] T8.2 `cargo fmt` — apply rustfmt; commit any reformatted lines.
  - [ ] T8.3 `cargo clippy --all-targets --all-features -- -D warnings`. Specifically watch for:
    - `clippy::missing_errors_doc` on `pub async fn detect` and `pub fn run` — `# Errors` rustdoc section per T3.12 / T5.2.
    - `clippy::missing_docs` on every `pub` item (`SandboxError`, `PreflightError`, `RuntimeProbe`, `ProbeAttempt`, `ProbeFailure`, `PrecedenceLayer`, `EnvSource`, `SystemEnv`, public methods).
    - `clippy::module_name_repetitions` may fire on `RuntimeProbe` / `PreflightError` inside the `runtime` module — the names already include the module's domain word. Suppress with `#[allow(clippy::module_name_repetitions)]` on the type if it does fire; the alternative (renaming to `Probe` / `Error`) clashes with the parent module's scope.
    - `clippy::redundant_closure_for_method_calls` may fire on `.map_err(|e| e.to_string())` constructions; rewrite as `.map_err(|e| ProbeFailure::PingFailed { source: e })` (which is what we already have) — clippy is happy with the explicit struct literal.
    - `clippy::result_large_err` should NOT fire — `PreflightError` carries a `Vec<ProbeAttempt>` (24 bytes for the Vec header) plus `attempts` items live on the heap. Total `Result<RuntimeProbe, PreflightError>` size is bounded by `RuntimeProbe` size (`PathBuf` 24 bytes + `PrecedenceLayer` 1 byte + padding ≈ 32 bytes) + the `PreflightError` enum tag + `Vec<ProbeAttempt>` header (24 bytes) ≈ under 64 bytes. Comfortable inside the 128-byte budget. If clippy disagrees, box the `Vec` (`Box<Vec<ProbeAttempt>>`) before suppressing.
    - `clippy::unnecessary_wraps` should NOT fire — both `detect` and `probe_one` return `Result` because they can fail.
    - `clippy::needless_pass_by_value` should NOT fire — public APIs take `&dyn EnvSource` and `&Candidate`.
  - [ ] T8.4 `cargo test` — all suites:
    - In-module: `cache::*`, `cli::*`, `error`, `exit_code`, `machine`, `output`, **`sandbox`** (new), **`sandbox::runtime`** (new), `util::tracing`, `version`.
    - Integration: `cache_migrations`, `cache_roundtrip`, `cli_exit_codes` (now with the new exit-11 test), `cli_help_version`, `machine_fingerprint`, **`sandbox_preflight`** (new).
    - Total target: ~110+ tests pass (Story 1.8 left ~94; this story adds the in-module sandbox tests + 6 integration tests + 1 cli-exit-code test).
  - [ ] T8.5 Manual scope-discipline grep:
    ```
    git grep -nE 'bollard::|tokio::net::UnixListener|UnixStream' src/ tests/ \
      | grep -v '^src/sandbox/runtime.rs:' \
      | grep -v '^src/cli/scan.rs:' \
      | grep -v '^tests/sandbox_preflight.rs:'
    ```
    Must produce zero matches — bollard + Unix-socket surface stays inside the sandbox modules and their tests + the cli scan wiring point. Same single-source-of-truth grep contract Stories 1.6 / 1.7 / 1.8 used.
  - [ ] T8.6 Verify locally that `RUST_LOG=info cargo run -- scan` (with no runtime available) prints the `INFO lcrc::sandbox::runtime: detected container runtime ...` line (if a runtime IS available) or the setup-instructions block (if not). Eyeball the rendering. AC6 is verified by the in-module + integration tests; this is the manual sanity check.

## Dev Notes

### Scope discipline (read this first)

This story authors **two new files** (`src/sandbox.rs`, `src/sandbox/runtime.rs`), updates **two existing files** (`src/lib.rs`, `src/cli/scan.rs`), and adds **one new integration-test file** (`tests/sandbox_preflight.rs`) plus **one new test in an existing file** (`tests/cli_exit_codes.rs`). Optionally bumps a `nix` feature in `Cargo.toml` (one line) if `Uid::current()` requires it.

This story does **not**:

- Author `src/sandbox/container.rs`, `src/sandbox/network.rs`, `src/sandbox/env_allowlist.rs`, `src/sandbox/image.rs`, or `src/sandbox/violation.rs`. Those files are owned by Stories 1.10 (container + network), 2.7 (env allowlist), 1.14 + 1.10 (image), and 2.8 (violation). Pre-creating empty stubs violates the tracer-bullet vertical-slice principle (`MEMORY.md → feedback_tracer_bullet_epics.md`).
- Construct or use a `bollard::Docker` for any purpose other than `/_ping`. Container creation, image pull, network setup are explicitly out of scope.
- Implement `Sandbox::run_task` from Story 1.10. The `Sandbox` struct does not exist yet; this story owns no `Sandbox` type. The successful preflight result is a `RuntimeProbe` value that future stories will consume to construct `Sandbox` (or whatever Story 1.10 ends up naming the orchestrator).
- Add a `--unsafe-no-sandbox` flag, a `--skip-preflight` flag, or any other escape hatch. NFR-S3 forbids them; the architecture line 643 reinforces ("no `--unsafe-no-sandbox` fallback exists").
- Add `From<SandboxError> for crate::error::Error` or `From<PreflightError> for crate::error::Error`. Story 1.12 (the consumer that wires the full scan pipeline) decides the boundary. `cli/scan.rs::run` does the inline `format!("{e}")` conversion in this story — see T5.1.
- Wire `scan` to anything beyond preflight. The current placeholder behavior (print "not yet implemented" diagnostic + exit 0 on success) stays in place; Story 1.12 replaces the placeholder with the end-to-end pipeline.
- Define structured tracing fields beyond `socket_path` and `source`. Future stories may add `runtime_version`, `image_pull_duration`, etc.; those are their owners' decisions.
- Add `lcrc-runtime-status`, `lcrc doctor`, or any introspection subcommand. v1.1+ candidates per architecture.md decision-priority summary line 239.
- Touch `src/cli.rs`, `src/cli/show.rs`, `src/cli/verify.rs`, `src/error.rs`, `src/exit_code.rs`, `src/main.rs`, `src/output.rs`, `src/util.rs`, `src/util/tracing.rs`, `src/version.rs`, `src/machine*.rs`, `src/cache*.rs`, `tests/cache_*.rs`, `tests/cli_help_version.rs`, `tests/machine_fingerprint.rs`, `Cargo.lock` (other than as a side-effect of the optional `nix` feature bump), `build.rs`, or `.github/workflows/*`. None of those need to change for Story 1.9.
- Author or update `tasks/swe-bench-pro/`, `image/Dockerfile`, `image/requirements.txt`, or `homebrew/lcrc.rb`. Container concerns are Story 1.10 / 1.14; Homebrew is Story 7.1.
- Add a CI workflow change. The runtime-real `tests/sandbox_envelope.rs` battery (Story 7.4) is what drives the GitHub Actions matrix question; this story's tests are deterministic without a real runtime.

### Architecture compliance (binding constraints)

- **Single source of truth: `src/sandbox/runtime.rs` for the precedence chain + bollard ping** [Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" lines 922-924 + § "Architectural Boundaries" line 994 + § "Runtime socket precedence (5 layers)" lines 458-463]: `runtime.rs` owns `Docker::connect_with_unix` + `Docker::ping`. Other modules go through `runtime::detect(...)`. After this story merges, `bollard::` references stay inside `src/sandbox/runtime.rs` + `tests/sandbox_preflight.rs`; the T8.5 grep guards this contract.
- **Architectural Boundary: `src/sandbox/container.rs` is the only module that calls `bollard::container::Container::create`** [Source: architecture.md line 799 + § "Sandbox Invariants — Structural, not Conventional"]: This story does NOT call `Container::create` (or any `bollard::container::*` API). It only calls `Docker::connect_with_unix` and `Docker::ping` — both at the *daemon* level, not the *container* level. The container-creation invariant lands when Story 1.10 authors `container.rs`.
- **No `unsafe` anywhere** [Source: `unsafe_code = "forbid"` in Cargo.toml line 78 + `lib.rs:3`]: Bollard ships internal `unsafe` for hyper / tokio FFI internally — that is its problem; the host crate stays `forbid(unsafe_code)`. Do NOT use `std::env::set_var` / `remove_var` in production code (Rust 2024 marks them `unsafe fn`); read-only `std::env::var_os` for `XDG_RUNTIME_DIR` and `HOME` in `podman_default_socket_path` is safe (per T3.10 documented exception).
- **All async file I/O via `tokio::fs` / `tokio::process`, never `std::fs` / `std::process`** [Source: architecture.md line 687]: `probe_one` uses `tokio::fs::try_exists` (not `std::fs::metadata` or `std::path::Path::exists`). The blocking `Path::exists` would force `probe_one` to use `tokio::task::spawn_blocking`, which is more code for the same result.
- **No `std::process` anywhere in this story's code** [Source: architecture.md AR-3]: N/A — preflight does not spawn subprocesses.
- **Workspace lints — `unwrap_used`, `expect_used`, `panic = "deny"`** [Source: Cargo.toml lines 83-85]: All `?` propagation against typed errors. The two test surfaces (`#[cfg(test)] mod tests` in `runtime.rs` + `sandbox.rs`, `tests/sandbox_preflight.rs`, `tests/cli_exit_codes.rs`) carry the documented `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` exemption. Production code uses zero `unwrap` / `expect` / `panic`.
- **`missing_docs = "warn"`** [Source: Cargo.toml line 79]: Every `pub` item gets a `///` doc — `SandboxError` + variants, `PreflightError` + variants, `RuntimeProbe` + fields, `ProbeAttempt` + fields, `ProbeFailure` + variants, `PrecedenceLayer` + variants + `name()`, `EnvSource` + `get`, `SystemEnv`, `detect`. `pub fn detect` returns `Result`, so it also needs a `# Errors` rustdoc section (clippy `missing_errors_doc`).
- **MSRV 1.95** [Source: Cargo.toml line 5]: `let-else` (stable 1.65), `tokio::fs::try_exists` (stable since tokio 1.18), `tokio::net::UnixListener::bind` + `accept` (stable since tokio 1.0), `bollard::Docker::connect_with_unix` (stable in bollard 0.18), `tokio::runtime::Builder::new_current_thread` (stable since tokio 0.2). No nightly features.
- **Crate is binary + library** [Source: architecture.md § "Complete Project Directory Structure" lines 874-876]: `sandbox::runtime` is library code; `tests/sandbox_preflight.rs` consumes it via `lcrc::sandbox::runtime::*`; `tests/cli_exit_codes.rs` exercises the binary via `assert_cmd`.
- **Tracing / logging discipline** [Source: AR `tracing` discipline + architecture.md § "Tracing / Logging" lines 760-770]: This story emits exactly one `tracing::info!` event in `cli/scan.rs::run` on the success path (AC6 contract). The event uses `target: "lcrc::sandbox::runtime"` and structured fields `socket_path` + `source` rather than string interpolation. Failure paths emit zero tracing events — the user-facing diagnostic flows through `output::diag` (the only allowed stderr writer per FR46).
- **stdout/stderr discipline (FR46)** [Source: architecture.md § "stdout / stderr Discipline (FR46)" lines 667-683 + `src/output.rs`]: All user-visible diagnostics go through `crate::output::diag` (the failure-path setup-instructions block). Tracing events go via the installed subscriber, which writes to stderr (`src/util/tracing.rs:38`). No direct `println!` / `eprintln!` / `print!` / `eprint!` / `dbg!` / `writeln!(io::stdout())` anywhere in this story's code.
- **No glob imports** [Source: implicit per existing code style]: Always name imported items (`use bollard::Docker;`, `use tokio::net::UnixListener;`).
- **`Cargo.lock` is committed; CI cache keys on it** [Source: Story 1.2 § Architecture compliance]. This story may add ONE Cargo.toml change (`nix` feature bump) — Cargo.lock will re-resolve `nix` accordingly. No other dep changes.
- **Single-writer model + lock-free reads** [Source: architecture.md § "Cache Architecture" line 287-294 + FR52, FR53]: N/A — this story does not touch the cache layer.
- **`Cache::open` and `Cache::write_cell` / `Cache::lookup_cell` are NOT called from preflight code.** Preflight is a pre-cache concern; the cache layer is invoked only after preflight succeeds (Story 1.12 wires the order).
- **Re-export policy** [Source: Story 1.5 / 1.6 / 1.7 / 1.8 conventions]: `RuntimeProbe`, `PreflightError`, `EnvSource`, `SystemEnv`, `detect` are accessed via `lcrc::sandbox::runtime::*`, not re-exported at `lcrc::sandbox::*` or `lcrc::*`. `SandboxError` lives at `lcrc::sandbox::SandboxError`. Re-exports are an Epic 6 polish concern.

### Resolved decisions (don't re-litigate)

These are choices the dev agent might be tempted to revisit. Each is locked here with rationale.

- **`detect` is `async fn`, not `fn` returning `impl Future`.** Why: `async fn` inside a public trait or struct gets the desugaring; it's the canonical Rust 2024 form. The signature `pub async fn detect(env: &dyn EnvSource) -> Result<RuntimeProbe, PreflightError>` is the locked shape.
- **`detect` takes `&dyn EnvSource`, not `impl EnvSource`.** Why: monomorphization adds binary size for every distinct caller without runtime benefit (the function's hot loop is the bollard ping, not env reads). `&dyn` keeps the codegen single-shape; matches the v1 single-call-site pattern. If a future caller benchmarks `&dyn` as a bottleneck, switching to `impl EnvSource` is a one-line, non-breaking signature change.
- **The precedence chain has exactly 4 layers, in this order: `LCRC_RUNTIME_DOCKER_HOST` → `DOCKER_HOST` → `/var/run/docker.sock` → Podman default per-uid socket.** Why: directly per architecture.md line 458-463 (the "5 layers" caption includes "CLI flag (none in v1)" as layer 1 and shifts the four code-relevant layers to 2-5; this story's enum names the four code-relevant layers). The `--runtime-socket` CLI flag is Epic 6 polish — not added in this story.
- **`PrecedenceLayer` derives `Copy + Clone + PartialEq + Eq + Hash`.** Why: pure tag-shaped enum, four nullary variants. Cheap to derive everything; the `Hash + Eq` pair lets future code use `PrecedenceLayer` as a `HashMap` key without re-deriving.
- **`ProbeAttempt` derives `Debug` only — NOT `PartialEq`, NOT `Clone`.** Why: `ProbeFailure` carries `std::io::Error` and `bollard::errors::Error`, neither of which is `Eq` or `Clone`. Tests inspect attempts via field-by-field destructuring + `matches!()` checks, not via `assert_eq!(attempt, expected)`.
- **`ProbeFailure::EnvVarUnset` for env-backed layers ONLY; `ProbeFailure::SocketFileMissing` for path-backed layers and for env layers whose resolved path doesn't exist.** Why: the user-visible distinction is "the env var is unset" (user can set it) vs "the path I tried doesn't exist" (user needs to install / start a runtime). Combining them would lose the actionable signal.
- **`ProbeAttempt.socket_path` for `EnvVarUnset` records `PathBuf::new()` (empty path).** Why: there is no path to record when the env var is unset. The Display template (T3.7) elides the path for `EnvVarUnset` failures so the rendered line reads `LCRC_RUNTIME_DOCKER_HOST: env var unset` (no trailing path placeholder). The empty `PathBuf` is a sentinel; alternatives considered (use `Option<PathBuf>` for the field, use a variant-specific structure) add API friction without changing the rendering. Locked: empty `PathBuf` + Display elision.
- **`SandboxError::Preflight(#[from] runtime::PreflightError)` is the ONLY variant in this story.** Why: scope discipline. Future variants (`ContainerCreate`, `ImagePull`, etc.) land in their owner stories. `#[from]` enables `?` propagation without manual wrapping at the future call sites.
- **`PreflightError::NoRuntimeReachable` is the ONLY variant in this story.** Same scope discipline. Future variants (`UnsupportedRuntime` for runtimes that don't expose iptables-rule injection per Story 1.10's AC) land later.
- **The setup-instructions block is hardcoded, identical for AC1 (no runtime installed) and AC2 (runtime not started).** Why: AC2 explicitly pins this — "the single-message-covers-both-modes design is locked: lcrc does not distinguish 'not installed' from 'installed-but-not-started' in the user-facing copy because the user remediation is the same superset of commands". A user who has Podman installed but not running runs the `init` step (no-op if already initialized) and the `start` step (starts it). A user without Podman runs all three. One copy block, both modes covered. Branching on `which podman` would add complexity for negative user value (the user reads three commands, runs the ones they need; the unused command is a no-op).
- **`detect` is sequential, not parallel.** Why: AC4 forbids probing later layers after an earlier layer succeeds. Parallel probing would either need to track which started first (complexity) or violate the "no extra connection" contract.
- **`bollard::Docker::ping()` is the canonical "is this a real Docker daemon" check.** Why: bollard's `connect_with_unix` constructs a client without round-tripping; only `ping()` (or the first real API call) verifies the other side speaks the protocol. Using `_ping` (idempotent, no auth, designed for health-checking) is the conventional check; alternatives (`info`, `version`) are heavier without added signal for our purposes. **Source for the ping endpoint contract**: Docker Engine API docs, `GET /_ping`.
- **The mock listener in `tests/sandbox_preflight.rs` returns a hand-rolled HTTP/1.1 response.** Why: spinning up `hyper::Server` for tests adds heavy boilerplate. A 200-byte canned response with the `Api-Version` header that bollard parses is enough. If bollard's ping handler tightens validation in a future release (e.g. requires `Server:` or `OSType:` headers), update `RESPONSE` accordingly.
- **`tokio::time::sleep(50ms)` between `spawn_mock_docker` and `detect` is the bind-readiness tradeoff.** Why: `tokio::spawn` schedules but does not yield; `UnixListener::bind` happens inside the spawned task. The cleanest fix is a `tokio::sync::oneshot` ready signal from inside the listener task to the test body, but that adds 4 lines per test and the 50 ms sleep is reliably sufficient on the local + CI runners we care about. If the test flakes on a future runner, switch to the oneshot pattern.
- **`cli/scan.rs::run` builds a current-thread tokio runtime in-place rather than promoting `lcrc::run` / `main` to async.** Why: smallest-diff approach matching Stories 1.5 / 1.6 / 1.7 / 1.8's discipline of keeping the CLI surface sync until a critical mass of async consumers forces the refactor. The current-thread runtime is dropped at the end of `scan::run`; no leak. When Stories 1.10 / 1.11 / 1.12 land, their author may decide the time is right for `#[tokio::main]` — that is their call, not this story's.
- **`enable_all()` on the runtime builder.** Why: the IO driver is needed for `bollard`'s tokio-backed connector; the time driver is needed if any future preflight code adds a connect-timeout (Story 1.10 likely will). Cheap to enable both now, no refactor when timeouts land.
- **`Error::Preflight(format!("{err}"))`** as the wrapping pattern from `cli/scan.rs::run` to the top-level `Error` type. Why: `Error::Preflight` is a `String` payload (`src/error.rs:21-22`); the format-into-String + `.into()`-style construction mirrors how Stories 1.5 / 1.6 / 1.7 / 1.8 punted CLI-error mapping until a `From` impl was actually warranted. The double-print of the diagnostic (once via `output::diag` in `scan::run`, once via `main.rs`'s `output::diag(format!("error: {e}"))`) is a known imperfection; see Resolved decisions § "double-print of preflight diagnostic" below.
- **Double-print of preflight diagnostic.** Why accepted: `cli/scan.rs::run` prints the rendered `format_no_runtime_reachable(...)` block via `output::diag` so the user sees the layer-by-layer breakdown + setup instructions on stderr. Then `main.rs` prints `error: preflight failed: <same block>` because it always prepends `error: ` to a top-level `Error`. The user sees the diagnostic twice. Alternatives considered: (a) skip the `output::diag` in `scan::run` and rely solely on `main.rs` — but then `main.rs`'s prefix `error: preflight failed:` is on the same line as the multi-line block, which renders awkwardly (`error: preflight failed: no container runtime reachable. lcrc tried (in order):\n  1. ...`). (b) Modify `main.rs` to render `Error::Preflight` differently — this changes the `main.rs` discipline of "render the same way for every variant", which Story 1.3 deliberately locked. The double-print is the lesser evil; Story 1.12 (the next CLI consumer) can revisit if it bites.
- **No `--runtime-socket` CLI flag in this story.** Why: architecture.md line 459 lists "CLI flag" as the topmost layer of the precedence chain BUT explicitly notes "(none in v1)". An Epic 6 polish story may add `--runtime-socket <path>`; until then the env-var + auto-probe layers are the surface. Adding a flag now would force a clap arg + `ScanArgs` field + plumbing down into `runtime::detect`, all for a feature the architecture defers.
- **`ScanArgs` stays empty (`pub struct ScanArgs {}`)** — no fields added. Why: the only Story-1.9-relevant CLI input is the env, not a flag. `--runtime-socket` is deferred (above). Stories 1.12 / 2.5 / 3.x will add real fields.
- **TCP-form `DOCKER_HOST` (`tcp://...`) is out of v1 scope.** Why: the architecture pins "Docker-Engine-API-compatible socket" — Unix-socket only in v1. A `DOCKER_HOST=tcp://...` value will be passed through `strip_unix_prefix` (no-op), then `tokio::fs::try_exists("tcp://...")` will return `false`, so the layer fails with `SocketFileMissing`. The user sees `DOCKER_HOST: socket file missing`, which is technically correct (the path doesn't exist as a Unix socket) but misleading. A future Epic 6 story may add a `tcp://` parser path; until then, the failure is semantically correct even if the diagnostic is slightly off. Documented here so a future reviewer doesn't "fix" it as a bug.
- **`std::env::var_os("HOME")` and `std::env::var_os("XDG_RUNTIME_DIR")` are read directly in `podman_default_socket_path`.** Why: architecture.md line 758 forbids direct `std::env::var` *for config env vars* (the `LCRC_*` namespace). Platform-discovery vars like `HOME` / `XDG_RUNTIME_DIR` are infrastructure concerns, not user-tunable configuration; reading them via `etcetera`'s XDG resolver would be over-abstraction (etcetera's API doesn't directly give us the per-uid socket path; we'd be peeling the abstraction back to the same env-var read). The exemption is documented at the call site.
- **No timeout on `bollard::Docker::ping()`.** Why: bollard's default connect timeout is 5 seconds (passed via `connect_with_unix(addr, 5, ...)`); the ping itself uses the configured client. If the runtime is unresponsive but the socket accepts connections, the ping will eventually fail; the connect-timeout already bounds the slow path. Adding a separate ping-timeout is a Story 1.10 (orchestrator) concern when timeout values become tunable via TOML.
- **No retry on probe failure.** Why: a runtime that doesn't respond on the first ping is functionally absent. Retrying buys nothing; the failure mode is "user starts the runtime + reruns scan", not "lcrc retries until the runtime wakes up". Same single-attempt-then-fail discipline as Story 1.5's machine fingerprint.
- **`SystemEnv` is `pub`, not `pub(crate)`.** Why: integration tests construct it explicitly to compare against `MapEnv`; library consumers (Story 1.12 + Story 2.6) construct it to call `detect`. Public surface; cheap.
- **No `serde::Serialize` derives on `RuntimeProbe`, `PreflightError`, `ProbeAttempt`, etc.** Why: this story exposes no JSON/serialization surface. Future `lcrc show --format json` (Story 4.4) deals with cell-side data, not preflight diagnostics. Adding `Serialize` now would lock a JSON contract before anyone needs it.

### Library / framework requirements

- **`bollard = "0.18"`** [Source: Cargo.toml line 42 — locked]: the async Docker Engine API client. Used in this story for `Docker::connect_with_unix(addr, timeout, version)` and `Docker.ping()`. Verify `bollard::API_DEFAULT_VERSION` is reachable in 0.18 (the constant's path may have shifted in major releases); if not, use the bollard-default version selector (likely `bollard::ClientVersion::default()` or similar) — the dev-story implementer adapts to the actual 0.18 API.
- **`tokio = { version = "1", features = ["full"] }`** [Source: Cargo.toml line 35 — locked]: covers `runtime::Builder::new_current_thread`, `net::UnixListener` + `UnixStream`, `fs::try_exists`, `time::sleep`, `task::spawn` + `JoinHandle`, `io::AsyncReadExt` + `AsyncWriteExt`. The `full` feature includes everything; no narrower feature set needed.
- **`thiserror = "2"`** [Source: Cargo.toml line 60 — locked]: derives `Display` + `Error` + `From` for `SandboxError`, `PreflightError`, `ProbeFailure`. Same pattern Stories 1.3 / 1.5 / 1.6 / 1.7 / 1.8 used.
- **`tracing = "0.1"`** [Source: Cargo.toml line 62 — locked]: emits the success-path `info!` event with structured fields. Subscriber install is Story 1.4's `util::tracing::init`; AC6 verification reads the rendered output via the subscriber installed at `cli::dispatch`.
- **`tempfile = "3"`** [Source: Cargo.toml line 50 — locked]: integration tests use `TempDir::new()` for socket paths. Standard pattern.
- **`nix = { version = "0.29", features = ["signal"] }`** [Source: Cargo.toml line 56 — currently locked with `signal` only]: for `Uid::current().as_raw()` in `podman_default_socket_path`. **MAY NEED**: bump to `features = ["signal", "user"]` if `unistd::Uid` is gated behind the `user` feature in nix 0.29 (verify during dev-story; if so, this is the only Cargo.toml change Story 1.9 introduces — record in File List).
- **NO new dependencies.** No `bollard-stubs`, no `serial_test`, no `mockito`, no `wiremock`, no `assert-json-diff`. The mock listener in tests is hand-rolled with `tokio::net::UnixListener` (already in `tokio` `full` features) — this is intentional because (a) it stays inside the locked dep tree per architecture.md Cargo.toml line 30-67, (b) wire-protocol mocking for one HTTP/1.1 endpoint is ~10 lines, (c) Story 1.10's heavier bollard testing may justify a deeper mock crate but that's its concern.

### File structure requirements

[Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" lines 922-929 + § "Architectural Boundaries" lines 994 + § "Implementation Sequence" line 540]

```
src/
├── lib.rs                              # UPDATED: declare `pub mod sandbox;`
├── sandbox.rs                          # NEW: module-root + SandboxError enum
└── sandbox/
    └── runtime.rs                      # NEW: precedence chain + bollard ping
src/cli/
└── scan.rs                             # UPDATED: wire preflight in scan::run
tests/
├── sandbox_preflight.rs                # NEW: integration tests with mock unix listeners
└── cli_exit_codes.rs                   # UPDATED: add exit-11 scenario test
Cargo.toml                              # OPTIONAL: bump `nix` features to ["signal", "user"]
                                        # (only if Uid::current() requires it)
```

After this story merges:
- `src/sandbox/` exists with one submodule (`runtime`). Future stories add siblings (`container.rs`, `network.rs`, `env_allowlist.rs`, `image.rs`, `violation.rs`).
- `src/sandbox.rs` declares `pub mod runtime;` and defines `SandboxError` (one variant).
- `src/cli/scan.rs::run` performs preflight; on success, behaves as the previous placeholder (prints "not yet implemented" diagnostic + exits 0); on failure, prints the rendered diagnostic + exits 11.
- The `bollard::` API surface is contained inside `src/sandbox/runtime.rs` + `tests/sandbox_preflight.rs`.

### Testing requirements

- **In-module unit tests** (`src/sandbox.rs::tests` and `src/sandbox/runtime.rs::tests`): exercise pure-function paths (`PrecedenceLayer::name`, `strip_unix_prefix`, `resolve_candidates`, `format_no_runtime_reachable`) plus the negative `detect()` path against a `MapEnv`-backed empty chain. ~7 unit tests total. Standard `#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` exemption.
- **Integration tests** (`tests/sandbox_preflight.rs`): exercise the public `detect` API with mock `UnixListener` instances simulating reachable Docker daemons. Cover AC3 (success via layer 1), AC3 (success via layer 2), AC4 (precedence stops at first success), AC5 (all-fail returns 4-attempt error), and the empty-string-env-var = unset semantics. ~5 integration tests; all `#[tokio::test(flavor = "current_thread")]`. Skip-on-real-runtime guard for the `NoRuntimeReachable` tests (when the test machine has `/var/run/docker.sock`).
- **CLI exit-code test** (`tests/cli_exit_codes.rs`): one new test asserting `lcrc scan` exits 11 with the expected stderr substrings when no runtime is reachable. Uses `assert_cmd` + `env_remove` to scrub the spawned process's env. Skip when host has a real runtime.
- **Error path coverage**: every `ProbeFailure` variant is exercised. `EnvVarUnset` via empty-string env (`tests/sandbox_preflight.rs::env_var_set_to_empty_string_treated_as_unset`). `SocketFileMissing` via `MapEnv` pointing at non-existent paths (`tests/sandbox_preflight.rs::no_runtime_reachable_returns_four_attempts_in_order_AC5`). `ConnectFailed` and `PingFailed` are surfaced opportunistically — `ConnectFailed` when `tokio::fs::try_exists` returns `Ok(false)` doesn't fire (that's the missing-socket path), so `ConnectFailed` is exercised when the path exists but isn't a real socket (covered transitively by the Podman default path on a machine without Podman). `PingFailed` is exercised when the mock listener returns malformed HTTP (deliberately not tested as a separate case — wire-protocol failures are bollard's concern; we trust it to surface failures correctly).
- **No real-runtime tests in this story.** `tests/sandbox_envelope.rs` (Story 1.10 + Story 7.4) is the runtime-real test surface; it gates v1 ship. Story 1.9's tests stay deterministic without Docker / Podman installed.
- **No `serial_test` crate.** Tests do not mutate process-global state. Each test creates its own `TempDir` + `MapEnv`; the cli-exit-code test uses `assert_cmd::Command::env_remove` to scrub the spawned child's env per-test.
- **Test naming**: descriptive `snake_case` matching the AC the test pins. Same naming convention Stories 1.7 / 1.8 used (`write_then_lookup_roundtrips_all_columns`, `lookup_existing_key_at_10k_cells_under_100ms_NFR_P5`). One test per AC + one test per error-variant-discriminator-fork.
- **No HTML snapshots, no insta usage.** This story produces no HTML / structured output files.
- **AC6 verification**: the AC tests for the `tracing::info!` event on success. The cli-exit-code test does NOT cover this (it's the failure-path test). A separate test would need to spawn `lcrc scan` with `RUST_LOG=info`, with a mock-listener-backed runtime visible at one of the precedence-chain paths — that is significant test machinery for a single substring assertion. The pragmatic approach: AC6 is verified manually in T8.6 + structurally by the in-module unit test that builds the same `tracing::info!` macro call (compile-time check that the field syntax compiles). Strictly: AC6 is a "soft" verification; if a hard verification is required, add a `tests/sandbox_preflight_logging.rs` test that uses the `tracing-subscriber::test` test layer to capture events programmatically. Defer if the soft path is acceptable; locked here as deferred unless explicit feedback raises it.

## Previous Story Intelligence

[Source: `_bmad-output/implementation-artifacts/1-8-cache-cell-write-read-api-with-atomic-semantics.md` — Status: done]

Story 1.8 added the cache cell write/read primitives. Relevant intelligence carried forward:

- **Module-root + submodule split** (cache.rs + cache/{key,migrations,schema,cell}.rs): the same pattern lands here as `sandbox.rs` + `sandbox/runtime.rs`. Module file declares submodules + parent error enum; submodule files own the concrete logic + their typed errors.
- **One typed error variant per story** (Story 1.7 added `Pragma`, `Open`, `MigrationFailed`, `FutureSchema`; Story 1.8 added `DuplicateCell`): Story 1.9 adds `SandboxError::Preflight` (one variant) + `PreflightError::NoRuntimeReachable` (one variant) + `ProbeFailure` (four variants — these are *internal-to-the-error* fields, not separate error variants). Same scope discipline.
- **No `From` impl to `crate::error::Error`** (Stories 1.5 / 1.6 / 1.7 / 1.8 all deferred boundary mapping to Story 1.12): Story 1.9 follows the same rule for `SandboxError → Error`. The `cli/scan.rs::run` consumer does inline `format!("{e}")` mapping per T5.1 — this is the *one* place this story explicitly bridges the boundary, justified by AC1/AC2 requiring `lcrc scan` to exit 11 (not just the primitive returning a typed error).
- **Sync primitives + `tokio::task::spawn_blocking` at consumer** (Story 1.7 + Story 1.8 locked the cache layer as sync): Story 1.9 inverts this — `detect` is `async fn` because `bollard` is async-native. The consumer (`cli/scan.rs::run`) spins up a current-thread tokio runtime in-place to call the async `detect`. Same architectural intent (async stays internal to the module that needs it; CLI surface stays sync) — different implementation because the underlying library imposes a different async/sync boundary.
- **In-module test exemption** (`#[cfg(test)] #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`): Story 1.9 follows the same boilerplate. Same exemption attribute set on the integration test files (`#![allow(...)]` at file top).
- **Single-source-of-truth grep at T8.5**: Story 1.7 + Story 1.8 added grep-based scope tests as a code-organization invariant. Story 1.9 extends the pattern: `bollard::` + `tokio::net::UnixListener` + `UnixStream` stay inside `src/sandbox/runtime.rs` + `tests/sandbox_preflight.rs` + `src/cli/scan.rs` (only for the runtime-builder, not bollard).
- **Display-substring pinning of error messages** (Story 1.5 § AC3 "unsupported hardware"; Story 1.7 § AC5 "upgrade lcrc"; Story 1.8 § T3.7 seven-PK-substring): Story 1.9 § T3.13 + T6.1 use the same approach for the setup-instructions block (`brew install podman`, `podman machine init`, `podman machine start`). Pins the AC contract against future Display-template edits.
- **Skip-on-host-state guard for runtime-dependent tests**: Story 1.5 used a similar pattern for `apple_silicon::tests` that detects non-arm64 hosts. Story 1.9 uses `if std::path::Path::new("/var/run/docker.sock").exists() { skip }` for tests that require the all-layers-fail path. Same `eprintln!("skipping: ...")` + `return` pattern.
- **No bulk APIs / no speculation surface** (Stories 1.7 + 1.8 explicitly forbade pre-adding bulk-write, range-scan, delete APIs): Story 1.9 explicitly forbids `--runtime-socket` flag, `lcrc-runtime-status` subcommand, `--unsafe-no-sandbox` escape hatch, and any `From<SandboxError>` / `From<PreflightError>` blanket impl.
- **Cargo.lock should not change** (Story 1.7 + Story 1.8 both achieved this). Story 1.9 may legitimately bump one feature on `nix` (T7.1); document in File List if it lands. Otherwise no Cargo.lock change.
- **Tracer-bullet vertical-slice principle** (`MEMORY.md → feedback_tracer_bullet_epics.md`): each story is a thin end-to-end demoable slice. Story 1.9 IS end-to-end — `lcrc scan` invokes preflight, surfaces success or failure to the user with exit codes + diagnostics. That's the demoable slice. The placeholder-after-success path is not "end-to-end scan" yet (Story 1.12), but Story 1.9 *is* a complete slice of the "preflight refusal" feature — it owns the user-facing contract from CLI invocation to exit code.

## Git Intelligence Summary

[Source: `git log --oneline -8`]

Recent merges (most recent first):

- `05af09d` Story 1.8: Cache cell write/read API with atomic semantics
- `babff77` fix: sudden exit in auto-bmad
- `1bd7814` Story 1.7: SQLite schema + migrations framework (#6)
- `ba42e15` Story 1.6: Cache key helpers in `src/cache/key.rs` (#5)
- `f98d307` Story 1.5: Machine fingerprint module (#4)
- `3cb7e77` bmad-auto: retry transient GitHub API failures + friction-report pause (#2)
- `ee6a89f` chore: strip planning-meta comments from story 1.4 modules (#3)
- `91b95be` Story 1.4: clap CLI root + `lcrc --version` + `lcrc --help` + tracing subscriber (#1)

**Patterns from recent work that apply to Story 1.9:**

- **Per-story branch + PR + squash-merge** (`MEMORY.md → feedback_lcrc_branch_pr_workflow.md`). Current branch `story/1-9-container-runtime-preflight-with-socket-precedence-chain` already exists per `gitStatus`. Workflow: implement → push → PR → wait green CI → squash-merge → delete branch.
- **No `Cargo.toml` churn outside scope** (Stories 1.5 / 1.6 / 1.7 / 1.8 each touched zero or one Cargo.toml line — locked deps are stable). Story 1.9 may bump one `nix` feature; otherwise stay still.
- **Stripped planning-meta comments** (`ee6a89f`): commit messages, PR descriptions, and code comments must NOT reference Story / Epic / FR identifiers (`MEMORY.md`-loaded `CLAUDE.md` rule). Comments justify *why* a non-obvious choice was made; planning-context belongs in PR descriptions and git blame.
- **`auto-bmad` workflow** (`babff77`): the `scripts/bmad-auto.sh` orchestrator handles the create-story → dev-story → code-review → squash-merge cycle. Story 1.9 will land via that workflow.

## Latest Tech Information

**Bollard 0.18 (Cargo.toml line 42)** — async Docker Engine API client. As of bollard 0.18.x:

- `bollard::Docker::connect_with_unix(addr: &str, timeout: u64, client_version: &ClientVersion) -> Result<Docker, Error>` — the connector this story uses. **Verify** the exact 0.18 signature during dev-story; in earlier versions the third arg was a `&str` instead of `&ClientVersion`. If the signature has shifted, adapt the call.
- `bollard::API_DEFAULT_VERSION` — the `ClientVersion` constant for "use the bollard-internal default". Path may have shifted to `bollard::ClientVersion::DEFAULT` or `bollard::DEFAULT_VERSION` depending on the 0.18 minor; use whichever constant resolves.
- `Docker::ping(&self) -> Result<String, Error>` — issues `GET /_ping`. Returns the daemon's response body (typically `"OK"`) on success; surfaces HTTP-status / parse failures via `Error`.
- `bollard::errors::Error` is the unified error type. Implements `Display` + `Error`; the source chain preserves the underlying cause. Story 1.9's `ProbeFailure::PingFailed { source: bollard::errors::Error }` wraps it directly.
- **No breaking changes from 0.17** (bollard's CHANGELOG): if the dev-story implementer is following 0.18.x docs, the API shape should be stable.

**Tokio 1.x** (Cargo.toml line 35, `features = ["full"]`):

- `tokio::net::UnixListener::bind(path: impl AsRef<Path>) -> std::io::Result<UnixListener>` — async-native Unix-socket listener. Used in tests.
- `tokio::net::UnixStream` — the connected stream type; `read` + `write_all` + `shutdown` via `AsyncReadExt` / `AsyncWriteExt`.
- `tokio::fs::try_exists(path: impl AsRef<Path>) -> Result<bool, std::io::Error>` — non-blocking exists check. Used in `probe_one` (T3.11).
- `tokio::runtime::Builder::new_current_thread().enable_all().build()` — current-thread runtime built in-place. Returns `Result<Runtime, std::io::Error>`. Used in `cli/scan.rs::run` (T5.2).
- `Runtime::block_on(future) -> T` — the canonical sync-to-async bridge. Used in `cli/scan.rs::run`.

**Nix 0.29** (Cargo.toml line 56):

- `nix::unistd::Uid::current() -> Uid` — wraps `getuid(2)`. Returns the real UID. Use `.as_raw() -> uid_t` (typically `u32`) for the integer value.
- **Feature gate**: `unistd` is in nix's `user` feature (verify); Cargo.toml currently enables `signal` only. If `Uid::current()` is unreachable under `signal`, add `"user"` to the feature list — this is the single permitted Cargo.toml change in Story 1.9.

**Rust 2024 edition + MSRV 1.95** [Cargo.toml line 5]:

- `let-else` stable since 1.65 — used in T3.11 to avoid `unwrap` on the `candidate.path` branch.
- `std::env::set_var` and `std::env::remove_var` are `unsafe fn` in Rust 2024 — Story 1.9 does NOT call them in production or tests (the `unsafe_code = "forbid"` lint would block them); the test in T5.3 checks env vars and skips rather than mutating them.
- `async fn` in trait methods is stable (Rust 1.75+) — but Story 1.9 does not put `async fn` in a trait. `EnvSource::get` is sync (env reads are sync); only `detect` and `probe_one` are `async fn` and they are free functions on the module.

**No external service / API / SDK lookups required** for this story. Bollard's Docker Engine API surface is documented inside the crate; the Docker `_ping` endpoint contract is documented at https://docs.docker.com/engine/api/v1.43/#tag/System/operation/SystemPing (URL is illustrative; do NOT include in code comments per the no-URL rule in CLAUDE.md).

## Project Context Reference

This story sits at:
- **Epic 1, Story 9** of 14 in Epic 1 (5 stories remain after this: 1.10–1.14).
- **Sprint 1** (Epic 1's tracer-bullet integration spine).
- **Implementation sequence step 6** per architecture.md line 540: "Sandbox/container layer (bollard wiring; per-task network; image pull on first run; pre-flight runtime detection per FR17a)." Story 1.9 owns the **pre-flight runtime detection** half; Stories 1.10 + 1.14 own the per-task network + image-pull halves.

**Cross-story dependencies:**
- **Depends on** (already done): Story 1.1 (project scaffold + locked deps including bollard 0.18 + nix 0.29), Story 1.3 (`ExitCode::PreflightFailed = 11` + `Error::Preflight(String)` mapping), Story 1.4 (clap CLI root + `lcrc scan` subcommand placeholder + `tracing-subscriber` install in `cli::dispatch`), `src/output.rs` (FR46 stderr discipline).
- **Unblocks**: Story 1.10 (`Sandbox::run_task` consumes a `RuntimeProbe` to construct a `bollard::Docker` client without re-probing), Story 1.12 (end-to-end one-cell scan; preflight is the first stage of the pipeline). Story 7.4 (acceptance check #9 sandbox negative test) consumes the same `runtime::detect` for its setup phase.

**Architectural touchpoints** the dev agent should keep in mind:
- The Sandbox enforcement design (NFR-S1–S6) is *structural*. Preflight is one of the structural layers — it refuses to run when the foundation isn't there. Other structural layers (workspace mount, custom Docker network, env allowlist) land in their owner stories.
- The `bollard` crate is the *only* Docker-Engine-API client lcrc uses. After Story 1.9, all `bollard::` references stay inside `src/sandbox/runtime.rs` (the daemon-level surface); Story 1.10 will add `src/sandbox/container.rs` (the container-level surface) and the rule extends — those two files become the only `bollard::` consumers.
- The precedence chain order is *user-facing contract*. Changing it (e.g. moving Podman default ahead of `/var/run/docker.sock`) would break users whose setup relies on the documented order. Lock-in is at the architecture level; this story implements the lock.

### References

- [Source: `_bmad-output/planning-artifacts/epics.md` § "Story 1.9: Container runtime preflight with socket precedence chain" (lines 573-603)] — the seven AC clauses paraphrased into ACs 1–6 with test simulation notes folded in
- [Source: `_bmad-output/planning-artifacts/epics.md` § "Epic 1: Integration spine" (lines 357-360)] — epic goal statement
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Sandbox & Container Runtime" (lines 297-334)] — runtime selection (Podman packaged default), precedence chain, sandbox-design intent
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Runtime socket precedence (5 layers)" (lines 458-463)] — the four code-relevant layers
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" (lines 922-929)] — `src/sandbox/{runtime,container,network,env_allowlist,image,violation}.rs` layout
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Architectural Boundaries" (lines 992-1009)] — `src/sandbox/container.rs` is the only module that calls `bollard::container::Container::create` (Story 1.9 extends this: `runtime.rs` is the only module that calls `bollard::Docker::connect_with_unix` + `Docker::ping`)
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Implementation Sequence" (line 540)] — sandbox/container step ordering; preflight comes first
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Sandbox Invariants — Structural, not Conventional" (lines 795-803)] — the structural-not-conventional invariant set; preflight enforces "the sandbox is structural or scan refuses to run"
- [Source: `_bmad-output/planning-artifacts/prd.md` § "FR17a" (line 509)] — the original FR text
- [Source: `_bmad-output/planning-artifacts/prd.md` § "NFR-S3" (line 593)] — "Container runtime is a hard dependency, no fallback"
- [Source: `_bmad-output/planning-artifacts/prd.md` § "NFR-I4" (line 619)] — Container runtime integration
- [Source: `_bmad-output/planning-artifacts/prd.md` § "NFR-S1, NFR-S2, NFR-S4, NFR-S5, NFR-S6" (lines 591-596)] — surrounding sandbox-isolation requirements
- [Source: `_bmad-output/implementation-artifacts/1-1-project-scaffold-with-locked-workspace-lints.md`] — workspace lints + dep lockset; `bollard = "0.18"` was added here
- [Source: `_bmad-output/implementation-artifacts/1-3-output-module-full-exitcode-enum-error-layer.md`] — `ExitCode::PreflightFailed = 11`, `Error::Preflight(String)` definition
- [Source: `_bmad-output/implementation-artifacts/1-4-clap-cli-root-lcrc-version-lcrc-help-tracing-subscriber.md`] — `cli::dispatch` shape, `tracing` subscriber install pattern, `cli/scan.rs::run` placeholder body
- [Source: `_bmad-output/implementation-artifacts/1-7-sqlite-schema-migrations-framework.md`] — module-root + submodule pattern (`cache.rs` + `cache/{schema,migrations}.rs`); typed error per-story pattern
- [Source: `_bmad-output/implementation-artifacts/1-8-cache-cell-write-read-api-with-atomic-semantics.md`] — single-source-of-truth grep contract; in-module test exemption pattern; "boundary mapping deferred to consumer story" rule
- [Source: `_bmad-output/implementation-artifacts/deferred-work.md`] — no Story-1.9-specific deferred items; existing items (1.5/1.6/1.7/1.8 + Story 1.2 `actions/checkout@v5`) are out of scope here
- [Source: `src/lib.rs:5-12`] — current `pub mod` block; this story inserts `pub mod sandbox;`
- [Source: `src/error.rs:18-43`] — `Error::Preflight(String)` variant + `exit_code()` mapping
- [Source: `src/exit_code.rs:14-35`] — `ExitCode::PreflightFailed = 11` discriminant
- [Source: `src/cli.rs:60-93`] — `parse_and_dispatch` + `handle_clap_error` flow; `dispatch` calls `scan::run`
- [Source: `src/cli/scan.rs`] — current placeholder body; T5 replaces it
- [Source: `src/output.rs:35-37`] — `output::diag(s)` is the only allowed stderr writer (besides the tracing subscriber)
- [Source: `src/util/tracing.rs:31-45`] — subscriber install (level from `RUST_LOG`, default `INFO`; writes to stderr with module-pathed targets)
- [Source: `tests/cli_exit_codes.rs:13-32`] — existing test shape for `assert_cmd::Command::cargo_bin("lcrc")`; T6 follows the same pattern
- [Source: `Cargo.toml` line 42] — `bollard = "0.18"` — locked
- [Source: `Cargo.toml` line 35] — `tokio = { version = "1", features = ["full"] }` — locked, covers UnixListener / UnixStream / runtime::Builder / time::sleep / fs::try_exists / task::spawn
- [Source: `Cargo.toml` line 50] — `tempfile = "3"` — locked, used here for tests
- [Source: `Cargo.toml` line 56] — `nix = { version = "0.29", features = ["signal"] }` — may need `"user"` feature for `unistd::Uid::current()`
- [Source: `Cargo.toml` line 60] — `thiserror = "2"` — locked
- [Source: `Cargo.toml` line 62] — `tracing = "0.1"` — locked
- [Source: `<claude-auto-memory>/feedback_tracer_bullet_epics.md`] — vertical-slice principle (Story 1.9 is a complete slice of the "preflight refusal" feature)
- [Source: `<claude-auto-memory>/feedback_lcrc_branch_pr_workflow.md`] — branch-then-PR-then-squash workflow
- [Source: `<claude-auto-memory>/CLAUDE.md` → "HIGH-PRECEDENCE RULES" → "Comments explain WHY, never planning meta"] — code comments justify *why* a non-obvious choice was made; do not reference Story / Epic / FR identifiers in comments
- [Source: `<claude-auto-memory>/CLAUDE.md` → "HIGH-PRECEDENCE RULES" → "No absolute or machine-specific paths"] — all paths in code/docs are relative to repo root (Story 1.9 deals with absolute Unix-socket paths because they ARE the user-visible interface — `/var/run/docker.sock` is the actual filesystem path; documented as the AR-12 / line 462 architecture decision, not a CLAUDE.md violation)

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List
