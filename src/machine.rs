//! Owner of the [`MachineFingerprint`] type and the [`MachineFingerprint::detect`]
//! entry point; delegates platform-specific I/O to the [`apple_silicon`]
//! submodule.
//!
//! Per FR24 the canonical fingerprint string is one of the seven cache-cell
//! PK dimensions; per NFR-C2 it must be byte-stable across macOS patch-level
//! upgrades. The pure parse/render functions in [`apple_silicon`] own that
//! stability — `detect()` is only the I/O wrapper that pipes
//! `sysctl` / `ioreg` stdout through them.

use thiserror::Error;

pub(crate) mod apple_silicon;

/// Errors that [`MachineFingerprint::detect`] can return.
///
/// The [`Display`](std::fmt::Display) text of [`FingerprintError::UnsupportedHardware`]
/// starts with the literal `"unsupported hardware"`; the `tests/machine_fingerprint.rs`
/// integration test pins that substring (AC3).
#[derive(Debug, Error)]
pub enum FingerprintError {
    /// Hardware (chip / GPU) does not match a known Apple Silicon
    /// configuration. NFR-C1: lcrc supports macOS Apple Silicon only;
    /// Intel Macs and Linux hit this branch.
    #[error("unsupported hardware: {reason}")]
    UnsupportedHardware {
        /// Human-readable detail about which input failed.
        reason: String,
    },

    /// Underlying `sysctl` invocation failed (binary missing, non-zero
    /// exit, …). On Linux this fires when the binary is absent or the MIB
    /// is unknown.
    #[error("sysctl execution failed")]
    SysctlExecFailed {
        /// The underlying I/O error (or a synthetic one carrying the
        /// non-zero exit + stderr).
        #[source]
        source: std::io::Error,
    },

    /// Underlying `ioreg` invocation failed (binary missing, non-zero
    /// exit, …). macOS-only by construction; on non-macOS hosts this
    /// fires before [`apple_silicon::parse_gpu_cores_from_ioreg`] runs.
    #[error("ioreg execution failed")]
    IoregExecFailed {
        /// The underlying I/O error (or a synthetic one carrying the
        /// non-zero exit + stderr).
        #[source]
        source: std::io::Error,
    },

    /// Sysctl returned data that could not be parsed (non-UTF-8 stdout,
    /// `hw.memsize` not fitting `u64`, …). Distinct from
    /// [`FingerprintError::UnsupportedHardware`] so "the data is corrupt"
    /// stays separable from "the hardware is the wrong shape" in
    /// diagnostics.
    #[error("parse error: {message}")]
    ParseError {
        /// Human-readable detail about which input could not be parsed.
        message: String,
    },
}

/// Canonical hardware identity used as the first dimension of every cache
/// cell's PK (FR24).
///
/// The wrapped string format is `"<chip-token>-<ram_gib>GB-<gpu_cores>gpu"`
/// (e.g. `"M1Pro-32GB-14gpu"`). Construct via [`MachineFingerprint::detect`]
/// only — there is intentionally no public constructor from raw strings, to
/// keep `crate::cache::key` (Story 1.6) the sole caller that derives the
/// cache-key string from a `MachineFingerprint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineFingerprint(String);

impl MachineFingerprint {
    /// Borrow the canonical fingerprint string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Detect the host hardware and return the canonical fingerprint.
    ///
    /// # Errors
    ///
    /// Returns [`FingerprintError::UnsupportedHardware`] on Intel Macs,
    /// Linux hosts, and any Apple Silicon variant whose chip brand string
    /// is not in the supported table. Returns
    /// [`FingerprintError::SysctlExecFailed`] /
    /// [`FingerprintError::IoregExecFailed`] when the underlying probes
    /// cannot be invoked. Returns [`FingerprintError::ParseError`] when
    /// probe output is structurally unexpected.
    pub async fn detect() -> Result<Self, FingerprintError> {
        // Probe order chip → RAM → GPU: on Intel/Linux the chip probe
        // fails first and we never spend cycles on `ioreg` (macOS-only).
        let chip = apple_silicon::read_chip().await?;
        let ram_bytes = apple_silicon::read_ram_bytes().await?;
        let gpu_cores = apple_silicon::read_gpu_cores().await?;
        let ram_gb = apple_silicon::ram_bytes_to_gb(ram_bytes);
        Ok(Self(apple_silicon::render(chip, ram_gb, gpu_cores)))
    }

    /// Pure constructor for tests and internal composition.
    #[cfg(test)]
    pub(crate) fn from_canonical_string(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for MachineFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{FingerprintError, MachineFingerprint, apple_silicon};

    #[test]
    fn nfr_c2_canonical_string_byte_stable_across_construction() {
        let canonical = "M1Pro-32GB-14gpu".to_string();
        let first = MachineFingerprint::from_canonical_string(canonical.clone());
        let second = MachineFingerprint::from_canonical_string(canonical);
        assert_eq!(first.as_str(), second.as_str());
        assert_eq!(first, second);
    }

    #[test]
    fn nfr_c2_render_matches_from_canonical_string() {
        let rendered = apple_silicon::render(apple_silicon::Chip::M1Pro, 32, 14);
        let via_constructor = MachineFingerprint::from_canonical_string(rendered.clone());
        assert_eq!(via_constructor.as_str(), rendered);
        assert_eq!(via_constructor.as_str(), "M1Pro-32GB-14gpu");
    }

    #[test]
    fn display_round_trips_to_as_str() {
        let fp = MachineFingerprint::from_canonical_string("M2-16GB-10gpu".into());
        assert_eq!(format!("{fp}"), fp.as_str());
    }

    #[test]
    fn unsupported_hardware_display_locks_substring_for_ac3() {
        let err = FingerprintError::UnsupportedHardware {
            reason: "intel brand string".into(),
        };
        let rendered = err.to_string();
        assert!(
            rendered.contains("unsupported hardware"),
            "AC3 substring missing in {rendered:?}"
        );
    }
}
