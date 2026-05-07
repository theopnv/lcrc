//! Ephemeral container lifecycle — the only module in the codebase that calls
//! `bollard::container::*` APIs. All other code reaches containers through
//! `Sandbox::run_task`.

use std::collections::HashMap;

use bollard::container::{
    CreateContainerOptions, RemoveContainerOptions, StartContainerOptions, WaitContainerOptions,
};
use bollard::models::HostConfig;
use futures_util::TryStreamExt as _;

use crate::sandbox::{SandboxError, TaskOutcome};

/// Run a single task in an ephemeral container and return its outcome.
///
/// Creates the container with a workspace bind-mount and the per-scan
/// network, starts it, waits for exit, force-removes it, and returns
/// [`TaskOutcome`]. The container is removed in all exit paths including
/// errors.
///
/// # Errors
///
/// [`SandboxError::ContainerCreate`] on container creation, start, or wait
/// failure.
pub async fn run_container(
    docker: &bollard::Docker,
    image_ref: &str,
    workspace_path: &std::path::Path,
    network_name: &str,
    scan_id: &str,
) -> Result<TaskOutcome, SandboxError> {
    let container_name = unique_container_name(scan_id);

    let bind = format!("{}:/workspace:rw", workspace_path.display());

    let mut labels = HashMap::new();
    labels.insert("lcrc-scan-id".to_string(), scan_id.to_string());

    let config = bollard::container::Config {
        image: Some(image_ref.to_string()),
        labels: Some(labels),
        host_config: Some(HostConfig {
            binds: Some(vec![bind]),
            network_mode: Some(network_name.to_string()),
            auto_remove: Some(false),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            ..Default::default()
        }),
        ..Default::default()
    };

    docker
        .create_container(
            Some(CreateContainerOptions {
                name: container_name.as_str(),
                platform: None,
            }),
            config,
        )
        .await
        .map_err(|e| SandboxError::ContainerCreate(format!("create: {e}")))?;

    let t0 = std::time::Instant::now();

    if let Err(e) = docker
        .start_container(&container_name, None::<StartContainerOptions<String>>)
        .await
    {
        force_remove_container(docker, &container_name).await;
        return Err(SandboxError::ContainerCreate(format!("start: {e}")));
    }

    let wait_result = docker
        .wait_container(&container_name, None::<WaitContainerOptions<String>>)
        .try_collect::<Vec<_>>()
        .await;

    let duration_seconds = t0.elapsed().as_secs_f64();

    force_remove_container(docker, &container_name).await;

    let responses = wait_result.map_err(|e| SandboxError::ContainerCreate(format!("wait: {e}")))?;

    let pass = responses.first().is_some_and(|r| r.status_code == 0);

    Ok(TaskOutcome {
        pass,
        duration_seconds,
    })
}

/// Force-remove a container, logging but not propagating errors.
pub(crate) async fn force_remove_container(docker: &bollard::Docker, container_name: &str) {
    if let Err(e) = docker
        .remove_container(
            container_name,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
    {
        tracing::warn!(
            target: "lcrc::sandbox::container",
            container = container_name,
            error = %e,
            "failed to force-remove container",
        );
    }
}

/// Generate a unique container name from `scan_id` and a nanosecond timestamp.
fn unique_container_name(scan_id: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("lcrc-task-{scan_id}-{nanos}")
}
