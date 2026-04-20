# ADR-019 Follow-up #2 — Grafana Dashboard Relabeling Design

**Date:** 2026-04-20
**Status:** 🟡 Rust-side ✅ SHIPPED (iter-206/208); ops-side external (not client-rust scope)
**Scope:** External observability infrastructure (Grafana dashboards, Loki log label pipeline); no Rust source change.
**Origin:** ADR-019 §Known follow-ups #2 — "Grafana dashboard relabeling"
**Parent ADR:** [ADR-019](../architecture/ADR-019-error-code-infrastructure.md)
**Target version:** N/A (ops-side follow-up; independent of client release cadence)

> **Rust-side actually changed:** the design framed this as "no Rust source change." In execution we added **16 scheduler-loop tracing emission sites** that carry `err.code = %e.code()` as a structured tracing field (intelligence 3 / events 4 / monitor 2 / network 6 / sync 1). `tracing-opentelemetry` bridges this into OTel span attributes automatically — ops can consume either channel. CLAUDE.md §Coding Conventions documents the canonical pattern + adapter-error conversion recipe (`let core: CoreError = e.into();`). `CoreError` Display keeps `[code]` embedded so the original Loki regex extraction is preserved as a fallback. Remaining work (Loki pipeline stage config + panel migration + alert-rule audit) lives in the ops infra repo.

## Context

Pre-ADR-019, the client emitted errors as free-form Display strings. Grafana panels that tracked error volume by category used substring regex matching on the log body:

```
{app="oneshim-client"} |= "Error" | regex `(Config|Network|Storage).*error`
```

This is brittle (log format changes break the pattern), slow (regex on every log line), and imprecise (ambiguous matches when one error wraps another).

Post-ADR-019, every error log line includes a stable `[code]` marker in the Display prefix (e.g., `Configuration error [config.invalid]: bad value`). The client can further annotate structured logs with a dedicated `err.code` field so Grafana/Loki/Prometheus can use it as a first-class label.

## Goal

Replace Grafana's Display-regex panels with `err.code` label group-by queries, so the observability pipeline sees the stable wire contract.

## Decision

### 1. Rust side: emit `err.code()` as a structured field in tracing macros

Where `CoreError` is logged via `tracing`, add `err.code = %e.code()` as a structured field. Pattern:

```rust
// Before
tracing::error!("failed to save: {}", err);

// After
tracing::error!(err.code = %err.code(), "failed to save: {}", err);
```

Locations to update (grep target):

```bash
rg -n 'tracing::(error|warn)!.*\{.*err' src-tauri/src/ crates/ | rg -v 'err\.code'
```

Estimated 150-200 emit sites. Not all need updating — only the "user-visible failure" categories (commands, adapter boundaries, sync pipelines).

### 2. OpenTelemetry side (if `--features telemetry`): emit as span attribute

Inside `TelemetryHandle` exported spans, add `error.code` as an OTel attribute following the [OpenTelemetry semantic convention](https://opentelemetry.io/docs/specs/semconv/exceptions/exceptions-spans/):

```rust
span.set_attribute("error.code", err.code().to_string());
```

### 3. Server/Loki side: promote `err.code` to an indexed label

In the Loki relabeling config (`loki-config.yaml` or equivalent):

```yaml
pipeline_stages:
  - regex:
      expression: '\[(?P<err_code>[a-z_.]+)\]'
  - labels:
      err_code:
```

This captures the `[code]` bracket from the Display string and promotes it to an indexed label. Works for BOTH tracing-emitted fields (via JSON log format) AND any historical Display-only logs.

### 4. Grafana panel migration

For each dashboard panel that currently matches on error strings:

```logql
# Before
{app="oneshim-client"} |= "Configuration error"

# After
{app="oneshim-client", err_code=~"config\\..*"}
```

Group-by use cases:

```logql
# Top N error codes in the last hour
sum by (err_code) (
  rate({app="oneshim-client", err_code!=""}[1h])
)
```

### 5. Test strategy

**Snapshot test (Rust side)**: verify tracing emissions include `err.code` for a representative set of errors:

```rust
#[tokio::test]
async fn tracing_log_includes_err_code() {
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::ERROR)
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        let err = CoreError::Network { code: NetworkCode::Timeout, message: "t/o".into() };
        tracing::error!(err.code = %err.code(), "test");
    });
    // Assert JSON output includes "err.code":"network.timeout"
}
```

**Observability smoke (ops-side)**: after rolling out, verify Grafana shows non-zero `err_code=~"internal\\..*"` volume (the fallback category) as a canary that the pipeline works.

## Consequences

### Positive
- 10-100× faster Grafana queries on error volumes (indexed label vs regex scan).
- Stable alerting contracts — alert rules keyed on `err_code` won't break when Display messages are reworded.
- Aligns with OpenTelemetry semantic conventions for error attributes.

### Negative
- Loki pipeline config change requires ops coordination (staged rollout: add label → migrate panels → remove old regex matches).
- Existing alert rules need review — any that regex-match on error string bodies must be rewritten.

### Neutral
- Historical logs (pre-rollout) will still work via the regex fallback in the Loki pipeline stage.

## Out of Scope

- Prometheus metrics (they already have structured labels; no change needed).
- Error alerting runbook rewrite — covered in a separate ops doc.

## Implementation Plan

Ops-heavy, doesn't fit the "Rust PR" pattern:

- **Rust side** (1 small PR): add `err.code` structured field at ~20-30 high-value emit sites + unit test. ~2-3 hours.
- **Loki config** (external repo): PR against `ops/loki-config.yaml`. ~1 hour.
- **Grafana dashboard migration**: 5-10 panels per dashboard × 3 dashboards = ~0.5 day spread across 2-3 sessions to avoid dashboard downtime.
- **Alert rule audit**: 20-30 minutes per week for 2 weeks after rollout, watching for misfires.

**Total effort estimate:** ~0.5 day Rust + ~0.5 day ops = ~1 day elapsed, staged over ~1 week to allow observation.
