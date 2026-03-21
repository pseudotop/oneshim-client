# ADR-003 Large File Split — Design Spec

> Created: 2026-03-21
> Status: Proposed
> Scope: oneshim-automation, src-tauri (scheduler, provider_adapters)
> Reference: ADR-003 (Directory Module Pattern for Large Source Files)

## 1. Goal

Split 3 files exceeding 2000 lines into directory modules per ADR-003, improving maintainability and code navigation without changing public APIs.

## 2. Files to Split

### 2.1 `gui_interaction/mod.rs` (2,446 lines)

**Current structure:**
- Lines 1-63: re-exports, constants, sub-module declarations
- Lines 64-391: test mocks + fixture builders (~330 lines)
- Lines 392-2446: 83 test functions organized by category (utility, session, highlight, confirm, execute, lifecycle, edge cases, M5 failure scenarios)

**Proposed split:**
```
crates/oneshim-automation/src/gui_interaction/
├── mod.rs           # Re-exports, constants, sub-module declarations (~65 lines)
├── service.rs       # (already exists — GuiInteractionService)
├── crypto.rs        # (already exists — HMAC signing)
├── helpers.rs       # (already exists — build_candidates, validate_action)
├── types.rs         # (already exists — DTOs)
└── tests/
    ├── mod.rs       # Test module declaration + shared mocks + fixtures (~330 lines)
    ├── session.rs   # Session creation/get tests
    ├── highlight.rs # Highlight tests
    ├── confirm.rs   # Confirm + ticket tests
    ├── execute.rs   # Execute + complete tests
    ├── lifecycle.rs # TTL, cleanup, cancel, subscribe tests
    └── m5.rs        # M5 failure scenario tests (403/409/422/nonce/TTL)
```

**Rationale:** The 2400 lines are almost entirely tests (2050+ lines). The production code is already well-split into `service.rs`, `crypto.rs`, `helpers.rs`, `types.rs`. Only the test infrastructure needs refactoring.

### 2.2 `scheduler/loops.rs` (1,983 lines)

**Current structure:**
- Lines 1-83: imports + helper functions (build_segment_stats, handle_event_analysis)
- Lines 84-187: handle_frame_capture
- Lines 188-764: `spawn_monitor_loop` (576 lines — the core loop)
- Lines 765-1499: 8 smaller spawn loops (metrics, process, sync, heartbeat, aggregation, notification, focus, event_snapshot)
- Lines 1499-1983: 4 more loops (analysis, oauth_refresh, cross_device_sync, coaching) + record_to_segment_summary

**Proposed split:**
```
src-tauri/src/scheduler/
├── mod.rs                 # (already exists — AdaptiveTriggerState, Scheduler struct)
├── config.rs              # (already exists)
├── gui_pipeline.rs        # (already exists)
├── heatmap.rs             # (already exists)
├── analysis_pipeline.rs   # (already exists)
├── loops/
│   ├── mod.rs             # Re-exports, helper functions, imports (~85 lines)
│   ├── monitor.rs         # spawn_monitor_loop (~580 lines)
│   ├── system.rs          # spawn_metrics_loop, spawn_process_loop, spawn_aggregation_loop (~260 lines)
│   ├── network.rs         # spawn_sync_loop, spawn_heartbeat_loop (~100 lines)
│   ├── intelligence.rs    # spawn_analysis_loop, spawn_focus_loop, spawn_coaching_loop (~350 lines)
│   ├── events.rs          # spawn_event_snapshot_loop, spawn_notification_loop (~130 lines)
│   ├── sync.rs            # spawn_cross_device_sync_loop, spawn_oauth_refresh_loop (~200 lines)
│   └── helpers.rs         # build_segment_stats_snapshot, handle_event_analysis, handle_frame_capture, record_to_segment_summary (~300 lines)
```

**Rationale:** `spawn_monitor_loop` alone is 576 lines. Each loop is functionally independent — they share only the `Scheduler` struct fields and `AdaptiveTriggerState`.

### 2.3 `provider_adapters.rs` (1,950 lines)

**Current structure:**
- Lines 1-100: types (ProviderSource, AiProviderAdapters, ExternalOcrPrivacyGuard)
- Lines 100-310: GuardedOcrProvider (privacy-wrapping adapter)
- Lines 310-415: resolve_ai_provider_adapters (main entry point)
- Lines 415-650: surface transport resolution (OCR, LLM)
- Lines 650-870: provider resolution (OCR, LLM) with OAuth
- Lines 870-1950: endpoint config, fallback logic, tests

**Proposed split:**
```
src-tauri/src/provider_adapters/
├── mod.rs           # Re-exports, AiProviderAdapters struct, resolve_ai_provider_adapters (~120 lines)
├── types.rs         # ProviderSource, ExternalOcrPrivacyGuard (~110 lines)
├── guarded_ocr.rs   # GuardedOcrProvider privacy wrapper (~200 lines)
├── surface.rs       # Surface transport resolution (OCR/LLM surface config) (~240 lines)
├── ocr_resolver.rs  # OCR provider resolution (direct, CLI subscription, OAuth) (~200 lines)
├── llm_resolver.rs  # LLM provider resolution (direct, CLI subscription, OAuth) (~200 lines)
├── helpers.rs       # endpoint config, fallback formatting, require_endpoint_config (~100 lines)
└── tests.rs         # All tests (~780 lines)
```

**Rationale:** Clear responsibility boundaries: types, privacy wrapper, surface resolution, per-provider resolution, helpers.

## 3. Constraints

- **No public API changes** — all `pub use` re-exports preserved
- **No logic changes** — pure structural refactoring
- **All tests pass** before and after
- **`cargo check/clippy/test` clean** throughout
- Follow existing ADR-003 patterns (e.g., `coaching_engine/`, `coaching_template/`, `controller/`)

## 4. Testing Strategy

For each split:
1. `cargo test` before split (baseline count)
2. Move code to new files with `use super::*` or specific imports
3. `cargo test` after split (same count, 0 failures)
4. `cargo clippy` clean

## 5. Execution Order

1. `gui_interaction/mod.rs` — lowest risk (tests only)
2. `provider_adapters.rs` — medium risk (production code + tests)
3. `scheduler/loops.rs` — highest risk (core loop wiring)

Each split is an independent commit.
