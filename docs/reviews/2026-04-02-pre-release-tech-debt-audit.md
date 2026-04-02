# Pre-Release Technical Debt Audit — 2026-04-02

**Version**: v0.4.14 (post Audio P1-P4)
**Scope**: 14-crate workspace + frontend (production code only)
**Method**: 3-way parallel review (code quality, architecture, frontend)

---

## Executive Summary

| Area | Score | Status |
|------|-------|--------|
| Hexagonal Architecture | 100% | PASS |
| Port Traits | 100% | PASS (documented exceptions) |
| Feature Flags | 100% | PASS |
| Production unwrap() | 0 | FIXED (22 → 0) |
| panic!() in hot paths | 0 | RESOLVED — all 36 were in `#[cfg(test)]` |
| Dependency health | 60% | windows-sys 5 versions |
| Frontend i18n | 100% | FIXED — tracking panel 30 keys added |
| Component size | 60% | 5 files > 500 lines |

---

## ~~CRITICAL~~ RESOLVED — panic!() in Production Hot Paths (0)

> **2026-04-02 correction**: Exhaustive re-audit confirmed ALL 36 `panic!()` calls are
> inside `#[cfg(test)]` modules. The original count did not filter test code.
> See `docs/specs/p0-panic-cleanup-spec.md` for the full verification table.

**Production panic!() count: 0**

Additional panic-family macros verified:
- `unreachable!()`: 2 production sites — both guarded by early returns (safe)
- `todo!()`: 0 production sites
- `unimplemented!()`: 0 production sites (12 in test mocks only)
- `expect()`: 15 sites from unwrap cleanup — all properly guarded (length checks, lock poisoning standard pattern)

---

## HIGH — Reliability Risks

### Concurrency
| Issue | Count | Location | Impact |
|-------|-------|----------|--------|
| ~~`block_in_place()` in async~~ | 2 | coaching_engine/mod.rs:304-311 | **Documented ADR-001 §2 deviation** (see `ports/coaching.rs` L12-32) |
| Untracked `tokio::spawn` | 50 | workspace-wide | Resource leaks, no graceful shutdown |
| `let _ =` (error suppression) | 257 | workspace-wide | Silent failures, undebuggable |

### Code Size Hotspots (top 5)
| File | Lines | Recommendation |
|------|-------|---------------|
| `oneshim-network/src/http_api_session.rs` | 2,381 | Split SSE parser into submodule |
| `src-tauri/src/session_manager.rs` | 1,194 | Extract provider adapters |
| `oneshim-analysis/src/adaptive_search.rs` | 1,139 | Split vector search submodule |
| `oneshim-network/src/local_llm_session.rs` | 999 | Split request/response |
| `oneshim-vision/src/accessibility/windows.rs` | 997 | Extract COM marshaling |

### Dead Code
- 66 `#[allow(dead_code)]` annotations across workspace
- Top files: runtime_state.rs (6), http_api_session.rs (2), suggestion_manager.rs (2)

---

## MEDIUM — Architecture & Dependencies

### Dependency Version Conflicts
| Dependency | Versions | Risk |
|-----------|----------|------|
| `windows-sys` | 0.45, 0.52, 0.59, 0.60, 0.61 | Potential linker conflicts on Windows |
| `reqwest` | 0.12.28, 0.13.2 | HTTP error mapping divergence |
| `thiserror` | 1.0.69, 2.0.18 | Ecosystem transition (no immediate risk) |

### Missing Crate Documentation
- `oneshim-api-contracts` — no `//!` module docs
- `oneshim-analysis` — no `//!` module docs
- `oneshim-lint` — no `//!` module docs

---

## Frontend Technical Debt

### i18n Gaps
| File | Severity | Hardcoded Strings |
|------|----------|-------------------|
| ~~`tracking-panel/App.tsx`~~ | ~~HIGH~~ | FIXED — 30 keys added, `useTranslation` integrated |
| Overlay UI | MEDIUM | Feedback messages not wrapped in `t()` |

### Component Size (> 500 lines)
| Component | Lines | Recommendation |
|-----------|-------|---------------|
| `Settings.tsx` | 1,541 | Extract form state to custom hook |
| `Chat.tsx` | 1,408 | Extract ChatMessage, FileUploadZone, ModelSelector |
| `AiAutomationTab.tsx` | 1,385 | Extract ProfileManager, ModelDiscovery, ProviderForm |
| `Automation.tsx` | 789 | Extract rule creation and filtering |
| `SessionReplay.tsx` | 756 | Split playback, timeline, event log |

### Performance
- 8 components missing `React.memo` (Timeline grid, CommandPalette, Dashboard metrics, etc.)
- Chart components (Recharts) not memoized on data updates

### Accessibility
- Tracking panel buttons: `title` only, no `aria-label`
- ActivityHeatmap: missing semantic landmarks

---

## Completed Items (This Session)

- [x] Production unwrap() cleanup: 22 → 0 (9 files, 6 crates)
- [x] `cargo check/test/clippy` all pass
- [x] Architecture compliance verified: 100% hexagonal
- [x] P0 panic!() re-audit: all 36 in test code (false alarm)
- [x] Tracking panel i18n: 30 keys across 5 locales (en/ko/ja/zh-CN/es)

---

## Prioritized Action Plan

### ~~P0~~ RESOLVED — No crash risks
1. ~~Replace 36 `panic!()` with proper error handling~~ — ALL in test code (false alarm)
2. ~~Remove `block_in_place()`~~ — Documented ADR-001 deviation (not a bug)

### P1 — Before Feature Freeze
3. ~~Track `tokio::spawn` handles~~ — Scheduler already manages all loops (sync.rs:365-410), non-scheduler spawns are bounded fire-and-forget
4. ~~Tracking panel i18n~~ — DONE (30 keys, 5 locales, `useTranslation` integrated)
5. ~~http_api_session split~~ — Already ADR-003 directory module (6 files). Chat.tsx (1,775L) deferred to separate PR

### P2 — Next Sprint
6. Dependency version unification (windows-sys, reqwest)
7. Dead code cleanup (66 `#[allow(dead_code)]`)
8. React.memo on list/grid components
9. Missing crate docs (3 crates)

### P3 — Backlog
10. `let _ =` → `debug!` logging (257 sites)
11. Component decomposition (Settings, AiAutomationTab)
12. Accessibility polish (tracking panel, heatmap)
