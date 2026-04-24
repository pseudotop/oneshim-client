// Cast safety: SQLite row IDs, byte counts, durations — precision loss acceptable.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// P2 PR-C: `missing_const_for_fn` accepted crate-wide. See
// docs/reviews/2026-04-21-p2-missing-const-for-fn-decision.md.
#![allow(clippy::missing_const_for_fn)]
// P2 remaining-nursery-lints: see decision doc.
#![allow(
    clippy::use_self,
    clippy::option_if_let_else,
    clippy::redundant_pub_crate
)]

//! # oneshim-storage

pub mod error;
pub use error::StorageError;

pub mod encryption;
pub mod env_secret_store;
pub mod file_secret_store;
pub mod file_transport;
pub mod frame_storage;
pub mod integration_state_store;
pub mod keychain;
pub mod migration;
pub mod process_env_projection;
pub mod regime_manager_state_store;
pub mod sqlite;
pub mod sync_extractor;
pub mod sync_merger;
pub mod temp_file_projection;
