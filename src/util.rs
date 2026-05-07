//! Cross-cutting helpers; see `src/util/<module>.rs` for individual helpers
//! per architecture §"Complete Project Directory Structure".

pub mod tracing;

pub use tracing::rfc3339_now;
