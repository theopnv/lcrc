# Release Process

## Bootstrap Image Publish (manual; superseded by automated release workflow)

This section documents the one-time manual publish of the per-task container image to GHCR.
The automated release workflow (`.github/workflows/release.yml`) supersedes these steps; the
bootstrap record below is the historical baseline and a fallback if automation is unavailable.

### Prerequisites

- Docker daemon running (Docker Desktop, Colima, OrbStack, or equivalent)
- GitHub PAT with `write:packages` scope — set without writing to shell history:
  ```bash
  read -rs GITHUB_PAT && export GITHUB_PAT
  ```
- Replace `<org>` and `<github-username>` with real values throughout all commands

### Steps

**1. Authenticate to GHCR**

```bash
echo $GITHUB_PAT | docker login ghcr.io -u <github-username> --password-stdin
```

**2. Build the image**

The build context is `image/` so `COPY requirements.txt` resolves within that directory.

```bash
docker build --no-cache image/ -t ghcr.io/<org>/lcrc-task:0.1.0
```

**3. Push to GHCR**

```bash
docker push ghcr.io/<org>/lcrc-task:0.1.0
```

**4. Capture the canonical digest reference**

```bash
docker inspect --format '{{index .RepoDigests 0}}' ghcr.io/<org>/lcrc-task:0.1.0
# Example output: ghcr.io/<org>/lcrc-task:0.1.0@sha256:abc123...64hexchars
# An empty result means the push did not complete — verify step 3 before continuing.
```

**5. Update `src/constants.rs`**

Replace the placeholder in `CONTAINER_IMAGE_DIGEST` with the full digest reference from step 4:

```rust
pub const CONTAINER_IMAGE_DIGEST: &str =
    "ghcr.io/<org>/lcrc-task:0.1.0@sha256:<64-hex-chars>";
```

The digest string must satisfy:
- Exactly one `@`
- `name:tag` before the `@`
- `sha256:` followed by exactly 64 lowercase hex characters after the `@`

**6. Verify the image boots correctly**

```bash
docker run --rm ghcr.io/<org>/lcrc-task:0.1.0 \
    python3 -c "import mini_swe_agent; print('mini-swe-agent ok')"

docker run --rm ghcr.io/<org>/lcrc-task:0.1.0 \
    python3 -m pytest --version
```

**7. Run the sandbox integration tests**

These tests are `#[ignore]` by default and require both a live Docker daemon and the published image:

```bash
cargo test --test sandbox -- --include-ignored
```

If `CONTAINER_IMAGE_DIGEST` does not match the pushed image, `verify_digest` fails with
`SandboxError::ImagePull("digest mismatch: ...")`.
