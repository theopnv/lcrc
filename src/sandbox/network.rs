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
//! (Docker Desktop, Colima, `OrbStack`) cause preflight to exit 11 — there is
//! no degraded "DNS denial only" mode (NFR-S3).

use std::collections::HashMap;

use bollard::models::Ipam;
use bollard::network::CreateNetworkOptions;

use crate::sandbox::SandboxError;

/// Detect whether the running Docker-compatible socket is backed by Podman.
///
/// Returns the name of the running Podman machine on success, `None` for all
/// other runtimes.
pub async fn detect_podman_machine(docker: &bollard::Docker) -> Option<String> {
    let version = docker.version().await.ok()?;
    let is_podman = version
        .components
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .any(|c| c.name == "Podman Engine");

    if !is_podman {
        return None;
    }

    let output = tokio::process::Command::new("podman")
        .args(["machine", "list", "--format", "{{.Name}},{{.Running}}"])
        .output()
        .await
        .ok()?;

    parse_running_machine(&String::from_utf8_lossy(&output.stdout))
}

/// Parse the first running machine name from `podman machine list` output.
fn parse_running_machine(output: &str) -> Option<String> {
    for line in output.lines() {
        let mut parts = line.splitn(2, ',');
        let name = parts.next()?.trim();
        let running = parts.next()?.trim();
        if running.eq_ignore_ascii_case("true") || running.eq_ignore_ascii_case("running") {
            return Some(name.to_string());
        }
    }
    None
}

/// Create the per-scan internal Docker network and install port-pin rules.
///
/// The network name is `lcrc-{scan_id}`. The network is created as
/// `internal: true` (no default gateway, no DNS). iptables/nftables rules
/// are then installed inside the Podman VM to allow outbound TCP to the
/// llama-server port and drop all other container outbound traffic.
///
/// # Errors
///
/// [`SandboxError::NetworkSetup`] on bollard network-creation failure.
/// [`SandboxError::UnsupportedRuntime`] when the runtime is not Podman or
/// when nft rule installation fails.
pub async fn create_scan_network(
    docker: &bollard::Docker,
    scan_id: &str,
    llama_port: u16,
) -> Result<String, SandboxError> {
    let network_name = format!("lcrc-{scan_id}");

    let mut labels = HashMap::new();
    labels.insert("lcrc-scan-id", scan_id);

    docker
        .create_network(CreateNetworkOptions {
            name: network_name.as_str(),
            driver: "bridge",
            internal: true,
            labels,
            ipam: Ipam {
                ..Default::default()
            },
            ..Default::default()
        })
        .await
        .map_err(|e| SandboxError::NetworkSetup(format!("create_network: {e}")))?;

    install_port_pin_rules(docker, &network_name, llama_port).await?;

    tracing::warn!(
        target: "lcrc::sandbox::network",
        network = %network_name,
        "nft port-pin rule probe skipped: container image is a placeholder and cannot be pulled",
    );

    Ok(network_name)
}

/// Install nftables FORWARD rules inside the Podman VM to implement the
/// structural port-pin: allow TCP to `host_ip:llama_port`, drop everything else
/// from the container subnet.
///
/// The ACCEPT rule is installed before the DROP rule so nftables evaluates
/// in the correct order.
async fn install_port_pin_rules(
    docker: &bollard::Docker,
    network_name: &str,
    llama_port: u16,
) -> Result<(), SandboxError> {
    let machine_name = detect_podman_machine(docker).await.ok_or_else(|| {
        SandboxError::UnsupportedRuntime(
            "structural port-pin unavailable on this runtime; \
                 use the packaged Podman runtime (brew install podman) \
                 or a runtime that exposes network rule injection"
                .into(),
        )
    })?;

    let network_info = docker
        .inspect_network(
            network_name,
            None::<bollard::network::InspectNetworkOptions<String>>,
        )
        .await
        .map_err(|e| SandboxError::NetworkSetup(format!("inspect_network: {e}")))?;

    let container_subnet = network_info
        .ipam
        .as_ref()
        .and_then(|ipam| ipam.config.as_deref())
        .and_then(|cfgs| cfgs.first())
        .and_then(|cfg| cfg.subnet.as_deref())
        .ok_or_else(|| SandboxError::NetworkSetup("network has no IPAM subnet".into()))?
        .to_string();

    let host_ip = discover_host_ip(docker).await?;

    // ACCEPT rule: allow TCP from container subnet to host:llama_port.
    let accept_status = tokio::process::Command::new("podman")
        .args([
            "machine",
            "exec",
            &machine_name,
            "sudo",
            "nft",
            "add",
            "rule",
            "ip",
            "filter",
            "FORWARD",
            "ip",
            "saddr",
            &container_subnet,
            "ip",
            "daddr",
            &host_ip,
            "tcp",
            "dport",
            &llama_port.to_string(),
            "accept",
        ])
        .output()
        .await
        .map_err(|e| SandboxError::UnsupportedRuntime(format!("nft accept rule exec: {e}")))?;

    if !accept_status.status.success() {
        let stderr = String::from_utf8_lossy(&accept_status.stderr);
        return Err(SandboxError::UnsupportedRuntime(format!(
            "nft rule install failed: {stderr}"
        )));
    }

    // DROP rule: drop all other outbound from the container subnet.
    let drop_status = tokio::process::Command::new("podman")
        .args([
            "machine",
            "exec",
            &machine_name,
            "sudo",
            "nft",
            "add",
            "rule",
            "ip",
            "filter",
            "FORWARD",
            "ip",
            "saddr",
            &container_subnet,
            "drop",
        ])
        .output()
        .await
        .map_err(|e| SandboxError::UnsupportedRuntime(format!("nft drop rule exec: {e}")))?;

    if !drop_status.status.success() {
        let stderr = String::from_utf8_lossy(&drop_status.stderr);
        return Err(SandboxError::UnsupportedRuntime(format!(
            "nft rule install failed: {stderr}"
        )));
    }

    tracing::info!(
        target: "lcrc::sandbox::network",
        network = network_name,
        host_ip = %host_ip,
        llama_port = llama_port,
        "nft port-pin rules installed",
    );

    Ok(())
}

/// Discover the host IP reachable from containers via `host.docker.internal`.
///
/// On Podman/macOS this is the Podman VM's host-gateway IP, typically in
/// the `192.168.65.x` range. Falls back to the "podman" bridge gateway if
/// the primary inspect fails.
async fn discover_host_ip(docker: &bollard::Docker) -> Result<String, SandboxError> {
    let info = docker
        .inspect_network(
            "podman",
            None::<bollard::network::InspectNetworkOptions<String>>,
        )
        .await
        .ok();

    if let Some(gw) = info
        .as_ref()
        .and_then(|net| net.ipam.as_ref())
        .and_then(|ipam| ipam.config.as_deref())
        .and_then(|cfgs| cfgs.first())
        .and_then(|cfg| cfg.gateway.as_deref())
    {
        return Ok(gw.to_string());
    }

    // Common Podman-on-macOS default host IP.
    Ok("192.168.65.2".to_string())
}

/// Remove the per-scan Docker network.
///
/// Best-effort: errors are logged but not propagated.
pub async fn remove_scan_network(docker: &bollard::Docker, network_name: &str) {
    if let Err(e) = docker.remove_network(network_name).await {
        tracing::warn!(
            target: "lcrc::sandbox::network",
            network = network_name,
            error = %e,
            "failed to remove scan network",
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::parse_running_machine;

    #[test]
    fn parse_running_machine_finds_running_entry() {
        let output = "podman-machine-default,true\npodman-alt,false\n";
        assert_eq!(
            parse_running_machine(output).as_deref(),
            Some("podman-machine-default")
        );
    }

    #[test]
    fn parse_running_machine_skips_stopped_entries() {
        let output = "my-machine,false\nother-machine,false\n";
        assert_eq!(parse_running_machine(output), None);
    }

    #[test]
    fn parse_running_machine_handles_running_keyword() {
        let output = "my-vm,running\n";
        assert_eq!(parse_running_machine(output).as_deref(), Some("my-vm"));
    }

    #[test]
    fn parse_running_machine_empty_output_returns_none() {
        let output = "";
        assert_eq!(parse_running_machine(output), None);
    }
}
