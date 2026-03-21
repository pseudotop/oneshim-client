# Tauri IPC Optimization — Design Spec

> Created: 2026-03-21
> Status: Proposed
> Scope: src-tauri (commands, scheduler), oneshim-web (REST handlers)
> Reference: ADR-004 (Tauri v2 Migration)

## 1. Goal

Optimize Tauri IPC and REST communication by eliminating dead code, reducing payload sizes, and adding caching for static endpoints.

## 2. Current State

### Code-Level Findings

| Area | Location | Finding |
|------|----------|---------|
| IPC commands | `src-tauri/src/commands/` | Tauri IPC commands for settings, metrics, update, automation |
| REST handlers | `crates/oneshim-web/src/handlers/` | Axum REST handlers for same endpoints |
| IPC dead code | `src-tauri/src/commands/` | Settings/metrics/update served via both IPC and REST; frontend uses REST only |
| REST caching | `crates/oneshim-web/src/routes.rs` | No REST caching middleware (ETag, Cache-Control) |
| Frontend polling | Various frontend components | StatusBar 5s, Settings 10s, Heatmap 60s, Automation 30s |
| String cloning | `crates/oneshim-web/src/handlers/` | ~48KB wasted per timeline request via String cloning |

## 3. Architecture

### A. Payload Size Reduction

- Audit all IPC and REST payloads for unnecessary fields
- Use `#[serde(skip_serializing_if = "Option::is_none")]` on optional fields
- Replace `String` cloning with `Cow<'_, str>` or `Arc<str>` where feasible
- Target: reduce timeline response payload by 30%+

### B. Serialization Optimization

- Profile `serde_json` serialization for large payloads (timeline, frames)
- Consider `simd-json` for read-only deserialization paths
- Pre-allocate `String` buffers based on expected payload size

### C. Frontend Polling Optimization

Current polling intervals:

| Component | Interval | Payload Size | Optimization |
|-----------|----------|-------------|--------------|
| StatusBar | 5s | ~2KB | Acceptable |
| Settings | 10s | ~8KB | Add ETag — skip if unchanged |
| Heatmap | 60s | ~15KB | Add ETag — skip if unchanged |
| Automation | 30s | ~4KB | Add ETag — skip if unchanged |

### D. Dead Code Audit

Tauri IPC commands that are duplicated by REST and where the frontend uses REST only:

- Identify all IPC commands in `src-tauri/src/commands/`
- Cross-reference with frontend `fetch()` / `axios` calls
- For each duplicated command:
  - If frontend uses REST only: mark IPC command as `#[deprecated]`
  - Add `// TODO: remove in next major version` comment
  - Remove from Tauri plugin registration after deprecation period
- Document which IPC commands are still actively used by the frontend

### E. REST Caching

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

## 4. String Cloning Analysis

Timeline request path (`handlers/frames.rs`):
- `frame.window_title.clone()` — 50-200 bytes per frame
- `frame.app_name.clone()` — 20-80 bytes per frame
- `frame.tags.clone()` — variable
- At 200 frames per request: ~48KB wasted on clones

Fix: Use `Arc<str>` for `window_title` and `app_name` in `RingFrame`, or serialize directly from storage without intermediate model cloning.

## 5. Testing Strategy

- Benchmark IPC vs REST latency for same endpoint
- Verify ETag correctness: change config → new ETag → 200; no change → same ETag → 304
- Load test polling intervals with 4 concurrent components
- Measure memory reduction from String cloning fix

## 6. Performance Budget

| Operation | Current | Target |
|-----------|---------|--------|
| Settings GET (ETag hit) | ~3ms | <0.5ms (304) |
| Timeline GET (200 frames) | ~15ms | <10ms |
| IPC round-trip | ~2ms | N/A (remove dead code) |
| Total polling bandwidth/min | ~120KB | <60KB |

## 7. Risks

- ETag computation overhead must be less than time saved by 304 responses
- Removing IPC commands may break undiscovered code paths — audit thoroughly
- `Arc<str>` migration requires touching model definitions across crates
