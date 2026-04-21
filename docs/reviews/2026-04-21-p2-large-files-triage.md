# P2 Tech-Debt: Large File Triage (>500 LOC)

**Date**: 2026-04-21
**Scope**: Classify Rust + frontend files >500 LOC as `keep` / `maybe-split` / `must-split` per SOLID violation. Output is a decision document, not a split plan.
**Spec ref**: [`docs/reviews/2026-04-16-p2-tech-debt-brief.md`](2026-04-16-p2-tech-debt-brief.md) §Item 3
**Policy ref**: `feedback_file_split_policy.md` (auto-memory) — "500-line split is over-engineering unless there's a SOLID violation."

## TL;DR

**1 must-split** (`app_runtime_launch.rs` — 1068 code lines, zero tests, orchestrates 20+ subsystems). **2 maybe-split** (`sqlite/web_storage_impl.rs`, `updater/install.rs` — both 900+ code lines). **Everything else: keep.** Most "large" Rust files are inflated by inline `#[cfg(test)]` modules (often 40-80% of total LOC) which correctly co-locate tests with code per [ADR-001 §5](../architecture/ADR-001-rust-client-architecture-patterns.md). Frontend triage: keep `api/{contracts,client,standalone}.ts` (generated / API contract) + `stories/mock-data.ts` (fixture); `useSettingsForm.ts` is the only real must-split candidate.

## Why line count alone is misleading

| File | Total | Test LOC | Non-test LOC |
|------|-------|----------|--------------|
| `src-tauri/src/updater/mod.rs` | 1722 | 1366 (79%) | **356** |
| `crates/oneshim-analysis/src/adaptive_search.rs` | 1170 | 798 (68%) | 372 |
| `crates/oneshim-analysis/src/coaching_engine/mod.rs` | 1201 | 649 (54%) | 552 |
| `crates/oneshim-storage/src/sqlite/maintenance.rs` | 1426 | 567 (40%) | 859 |
| `crates/oneshim-vision/src/privacy.rs` | 1273 | 557 (44%) | 716 |
| `crates/oneshim-storage/src/frame_storage.rs` | 1167 | 332 (28%) | 835 |
| `src-tauri/src/app_runtime_launch.rs` | **1068** | 0 | **1068** |

`updater/mod.rs` leading the "largest files" list is misleading — it's actually 356 LoC of code with a massive test suite. The real maintenance burden ranking is by **non-test LoC**.

## Rust triage (top 20 non-test LOC, excluding `tests/` dirs)

| Non-test LOC | File | Verdict | Rationale |
|--------------|------|---------|-----------|
| **1068** | `src-tauri/src/app_runtime_launch.rs` | **🔴 must-split** | Zero tests, orchestrates 20+ subsystems in `build_and_spawn()`: health probe, core resources, server context, web server, gRPC dashboard, scheduler loops, magic overlay, update coordinator, etc. Clear SOLID-S violation — this function knows about every runtime subsystem. Candidates for extraction: `HealthProbeBoot`, `WebServerLaunchPhase`, `SchedulerBootPhase`. |
| **964** | `src-tauri/src/updater/install.rs` | 🟡 maybe-split | Install pipeline with multiple phases (download, verify, stage, swap, rollback). Currently cohesive as a pipeline but testing individual phases is awkward. If a new phase is added, consider extracting by phase (signature verification, staging, atomic swap). |
| **946** | `crates/oneshim-storage/src/sqlite/web_storage_impl.rs` | 🟡 maybe-split | Large handler dispatch — many REST-endpoint-backing impls in one file. Growing adds friction. Split-by-endpoint-group would work: `frames_impl.rs`, `sessions_impl.rs`, `focus_impl.rs`. Defer unless churn increases. |
| **859** | `crates/oneshim-storage/src/sqlite/maintenance.rs` | ✅ keep | Single responsibility: DB maintenance (vacuum, checkpoint, retention). Large due to SQL + retry logic; splitting by verb (checkpoint vs vacuum) would scatter transactional context. |
| **835** | `crates/oneshim-storage/src/frame_storage.rs` | ✅ keep | Single responsibility: frame image file storage + buffer pool + parallel I/O. Cohesive. |
| **834** | `crates/oneshim-vision/src/accessibility/windows.rs` | ✅ keep | Windows UIA FFI adapter. Single responsibility, hard to split without leaking Win32 types across module boundaries. Already gated by `#[cfg(target_os = "windows")]`. |
| **716** | `crates/oneshim-vision/src/privacy.rs` | ✅ keep | Regex-heavy PII sanitizer. Single responsibility, splitting by pattern-type would scatter the level cascade logic. |
| **666** | `crates/oneshim-network/src/local_llm_session.rs` | ✅ keep | Local LLM ChatSession impl — single stateful session type. Splitting by provider would break the shared request/response/streaming logic. |
| **636** | `crates/oneshim-core/src/models/intent.rs` | ✅ keep | Domain models (enums + structs). Splitting models by shape is premature abstraction. |
| **552** | `crates/oneshim-analysis/src/coaching_engine/mod.rs` | ✅ keep | Already a [directory module (ADR-003)](../architecture/ADR-003-directory-module-pattern.md) with `guards.rs` + `triggers.rs`. Further split would be YAGNI. |
| **517** | `src-tauri/src/update_coordinator.rs` | ✅ keep | Update control plane — single responsibility (coordinator state machine). Splitting by state would obscure the transition logic. |
| **506** | `src-tauri/src/scheduler/loops/monitor.rs` | ✅ keep | Already guarded by the 500-LOC lefthook hook + forcing helpers per the [architecture guardrail](../../CLAUDE.md#monitor-loop-complexity). |
| **500-600** | Various `handlers/*.rs`, `updater/*.rs`, `suggestion/*.rs` | ✅ keep | Mostly domain-scoped — handler-per-domain or policy-per-surface patterns already split at the correct boundary. |

## Frontend triage (>500 LOC TypeScript/TSX)

| LOC | File | Verdict | Rationale |
|-----|------|---------|-----------|
| **1743** | `frontend/src/api/contracts.ts` | ✅ keep | Generated from `http-interface-manifest.v1.json` — editing is contract-level. |
| **1235** | `frontend/src/api/client.ts` | ✅ keep | Thin API client; one source of truth for fetch wrappers. |
| **1219** | `frontend/src/api/standalone.ts` | ✅ keep | Mirrors `client.ts` for standalone deployment. |
| **984** | `frontend/src/pages/hooks/useSettingsForm.ts` | 🔴 must-split | SettingsForm hook managing 10+ independent concern groups (network, privacy, telemetry, AI, etc.). Classic SOLID-S violation — every new setting adds to a single reducer. Split by concern group (`useNetworkSettings`, `usePrivacySettings`, …). |
| **758** | `frontend/src/pages/setting-tabs/ai-automation/index.tsx` | 🟡 maybe-split | Multi-section AI/automation settings page. Growing — split by section when it hits ~1000. |
| **607** | `frontend/src/pages/Onboarding.tsx` | ✅ keep | Step-by-step onboarding wizard; linear flow doesn't benefit from split. |
| **606** | `frontend/src/pages/timeline/AllFrames.tsx` | ✅ keep | Timeline page with filter/sort/render responsibilities — already uses extracted hooks. |
| **589** | `frontend/src/pages/chat/index.tsx` | ✅ keep | Single-page chat UI; splitting would fragment state. |
| **566** | `frontend/src/stories/mock-data.ts` | ✅ keep | Storybook fixture data; expected to be large. |
| **548** | `frontend/src/components/BugReportWizard.tsx` | ✅ keep | Linear wizard; splitting would fragment step sequencing. |
| **516** | `frontend/src/pages/setting-tabs/GeneralTab.tsx` | ✅ keep | Single settings tab — on the edge but cohesive. |

## Recommended action

**Execute on `must-split` only.** Each as a separate PR.

### 1. `app_runtime_launch.rs` (Rust, priority 1)

**Target:** extract 3 phase modules, each ~300-400 LoC:
- `app_runtime_launch/health_probe_phase.rs` — lines ~89-154 (startup health probe + rollback execution)
- `app_runtime_launch/web_server_phase.rs` — lines ~700-790 (web server + gRPC dashboard spawn)
- `app_runtime_launch/scheduler_phase.rs` — lines ~800-end (scheduler loops + background tasks)

The orchestrating `build_and_spawn()` becomes a thin sequence of phase calls with named checkpoints. Improves testability (phase functions can be unit-tested), readability, and reduces monitor-loop-like drift risk.

**Effort:** ~1 day. Mostly mechanical extraction + reference updates.

### 2. `useSettingsForm.ts` (frontend, priority 2)

**Target:** split by concern group:
- `useNetworkSettings.ts`
- `usePrivacySettings.ts`
- `useTelemetrySettings.ts`
- `useAiSettings.ts`
- …
- `useSettingsForm.ts` (reduced to composition root that pulls the focused hooks together)

**Effort:** ~1 day. Some shared-state threading required; probably needs a settings context.

### 3. Defer everything else

`maybe-split` items are watch-items, not work-items. Revisit if any grows beyond 1200 LoC or gets frequent churn.

## Follow-up triggers

- **IF** `app_runtime_launch.rs` grows beyond 1200 LoC before the must-split lands → escalate priority
- **IF** a new lifecycle phase is added to the updater → split `updater/install.rs` preemptively
- **IF** SQLite handler count in `web_storage_impl.rs` exceeds ~40 → split by endpoint group

## Methodology

```bash
# Rust non-test LoC breakdown:
for f in <file>; do
  TOTAL=$(wc -l < "$f")
  TEST_LINES=$(awk '/^#\[cfg\(test\)\]|^mod tests/{p=1} p' "$f" | wc -l)
  echo "$f: total=$TOTAL code=$((TOTAL - TEST_LINES))"
done

# Frontend top N:
find crates/oneshim-web/frontend/src -name "*.ts" -o -name "*.tsx" \
  | xargs wc -l | awk '$1 > 500' | sort -rn | head -20
```

Triage criteria applied:
1. **must-split**: SOLID-S violation + clear extraction boundaries + measurable friction
2. **maybe-split**: watch-list — borderline SOLID but split is premature
3. **keep**: cohesive single-responsibility + split would fragment logic

## Related

- [`docs/reviews/2026-04-16-p2-tech-debt-brief.md`](2026-04-16-p2-tech-debt-brief.md) §Item 3 — brief
- [`docs/reviews/2026-04-16-p2-tech-debt-plan.md`](2026-04-16-p2-tech-debt-plan.md) — parent plan
- [ADR-003 Directory Module Pattern](../architecture/ADR-003-directory-module-pattern.md) — preferred split idiom
- `feedback_file_split_policy.md` (auto-memory) — the "SOLID over LOC" rule
