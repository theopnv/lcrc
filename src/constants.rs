//! Compile-time pinned values: container image digest, schema version, and
//! other cross-cutting constants that must not drift between build artifacts.

/// Pinned container image reference for the per-task execution environment.
///
/// Includes both the registry tag and the digest so bollard can verify the
/// local layer cache matches exactly what was published.
pub const CONTAINER_IMAGE_DIGEST: &str = "ghcr.io/<org>/lcrc-task:0.1.0@sha256:0000000000000000000000000000000000000000000000000000000000000000";
