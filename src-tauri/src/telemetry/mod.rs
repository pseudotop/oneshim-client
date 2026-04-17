//! Telemetry bootstrap.
//!
//! - **Feature-off (`default`)**: zero-cost no-op layer and handle.
//! - **Feature-on (`telemetry`)**: OpenTelemetry OTLP pipeline wrapped in
//!   `tracing_subscriber::reload::Layer<Option<OtelLayer>, _>` so runtime
//!   toggles (`telemetry.enabled`) can attach and detach the layer without
//!   restarting the process.
//!
//! See `docs/guides/telemetry.md` (ships with Task 12) and
//! `docs/reviews/2026-04-17-phase2-config-telemetry-spec.md` §3 for design.

use oneshim_core::config::TelemetryConfig;

#[cfg(feature = "telemetry")]
mod instance_id;
#[cfg(feature = "telemetry")]
mod otlp;

#[cfg(all(test, feature = "telemetry"))]
mod mock_otlp;

/// Public handle for runtime toggle. Construct via [`Handle::new_with_layer`].
pub struct Handle {
    #[cfg(feature = "telemetry")]
    inner: parking_lot::Mutex<otlp::Inner>,
}

/// Layer attached to the tracing subscriber. Type alias keeps the `.with()`
/// call-site monomorphic across feature states.
#[cfg(feature = "telemetry")]
pub type Layer =
    tracing_subscriber::reload::Layer<Option<otlp::OtelLayer>, tracing_subscriber::Registry>;

#[cfg(not(feature = "telemetry"))]
pub type Layer = NoopLayer;

/// No-op placeholder layer when the `telemetry` feature is off. Satisfies
/// `tracing_subscriber::Layer<S>` via an empty impl so `.with(layer)` compiles
/// under either feature state without `#[cfg]` at the call site.
#[cfg(not(feature = "telemetry"))]
#[derive(Clone, Copy, Default)]
pub struct NoopLayer;

#[cfg(not(feature = "telemetry"))]
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for NoopLayer {}

impl Handle {
    /// Build the layer and handle for the tracing subscriber.
    ///
    /// - **Feature-off**: infallible, `data_dir` ignored, returns a no-op layer.
    /// - **Feature-on + `cfg.enabled == false`**: returns a `reload::Layer`
    ///   wrapping `None`. No exporter built.
    /// - **Feature-on + `cfg.enabled == true`**: builds the OTLP pipeline,
    ///   reads/writes `telemetry_instance_id` under `data_dir`, wraps the
    ///   `OtelLayer` in the reload wrapper.
    ///
    /// Signature is identical across feature states so `main.rs` and tests can
    /// pass `data_dir` unconditionally.
    pub fn new_with_layer(
        _cfg: &TelemetryConfig,
        _data_dir: &std::path::Path,
    ) -> anyhow::Result<(Layer, Self)> {
        #[cfg(not(feature = "telemetry"))]
        {
            Ok((NoopLayer, Handle {}))
        }
        #[cfg(feature = "telemetry")]
        {
            otlp::build_initial_handle(_cfg, _data_dir)
        }
    }

    /// Apply a runtime toggle. Idempotent when `cfg` matches the last applied
    /// value.
    ///
    /// - **Feature-off**: always `Ok(())`.
    /// - **Feature-on**: drives the off↔on transitions inside the reload
    ///   wrapper, building or shutting down the OTLP provider as needed.
    pub fn apply(&self, _cfg: &TelemetryConfig) -> anyhow::Result<()> {
        #[cfg(not(feature = "telemetry"))]
        {
            Ok(())
        }
        #[cfg(feature = "telemetry")]
        {
            self.inner.lock().apply(_cfg)
        }
    }
}

#[cfg(all(test, not(feature = "telemetry")))]
mod tests {
    use super::*;

    /// T-X2-1 — feature-off construction is a no-op (no panic, no allocation).
    #[test]
    fn feature_off_construction_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TelemetryConfig {
            enabled: true,
            ..Default::default()
        };
        let (_layer, handle) = Handle::new_with_layer(&cfg, tmp.path())
            .expect("feature-off construction is infallible");
        handle
            .apply(&cfg)
            .expect("apply is a no-op when feature is off");
    }
}
