//! Integration tests for [`lcrc::scan::server_lifecycle`].
//!
//! These tests require a real `llama-server` binary and a real GGUF model.
//! Gate: `LCRC_INTEGRATION_TEST_SERVER=1` and `LCRC_TEST_MODEL_PATH=<path>`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lcrc::scan::server_lifecycle::{LlamaServer, Params, ServerError};

fn check_server_available() -> bool {
    if std::env::var("LCRC_INTEGRATION_TEST_SERVER").is_err() {
        eprintln!(
            "skipping: set LCRC_INTEGRATION_TEST_SERVER=1 and LCRC_TEST_MODEL_PATH=<path> to run"
        );
        return false;
    }
    let binary_ok = std::process::Command::new("llama-server")
        .arg("--version")
        .output()
        .is_ok();
    if !binary_ok {
        eprintln!("skipping: llama-server not found in PATH");
        return false;
    }
    true
}

fn check_prerequisites() -> Option<std::path::PathBuf> {
    if !check_server_available() {
        return None;
    }
    let Some(model_path) = std::env::var("LCRC_TEST_MODEL_PATH")
        .ok()
        .map(std::path::PathBuf::from)
    else {
        eprintln!("skipping: LCRC_TEST_MODEL_PATH not set");
        return None;
    };
    Some(model_path)
}

#[tokio::test(flavor = "current_thread")]
async fn server_starts_and_port_is_nonzero() {
    let Some(model_path) = check_prerequisites() else {
        return;
    };
    let handle = LlamaServer::new()
        .start(&model_path, &Params { ctx: 512 })
        .await
        .expect("server should start with a valid model");
    assert!(handle.port() > 0, "port must be non-zero");
    drop(handle);
}

#[tokio::test(flavor = "current_thread")]
async fn server_drop_terminates_process() {
    let Some(model_path) = check_prerequisites() else {
        return;
    };
    let handle = LlamaServer::new()
        .start(&model_path, &Params { ctx: 512 })
        .await
        .expect("server should start with a valid model");
    let port = handle.port();
    drop(handle);
    // Give the OS a moment to release the port after SIGKILL
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let result = reqwest::get(format!("http://127.0.0.1:{port}/health")).await;
    assert!(
        result.is_err(),
        "connection should be refused after server is dropped"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn server_corrupt_model_returns_err() {
    if !check_server_available() {
        return;
    }

    let tmp = tempfile::NamedTempFile::new().expect("tempfile creation");
    std::fs::write(tmp.path(), b"not a gguf").expect("write junk bytes");

    let result = LlamaServer::new()
        .start(tmp.path(), &Params { ctx: 512 })
        .await;

    assert!(
        matches!(result, Err(ServerError::StartupFailure(_))),
        "expected StartupFailure, got: {result:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "reliably simulating a hanging llama-server in CI is complex"]
async fn server_startup_timeout_kills_process() {
    // Use LlamaServer::with_timeout(Duration::from_millis(100)) with a valid
    // model path; the model takes longer than 100ms to load on most hardware.
    let Some(model_path) = check_prerequisites() else {
        return;
    };
    let result = LlamaServer::with_timeout(std::time::Duration::from_millis(100))
        .start(&model_path, &Params { ctx: 512 })
        .await;
    assert!(
        matches!(result, Err(ServerError::StartupFailure(_))),
        "expected StartupFailure from timeout, got: {result:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_starts_different_ports() {
    let Some(model_path) = check_prerequisites() else {
        return;
    };
    let model_path2 = model_path.clone();
    let launcher1 = LlamaServer::with_timeout(std::time::Duration::from_mins(2));
    let launcher2 = LlamaServer::with_timeout(std::time::Duration::from_mins(2));
    let (r1, r2) = tokio::join!(
        launcher1.start(&model_path, &Params { ctx: 512 }),
        launcher2.start(&model_path2, &Params { ctx: 512 }),
    );
    let h1 = r1.expect("first server should start");
    let h2 = r2.expect("second server should start");
    assert_ne!(
        h1.port(),
        h2.port(),
        "concurrent servers must bind to different ports"
    );
    drop(h1);
    drop(h2);
}
