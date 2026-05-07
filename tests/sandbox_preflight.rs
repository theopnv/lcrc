//! Integration tests for the container-runtime preflight detection.
//!
//! All tests exercise the public `detect` API using mock `UnixListener`
//! instances that simulate reachable Docker daemons.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lcrc::sandbox::runtime::{EnvSource, PrecedenceLayer, PreflightError, ProbeFailure, detect};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::task::JoinHandle;

/// Test-only [`EnvSource`] backed by an in-memory map.
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
        self.0
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .filter(|v| !v.is_empty())
    }
}

const PING_RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\
                               Content-Type: text/plain\r\n\
                               Content-Length: 2\r\n\
                               Api-Version: 1.43\r\n\
                               \r\nOK";

/// Minimal HTTP/1.1 handler that mimics `/_ping`. Returns immediately
/// after writing the response; does not implement keep-alive.
async fn handle_one(mut stream: tokio::net::UnixStream) {
    let mut buf = vec![0_u8; 4096];
    let _ = stream.read(&mut buf).await;
    let _ = stream.write_all(PING_RESPONSE).await;
    let _ = stream.shutdown().await;
}

fn spawn_mock_docker(path: PathBuf, accept_count: Arc<AtomicUsize>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let listener = UnixListener::bind(&path).expect("UnixListener bind must succeed in tests");
        while let Ok((stream, _addr)) = listener.accept().await {
            accept_count.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(handle_one(stream));
        }
    })
}

#[tokio::test(flavor = "current_thread")]
async fn successful_probe_via_lcrc_runtime_docker_host() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("lcrc.sock");
    let counter = Arc::new(AtomicUsize::new(0));
    let _h = spawn_mock_docker(sock.clone(), counter.clone());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let env = MapEnv::with(&[("LCRC_RUNTIME_DOCKER_HOST", sock.to_str().unwrap())]);
    let probe = detect(&env).await.unwrap();
    assert_eq!(probe.source, PrecedenceLayer::LcrcRuntimeDockerHost);
    assert_eq!(probe.socket_path, sock);
    assert!(counter.load(Ordering::SeqCst) >= 1);
}

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

#[tokio::test(flavor = "current_thread")]
#[allow(non_snake_case)]
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
    assert_eq!(probe.source, PrecedenceLayer::LcrcRuntimeDockerHost);
    assert_eq!(probe.socket_path, sock1);
    assert!(c1.load(Ordering::SeqCst) >= 1, "LCRC layer was not probed");
    assert_eq!(
        c2.load(Ordering::SeqCst),
        0,
        "DOCKER_HOST layer was probed despite earlier success"
    );
}

#[tokio::test(flavor = "current_thread")]
#[allow(non_snake_case)]
async fn no_runtime_reachable_returns_four_attempts_in_order_AC5() {
    let dir = TempDir::new().unwrap();
    let env = MapEnv::with(&[
        (
            "LCRC_RUNTIME_DOCKER_HOST",
            dir.path().join("nope1.sock").to_str().unwrap(),
        ),
        (
            "DOCKER_HOST",
            dir.path().join("nope2.sock").to_str().unwrap(),
        ),
    ]);
    if std::path::Path::new("/var/run/docker.sock").exists() {
        eprintln!("skipping: /var/run/docker.sock exists on this machine");
        return;
    }
    let PreflightError::NoRuntimeReachable { attempts } = detect(&env).await.unwrap_err();
    assert_eq!(
        attempts.len(),
        4,
        "expected 4 attempts, got {}",
        attempts.len()
    );
    assert_eq!(attempts[0].source, PrecedenceLayer::LcrcRuntimeDockerHost);
    assert!(matches!(
        attempts[0].failure,
        ProbeFailure::SocketFileMissing
    ));
    assert_eq!(attempts[1].source, PrecedenceLayer::DockerHost);
    assert!(matches!(
        attempts[1].failure,
        ProbeFailure::SocketFileMissing
    ));
    assert_eq!(attempts[2].source, PrecedenceLayer::DefaultDockerSock);
    assert!(matches!(
        attempts[2].failure,
        ProbeFailure::SocketFileMissing
    ));
    assert_eq!(attempts[3].source, PrecedenceLayer::PodmanDefaultSock);
    assert!(matches!(
        attempts[3].failure,
        ProbeFailure::SocketFileMissing | ProbeFailure::ConnectFailed { .. }
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn env_var_set_to_empty_string_treated_as_unset() {
    let env = MapEnv::with(&[("LCRC_RUNTIME_DOCKER_HOST", ""), ("DOCKER_HOST", "")]);
    if std::path::Path::new("/var/run/docker.sock").exists() {
        eprintln!("skipping: /var/run/docker.sock exists on this machine");
        return;
    }
    let PreflightError::NoRuntimeReachable { attempts } = detect(&env).await.unwrap_err();
    assert!(matches!(attempts[0].failure, ProbeFailure::EnvVarUnset));
    assert!(matches!(attempts[1].failure, ProbeFailure::EnvVarUnset));
}
