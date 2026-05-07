//! One-cell scan orchestrator.
//!
//! `run()` executes the full pipeline for a single hardcoded canary
//! measurement: preflight → fingerprint → `model_sha` → cache miss → server
//! start → workspace → sandbox → cell write → teardown.
//!
//! Model path comes from `LCRC_DEV_MODEL_PATH`.

use etcetera::BaseStrategy as _;

/// Execute the one-cell scan pipeline.
///
/// # Errors
///
/// - [`crate::error::Error::Preflight`] for model path missing, machine
///   fingerprint failure, or sandbox setup failure.
/// - [`crate::error::Error::AbortedBySignal`] if SIGINT fires during measurement.
/// - [`crate::error::Error::Other`] for cache I/O errors.
#[allow(clippy::too_many_lines)]
pub async fn run(probe: crate::sandbox::runtime::RuntimeProbe) -> Result<(), crate::error::Error> {
    // --- Step 1: Model path ---
    let model_path_str = std::env::var("LCRC_DEV_MODEL_PATH").map_err(|_| {
        crate::error::Error::Preflight(
            "LCRC_DEV_MODEL_PATH not set; set it to a GGUF file path".into(),
        )
    })?;
    let model_path = std::path::PathBuf::from(&model_path_str);
    if !model_path.is_file() {
        return Err(crate::error::Error::Preflight(format!(
            "LCRC_DEV_MODEL_PATH='{}' is not a readable file",
            model_path.display()
        )));
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
        backend_build,
        params_hash,
        task_id: crate::scan::canary::CANARY_TASK_ID.to_owned(),
        harness_version: crate::version::HARNESS_VERSION.to_owned(),
        task_subset_version,
    };

    // --- Step 6: Open / initialise cache dir ---
    let cache_dir = etcetera::base_strategy::choose_base_strategy()
        .map_err(|e| crate::error::Error::Preflight(format!("XDG base dirs: {e}")))?
        .data_dir()
        .join("lcrc");
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| crate::error::Error::Preflight(format!("create cache dir: {e}")))?;
    let db_path = cache_dir.join("lcrc.db");

    // Verify the DB can be opened (runs migrations on first use).
    {
        let p = db_path.clone();
        tokio::task::spawn_blocking(move || crate::cache::cell::Cache::open(&p))
            .await
            .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("spawn_blocking join: {e}")))?
            .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("cache open: {e}")))?;
    }

    // --- Step 7: Cache lookup ---
    let existing = {
        let p = db_path.clone();
        let k = cell_key.clone();
        tokio::task::spawn_blocking(move || crate::cache::cell::Cache::open(&p)?.lookup_cell(&k))
            .await
            .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("spawn_blocking join: {e}")))?
            .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("cache lookup: {e}")))?
    };

    if existing.is_some() {
        crate::output::diag("lcrc scan: cache hit — no measurement needed.");
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
    // When wait_for_sigint() wins the race, measure_and_persist is dropped.
    // Async code in the dropped future halts at the current .await point,
    // so sandbox.cleanup() inside the dropped future does NOT run. The
    // ServerHandle Drop impl is synchronous and DOES run, terminating llama-server.
    tokio::select! {
        result = measure_and_persist(
            probe,
            model_path,
            server_params,
            cell_key,
            db_path,
        ) => result,

        () = crate::scan::signal::wait_for_sigint() => {
            tracing::warn!(
                target: "lcrc::scan::orchestrator",
                "SIGINT received; aborting scan without writing cell",
            );
            Err(crate::error::Error::AbortedBySignal)
        }
    }
}

async fn measure_and_persist(
    probe: crate::sandbox::runtime::RuntimeProbe,
    model_path: std::path::PathBuf,
    server_params: crate::scan::server_lifecycle::Params,
    cell_key: crate::cache::cell::CellKey,
    db_path: std::path::PathBuf,
) -> Result<(), crate::error::Error> {
    // Start llama-server; drop order matters: sandbox must be cleaned up
    // before handle drops so the network cleanup runs before server socket closes.
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

    // Create sandbox — uses handle.port() as the pinned llama port.
    let sandbox = crate::sandbox::Sandbox::new(&probe, handle.port())
        .await
        .map_err(|e| crate::error::Error::Preflight(format!("sandbox setup: {e}")))?;

    // Prepare workspace with canary spec.
    let workspace_dir = tempfile::TempDir::new()
        .map_err(|e| crate::error::Error::Preflight(format!("temp workspace: {e}")))?;
    crate::scan::canary::setup_workspace(workspace_dir.path())
        .await
        .map_err(|e| crate::error::Error::Preflight(format!("canary workspace: {e}")))?;

    // Run task in container.
    crate::output::diag("lcrc scan: running canary task (this may take up to 120 s)…");
    let run_result = sandbox
        .run_task(
            crate::constants::CONTAINER_IMAGE_DIGEST,
            workspace_dir.path(),
        )
        .await;

    // Sandbox cleanup is best-effort; run regardless of run_task outcome so
    // the network and containers are not left behind on task failure.
    sandbox.cleanup().await;

    let outcome = run_result
        .map_err(|e| crate::error::Error::Preflight(format!("run_task: {e}")))?;

    tracing::info!(
        target: "lcrc::scan::orchestrator",
        pass = outcome.pass,
        duration_seconds = outcome.duration_seconds,
        "task completed",
    );

    // Construct cell for persistence.
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

    // Write cell atomically.
    {
        let p = db_path;
        tokio::task::spawn_blocking(move || -> Result<(), crate::cache::CacheError> {
            let mut cache = crate::cache::cell::Cache::open(&p)?;
            cache.write_cell(&cell)
        })
        .await
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("spawn_blocking join: {e}")))?
        .map_err(|e| match e {
            crate::cache::CacheError::DuplicateCell { ref key } => crate::error::Error::Other(
                anyhow::anyhow!("BUG: duplicate cell for key {key:?}; lookup-before-write invariant violated"),
            ),
            other => crate::error::Error::Other(anyhow::anyhow!("cache write: {other}")),
        })?;
    }

    crate::output::diag(&format!(
        "lcrc scan: done — pass={}, duration={:.1}s",
        outcome.pass, outcome.duration_seconds,
    ));
    tracing::info!(
        target: "lcrc::scan::orchestrator",
        pass = outcome.pass,
        "cell written; scan complete",
    );

    // `handle` (ServerHandle) drops here: SIGTERM + SIGKILL via Drop impl.
    // `workspace_dir` (TempDir) drops here: temp directory cleaned up.
    Ok(())
}

/// Detect the llama-server version string for `backend_build`.
///
/// Runs `llama-server --version`, parses the output, and returns a
/// canonical `BackendInfo`. If the binary is absent or the output format
/// is unrecognised, returns a sentinel so the cache key degrades
/// gracefully rather than blocking the scan.
async fn detect_backend_build() -> crate::cache::key::BackendInfo {
    let out = tokio::process::Command::new("llama-server")
        .arg("--version")
        .output()
        .await;

    let Ok(output) = out else {
        tracing::warn!(
            target: "lcrc::scan::orchestrator",
            "llama-server --version failed; cache key will use sentinel backend_build",
        );
        return crate::cache::key::BackendInfo {
            name: "llama.cpp".into(),
            semver: "unknown".into(),
            commit_short: "unknown".into(),
        };
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stdout.trim().is_empty() {
        &stderr
    } else {
        &stdout
    };

    parse_backend_build(text)
}

fn parse_backend_build(text: &str) -> crate::cache::key::BackendInfo {
    let first_line = text.lines().next().unwrap_or("");

    // Look for "b<digits>" token (e.g. "b3791") in the first line.
    let semver = first_line
        .split_whitespace()
        .find(|t| t.starts_with('b') && t[1..].chars().all(|c| c.is_ascii_digit()))
        .map_or_else(|| "unknown".to_owned(), str::to_owned);

    // Look for commit hash inside parentheses: "b3791 (a1b2c3d4)".
    let commit_short = first_line
        .find('(')
        .and_then(|i| {
            first_line[i + 1..]
                .find(')')
                .map(|j| first_line[i + 1..i + 1 + j].to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned());

    crate::cache::key::BackendInfo {
        name: "llama.cpp".into(),
        semver,
        commit_short,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::parse_backend_build;

    #[test]
    fn parse_backend_build_extracts_semver_and_commit() {
        let text = "version: b3791 (a1b2c3d4)\nbuilt with clang 15.0.0";
        let info = parse_backend_build(text);
        assert_eq!(info.name, "llama.cpp");
        assert_eq!(info.semver, "b3791");
        assert_eq!(info.commit_short, "a1b2c3d4");
    }

    #[test]
    fn parse_backend_build_falls_back_on_unrecognised_format() {
        let text = "some random output";
        let info = parse_backend_build(text);
        assert_eq!(info.name, "llama.cpp");
        assert_eq!(info.semver, "unknown");
        assert_eq!(info.commit_short, "unknown");
    }

    #[test]
    fn parse_backend_build_empty_input_returns_sentinels() {
        let info = parse_backend_build("");
        assert_eq!(info.semver, "unknown");
        assert_eq!(info.commit_short, "unknown");
    }
}
