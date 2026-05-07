//! Canary task: stable identifier and workspace setup.
//!
//! Uses a vendored spec from `tasks/swe-bench-pro/canary/spec.json`
//! that mini-swe-agent executes inside the per-task container.

/// Stable task identifier for the canary cell PK.
pub const CANARY_TASK_ID: &str = "swe-bench-pro:canary";

/// Path to the vendored canary spec relative to the crate root.
const CANARY_SPEC_PATH: &str = "tasks/swe-bench-pro/canary/spec.json";

/// Path to the vendored task-subset version file relative to crate root.
const TASK_SUBSET_VERSION_PATH: &str = "tasks/swe-bench-pro/version";

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{CANARY_TASK_ID, setup_workspace, task_subset_version};
    use tempfile::TempDir;

    #[test]
    fn canary_task_id_matches_spec_format() {
        assert!(
            CANARY_TASK_ID.contains(':'),
            "CANARY_TASK_ID must use task_source:task_name format"
        );
        assert_eq!(CANARY_TASK_ID, "swe-bench-pro:canary");
    }

    #[tokio::test]
    async fn task_subset_version_returns_string() {
        // Either reads the file or falls back to "unknown" — both are valid.
        let v = task_subset_version().await.unwrap();
        assert!(
            !v.is_empty(),
            "task_subset_version must not return empty string"
        );
    }

    #[tokio::test]
    async fn setup_workspace_copies_spec_json() {
        let dir = TempDir::new().unwrap();
        let result = setup_workspace(dir.path()).await;
        // The test may fail if run outside the crate root (spec file absent).
        // That is expected in CI — the integration test gates on the env var.
        if result.is_err() {
            eprintln!("skipping: canary spec not found (run from crate root)");
            return;
        }
        let spec_path = dir.path().join("spec.json");
        assert!(
            spec_path.exists(),
            "spec.json must exist after setup_workspace"
        );
        let content = tokio::fs::read_to_string(&spec_path).await.unwrap();
        assert!(
            content.contains("swe-bench-pro:canary"),
            "spec.json must contain the canary task_id"
        );
    }
}
