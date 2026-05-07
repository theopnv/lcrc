//! Scan subsystem — one-cell measurement pipeline.
//!
//! Submodules:
//! - `canary` — stable canary task identifier and workspace setup
//! - `orchestrator` — one-cell scan pipeline (preflight → measure → persist)
//! - `server_lifecycle` — llama-server process lifecycle
//! - `signal` — SIGINT / Ctrl-C detection

pub mod canary;
pub mod orchestrator;
pub mod server_lifecycle;
pub mod signal;
