# P0 Panic Cleanup — Spec Document

**Date**: 2026-04-02
**Branch**: `feat/p4-voice-activity-detection`
**Scope**: Verify and correct the pre-release audit P0 findings

---

## 1. Background

The pre-release tech debt audit (`docs/reviews/2026-04-02-pre-release-tech-debt-audit.md`)
identified two P0 issues:

1. **36 `panic!()` in production hot paths** — claimed to crash the app
2. **`block_in_place()` in coaching_engine** — claimed to cause tokio panic

## 2. Investigation Results

### 2.1 panic!() — ALL IN TEST CODE (False Alarm)

Exhaustive grep + manual verification of every `panic!()` call in the workspace:

| File | Lines | Verdict |
|------|-------|---------|
| `http_api_session/tests.rs` | 37,49,63,82,93,107,120,140,166,182,197,747,760,773,786,801,815,831,841,854 | `#[cfg(test)]` — TEST |
| `ai_session.rs` | 437,474,493,518,568 | `#[test]` functions — TEST |
| `coaching_engine/mod.rs` | 410,428,494,769,795,819 | `#[cfg(test)]` (starts L346) — TEST |
| `coaching_engine/triggers.rs` | 392,412,480 | `#[cfg(test)]` (starts L229) — TEST |
| `refresh_coordinator.rs` | 332,360,382,421 | `#[cfg(test)]` (starts L236) — TEST |
| `intent_planner.rs` | 286 | `#[cfg(test)]` (starts L192) — TEST |
| `scheduler/mod.rs` | 581,609 | `#[cfg(test)]` (starts L528) — TEST |
| `claude_normalizer.rs` | 199,214,228,249,270 | `#[cfg(test)]` (starts L180) — TEST |
| `subprocess_session.rs` | 691,714,740 | `#[cfg(test)]` (starts L647) — TEST |
| `automation_runtime.rs` | 559 | `#[cfg(test)]` (starts L297) — TEST |
| `analysis_client.rs` | 361 | `#[cfg(test)]` (starts L332) — TEST |
| `error_mapping.rs` | 69 | `#[cfg(test)]` (starts L48) — TEST |
| `updater/mod.rs` | 869 | `#[cfg(test)]` (starts L195) — TEST |
| `producer_coordinator.rs` | 131 | `#[cfg(test)]` (starts L58) — TEST |
| `egress_coordinator.rs` | 389 | `#[cfg(test)]` (starts L153) — TEST |
| `oauth/mod.rs` | 761,786 | `#[cfg(test)]` (starts L512) — TEST |
| `override_store_impl.rs` | 250,286 | `#[cfg(test)]` (starts L191) — TEST |
| `temp_file_projection.rs` | 398,439,485,499 | `#[cfg(test)]` (starts L305) — TEST |
| `integration.rs` | 578,621 | `#[test]` functions — TEST |
| `tiered_memory/mod.rs` | 207 | `#[test]` function — TEST |
| `model_downloader.rs` | 225 | `#[cfg(test)]` — TEST |
| `config_manager.rs` | 319 | `#[cfg(test)]` (starts L230) — TEST |
| `linux.rs` (accessibility) | 821,846 | `#[cfg(test)]` — TEST |
| `settings_service/tests_validation.rs` | 24,174,338,463,495,528,565,655 | Test file — TEST |
| `automation_service/commands.rs` | 538,590 | `#[cfg(test)]` — TEST |
| `handlers/ai_models.rs` | 94 | `#[cfg(test)]` — TEST |
| `controller/tests.rs` | 199 | Test file — TEST |
| `session_manager/tests.rs` | 24,70 | Test file — TEST |
| `provider_adapters/tests.rs` | 240,242,468,469 | Test file — TEST |
| `tests/*.rs` (integration) | multiple | Test directory — TEST |

**Result: 0 production panic!() calls. ALL are in test modules.**

The audit error: the auditor likely ran `grep panic! --count` without filtering
`#[cfg(test)]` blocks, counting test assertion panics as production code.

### 2.2 block_in_place() — Documented ADR Deviation (Not a Bug)

Three production `block_in_place()` sites found:

| File | Line | Purpose | Status |
|------|------|---------|--------|
| `coaching_engine/mod.rs` | 304-306 | `all_goal_progress_blocking()` — Axum sync handler | **Documented** in `ports/coaching.rs` L12-32 |
| `coaching_engine/mod.rs` | 310-312 | `update_regime_goals_blocking()` — Axum sync handler | **Documented** in `ports/coaching.rs` L12-32 |
| `commands/capture.rs` | 124 | Sync SQLite `save_frame_metadata` from async IPC | **Accepted** pattern (same as SqliteStorage deviation) |

The `CoachingPort` trait has extensive documentation (30 lines) explaining WHY
this ADR-001 §2 deviation exists:
- Axum web handlers need sync access to `tokio::sync::RwLock`-backed state
- The `_blocking` suffix is an intentional naming convention
- Both sync (Axum) and async (Tauri IPC) consumers are supported

**Result: Not a bug. Intentional, documented architecture decision.**

## 3. Corrected Audit Status

| Original Claim | Reality | Action |
|----------------|---------|--------|
| 36 panic!() in production | 0 — all in test code | Update audit doc |
| block_in_place tokio panic | Documented ADR deviation | No action needed |

## 4. Corrected Priority List

With P0 resolved as false alarm, the actual priority order is:

### NEW P0 — None (no crash risks)

### P1 — Before Feature Freeze
1. `tokio::spawn` handle tracking (50 sites) — graceful shutdown risk
2. Tracking panel i18n (22+ hardcoded strings)
3. Large file splits (http_api_session 2,381L, Chat.tsx 1,408L)

### P2 — Next Sprint
4. Dependency version unification (windows-sys 5 versions)
5. Dead code cleanup (66 `#[allow(dead_code)]`)
6. React.memo on list/grid components (8 components)

### P3 — Backlog
7. `let _ =` → debug logging (257 sites)
8. Component decomposition (Settings 1,541L, AiAutomationTab 1,385L)
9. Missing crate docs (3 crates)

## 5. Deep Review: Additional Panic-Family Macros

Beyond `panic!()`, checked `unreachable!()`, `todo!()`, `unimplemented!()`:

### unreachable!() — 2 production sites (both safe)

| File | Line | Guard | Risk |
|------|------|-------|------|
| `adaptive_search.rs` | 255 | Early return for Hnsw at L247-252 | Safe — `#[cfg(feature = "hnsw")]` |
| `ai_model_catalog_web_service.rs` | 97 | Early return at L50-58 | Safe — AwsSignatureV4 returns first |

Both are guarded by early returns that handle the variant before reaching the match arm.
If someone refactors the early return away, these would crash. Low risk but worth
a future cleanup to replace with `Err(...)` or `debug_assert!`.

### todo!() — 0 sites

### unimplemented!() — 12 sites (all in test mocks)

All 12 in `refresh_coordinator.rs` mock trait impls inside `#[cfg(test)]`.

### expect() from unwrap cleanup — all safe

| Pattern | Count | Safety |
|---------|-------|--------|
| `expect("len >= 2")` | 3 | Guarded by `if len >= 2` check |
| `expect("lock poisoned")` | 9 | Standard Rust mutex pattern |
| `expect("non-empty when merging")` | 1 | Guarded by `should_merge` flag |
| Control flow refactor (no expect) | 2 | Proper `let Some(...) else` |

## 6. Deep Review Conclusion

**No Critical or Important issues found.** The spec is accurate:
- 0 production panic!() calls
- 0 production todo!() / unimplemented!() calls
- 2 production unreachable!() calls — both genuinely unreachable with guards
- block_in_place() is documented ADR deviation
- All expect() calls are properly guarded

## 7. Required Changes

1. **Update audit document** — correct the P0 section to reflect test-only status
2. **Update memory** — correct the task list to remove false P0
