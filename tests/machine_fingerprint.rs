//! Integration tests for `lcrc::machine::MachineFingerprint::detect()` —
//! AC1 (Apple Silicon canonical string structure) on macOS, plus the
//! cfg-gated mirror that pins the AC3 unsupported-hardware substring on
//! every other target so the v1.1 NFR-C5 Linux additive port has a
//! pre-existing assertion to satisfy.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[cfg(target_os = "macos")]
#[tokio::test]
async fn detect_returns_apple_silicon_canonical_string() {
    let fp = lcrc::machine::MachineFingerprint::detect()
        .await
        .expect("detect() must succeed on macOS Apple Silicon CI runner");
    let canonical = fp.as_str();

    let parts: Vec<&str> = canonical.split('-').collect();
    assert_eq!(
        parts.len(),
        3,
        "canonical string {canonical:?} must split into 3 dash-separated tokens"
    );

    // Explicit allow-list (not a regex pattern) so a future M5 chip cannot
    // silently slip through this test without a deliberate update.
    let allowed_chip_tokens = [
        "M1", "M1Pro", "M1Max", "M1Ultra", "M2", "M2Pro", "M2Max", "M2Ultra", "M3", "M3Pro",
        "M3Max", "M4", "M4Pro", "M4Max",
    ];
    assert!(
        allowed_chip_tokens.contains(&parts[0]),
        "chip token {chip:?} not in allow-list {allowed_chip_tokens:?}",
        chip = parts[0]
    );

    let ram_token = parts[1];
    let ram_prefix = ram_token
        .strip_suffix("GB")
        .unwrap_or_else(|| panic!("ram token {ram_token:?} must end with 'GB'"));
    let ram_value: u64 = ram_prefix
        .parse()
        .unwrap_or_else(|e| panic!("ram prefix {ram_prefix:?} must parse as u64: {e}"));
    assert!(ram_value > 0, "ram value must be positive, got {ram_value}");

    let gpu_token = parts[2];
    let gpu_prefix = gpu_token
        .strip_suffix("gpu")
        .unwrap_or_else(|| panic!("gpu token {gpu_token:?} must end with 'gpu'"));
    let gpu_value: u32 = gpu_prefix
        .parse()
        .unwrap_or_else(|e| panic!("gpu prefix {gpu_prefix:?} must parse as u32: {e}"));
    assert!(gpu_value > 0, "gpu value must be positive, got {gpu_value}");
}

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn detect_returns_unsupported_hardware_on_non_macos() {
    let result = lcrc::machine::MachineFingerprint::detect().await;
    let err = result.expect_err("detect() must fail on non-macOS hosts (NFR-C1)");
    assert!(
        err.to_string().contains("unsupported"),
        "non-macOS Display rendering {err:?} missing 'unsupported' substring"
    );
}
