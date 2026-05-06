//! Apple Silicon chip / RAM / GPU-core detection backing
//! [`crate::machine::MachineFingerprint`].
//!
//! The pure `parse_*` and `render` functions own the canonical fingerprint
//! string format; `read_*` are thin async wrappers over `tokio::process::Command`
//! that exec `sysctl` / `ioreg` and pipe stdout into the pure functions. The
//! split keeps every byte-stable invariant testable without spawning real
//! subprocesses.

use std::io;

use tokio::process::Command;

use super::FingerprintError;

/// Apple Silicon chip identifier. Each variant maps to exactly one
/// `<chip>` token in the canonical fingerprint string via [`Chip::token`].
///
/// The token spelling is load-bearing: every cache cell is keyed on it, so
/// renaming a variant or its token requires a cache migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Chip {
    M1,
    M1Pro,
    M1Max,
    M1Ultra,
    M2,
    M2Pro,
    M2Max,
    M2Ultra,
    M3,
    M3Pro,
    M3Max,
    M4,
    M4Pro,
    M4Max,
}

impl Chip {
    /// The dash-prefix token of the canonical fingerprint string.
    pub(crate) fn token(self) -> &'static str {
        match self {
            Chip::M1 => "M1",
            Chip::M1Pro => "M1Pro",
            Chip::M1Max => "M1Max",
            Chip::M1Ultra => "M1Ultra",
            Chip::M2 => "M2",
            Chip::M2Pro => "M2Pro",
            Chip::M2Max => "M2Max",
            Chip::M2Ultra => "M2Ultra",
            Chip::M3 => "M3",
            Chip::M3Pro => "M3Pro",
            Chip::M3Max => "M3Max",
            Chip::M4 => "M4",
            Chip::M4Pro => "M4Pro",
            Chip::M4Max => "M4Max",
        }
    }
}

/// Parse the output of `sysctl -n machdep.cpu.brand_string` into a [`Chip`].
///
/// Apple Silicon brand strings are of the form `"Apple M<N>[ <Variant>][ <decoration>]"`
/// where `<decoration>` may be appended on virtualized hosts (e.g. GitHub
/// Actions `macos-14` runners report `"Apple M1 (Virtual)"`) or by future
/// macOS releases. We therefore match the chip family by **prefix** after
/// stripping the leading `"Apple "`, ordered longest-first so `"M1 Pro"` is
/// tried before bare `"M1"`. The space inside the suffix is collapsed in the
/// canonical token so `"Apple M1 Pro"` → [`Chip::M1Pro`] (token `"M1Pro"`).
///
/// Cache semantics: virtualized chips share the bare chip's cache cells —
/// the underlying silicon and ISA are identical, so any cached result is
/// equally valid on either.
pub(crate) fn parse_chip(brand_string: &str) -> Result<Chip, FingerprintError> {
    // Longest-first so e.g. `"M1 Ultra ..."` is not misclassified as `M1`.
    const PREFIXES: &[(&str, Chip)] = &[
        ("M1 Ultra", Chip::M1Ultra),
        ("M1 Pro", Chip::M1Pro),
        ("M1 Max", Chip::M1Max),
        ("M1", Chip::M1),
        ("M2 Ultra", Chip::M2Ultra),
        ("M2 Pro", Chip::M2Pro),
        ("M2 Max", Chip::M2Max),
        ("M2", Chip::M2),
        ("M3 Pro", Chip::M3Pro),
        ("M3 Max", Chip::M3Max),
        ("M3", Chip::M3),
        ("M4 Pro", Chip::M4Pro),
        ("M4 Max", Chip::M4Max),
        ("M4", Chip::M4),
    ];

    let trimmed = brand_string.trim();
    let suffix =
        trimmed
            .strip_prefix("Apple ")
            .ok_or_else(|| FingerprintError::UnsupportedHardware {
                reason: format!("unsupported chip brand string: {brand_string:?}"),
            })?;
    for (prefix, chip) in PREFIXES {
        let Some(after) = suffix.strip_prefix(prefix) else {
            continue;
        };
        // Reject `"M11"` / `"M1Pro"` / `"M1X"`: anything following the chip
        // family token must be either end-of-string or a non-alphanumeric
        // boundary (space before a variant or decoration).
        if after
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric())
        {
            return Ok(*chip);
        }
    }
    Err(FingerprintError::UnsupportedHardware {
        reason: format!("unsupported chip brand string: {brand_string:?}"),
    })
}

/// Parse the output of `sysctl -n hw.memsize` into a byte count.
pub(crate) fn parse_ram_bytes(s: &str) -> Result<u64, FingerprintError> {
    s.trim()
        .parse::<u64>()
        .map_err(|e| FingerprintError::ParseError {
            message: format!("hw.memsize parse failure: {e} (input: {s:?})"),
        })
}

/// Convert raw `hw.memsize` bytes to the binary-GiB integer used in the
/// canonical fingerprint string. The `GB` suffix in the canonical string is
/// shorthand for GiB by convention — `sysctl hw.memsize` reports
/// binary-prefixed bytes and Apple's `"32GB"` SKU labels are also binary
/// prefix, so `34_359_738_368 / 2^30 == 32` exactly. Truncates; never rounds.
pub(crate) fn ram_bytes_to_gb(bytes: u64) -> u64 {
    bytes / (1024 * 1024 * 1024)
}

/// Parse `ioreg -l` output and extract the `gpu-core-count` integer.
///
/// Scans line-by-line for the quoted `ioreg` key `"gpu-core-count"`; on the
/// first match, splits on `=` and parses the trimmed right-hand side as
/// `u32`. The quotes are part of the match so a future ioreg key whose name
/// happens to contain `gpu-core-count` as a substring (e.g. a hypothetical
/// `gpu-core-count-helper`) cannot collide. `regex` is intentionally not
/// used (out of the locked dependency set).
pub(crate) fn parse_gpu_cores_from_ioreg(ioreg_output: &str) -> Result<u32, FingerprintError> {
    for line in ioreg_output.lines() {
        if line.contains("\"gpu-core-count\"")
            && let Some((_, value)) = line.split_once('=')
        {
            return value.trim().parse::<u32>().map_err(|e| {
                FingerprintError::UnsupportedHardware {
                    reason: format!("ioreg gpu-core-count parse failure: {e} (line: {line:?})"),
                }
            });
        }
    }
    Err(FingerprintError::UnsupportedHardware {
        reason: "ioreg output does not expose gpu-core-count (non-Apple-Silicon GPU?)".into(),
    })
}

/// Single source of truth for the canonical fingerprint string format.
///
/// Format: `"<chip-token>-<ram_gb>GB-<gpu_cores>gpu"`. Story 1.6's
/// `cache::key::machine_fingerprint` reads a `MachineFingerprint`'s
/// `as_str()` directly; the cache cell schema stores this string verbatim.
/// Changing this format is a breaking cache-schema change.
pub(crate) fn render(chip: Chip, ram_gb: u64, gpu_cores: u32) -> String {
    format!(
        "{chip_token}-{ram_gb}GB-{gpu_cores}gpu",
        chip_token = chip.token()
    )
}

/// Exec `sysctl -n machdep.cpu.brand_string` and feed stdout to [`parse_chip`].
pub(crate) async fn read_chip() -> Result<Chip, FingerprintError> {
    let stdout = run_capture(
        "sysctl",
        &["-n", "machdep.cpu.brand_string"],
        CaptureKind::Sysctl,
    )
    .await?;
    parse_chip(&stdout)
}

/// Exec `sysctl -n hw.memsize` and feed stdout to [`parse_ram_bytes`].
pub(crate) async fn read_ram_bytes() -> Result<u64, FingerprintError> {
    let stdout = run_capture("sysctl", &["-n", "hw.memsize"], CaptureKind::Sysctl).await?;
    parse_ram_bytes(&stdout)
}

/// Exec `ioreg -l` and feed stdout to [`parse_gpu_cores_from_ioreg`].
pub(crate) async fn read_gpu_cores() -> Result<u32, FingerprintError> {
    let stdout = run_capture("ioreg", &["-l"], CaptureKind::Ioreg).await?;
    parse_gpu_cores_from_ioreg(&stdout)
}

#[derive(Clone, Copy)]
enum CaptureKind {
    Sysctl,
    Ioreg,
}

impl CaptureKind {
    fn wrap(self, source: io::Error) -> FingerprintError {
        match self {
            CaptureKind::Sysctl => FingerprintError::SysctlExecFailed { source },
            CaptureKind::Ioreg => FingerprintError::IoregExecFailed { source },
        }
    }
}

async fn run_capture(
    bin: &str,
    args: &[&str],
    kind: CaptureKind,
) -> Result<String, FingerprintError> {
    let output = Command::new(bin)
        .args(args)
        .output()
        .await
        .map_err(|e| kind.wrap(e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let synthetic = io::Error::other(format!(
            "{bin} {args:?} exited {status:?}: {stderr}",
            status = output.status.code()
        ));
        return Err(kind.wrap(synthetic));
    }
    String::from_utf8(output.stdout).map_err(|e| FingerprintError::ParseError {
        message: format!("non-utf8 {bin} stdout: {e}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{
        Chip, parse_chip, parse_gpu_cores_from_ioreg, parse_ram_bytes, ram_bytes_to_gb, render,
    };
    use crate::machine::FingerprintError;

    #[test]
    fn parse_chip_recognises_six_required_variants() {
        let cases = [
            ("Apple M1", Chip::M1, "M1"),
            ("Apple M1 Pro", Chip::M1Pro, "M1Pro"),
            ("Apple M1 Max", Chip::M1Max, "M1Max"),
            ("Apple M2", Chip::M2, "M2"),
            ("Apple M3", Chip::M3, "M3"),
            ("Apple M4", Chip::M4, "M4"),
        ];
        for (brand, expected_chip, expected_token) in cases {
            let chip = parse_chip(brand).unwrap();
            assert_eq!(chip, expected_chip, "brand: {brand}");
            assert_eq!(chip.token(), expected_token, "brand: {brand}");
        }
    }

    #[test]
    fn parse_chip_recognises_extra_binning_variants() {
        assert_eq!(parse_chip("Apple M2 Pro").unwrap(), Chip::M2Pro);
        assert_eq!(parse_chip("Apple M3 Max").unwrap(), Chip::M3Max);
        assert_eq!(parse_chip("Apple M1 Ultra").unwrap(), Chip::M1Ultra);
    }

    #[test]
    fn parse_chip_trims_whitespace() {
        assert_eq!(parse_chip("  Apple M1 Pro\n").unwrap(), Chip::M1Pro);
    }

    #[test]
    fn parse_chip_rejects_intel_brand_string() {
        let err =
            parse_chip("Intel(R) Core(TM) i9-9880H CPU @ 2.30GHz").expect_err("intel must reject");
        assert!(matches!(err, FingerprintError::UnsupportedHardware { .. }));
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn parse_chip_rejects_empty() {
        let err = parse_chip("").expect_err("empty must reject");
        assert!(matches!(err, FingerprintError::UnsupportedHardware { .. }));
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn parse_chip_rejects_unknown_apple_suffix() {
        let err = parse_chip("Apple M99 Hyperthreaded").expect_err("unknown must reject");
        assert!(matches!(err, FingerprintError::UnsupportedHardware { .. }));
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn parse_chip_accepts_virtualized_suffix() {
        // GitHub Actions `macos-14` runners report this exact brand string;
        // virtualized hosts collapse to the bare-chip token.
        assert_eq!(parse_chip("Apple M1 (Virtual)").unwrap(), Chip::M1);
        assert_eq!(parse_chip("Apple M2 Pro (Virtual)").unwrap(), Chip::M2Pro);
        assert_eq!(parse_chip("Apple M3 Max (Virtual)\n").unwrap(), Chip::M3Max);
    }

    #[test]
    fn parse_chip_rejects_alphanumeric_run_on() {
        // Boundary check: `Apple M11` must not match `M1` despite the prefix.
        let err = parse_chip("Apple M11").expect_err("M11 must reject");
        assert!(matches!(err, FingerprintError::UnsupportedHardware { .. }));
    }

    #[test]
    fn parse_ram_bytes_parses_trimmed_decimal() {
        assert_eq!(parse_ram_bytes("34359738368\n").unwrap(), 34_359_738_368);
    }

    #[test]
    fn parse_ram_bytes_rejects_empty() {
        let err = parse_ram_bytes("").expect_err("empty must reject");
        assert!(matches!(err, FingerprintError::ParseError { .. }));
    }

    #[test]
    fn ram_bytes_to_gb_truncates_to_binary_gib() {
        assert_eq!(ram_bytes_to_gb(34_359_738_368), 32);
        assert_eq!(ram_bytes_to_gb(17_179_869_184), 16);
        assert_eq!(ram_bytes_to_gb(68_719_476_736), 64);
    }

    #[test]
    fn parse_gpu_cores_from_ioreg_extracts_first_match() {
        let fixture = "        | |   \"gpu-core-count\" = 14\n        | |   \"other\" = 99\n";
        assert_eq!(parse_gpu_cores_from_ioreg(fixture).unwrap(), 14);
    }

    #[test]
    fn parse_gpu_cores_from_ioreg_rejects_empty() {
        let err = parse_gpu_cores_from_ioreg("").expect_err("empty must reject");
        assert!(matches!(err, FingerprintError::UnsupportedHardware { .. }));
    }

    #[test]
    fn parse_gpu_cores_from_ioreg_rejects_present_key_but_missing_integer() {
        let err = parse_gpu_cores_from_ioreg("\"gpu-core-count\" = ")
            .expect_err("missing integer must reject");
        assert!(matches!(err, FingerprintError::UnsupportedHardware { .. }));
    }

    #[test]
    fn render_produces_canonical_format() {
        assert_eq!(render(Chip::M1Pro, 32, 14), "M1Pro-32GB-14gpu");
        assert_eq!(render(Chip::M2, 16, 10), "M2-16GB-10gpu");
        assert_eq!(render(Chip::M3Max, 64, 40), "M3Max-64GB-40gpu");
    }

    #[test]
    fn render_is_byte_stable_across_calls() {
        let first = render(Chip::M1Pro, 32, 14);
        let second = render(Chip::M1Pro, 32, 14);
        assert_eq!(first, second);
    }

    #[test]
    fn pure_pipeline_is_byte_stable_across_calls() {
        let brand = "Apple M1 Pro\n";
        let memsize = "34359738368\n";
        let ioreg = "        | |   \"gpu-core-count\" = 14\n";

        let render_once = || -> String {
            let chip = parse_chip(brand).unwrap();
            let ram_gb = ram_bytes_to_gb(parse_ram_bytes(memsize).unwrap());
            let gpu = parse_gpu_cores_from_ioreg(ioreg).unwrap();
            render(chip, ram_gb, gpu)
        };

        assert_eq!(render_once(), render_once());
        assert_eq!(render_once(), "M1Pro-32GB-14gpu");
    }
}
