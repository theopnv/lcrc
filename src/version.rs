//! Long-form `--version` output: a header line
//! `lcrc <semver> (build <commit-short>)` followed by four indented field
//! rows (task source, harness, backend, container). Fields whose source is
//! not yet wired render as the literal string `"unknown"`.

use std::sync::OnceLock;

/// Crate semver, populated from `Cargo.toml` by cargo at compile time.
pub const LCRC_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Short git commit hash captured by `build.rs` at compile time. The literal
/// `"unknown"` is substituted when the build runs outside a git checkout.
pub const BUILD_COMMIT: &str = env!("LCRC_BUILD_COMMIT");

/// Vendored task-source revision (e.g. `tasks/swe-bench-pro/version`).
pub const TASK_SOURCE_VERSION: &str = "unknown";

/// Bundled harness revision (e.g. `image/requirements.txt`).
pub const HARNESS_VERSION: &str = "unknown";

/// GHCR container image digest.
pub const CONTAINER_DIGEST: &str = "unknown";

/// Render the 5-line `--version` self-attestation block.
#[must_use]
pub fn render_long() -> String {
    format!(
        "lcrc {LCRC_VERSION} (build {BUILD_COMMIT})\n  \
         task source: {TASK_SOURCE_VERSION}\n  \
         harness:     {HARNESS_VERSION}\n  \
         backend:     llama.cpp (auto-detected at runtime)\n  \
         container:   {CONTAINER_DIGEST}"
    )
}

/// Render the single-line short version used by clap when `-V` is passed.
#[must_use]
pub fn render_short() -> String {
    format!("lcrc {LCRC_VERSION}")
}

/// Memoize [`render_long`] and return a `&'static str` slice suitable for
/// clap's `Command::long_version` builder, which wants `impl IntoResettable<Str>`.
///
/// `OnceLock` keeps the allocation count at exactly 1 per process and avoids
/// `Box::leak`, which would be the only safe alternative for a `&'static str`.
#[must_use]
pub fn long_version_static() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(render_long).as_str()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{long_version_static, render_long, render_short};

    #[test]
    fn long_starts_with_lcrc_and_contains_one_build_marker() {
        let s = render_long();
        assert!(s.starts_with("lcrc "), "expected `lcrc ` prefix, got: {s}");
        let count = s.matches("(build ").count();
        assert_eq!(
            count, 1,
            "expected exactly one `(build ` marker, got {count}: {s}"
        );
    }

    #[test]
    fn long_contains_all_four_field_labels() {
        let s = render_long();
        assert!(s.contains("task source:"), "missing `task source:`: {s}");
        assert!(s.contains("harness:"), "missing `harness:`: {s}");
        assert!(s.contains("backend:"), "missing `backend:`: {s}");
        assert!(s.contains("container:"), "missing `container:`: {s}");
    }

    #[test]
    fn long_renders_without_panic_when_commit_is_unknown_literal() {
        let s = render_long();
        assert!(!s.is_empty());
    }

    #[test]
    fn short_is_single_line_with_lcrc_prefix() {
        let s = render_short();
        assert!(s.starts_with("lcrc "));
        assert!(!s.contains('\n'));
    }

    #[test]
    fn long_version_static_is_idempotent_and_matches_render_long() {
        let a = long_version_static();
        let b = long_version_static();
        assert!(
            std::ptr::eq(a, b),
            "OnceLock memoization must return the same slice"
        );
        assert_eq!(a, render_long());
    }
}
