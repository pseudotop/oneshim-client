# D5 PII Audit Matrix — 44 text-extraction paths

**Date**: 2026-04-20
**Scope**: Client-rust workspace PII coverage audit (11-round Loop 1 deep review)
**Companion docs**: [Contract](../guides/pii-sanitization-contract.md) · [Spec](../superpowers/specs/2026-04-20-d5-pii-filter-audit-design.md) · [Plan](../superpowers/plans/2026-04-20-d5-pii-filter-audit.md)

## Legend

| Symbol | Meaning |
|--------|---------|
| ❌ Gap | Confirmed unsanitized boundary — fix required |
| ⚠️ Drift | Configuration/consistency issue (not total miss, but inconsistent) |
| ✅ Covered | Already sanitized (or covered transitively) |
| 🛡 Exempt | Documented exception (user-authored / intra-process / encrypted transport) |
| 📋 Defer | Deferred to v2 or follow-up PR |
| 🔥 Critical | Logic inversion / active leak — highest priority |

## Matrix

| # | Path | Round found | Verdict | Fix iter | File:line | Notes |
|---|------|-------------|---------|----------|-----------|-------|
| 1 | OCR result → SQLite `frames.ocr_text` | R1 | ❌ Gap | 3 | `src-tauri/src/scheduler/loops/helpers.rs:172` | Raw OCR text persisted; fix inserts `sanitize_title_with_level` before `save_frame_metadata_with_bounds` |
| 2 | Event batch upload (`Event.window_title`) | R1 | ⚠️ Test-lock | 14 | `src-tauri/src/scheduler/config.rs:275,283,291` | Sanitization exists at capture; missing regression test |
| 3 | OCR/context → LLM analysis request body | R1 | ✅ Covered | — | `oneshim-analysis::assembler.rs:9,207` | `PiiFilter` injected via `ContextAssembler::new` |
| 4 | Accessibility extractor text (macOS/Windows/Linux) | R2 | ⚠️ Drift | 7 | `.../accessibility/{macos/extractor.rs:443, windows.rs:693, linux.rs:327}` | macOS/Windows hardcoded `Basic`; Linux respects config — align per O6 |
| 5 | Suggestion persist (`Suggestion.content`/`reasoning`) | R1 | ❌ Gap | 5 | `oneshim-network::analysis_client.rs::candidate_to_suggestion` + `oneshim-suggestion::receiver.rs` | LLM response echoes can contain PII; sanitize at exit |
| 6 | Audio STT transcripts | R1 | ❌ Gap | 4 | `oneshim-audio::{whisper.rs, cloud_stt.rs}` | Zero sanitization in crate; add via `PiiSanitizer` port injection |
| 7 | Web dashboard responses | R1 | ✅ Covered | — | `src-tauri/src/web_server_runtime.rs:384` | `VisionPiiSanitizer` wired via `with_pii_sanitizer` |
| 8 | Audit log entries | R1 | ❌ Gap | 6 | `oneshim-automation::audit.rs::AuditLogger::record` | Apply `PiiFilterLevel::Strict` unconditionally per O3 |
| 9 | Sync payloads (cross-device) | R1 | 🛡 Exempt | — | `oneshim-storage::sync_extractor.rs` | E2E encrypted; receiver is user-owned device |
| 10 | `CoreError::Display` body + tracing user-text fields | R1 | ⚠️ Audit+fix | 16 | `oneshim-core/src/error.rs::impl Display for CoreError` + various `tracing::*` sites | Sanitize at Display boundary per O7; audit 20-ish format sites |
| 11 | `CoachingMessage.template_text` / `personalized_message` | R5 | ❌ Gap | 8 | `oneshim-analysis::coaching_engine/*` + `crates/oneshim-core/src/models/coaching.rs` | LLM/template can embed user context |
| 12 | `BugReport` user-composed | R5 | 🛡 Exempt | — | `oneshim-core/src/models/bug_report.rs` + `src-tauri/src/commands/bug_report.rs` | User intentionally authors; downstream uploader responsible |
| 13 | `DailyInsight.narrative` + `DigestHighlight.text` | R5 | ❌ Gap | 9 | `oneshim-analysis::daily_digest_generator.rs` | LLM-generated narrative quotes activity context |
| 14 | `FrameAnnotation.text` (user sticky notes) | R5 | 📋 Defer | — | `oneshim-core/src/models/annotation.rs` | User-authored, local-only; export flow audit deferred |
| 15 | 🔥 `ClipboardEvent.preview` (LOGIC INVERSION BUG) | R5 | 🔥 Critical | 2 | `oneshim-monitor/src/clipboard.rs:60-64` | `pii_level != Off` branch truncates WITHOUT sanitization; first 50 chars of any clipboard content (passwords, cards, addresses) leaks raw |
| 16 | `WorkSession.primary_app` | R5 | 📋 Defer | — | `oneshim-core/src/models/work_session.rs` | App names; edge cases like "MyTaxes.exe" low occurrence |
| 17 | Bug report telemetry | R6 | 🛡 Exempt | — | — | User authors bug description; respect intent |
| 18 | Export handlers (`/api/export/*`) | R6 | ⚠️ Defense-in-depth | 15 | `oneshim-web::handlers/export.rs` | Transitively safe IF storage is sanitized; add belt-and-suspenders |
| 19 | `ActivityPattern` mined descriptions | R6 | ✅ Transitive | — | `oneshim-analysis::pattern_miner/*` | Derived from events; safe IF Path 2 test locks |
| 20 | FTS search queries | R6 | 📋 Defer | — | `oneshim-storage::sqlite/fts_search_impl` | User-typed query; may contain PII — defer to frontend audit |
| 21 | Tauri IPC command inputs (broad audit) | R6 | 📋 Defer | — | `src-tauri/src/commands/*` | Too broad; defer to v2 |
| 22 | `tracing::info!/warn!/error!` fields | R6 | Linked | 16 | various | Handled together with Path 10 iter-16 |
| 23 | `KeystrokeEvent.key_code` | R7 | ⚠️ Verify | 17 | `oneshim-monitor::input_detail.rs` | Verify `key_code` is single-key identifier only |
| 24 | `KeyboardPatternTracker` | R7 | 📋 Defer | — | `oneshim-monitor::keyboard_pattern.rs` | Statistical-only; low risk |
| 25 | gRPC `UploadBatchRequest` serialization | R7 | ✅ Transitive | — | `oneshim-network::grpc/context_client.rs` | Covered IF Event sanitization is universal (Path 2 contract) |
| 26 | OAuth `redirect_uri` / state | R7 | ✅ Not a PII path | — | `oneshim-network::oauth/*` | Opaque tokens/URLs |
| 27 | `ChatMessage` history (AI session) | R7 | ❌ Gap | 10 | `oneshim-network::http_api_session::mod.rs` + `ai_sessions` table | Chat history persists to SQLite + potentially syncs |
| 28 | `FileAccessEvent.relative_path` / `extension` | R8 | ❌ Gap | 11 | `oneshim-monitor::file_access.rs:86` | Filename can contain PII ("Resume_JohnDoe.pdf"); `UserPath` marker exists but not applied |
| 29 | Integration transport egress (ERP/MES/CRM) | R8 | ❌ Gap | 12 | `oneshim-network::integration/policy_egress.rs::enqueue_insight` | High risk — enterprise integration target; zero sanitization |
| 30 | `ProcessEnvSecretProjection` | R8 | 🛡 Exempt | — | `oneshim-storage::process_env_projection.rs` | Secret-handling infrastructure (opposite direction from leak) |
| 31 | Session audit records | R8 | ✅ Transitive | — | `oneshim-storage::session_storage_impl.rs` | Covered via Path 27 iter-10 |
| 32 | `report_frontend_error` Tauri command | R9 | ❌ Gap | 13 | `src-tauri/src/commands/error_report.rs:236` | Frontend JS errors + stack can contain user input |
| 33 | Desktop notification body | R9 | 🛡 Exempt | — | `src-tauri/src/focus_analyzer/suggestions.rs:63,114,169,215` | User's own device; document risk if OS syncs notifications |
| 34 | `IntegrationRuntimeTelemetryHandle.record_success` | R9 | ✅ Not a PII path | — | `oneshim-network::integration/runtime_telemetry.rs` | Numeric counters only |
| 35 | `MagicOverlay` Tauri emits (~15 emit sites) | R10 | 📋 Defer | — | `src-tauri/src/magic_overlay.rs:196-552` | Intra-process IPC; low risk unless frontend leaks via logs |
| 36 | OAuth `redirect_uri` (duplicate) | R10 | ✅ Not a PII path | — | `oneshim-network::oauth/token_exchange.rs:39` | Opaque localhost URL |
| 37 | Model download URL logs | R10 | ✅ Not a PII path | — | `oneshim-audio::model_downloader.rs:91` | Vendor URLs |
| 38 | `ai_sessions` state persistence | R10 | ✅ Transitive | — | `oneshim-storage::session_storage_impl.rs:61` | Chat history — covered by Path 27 iter-10 |
| 39 | `regime_manager_state` JSON payload | R10 | ✅ Transitive | — | `oneshim-storage::regime_manager_state_store.rs:87` | Regime labels + stats — covered if events sanitized |
| 40 | Tauri commands (5 total) | R11 | ✅ Fully scoped | — | `src-tauri/src/commands/*` | All commands reviewed: Path 32 covered by iter-13; others no-PII |
| 41 | `ConsentRecord` (GDPR) | R11 | ✅ Not a PII path | — | `oneshim-core/src/consent.rs` | Timestamps + UUID only |
| 42 | `PlaybookSignal` / automation presets | R11 | 🛡 Exempt | — | `src-tauri/src/workflow_intelligence.rs:68` | User-authored local content |
| 43 | SQL query logging | R11 | ✅ Not a PII path | — | `oneshim-storage/src/*` | No query-body logging at warn/info level |
| 44 | Config persistence | R11 | ✅ Not a PII path | — | `oneshim-core::config_manager` | Secrets in separate `secret_store`; config files don't hold raw API keys |

## Summary

- **44 paths reviewed across 11 deep-review rounds**
- **1 🔥 Critical** (Path 15: Clipboard preview logic inversion)
- **~10 ❌ Gaps** requiring code fixes (Paths 1, 5, 6, 8, 11, 13, 27, 28, 29, 32)
- **~3 ⚠️ Drift/Defense-in-depth** (Paths 2, 4, 10, 18)
- **~5 🛡 Exemptions** (Paths 9, 12, 17, 30, 33, 42)
- **~10 ✅ Covered/Transitive/Not-PII** (Paths 3, 7, 19, 25, 26, 31, 34, 36, 37, 38, 39, 40, 41, 43, 44)
- **~5 📋 Deferred to v2** (Paths 14, 16, 20, 21, 24, 35)

## Migration iter mapping

Each ❌ Gap, 🔥 Critical, ⚠️ Drift fix has a corresponding iter in the [D5 implementation plan](../superpowers/plans/2026-04-20-d5-pii-filter-audit.md):

- iter-1: Contract doc + this matrix
- iter-2: 🔥 Path 15 (Critical)
- iter-3: Path 1
- iter-4: Path 6
- iter-5: Path 5
- iter-6: Path 8
- iter-7: Path 4
- iter-8: Path 11
- iter-9: Path 13
- iter-10: Path 27
- iter-11: Path 28
- iter-12: Path 29
- iter-13: Path 32
- iter-14: Path 2 (regression lock)
- iter-15: Path 18 (defense-in-depth)
- iter-16: Paths 10 + 22 (error Display + tracing)
- iter-17: Path 23 (KeystrokeEvent verify)
- iter-18: Docs + PR open

## Maintenance

This matrix is the source-of-truth for "which text paths are audited". When a new text-producing adapter or boundary is added to the workspace:

1. Add a row to this matrix with initial verdict
2. Link to the contract doc for the resolution pattern
3. Mark the fix iter (or exemption) per the D5 pattern
