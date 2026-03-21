# Tauri IPC Optimization Design Spec

**Date:** 2026-03-21
**Priority:** P3
**Effort:** 2.5 days
**Status:** Proposed (low ROI — implement only if profiling justifies)

---

## 1. Current State

**IPC deep dive findings (2026-03-21):**

| Endpoint | Queries | Default Limit | Payload Size |
|---|---|---|---|
| `/api/timeline` | 3 sequential | 1,000 events + 500 frames | **260-365KB** |
| `/api/focus/metrics` | 2 sequential | 7 days | 10-20KB |
| `/api/focus/sessions` | 1 | **No limit** | Variable |
| `/api/focus/interruptions` | 1 | **No limit** | Variable |
| `/api/focus/suggestions` | 1 | Hardcoded 50 | 5-10KB |
| `/api/automation/audit` | 1 | Default 50, no max | 5-10KB |
| `/api/automation/policy-events` | 1 (reads 8x) | Clamped 1-500 | 50-100KB |
| `/api/settings` | 1 | Full config | **1.5-2KB** |
| `/api/integration/status` | **7 parallel** | All data | 800-1.5KB |
| `/api/integration/audit` | 1 | Hardcoded 50 | 5KB |

**Key findings:**
- **N+1 NOT confirmed** — `record_to_segment_summary()` is pure in-memory transformation
- **Timeline is the largest payload** at 260-365KB (1,500 items default)
- **Policy events reads 8x limit** then filters in-memory
- **Focus sessions/interruptions** have no pagination bounds
- **Settings payload** is tiny (1.5-2KB) — optimization unjustified

---

## 2. Actionable Items

### A. Pagination bounds (quick fix)

Add `limit` enforcement to unbounded endpoints:
- `focus/sessions`, `focus/interruptions`: add default limit 100, max 500
- `automation/audit`: add max validation (currently no upper bound)

### B. Policy events 8x over-read fix

Replace in-memory filter with SQL `WHERE action_type LIKE 'policy.%'` to avoid reading 8x the requested limit.

### C. Timeline payload reduction (if profiling justifies)

- Default 1,500 items may be excessive for most dashboards
- Consider reducing defaults: 500 events + 200 frames
- Add hash-based conditional fetch for repeated polls

---

## 3. Modified Files

| File | Change |
|---|---|
| `crates/oneshim-web/src/services/focus_service.rs` | Add limit bounds to sessions/interruptions |
| `crates/oneshim-web/src/services/automation_service/queries.rs` | Fix policy events SQL filter |
| `crates/oneshim-web/src/services/timeline_service.rs` | Reduce default limits (optional) |

## 4. Effort

| Task | Estimate |
|---|---|
| Pagination bounds (focus + audit) | 0.5 day |
| Policy events SQL fix | 0.5 day |
| Timeline defaults + conditional fetch | 1.5 days |
| **Total** | **2.5 days** |

## 5. Phased Rollout

| Phase | Scope |
|---|---|
| **A** | Pagination bounds + policy events fix (quick wins) |
| **B** | Timeline defaults reduction (if profiling justifies) |
| **C** | Conditional fetch (if profiling justifies) |
