mod events;
pub(crate) mod health;
mod helpers;
mod intelligence;
mod monitor;
mod network;
#[cfg(feature = "server")]
pub(crate) mod suggestions;
mod sync;
mod system;

// ── Public re-exports ────────────────────────────────────────────────
pub(crate) use helpers::record_to_segment_summary;
