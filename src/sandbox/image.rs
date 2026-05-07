//! Container image pull and digest verification.
//!
//! Pulls the per-task container image from GHCR on first use, verifies the
//! digest matches [`crate::constants::CONTAINER_IMAGE_DIGEST`], and is the
//! single gatekeeper for container image content integrity.

use bollard::image::CreateImageOptions;
use futures_util::StreamExt as _;

use crate::sandbox::SandboxError;

/// Ensure the named image is present locally with a matching digest.
///
/// Parses `image_ref` to separate the name from the `@sha256:` digest
/// component. If the image is already local the digest is verified. If it
/// is missing the image is pulled, then the digest is verified.
///
/// # Errors
///
/// [`SandboxError::ImagePull`] when the pull fails or the local image's
/// digest does not match the expected value embedded in `image_ref`.
pub async fn ensure_image(docker: &bollard::Docker, image_ref: &str) -> Result<(), SandboxError> {
    let (name_part, expected_digest) = parse_image_ref(image_ref)?;

    if let Ok(inspect) = docker.inspect_image(&name_part).await {
        verify_digest(&inspect, &expected_digest, image_ref)?;
        tracing::info!(
            target: "lcrc::sandbox::image",
            image = image_ref,
            "container image already local with matching digest",
        );
    } else {
        pull_image(docker, &name_part).await?;
        let inspect = docker
            .inspect_image(&name_part)
            .await
            .map_err(|e| SandboxError::ImagePull(format!("inspect after pull: {e}")))?;
        verify_digest(&inspect, &expected_digest, image_ref)?;
        tracing::info!(
            target: "lcrc::sandbox::image",
            image = image_ref,
            "container image pulled and digest verified",
        );
    }

    Ok(())
}

/// Split `image_ref` at `@` into (`name_with_tag`, `digest_part`).
///
/// Returns `SandboxError::ImagePull` when the `@sha256:` separator is absent.
fn parse_image_ref(image_ref: &str) -> Result<(String, String), SandboxError> {
    match image_ref.split_once('@') {
        Some((name, digest)) => Ok((name.to_string(), digest.to_string())),
        None => Err(SandboxError::ImagePull(format!(
            "image reference has no digest component: {image_ref}"
        ))),
    }
}

/// Check that the `RepoDigests` list contains an entry whose digest component
/// exactly matches `expected_digest` (the portion after `@`).
fn verify_digest(
    inspect: &bollard::models::ImageInspect,
    expected_digest: &str,
    image_ref: &str,
) -> Result<(), SandboxError> {
    let digests = inspect.repo_digests.as_deref().unwrap_or(&[]);
    let matches = digests.iter().any(|d| {
        d.split_once('@')
            .is_some_and(|(_, hash)| hash == expected_digest)
    });
    if matches {
        Ok(())
    } else {
        Err(SandboxError::ImagePull(format!(
            "digest mismatch: expected {expected_digest} for {image_ref}, got {digests:?}"
        )))
    }
}

/// Stream-drain a `create_image` call, surfacing any stream-level errors.
async fn pull_image(docker: &bollard::Docker, name_part: &str) -> Result<(), SandboxError> {
    let (from_image, tag) = name_part.split_once(':').map_or_else(
        || (name_part.to_string(), "latest".to_string()),
        |(n, t)| (n.to_string(), t.to_string()),
    );

    let mut stream = docker.create_image(
        Some(CreateImageOptions {
            from_image,
            tag,
            ..Default::default()
        }),
        None,
        None,
    );

    while let Some(event) = stream.next().await {
        event.map_err(|e| SandboxError::ImagePull(format!("pull stream error: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::parse_image_ref;
    use crate::sandbox::SandboxError;

    #[test]
    fn parse_image_ref_splits_at_at_sign() {
        let (name, digest) =
            parse_image_ref("ghcr.io/org/lcrc-task:0.1.0@sha256:abcd1234").unwrap();
        assert_eq!(name, "ghcr.io/org/lcrc-task:0.1.0");
        assert_eq!(digest, "sha256:abcd1234");
    }

    #[test]
    fn parse_image_ref_no_digest_returns_error() {
        let result = parse_image_ref("ghcr.io/org/lcrc-task:0.1.0");
        assert!(matches!(result, Err(SandboxError::ImagePull(_))));
    }

    #[test]
    fn parse_image_ref_preserves_full_name_with_tag() {
        let (name, _) = parse_image_ref("ghcr.io/org/img:1.0@sha256:0000").unwrap();
        assert_eq!(name, "ghcr.io/org/img:1.0");
    }
}
