//! **Stub** — Task 8 replaces this with the real OTLP pipeline.
//!
//! The type aliases and function signatures here are pinned by `mod.rs`; Task 8
//! must preserve them (`Inner::apply`, `OtelLayer`, `build_initial_handle`).
//! Running any feature-on test against this stub will hit `unimplemented!()`;
//! no feature-on test exists yet — that is by design for Task 7.

use crate::telemetry::{Handle, Layer};
use oneshim_core::config::TelemetryConfig;

pub(super) struct Inner;

// Cheap stand-in satisfying `Layer<Registry>` until the real OtelLayer lands.
// The real alias in Task 8 becomes
// `tracing_opentelemetry::OpenTelemetryLayer<Registry, opentelemetry_sdk::trace::Tracer>`.
pub(super) type OtelLayer = tracing_subscriber::fmt::Layer<tracing_subscriber::Registry>;

impl Inner {
    pub(super) fn apply(&mut self, _cfg: &TelemetryConfig) -> anyhow::Result<()> {
        Ok(())
    }
}

pub(super) fn build_initial_handle(
    _cfg: &TelemetryConfig,
    _data_dir: &std::path::Path,
) -> anyhow::Result<(Layer, Handle)> {
    unimplemented!("Task 8 wires the real pipeline; Task 7 only scaffolds the feature gate")
}
