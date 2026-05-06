//! Canonical derivation of the cache-key components.
//!
//! All four PK component values (`model_sha`, `params_hash`,
//! `machine_fingerprint`, `backend_build`) are computed exclusively here.
//! Inline `format!`, `Sha256::digest`, or hand-rolled JSON construction at any
//! other site silently breaks cache-key stability and invalidates cached cells.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncReadExt;

/// Render `bytes` as a lowercase, no-prefix hex string (`b"\xab\xcd"` →
/// `"abcd"`). Inline `format!("{:02x}", b)` loop avoids a `hex` crate dep.
fn hex_lowercase(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // Writes to `String` are infallible; `let _` because workspace lints
        // deny `unwrap`/`expect` outside tests.
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Inference parameters that participate in [`params_hash`].
///
/// Field types match `llama-server` API conventions: `ctx` is the positive
/// context-window length, `temp` is sampling temperature, `threads` is the
/// CPU thread count, and `n_gpu_layers` is the count of layers offloaded to
/// the GPU (`0` = CPU-only).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Params {
    /// Context-window length passed to the backend.
    pub ctx: u32,
    /// Sampling temperature; in practice in the range `0.0..=2.0`.
    pub temp: f32,
    /// CPU thread count.
    pub threads: u32,
    /// Number of layers offloaded to the GPU (`0` = CPU-only).
    pub n_gpu_layers: u32,
}

/// Identity of the LLM backend whose build participates in the cache key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendInfo {
    /// Backend identifier slug (e.g. `"llama.cpp"`).
    pub name: String,
    /// Build's semver / build-number string (e.g. `"b3791"`).
    pub semver: String,
    /// 7-character git short-SHA (e.g. `"a1b2c3d"`).
    pub commit_short: String,
}

/// Errors returned by helpers in this module.
#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum KeyError {
    /// File open or read failure inside [`model_sha`].
    #[error("failed to read model file '{}' for model_sha: {source}", path.display())]
    ModelShaIo {
        /// Path that failed to open or read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Canonical-JSON encoding failure inside [`params_hash`].
    ///
    /// Unreachable in practice for the current four-field [`Params`] shape;
    /// the `Result` is preserved because workspace lints forbid
    /// `unwrap`/`expect` outside tests and `serde_json` returns `Result`.
    #[error("failed to canonicalize params for params_hash: {source}")]
    ParamsHashSerialize {
        /// Underlying serializer error.
        #[source]
        source: serde_json::Error,
    },
}

/// Streaming SHA-256 of the file at `path`, returned as 64-char lowercase hex.
///
/// The file is read in 64 KiB chunks via `tokio::fs`; no full-file load into
/// memory.
///
/// # Errors
///
/// Returns [`KeyError::ModelShaIo`] when the file cannot be opened or any
/// chunked read fails.
pub async fn model_sha(path: &Path) -> Result<String, KeyError> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|source| KeyError::ModelShaIo {
            path: path.to_path_buf(),
            source,
        })?;
    let mut reader = tokio::io::BufReader::new(file);
    // Heap-allocated so the chunk buffer does not inflate the future.
    let mut buf = vec![0u8; 64 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let n = reader
            .read(&mut buf)
            .await
            .map_err(|source| KeyError::ModelShaIo {
                path: path.to_path_buf(),
                source,
            })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_lowercase(&hasher.finalize()))
}

/// SHA-256 of the canonical (sorted-key) JSON encoding of `params`, returned
/// as 64-char lowercase hex.
///
/// # Errors
///
/// Returns [`KeyError::ParamsHashSerialize`] if `serde_json` fails to encode
/// the value. Unreachable in practice for the current [`Params`] shape; the
/// `Result` is preserved because workspace lints forbid `unwrap`/`expect`
/// outside tests.
pub fn params_hash(params: &Params) -> Result<String, KeyError> {
    // The `to_value` → `to_string` round-trip is load-bearing: a direct
    // `serde_json::to_string(params)` uses `serialize_struct`, which preserves
    // field *declaration* order. Routing through `Value` (whose `Map` is a
    // `BTreeMap` alias when `serde_json/preserve_order` is OFF — the default
    // we lock in `Cargo.toml`) emits keys alphabetically and pins the
    // canonical encoding against future field reorderings.
    let value =
        serde_json::to_value(params).map_err(|source| KeyError::ParamsHashSerialize { source })?;
    let canonical =
        serde_json::to_string(&value).map_err(|source| KeyError::ParamsHashSerialize { source })?;
    let digest = Sha256::digest(canonical.as_bytes());
    Ok(hex_lowercase(&digest))
}

/// Canonical `<name>-<semver>+<commit_short>` formatting of `info`.
#[must_use]
pub fn backend_build(info: &BackendInfo) -> String {
    format!("{}-{}+{}", info.name, info.semver, info.commit_short)
}

/// Canonical fingerprint string for `fp` — the cache-key form of a
/// [`crate::machine::MachineFingerprint`].
///
/// Delegates to [`crate::machine::MachineFingerprint::as_str`] so that the
/// `<chip>-<ram>GB-<gpu>gpu` shape stays owned by the `machine` module.
#[must_use]
pub fn machine_fingerprint(fp: &crate::machine::MachineFingerprint) -> String {
    fp.as_str().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{
        BackendInfo, KeyError, Params, backend_build, machine_fingerprint, model_sha, params_hash,
    };
    use crate::machine::MachineFingerprint;
    use sha2::{Digest, Sha256};
    use std::io::Write as _;
    use tempfile::NamedTempFile;

    fn write_temp(bytes: &[u8]) -> NamedTempFile {
        let mut tf = NamedTempFile::new().unwrap();
        tf.write_all(bytes).unwrap();
        tf.flush().unwrap();
        tf
    }

    #[tokio::test]
    async fn model_sha_well_known_fixture() {
        let tf = write_temp(b"hello world\n");
        let digest = model_sha(tf.path()).await.unwrap();
        assert_eq!(
            digest,
            "a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447"
        );
    }

    #[tokio::test]
    async fn model_sha_streaming_matches_bulk_one_mib() {
        let bytes = vec![0xab; 1024 * 1024];
        let tf = write_temp(&bytes);
        let streamed = model_sha(tf.path()).await.unwrap();
        let bulk = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(streamed, bulk);
    }

    #[tokio::test]
    async fn model_sha_multi_chunk_two_hundred_kib() {
        // 200 KiB exercises the 64 KiB read loop boundary (3 full chunks +
        // an 8 KiB tail), confirming the multi-chunk accumulation is correct.
        let bytes = vec![0x5au8; 200 * 1024];
        let tf = write_temp(&bytes);
        let streamed = model_sha(tf.path()).await.unwrap();
        let bulk = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(streamed, bulk);
    }

    #[tokio::test]
    async fn model_sha_missing_path_returns_error_with_locked_substring() {
        let path = std::path::Path::new("/nonexistent/lcrc-test-cache-key/missing-fixture.gguf");
        let err = model_sha(path).await.unwrap_err();
        assert!(
            matches!(err, KeyError::ModelShaIo { .. }),
            "unexpected variant: {err:?}"
        );
        let rendered = err.to_string();
        assert!(
            rendered.contains("failed to read model file"),
            "Display contract substring missing in {rendered:?}"
        );
    }

    #[test]
    fn params_hash_field_order_independence() {
        let a = Params {
            ctx: 4096,
            temp: 0.2,
            threads: 8,
            n_gpu_layers: 99,
        };
        // Same data, fields written in a different literal order: must hash
        // identically because the canonical encoding sorts keys.
        let b = Params {
            n_gpu_layers: 99,
            threads: 8,
            temp: 0.2,
            ctx: 4096,
        };
        assert_eq!(params_hash(&a).unwrap(), params_hash(&b).unwrap());
    }

    #[test]
    fn params_hash_output_is_64_lowercase_hex() {
        let params = Params {
            ctx: 4096,
            temp: 0.2,
            threads: 8,
            n_gpu_layers: 99,
        };
        let digest = params_hash(&params).unwrap();
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .chars()
                .all(|c| c.is_ascii_hexdigit()
                    && (!c.is_ascii_alphabetic() || c.is_ascii_lowercase())),
            "non-lowercase-hex char in {digest:?}"
        );
    }

    #[test]
    fn params_hash_pinned_reference_digest() {
        // Pinned to guard NFR-R3 (cache durable across patch upgrades): any
        // silent change to canonical encoding (key ordering, float rendering,
        // etc.) flips this assertion.
        let params = Params {
            ctx: 4096,
            temp: 0.2,
            threads: 8,
            n_gpu_layers: 99,
        };
        assert_eq!(
            params_hash(&params).unwrap(),
            "932f844986b8c9c2ce2cfa6ec95181c49e11861e86e035ef4833cce947904e42"
        );
    }

    #[test]
    fn params_hash_temp_participates() {
        let base = Params {
            ctx: 4096,
            temp: 0.2,
            threads: 8,
            n_gpu_layers: 99,
        };
        // Story spec suggests `0.20000001f32`, but that literal rounds to the
        // same f32 bit pattern as `0.2_f32` (the gap is ~1.5e-8). Use a value
        // clearly outside the f32 quantization at 0.2 so the assertion has
        // teeth.
        let bumped = Params { temp: 0.21, ..base };
        assert_ne!(params_hash(&base).unwrap(), params_hash(&bumped).unwrap());
    }

    #[test]
    fn backend_build_locked_example() {
        let info = BackendInfo {
            name: "llama.cpp".into(),
            semver: "b3791".into(),
            commit_short: "a1b2c3d".into(),
        };
        assert_eq!(backend_build(&info), "llama.cpp-b3791+a1b2c3d");
    }

    #[test]
    fn backend_build_empty_inputs_do_not_panic() {
        // Empty fields are the source's problem (Backend::version output).
        // The formatter is a pure pass-through and must not validate.
        let info = BackendInfo {
            name: String::new(),
            semver: String::new(),
            commit_short: "a1b2c3d".into(),
        };
        assert_eq!(backend_build(&info), "-+a1b2c3d");
    }

    #[test]
    fn machine_fingerprint_round_trips_via_cfg_test_constructor() {
        // Cross-module assertion: pins the `MachineFingerprint::as_str` →
        // `key::machine_fingerprint` integration contract in code.
        let fp = MachineFingerprint::from_canonical_string("M1Pro-32GB-14gpu".into());
        assert_eq!(machine_fingerprint(&fp), "M1Pro-32GB-14gpu");
    }
}
