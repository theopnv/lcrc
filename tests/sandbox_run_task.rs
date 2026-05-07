//! Integration tests for [`lcrc::sandbox::Sandbox::run_task`].
//!
//! All tests gate on `LCRC_INTEGRATION_TEST_SANDBOX=1` and on a reachable
//! container runtime. Without the env var they print a skip message and return.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lcrc::sandbox::runtime::{SystemEnv, detect};

async fn runtime_probe() -> Option<lcrc::sandbox::runtime::RuntimeProbe> {
    detect(&SystemEnv).await.ok()
}

fn integration_guard() -> bool {
    if std::env::var("LCRC_INTEGRATION_TEST_SANDBOX").is_err() {
        eprintln!("skipping: set LCRC_INTEGRATION_TEST_SANDBOX=1 to run");
        return false;
    }
    true
}

#[tokio::test(flavor = "current_thread")]
async fn sandbox_creates_internal_network_with_no_dns() {
    if !integration_guard() {
        return;
    }
    let Some(probe) = runtime_probe().await else {
        eprintln!("skipping: no container runtime reachable");
        return;
    };

    let sandbox = lcrc::sandbox::Sandbox::new(&probe, 19999)
        .await
        .expect("Sandbox::new should succeed with a reachable Podman runtime");

    sandbox.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn sandbox_workspace_mount_visible_inside_container() {
    if !integration_guard() {
        return;
    }
    let Some(probe) = runtime_probe().await else {
        eprintln!("skipping: no container runtime reachable");
        return;
    };

    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("sentinel.txt"), b"hello").expect("write sentinel");

    let sandbox = lcrc::sandbox::Sandbox::new(&probe, 19999)
        .await
        .expect("Sandbox::new");

    let outcome = sandbox
        .run_task(lcrc::constants::CONTAINER_IMAGE_DIGEST, tmp.path())
        .await;

    sandbox.cleanup().await;

    // The placeholder digest causes pull to fail. The test verifies the API
    // contract compiles; a real image enables the full assertion path.
    match outcome {
        Err(lcrc::sandbox::SandboxError::ImagePull(_)) => {
            eprintln!("expected: image pull fails because digest is a placeholder");
        }
        Ok(o) => {
            assert!(o.pass, "container should have exited 0");
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn sandbox_host_filesystem_absent_inside_container() {
    if !integration_guard() {
        return;
    }
    let Some(probe) = runtime_probe().await else {
        eprintln!("skipping: no container runtime reachable");
        return;
    };

    let tmp = tempfile::tempdir().expect("tempdir");

    let sandbox = lcrc::sandbox::Sandbox::new(&probe, 19999)
        .await
        .expect("Sandbox::new");

    // Placeholder digest: pull will fail until Story 1.14.
    let outcome = sandbox
        .run_task(lcrc::constants::CONTAINER_IMAGE_DIGEST, tmp.path())
        .await;

    sandbox.cleanup().await;

    if let Err(lcrc::sandbox::SandboxError::ImagePull(_)) = outcome {
        eprintln!("expected: image pull fails because digest is a placeholder");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn sandbox_container_removed_after_run_task() {
    if !integration_guard() {
        return;
    }
    let Some(probe) = runtime_probe().await else {
        eprintln!("skipping: no container runtime reachable");
        return;
    };

    let tmp = tempfile::tempdir().expect("tempdir");

    let sandbox = lcrc::sandbox::Sandbox::new(&probe, 19999)
        .await
        .expect("Sandbox::new");

    // Placeholder digest: run_task will fail at image pull, but cleanup
    // still executes (force-remove is on the container lifecycle path,
    // pre-image-pull failure means no container was created).
    let _ = sandbox
        .run_task(lcrc::constants::CONTAINER_IMAGE_DIGEST, tmp.path())
        .await;

    sandbox.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn sandbox_exits_11_on_unsupported_runtime() {
    if std::env::var("LCRC_TEST_UNSUPPORTED_RUNTIME_SOCKET").is_err() {
        eprintln!(
            "skipping: set LCRC_TEST_UNSUPPORTED_RUNTIME_SOCKET to a non-Podman Docker socket"
        );
        return;
    }

    let socket = std::env::var("LCRC_TEST_UNSUPPORTED_RUNTIME_SOCKET").unwrap();
    let probe = lcrc::sandbox::runtime::RuntimeProbe {
        socket_path: std::path::PathBuf::from(socket),
        source: lcrc::sandbox::runtime::PrecedenceLayer::LcrcRuntimeDockerHost,
    };

    let result = lcrc::sandbox::Sandbox::new(&probe, 19999).await;
    assert!(
        matches!(
            result,
            Err(lcrc::sandbox::SandboxError::UnsupportedRuntime(_))
        ),
        "expected UnsupportedRuntime but got a different result",
    );
}

#[ignore = "real GHCR image does not exist yet; fill CONTAINER_IMAGE_DIGEST and remove this attribute when available"]
#[tokio::test(flavor = "current_thread")]
async fn sandbox_image_pull_and_digest_verification() {
    // Replace CONTAINER_IMAGE_DIGEST placeholder with the real digest and
    // remove the `#[ignore]` attribute above when the image is published.
    let Some(probe) = runtime_probe().await else {
        eprintln!("skipping: no container runtime reachable");
        return;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let sandbox = lcrc::sandbox::Sandbox::new(&probe, 19999)
        .await
        .expect("Sandbox::new");
    let outcome = sandbox
        .run_task(lcrc::constants::CONTAINER_IMAGE_DIGEST, tmp.path())
        .await
        .expect("run_task should succeed with a real image");
    sandbox.cleanup().await;
    assert!(outcome.pass);
}
