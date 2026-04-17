//! OTLP span pipeline. Feature-gated at the `mod.rs` declaration.
//!
//! - `build_initial_handle(cfg, data_dir) -> Result<(Layer, Handle)>`
//!   Builds the pipeline ONCE when `cfg.enabled == true`, wraps the layer in
//!   `reload::Layer`, stashes the `TracerProvider` in `Inner.active` so we
//!   can shut it down on toggle-off.
//! - `Inner::apply(cfg)` drives off↔on transitions in place.
//! - `resolve_endpoint(cfg)` implements the precedence from spec §3.2.

use crate::telemetry::{Handle, Layer};
use oneshim_core::config::TelemetryConfig;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{self as sdktrace, TracerProvider};
use opentelemetry_sdk::Resource;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{reload, Registry};

pub(super) type OtelLayer = OpenTelemetryLayer<Registry, sdktrace::Tracer>;

pub(super) struct Inner {
    /// Handle to swap the `Option<OtelLayer>` baked into the subscriber at init.
    reload_handle: reload::Handle<Option<OtelLayer>, Registry>,
    /// Currently-active provider. `None` when disabled. `shutdown()` on
    /// toggle-off; rebuilt via `build_pipeline` on toggle-on.
    active: Option<TracerProvider>,
    /// Last config we applied. Used to detect transitions and avoid redundant work.
    last_cfg: TelemetryConfig,
    /// Captured from boot so the off→on transition can regenerate the pipeline
    /// (including `service.instance.id` — Task 9) without re-plumbing it in.
    data_dir: std::path::PathBuf,
}

impl Inner {
    pub(super) fn apply(&mut self, cfg: &TelemetryConfig) -> anyhow::Result<()> {
        match (self.last_cfg.enabled, cfg.enabled) {
            (false, true) => {
                // off -> on: build a new pipeline and swap in.
                let (provider, layer) = build_pipeline(cfg, &self.data_dir)?;
                self.reload_handle
                    .modify(|opt| *opt = Some(layer))
                    .map_err(|e| anyhow::anyhow!("reload modify failed: {e:?}"))?;
                self.active = Some(provider);
            }
            (true, false) => {
                // on -> off: detach and shut down.
                self.reload_handle
                    .modify(|opt| *opt = None)
                    .map_err(|e| anyhow::anyhow!("reload modify failed: {e:?}"))?;
                if let Some(provider) = self.active.take() {
                    shutdown(provider);
                }
            }
            _ => {
                // No transition. Idempotent.
            }
        }
        self.last_cfg = cfg.clone();
        Ok(())
    }
}

pub(super) fn build_initial_handle(
    cfg: &TelemetryConfig,
    data_dir: &std::path::Path,
) -> anyhow::Result<(Layer, Handle)> {
    // Build the pipeline at most ONCE at boot. If enabled, the provider lives
    // in Inner.active so we can shut it down on toggle-off; the layer is moved
    // into the reload wrapper.
    let (initial_layer, active) = if cfg.enabled {
        let (provider, layer) = build_pipeline(cfg, data_dir)?;
        (Some(layer), Some(provider))
    } else {
        (None, None)
    };

    let (reload_layer, reload_handle) = reload::Layer::new(initial_layer);

    let handle = Handle {
        inner: parking_lot::Mutex::new(Inner {
            reload_handle,
            active,
            last_cfg: cfg.clone(),
            data_dir: data_dir.to_path_buf(),
        }),
    };

    Ok((reload_layer, handle))
}

fn build_pipeline(
    cfg: &TelemetryConfig,
    data_dir: &std::path::Path,
) -> anyhow::Result<(TracerProvider, OtelLayer)> {
    let endpoint = resolve_endpoint(cfg);
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()?;

    // Ensure the per-install UUID exists and attach it as `service.instance.id`.
    // Failure here is non-fatal for the overall pipeline — we still ship spans
    // without the instance attribute rather than refusing to export at all.
    let instance_id = super::instance_id::ensure_instance_id(data_dir)
        .map_err(|e| anyhow::anyhow!("telemetry_instance_id: {e}"))?;

    let resource = Resource::new(vec![
        KeyValue::new("service.name", cfg.service_name.clone()),
        KeyValue::new("service.instance.id", instance_id),
    ]);

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(resource)
        .build();

    use opentelemetry::trace::TracerProvider as _;
    let tracer = provider.tracer("oneshim");
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);
    Ok((provider, layer))
}

/// Endpoint resolution precedence (§3.2):
/// 1. Explicit `config.telemetry.otlp_endpoint` (Some).
/// 2. Env var `OTEL_EXPORTER_OTLP_ENDPOINT`.
/// 3. `http://localhost:4318` (OTLP/HTTP default).
pub(crate) fn resolve_endpoint(cfg: &TelemetryConfig) -> String {
    if let Some(ref explicit) = cfg.otlp_endpoint {
        return explicit.clone();
    }
    if let Ok(env) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        if !env.is_empty() {
            return env;
        }
    }
    "http://localhost:4318".to_string()
}

/// Run `provider.shutdown()` on a dedicated thread with a 4 s watchdog so a
/// wedged exporter (collector down, network partition) cannot block the app
/// on toggle-off or exit. Past the deadline we log a warning and proceed; the
/// SDK may retain a zombie I/O task but the caller is never blocked.
fn shutdown(provider: TracerProvider) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = provider.shutdown();
        let _ = tx.send(());
    });
    if rx.recv_timeout(std::time::Duration::from_secs(4)).is_err() {
        tracing::warn!("otel provider shutdown exceeded 4s; continuing without waiting");
    }
}
