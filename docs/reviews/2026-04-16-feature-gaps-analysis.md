# Feature Gap Analysis — client-rust

_Analysis date: 2026-04-16_
_Baseline: `fix/pr422-followups` branch (PR #423 pending merge) = `origin/main` + PR #423's 5 commits._
_Scope: functional gaps across 14 workspace crates, assuming P2 tech-debt items are resolved._
_Method: 3 parallel Explore agents (sensor / intelligence / plumbing+app layers) + TODO/stub scan._

## Executive Summary

- **3 Critical** gaps (end-to-end flows broken): feedback→learning, coaching trigger, regime lifecycle.
- **2 Critical** platform/integration gaps: updater panic on unknown platform, AWS SigV4 missing in 3 AI clients.
- **13 Degraded** paths (feature works but diminished): OCR fallback, STT fallback, circuit breaker coverage, PII propagation, etc.
- **6 Cross-domain disconnects**: config propagation, telemetry export, feedback→learning, shutdown integrity, presenter→feedback, regime persistence.
- **5 Polish** items (tests, stubs, UI incompleteness).

Recommended sequencing: Critical → Cross-domain A/B (config/telemetry) → Degraded (user-facing first) → Polish (merge with P2 tech-debt plan).

---

## 🔴 Critical Gaps

### C1. Feedback → Learning loop disconnected

- **File**: `crates/oneshim-suggestion/src/feedback.rs:45-70`, `feedback_retry.rs` (queue exists but unwired)
- **Symptom**: `FeedbackSender::send_feedback()` is send-and-forget. If the API call fails, no retry is queued automatically. `FeedbackRetryQueue` is a standalone type with no background poller.
- **Secondary symptom**: Feedback (accept / reject / dismiss) does NOT flow back into `oneshim-analysis::CoachingEngine` or `RegimeClassifier`. No online learning signal.
- **Impact**: User reactions are lost on network blips; suggestions never adapt to feedback.
- **Work required**:
  1. Wire `FeedbackSender` to enqueue into `FeedbackRetryQueue` on failure.
  2. Add scheduler loop calling `FeedbackRetryQueue::collect_ready()` → retry.
  3. Route accepted/rejected feedback metadata into coaching/regime modules via a new port (e.g., `FeedbackSignalSink`).
- **Effort**: ~1 developer-week.

### C2. CoachingEngine never invoked in production

- **File**: `crates/oneshim-analysis/src/coaching_engine/mod.rs:1-100`
- **Symptom**: `CoachingEngine::evaluate()` is fully implemented (triggers + guards + templates) but no scheduler or event handler calls it. Quiet-hours, cooldown, snoozed-profile guards are all dead code.
- **Impact**: Proactive coaching (the core Superpowers S5 feature) does not deliver messages to users. MagicOverlay is a ghost surface.
- **Work required**:
  1. Add a scheduler loop in `src-tauri/src/scheduler/loops/` that periodically calls `CoachingEngine::evaluate(current_context)`.
  2. Route the returned `CoachingMessage` to `MagicOverlayDriver` (Tauri WebView bridge).
  3. Integration test from context change → evaluate → overlay render.
- **Effort**: ~1 developer-week.

### C3. Regime lifecycle partial

- **Files**:
  - `crates/oneshim-analysis/src/regime_manager.rs` — deactivation rules defined but never executed
  - `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs:132, 388-390` — `regime_id` filter warns "not yet implemented, ignoring" in both `search_filtered` and `search_quantized`
  - No persistence — `RegimeManager` is in-memory only; restart wipes state
- **Impact**:
  - Regime-scoped vector search returns results from unrelated regimes (filter silently ignored).
  - Regimes accumulate indefinitely across user sessions (no deactivation/archival).
  - Fresh start means re-clustering all embeddings (expensive, user-visible latency).
- **Work required**:
  1. Implement `regime_id` filter in both `search_filtered` and `search_quantized` (SQL `WHERE regime_id = ?`).
  2. Add a scheduler tick calling `RegimeManager::enforce_lifecycle_rules(now)` — walks regimes, marks inactive, archives by rule.
  3. Persistence: extend `RegimeStorage` port with `save_manager_state` / `load_manager_state`; call at startup/shutdown.
- **Effort**: ~2 developer-weeks.

### C4. Updater panics on unknown platform

- **File**: `src-tauri/src/updater/state.rs:1342` (per audit; verify line)
- **Symptom**: Platform detection `match` has no default arm — new OS/arch combinations abort the updater task.
- **Impact**: Future platform support (e.g., Linux arm64) crashes the running binary when an update is fetched.
- **Work required**: Add a default arm returning `Err(UpdaterError::UnsupportedPlatform { os, arch })` + graceful degradation (skip update, log warning).
- **Effort**: ~0.5 day.

### C5. AWS Signature V4 missing in 3 AI client paths — **RESOLVED 2026-04-19 (intentional non-support)**

**Resolution**: "intentionally unsupported" path chosen via [ADR-019](../architecture/ADR-019-error-code-infrastructure.md) §3. AWS Bedrock is no longer advertised. Shipped on branch `feature/error-code-phase1`:

- Catalog: `specs/providers/provider-surface-catalog.json` no longer has `bedrock` vendor or `provider_surface.bedrock.direct_api` surface.
- 8 match arms in `oneshim-network` + 2 defense-in-depth guards (`analysis_client::analyze`, `ai_model_catalog_web_service::list_models`) return `CoreError::Config { code: ConfigCode::UnsupportedProviderBedrock, message: "AWS Bedrock is intentionally unsupported in this build" }`. Arm count bumped during post-merge drift audit: `http_api_session::build_request_body` now has an explicit `BedrockConverse` arm (previously collapsed into a generic `InternalCode::Generic` wildcard).
- OCR `apply_auth_headers` signature: infallible → `Result<_, CoreError>` — closes silent no-auth fallthrough security bug on `AwsSignatureV4` auth scheme.
- Frontend `ProviderWizard.tsx` removed Amazon Bedrock card.
- Regression guard: `feature_capabilities::bedrock_surface_removed_per_adr_019` test asserts absence.
- Re-introduction gated by ADR-019 §5 8-step checklist (SigV4 signing impl, credential loader, UI fields, catalog re-entry, live smoke test).

Enum variants `AiProviderType::Bedrock`, `ProviderAuthScheme::AwsSignatureV4`, `ProviderRequestShape::BedrockConverse` retained as runtime-unreachable for minimal-churn future re-introduction.

**Original analysis** (preserved):

- **Files** (original markers, all retrofitted during ADR-019 Phase 3):
  - `crates/oneshim-network/src/ai_ocr_client/mod.rs` — OCR auth arm
  - `crates/oneshim-network/src/ai_llm_client/request.rs` — LLM request/auth/response arms
  - `crates/oneshim-network/src/http_api_session/mod.rs` — Session auth arm
- **Impact** (pre-resolution): AWS Bedrock (LLM), AWS Textract (OCR), AWS API Gateway with IAM (session) all unusable.
- **Decision** (resolved): "intentionally unsupported" chosen — current user base has no AWS Bedrock / Textract requirement; avoids 1.5 weeks of SigV4 engineering.
- **Work required if documenting** (as forecast): ~0.5 day. Actual shipped size: 24 commits across Phase 1-4 of the broader error-code infrastructure that ADR-019 bundled with C5.

---

## 🟡 Degraded Paths

### Sensor layer

| # | Symptom | Location | Impact | Mitigation |
|---|---------|----------|--------|-----------|
| D1 | OCR disabled without `ocr` feature | `oneshim-vision/src/ocr.rs`, gated by `ocr` feature | OCR-dependent analysis silently returns empty | Add explicit "OCR unavailable" log + optional cloud OCR fallback |
| D2 | Linux AT-SPI2 needs `linux-atspi` feature | `oneshim-vision/src/accessibility/linux.rs` | Default Linux build has no accessibility tree | Enable `linux-atspi` in default features on Linux target (Cargo cfg) |
| D3 | STT needs `whisper` OR `cloud-stt` | `oneshim-audio/src/whisper.rs` + `cloud_stt.rs` | Both features optional; without either, VAD only | Default to `whisper` on desktop; document fallback |
| D4 | Multi-monitor selection missing | `oneshim-vision/src/capture.rs` | Primary monitor hardcoded | Expose display-index parameter; document `ScreenCapture::capture_display(idx)` API |
| D5 | PII filter not uniformly applied | `oneshim-vision/src/privacy.rs` applied in `processor.rs:52`, not all OCR regions | Sensitive content may leak to analysis | Audit all text extraction paths; require `PiiFilterLevel` gate at port boundary |
| D6 | Windows accessibility has no tests | `oneshim-vision/src/accessibility/windows.rs` (997 LOC COM) | COM regressions undetected | Add integration tests behind a Windows CI matrix |

### Network + storage

| # | Symptom | Location | Impact | Mitigation |
|---|---------|----------|--------|-----------|
| D7 | Circuit breaker only on batch upload | `oneshim-network/src/batch_uploader.rs` | LLM/OCR/API call storms can cascade | Extract shared `CircuitBreaker` component; apply at `ApiClient`, LLM, OCR call sites |
| D8 | 6 storage modules untested | `process_env_projection`, `sync_merger`, `sync_extractor`, `temp_file_projection`, `encryption`, various migrations | Silent data-transform bugs | Add unit tests per module (~2 weeks total) |
| D9 | Updater: SHA256 only | `src-tauri/src/updater/install.rs` | No signature verification; a SHA collision or MITM swap is undetectable | Add Ed25519/minisign verification using published public key |
| D10 | No staged rollout in updater | `src-tauri/src/updater/mod.rs` | Single bad release affects all users simultaneously | Add percentage-based rollout or cohort pinning |
| D11 | No health-check rollback | `src-tauri/src/updater/install.rs` | Bad updates stick until user manually intervenes | Post-install health probe; if failed, restore previous binary |

### Intelligence

| # | Symptom | Location | Impact | Mitigation |
|---|---------|----------|--------|-----------|
| D12 | Embedding stub mode silently breaks downstream | `oneshim-embedding/src/lib.rs:237-308` | Without `fastembed-local`, clustering/search fail with bare errors | Detect stub via `EmbeddingProvider::is_stub()` probe; skip clustering features |

### Web + app

| # | Symptom | Location | Impact | Mitigation |
|---|---------|----------|--------|-----------|
| D13 | gRPC endpoints defined but not exposed | `oneshim-network/src/grpc/*` adapters exist; `oneshim-web` Axum is REST-only | gRPC clients (future server-to-server) cannot talk to the app | Add a gRPC server layer or document as deliberate (REST-only) |

---

## 🔀 Cross-Domain Disconnects

| # | Path | Broken Because | Fix Effort |
|---|------|----------------|------------|
| X1 | Settings change → consumer crates | No broadcast mechanism for config/consent updates; crates cache independently | Introduce `ConfigChangeBus` (tokio broadcast) in `oneshim-core`; all consumers subscribe. **~1 week** |
| X2 | Telemetry config → exporter | `TelemetryConfig` defined, no OpenTelemetry/Prometheus integration | Wire `tracing-opentelemetry` + OTLP exporter gated on `telemetry` feature. **~1 week** |
| X3 | Feedback → Coaching/Regime | No callback path from accept/reject events into analysis crates | Add `FeedbackSignalSink` port; wire from feedback queue to `CoachingEngine::record_user_reaction()`. Combined with C1. **included in C1** |
| X4 | Shutdown → DB integrity | `LifecycleManager` signals but doesn't checkpoint SQLite WAL | In `src-tauri/src/lifecycle.rs` shutdown handler, call `conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")` before joining. **~0.5 day** |
| X5 | Presenter → feedback receiver | `SuggestionView` renders; no channel back to log user reactions | Accept/reject buttons in presenter call `FeedbackSender::send_feedback`. Simpler than it sounds; likely exists in src-tauri but not traced. **~1 day** |
| X6 | Regime persistence | `RegimeManager` ephemeral; SQLite schema `regimes` table exists but `RegimeManager` doesn't use it | Extend `RegimeStorage` port; wire at startup/shutdown. Included in C3. **included in C3** |

---

## 🟢 Polish Items

| # | Item | Location | Note |
|---|------|----------|------|
| P1 | Intent planner real-path tests absent | `oneshim-automation/src/intent_planner.rs` | Currently only stub tests |
| P2 | `oneshim-api-contracts` zero unit test | crate-wide | DTOs only, but `provider_specs/resolvers.rs` has logic |
| P3 | OAuth test uses `panic!()` assertions | `oneshim-network/src/oauth/**` | Brittle tests |
| P4 | "Coming soon" in ProviderWizard UI | `frontend/src/pages/setting-tabs/ProviderWizard.tsx:406` | New-provider setup incomplete |
| P5 | Fresh frontend e2e coverage light | `crates/oneshim-web/frontend/e2e/*.spec.ts` | Add coverage for new pages |

---

## Dependencies Between Items

```
C1 (feedback retry)  ──┬── needs ──> X3 (feedback signal sink)
C2 (coaching trigger) ─┤
C3 (regime lifecycle) ─┴── needs ──> X6 (regime persistence)

C1, C2, C3 all benefit from X1 (config propagation) landing first.

D7 (circuit breaker) is a refactor that should precede C5 (AWS SigV4) — to avoid
  introducing new uncovered outbound call sites.

X4 (WAL checkpoint) is trivial and should land with any other src-tauri touch.
```

---

## Recommended Execution Order

### Phase 1 — Quick wins (1 week total)

- **C4** Updater platform panic — 0.5 day
- **X4** WAL checkpoint on shutdown — 0.5 day
- **X5** Presenter → feedback wiring audit — 1 day
- **D2** Enable `linux-atspi` in default Linux feature flags — 0.5 day
- **D4** Multi-monitor selection API — 2 days

### Phase 2 — Cross-domain plumbing (2 weeks)

- **X1** ConfigChangeBus — 1 week
- **X2** Telemetry exporter wiring — 1 week

### Phase 3 — Core loops (4 weeks)

- **C1 + X3** Feedback retry + signal sink — 1.5 weeks
- **C2** Coaching scheduler + overlay path — 1 week
- **C3 + X6** Regime lifecycle + persistence + id filter — 2 weeks

### Phase 4 — Provider / platform polish (2-3 weeks)

- **C5** AWS SigV4 — decision first, then 1.5 weeks or 0.5 day
- **D7** Circuit breaker broadening — 1 week
- **D9/D10/D11** Updater hardening — 1 week
- **D13** gRPC server exposure (if needed) — 1 week

### Phase 5 — Test backfill + polish (2-3 weeks)

- **D8** 6 untested storage modules — 2 weeks (can parallel with other work)
- **D5** PII filter audit across text paths — 1 week
- **D6** Windows accessibility tests — 1 week
- **P1-P5** As fits

**Total calendar**: ~12-14 weeks of focused work across all phases. Phases 1 and 2 are standalone; 3 is the heaviest; 4 can run parallel to 3 for different contributors.

---

## Next Session Kickoff Checklist

When resuming this analysis in a new session:

1. **Re-verify against latest main** — this doc is baselined on `fix/pr422-followups`. If PRs #423 / #414 / #407 merged with changes, some line numbers and status tags may have moved.
2. **Confirm severity** — the severity tags (Critical / Degraded / Polish) are author's judgment; user may reclassify.
3. **Decide scope** — the full plan is 12-14 weeks. The user may want to pick a subset (e.g., just Phase 1 + Phase 3).
4. **Commit decision** — should these docs be committed to the repo under `docs/reviews/` or stay in `.claude/plans/`?
5. **Integration with P2 tech-debt plan** — `.claude/plans/p2-tech-debt-spec.md` + `-plan.md` is a parallel effort. Decide whether to execute P2 and feature gaps concurrently or in sequence.

## References

- 3-agent parallel Explore reports (sensor / intelligence / plumbing layers) synthesized above.
- `.claude/plans/p2-tech-debt-spec.md` and `-plan.md` — related tech-debt work.
- Memory `reference_feature_audit.md` (2026-04-13) — 3-day-old baseline that this analysis supersedes.
