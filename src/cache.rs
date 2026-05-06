//! Cache module root. The [`key`] submodule owns canonical derivation of the
//! four cache-key components (`model_sha`, `params_hash`, `machine_fingerprint`,
//! `backend_build`); future submodules (`schema`, `migrations`, `cell`,
//! `query`) layer `SQLite` storage and the public cache API on top.

pub mod key;
