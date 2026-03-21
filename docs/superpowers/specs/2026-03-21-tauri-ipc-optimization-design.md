# Tauri IPC Optimization — Design Spec

> Created: 2026-03-21
> Revised: 2026-03-21 (post-review)
> Priority: P3 overall (CompressionLayer is P1 — ship immediately)
> Effort: 4 days
> Status: Proposed
> Scope: src-tauri (commands, scheduler), oneshim-web (REST handlers, routes), frontend (React Query config)
> Reference: ADR-004 (Tauri v2 Migration)

## 1. Goal

Optimize Tauri IPC and REST communication by adding response compression, fixing frontend cache configuration, eliminating dead code, reducing payload sizes, and adding caching for static endpoints.

## 2. Current State

### Code-Level Findings

| Area | Location | Finding |
|------|----------|---------|
| IPC commands | `src-tauri/src/commands/` | 38 Tauri IPC commands for settings, metrics, update, automation, OAuth, coaching, analysis |
| REST handlers | `crates/oneshim-web/src/handlers/` | Axum REST handlers for overlapping endpoints |
| Response compression | `crates/oneshim-web/src/routes.rs` | No `CompressionLayer` — all JSON responses sent uncompressed |
| REST caching | `crates/oneshim-web/src/routes.rs` | No REST caching middleware (ETag, Cache-Control) |
| Frontend polling | `frontend/src/main.tsx` | Global `refetchInterval: 10000` — ALL queries refetch every 10s regardless of volatility |
| Frontend stale time | `frontend/src/main.tsx` | Global `staleTime: 5000` — queries go stale after 5s, too aggressive for most data |
| String cloning | `crates/oneshim-web/src/handlers/` | ~48KB wasted per timeline request via String cloning |

## 3. Architecture

### A. CompressionLayer (SHIP IMMEDIATELY — P1)

**Effort: 15 minutes. Highest ROI change in this entire spec.**

Add `tower-http` `CompressionLayer` to the Axum router:

```rust
// crates/oneshim-web/src/routes.rs
use tower_http::compression::CompressionLayer;

let app = Router::new()
    .route("/api/metrics", get(handlers::metrics))
    // ... other routes
    .layer(CompressionLayer::new());
```

Requirements:
- Add `compression-gzip` feature to `tower-http` in `Cargo.toml`: `tower-http = { version = "0.6", features = ["cors", "compression-gzip"] }`
- Verify SSE endpoints (`text/event-stream`) are not broken by compression — `CompressionLayer` should skip streaming responses automatically, but test this
- Binary responses (WebP images from frame endpoints) are already compressed and `CompressionLayer` skips incompressible content
- Expected reduction: 60-80% for JSON payloads (timeline, events, stats)

### B. Frontend Cache Configuration Fix

**The biggest waste is the global `refetchInterval: 10000` in `main.tsx`.**

Current config (applies to ALL queries):
```tsx
// frontend/src/main.tsx
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchInterval: 10000,  // Every query refetches every 10s
      staleTime: 5000,         // Data goes stale after 5s
    },
  },
});
```

Fix: Remove global `refetchInterval`, set per-query where polling is actually needed. Increase `staleTime` to 30-60s for most queries.

```tsx
// frontend/src/main.tsx
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchInterval: false,   // No global polling — set per-query
      staleTime: 30_000,        // 30s stale time for most queries
    },
  },
});
```

Per-query overrides:

| Query | refetchInterval | staleTime | Rationale |
|-------|----------------|-----------|-----------|
| StatusBar metrics | 5000 | 5000 | Real-time display, must poll |
| Heatmap data | 60000 | 60000 | Updates once per minute |
| Settings | false | Infinity | Only changes on user action |
| Provider surfaces | false | 300000 (5min) | Rarely changes |
| Session list | 30000 | 30000 | Background updates acceptable |
| Automation status | 30000 | 30000 | Background updates acceptable |
| Activity events | false | 10000 | Load on navigation, not polling |

### C. Dead Code Audit

Only 3 of the 38 IPC commands are truly dead (frontend uses REST for the same data, IPC handler is never called):

| Command | Status | Action |
|---------|--------|--------|
| `get_metrics` | Dead — frontend uses REST `/api/metrics` | Deprecate with `#[deprecated]` |
| `get_settings` | Dead — frontend uses REST `/api/settings` | Deprecate with `#[deprecated]` |
| `get_update_status` | Dead — frontend uses REST `/api/update-status` | Deprecate with `#[deprecated]` |

The following are NOT dead and must be preserved:
- `get_automation_status` — used by system tray menu
- `get_web_port` — used by Tauri window URL resolution
- All OAuth commands (`start_oauth_flow`, `check_oauth_status`, etc.) — used by Tauri-native auth flow
- All coaching overlay commands — used by MagicOverlay WebView bridge
- Analysis config commands — used by settings panel IPC path

Action: Mark 3 dead commands with `#[deprecated(since = "0.X.0", note = "Use REST endpoint instead")]`. Do not remove them yet (low risk, not worth the churn).

### D. REST Caching (ETag + Cache-Control)

Add ETag/Cache-Control middleware for static and semi-static endpoints:

```rust
// tower-http middleware for caching
use tower_http::set_header::SetResponseHeaderLayer;

// Static endpoints (change only on user action)
// - GET /api/settings — ETag based on config file mtime
// - GET /api/provider-surfaces — ETag based on provider config hash

// Semi-static endpoints (change periodically)
// - GET /api/stats — Cache-Control: max-age=5
// - GET /api/heatmap — Cache-Control: max-age=30

// Dynamic endpoints (no caching)
// - GET /api/metrics — always fresh
// - GET /api/events — always fresh
```

Implementation approach:
1. Add `tower-http` `SetResponseHeader` layer for Cache-Control
2. Compute ETag from content hash (FNV-1a) for settings/provider-surfaces
3. Handle `If-None-Match` header — return 304 Not Modified when ETag matches
4. Frontend: add `If-None-Match` header to polling requests

### E. Payload Size Reduction

- Audit all IPC and REST payloads for unnecessary fields
- Use `#[serde(skip_serializing_if = "Option::is_none")]` on optional fields
- Replace `String` cloning with `Cow<'_, str>` or `Arc<str>` where feasible
- Target: reduce timeline response payload by 30%+

### F. String Cloning Fix

Timeline request path (`handlers/frames.rs`):
- `frame.window_title.clone()` — 50-200 bytes per frame
- `frame.app_name.clone()` — 20-80 bytes per frame
- `frame.tags.clone()` — variable
- At 200 frames per request: ~48KB wasted on clones

Fix: Use `Arc<str>` for `window_title` and `app_name` in `RingFrame`, or serialize directly from storage without intermediate model cloning.

### G. Policy Events SQL Optimization

Currently policy audit events are fetched without filtering by action type. Add WHERE clause:

```sql
-- Before: fetches all events then filters in Rust
SELECT * FROM audit_events ORDER BY timestamp DESC LIMIT 100;

-- After: filter at SQL level
SELECT * FROM audit_events
WHERE action_type LIKE 'policy.%'
ORDER BY timestamp DESC LIMIT 100;
```

HIGH feasibility — single SQL change, no schema modification needed.

## 4. Frontend Work Breakdown

The frontend changes (B, D frontend portion) are substantial:

| Task | Effort |
|------|--------|
| Remove global `refetchInterval`, set per-query overrides | 0.5d |
| Increase `staleTime` defaults, tune per-query | 0.25d |
| Add `If-None-Match` header support to fetch wrapper | 0.25d |
| Test all query behaviors after cache config change | 0.5d |
| **Frontend subtotal** | **1.5d** |

## 5. Testing Strategy

- Verify `CompressionLayer` reduces JSON payload sizes (gzip Content-Encoding header present)
- Verify SSE endpoints still work with CompressionLayer active
- Benchmark IPC vs REST latency for same endpoint
- Verify ETag correctness: change config -> new ETag -> 200; no change -> same ETag -> 304
- Load test polling intervals with 4 concurrent components
- Measure memory reduction from String cloning fix
- Verify 3 deprecated IPC commands still compile (backward compat)
- Verify per-query refetchInterval overrides work correctly

## 6. Performance Budget

| Operation | Current | Target |
|-----------|---------|--------|
| JSON response size (gzip) | 100% | 20-40% (60-80% reduction) |
| Settings GET (ETag hit) | ~3ms | <0.5ms (304) |
| Timeline GET (200 frames) | ~15ms | <10ms |
| Total polling bandwidth/min | ~120KB | <40KB |
| Queries refetching every 10s | All | Only 2-3 (StatusBar, Heatmap, Session) |

## 7. Effort (4 days)

| Task | Days |
|------|------|
| A. CompressionLayer (P1, ship immediately) | 0.1 |
| B. Frontend cache config fix (refetchInterval + staleTime) | 1.5 |
| C. Dead code audit + deprecation | 0.25 |
| D. REST caching (ETag + Cache-Control) | 0.75 |
| E. Payload size reduction (serde skip_serializing_if) | 0.25 |
| F. String cloning fix (Arc<str>) | 0.5 |
| G. Policy events SQL filter | 0.15 |
| Testing + validation | 0.5 |
| **Total** | **4.0** |

## 8. Phased Rollout

**Phase 1 (Ship Immediately)**: CompressionLayer
- 15-minute change, massive ROI
- Add `compression-gzip` feature to `tower-http`
- Verify SSE compatibility

**Phase 2 (Day 1-2)**: Frontend cache configuration
- Remove global `refetchInterval: 10000`
- Set per-query overrides
- Increase `staleTime` to 30s default
- This is the highest-impact optimization after compression

**Phase 3 (Day 2-3)**: REST caching + payload optimization
- ETag for static endpoints
- Cache-Control for semi-static endpoints
- `skip_serializing_if` on optional fields
- Policy events SQL filter

**Phase 4 (Day 3-4)**: Dead code + String cloning
- Deprecate 3 IPC commands
- `Arc<str>` migration for timeline models

## 9. Risks

| Risk | Mitigation |
|------|------------|
| CompressionLayer breaks SSE streaming | Test `text/event-stream` content type; CompressionLayer should skip it automatically |
| ETag computation overhead must be less than time saved by 304 responses | FNV-1a hash is <1us for typical payloads |
| Removing global `refetchInterval` may cause stale UI in unexpected places | Audit all `useQuery` calls for implicit polling dependency |
| `Arc<str>` migration requires touching model definitions across crates | Limit to `RingFrame` in `oneshim-web` only |
