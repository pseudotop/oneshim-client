# P1: Replay Image Loading Failure — Spec Document

**Status**: Analysis Complete
**Severity**: P1 — Core feature broken
**Affected Pages**: Session Replay, Timeline (grid/list views, lightbox)
**Branch**: `fix/dmg-background`

---

## 1. Problem Statement

Frame screenshot images fail to load in Session Replay and Timeline pages when running inside the Tauri desktop application. The `<img>` tags show broken image icons or trigger `onError` callbacks, displaying fallback "image unavailable" messages.

## 2. Root Cause Analysis

### 2.1 Primary Cause: Missing `img-src` in Tauri CSP

The Content Security Policy in `src-tauri/tauri.conf.json` (line 32):

```
default-src 'self';
script-src 'self' 'unsafe-eval';
style-src 'self';
connect-src 'self' http://127.0.0.1:10090 ... http://127.0.0.1:10099;
object-src 'none';
base-uri 'self'
```

**Missing**: `img-src` directive.

When `img-src` is not specified, it falls back to `default-src 'self'`. In the Tauri WebView context, `'self'` resolves to `tauri://localhost`, NOT `http://127.0.0.1:*`.

#### Request Flow (Current — Broken)

```
1. Frontend (tauri://localhost) renders:
   <img src="http://127.0.0.1:10090/api/frames/123/image">

2. WebView CSP evaluation:
   - img-src not defined → falls back to default-src 'self'
   - 'self' = tauri://localhost
   - http://127.0.0.1:10090 ≠ tauri://localhost
   - ❌ BLOCKED by CSP

3. Browser fires `onerror` event
4. React sets imageLoadFailed = true
5. User sees "image unavailable" fallback
```

### 2.2 Why `connect-src` Works But `img-src` Doesn't

| CSP Directive | Covers | Status |
|---------------|--------|--------|
| `connect-src` | `fetch()`, `XHR`, `EventSource`, WebSocket | ✅ Allows `http://127.0.0.1:10090-10099` |
| `img-src` | `<img src>`, CSS `background-image`, `<picture>`, favicon | ❌ Falls back to `default-src 'self'` |

API calls via `fetch()` work because `connect-src` explicitly lists the localhost ports. But `<img>` tags are governed by `img-src`, which has no such allowance.

### 2.3 Backend Path Chain Verification (Confirmed Working)

The backend image serving pipeline was verified to be **correctly wired**:

```
FrameFileStorage.save_frame()
  → saves to: {data_dir}/frames/{YYYY-MM-DD}/{HH-MM-SS-NNN}.webp
  → returns:  PathBuf("frames/{date}/{filename}")   (relative)

SQLite frames table
  → stores:   "frames/{date}/{filename}"             (file_path column)

WebServer AppState
  → frames_dir = data_dir                            (set in build_runtime_bindings, line 172)

FramesQueryService.get_frame_image()
  → full_path = frames_dir.join(file_path)
  → result:    {data_dir}/frames/{date}/{filename}    ✅ matches save location
```

Key evidence:
- `web_server_runtime.rs:172` — `frames_dir: Some(data_dir.to_path_buf())`
- `web_server_runtime.rs:329` — `.with_runtime_bindings(runtime_bindings)` applied to WebServer
- `lib.rs:285-286` — `frames_dir` copied from bindings to `AppState`
- Both scheduler and web server use the same `data_dir_path` (from `app_runtime_launch.rs:171`)

### 2.4 Frontend URL Resolution (Confirmed Correct)

```typescript
// api-base.ts:84-88
export function resolveImageUrl(url: string | null | undefined): string | null {
  if (!url) return null
  if (!IS_TAURI || !url.startsWith('/api')) return url
  return `http://127.0.0.1:${resolvedPort}${url}`
}
```

- Input: `/api/frames/123/image` (from `frames_assembler.rs`)
- Output (Tauri): `http://127.0.0.1:10090/api/frames/123/image`
- Output (browser): `/api/frames/123/image` (relative, works with same-origin)

The URL generation is correct. The request would succeed if the CSP allowed it.

### 2.5 CORS (Not the Issue)

The Axum backend's CORS layer (`lib.rs:354-371`) includes `tauri://localhost` in `AllowOrigin::list()`. However, CORS only applies to `fetch()`/`XHR` requests. `<img>` tags make simple GET requests that don't require CORS. The issue is CSP, not CORS.

## 3. Affected Components

| Component | File | Impact |
|-----------|------|--------|
| Session Replay | `frontend/src/pages/SessionReplay.tsx:424` | Main screenshot viewer broken |
| Timeline Grid | `frontend/src/pages/Timeline.tsx:368` | Frame thumbnails broken |
| Timeline List | `frontend/src/pages/Timeline.tsx:415` | Frame thumbnails broken |
| Timeline Detail | `frontend/src/pages/Timeline.tsx:485` | Selected frame image broken |
| Lightbox | `frontend/src/pages/Timeline.tsx:664` | Fullscreen image broken |

All 5 locations use `resolveImageUrl(frame.image_url)` in `<img src>`.

## 4. Fix

### 4.1 Add `img-src` Directive to CSP

**File**: `src-tauri/tauri.conf.json`, line 32

Add `img-src` with the same localhost port range as `connect-src`:

```
img-src 'self' http://127.0.0.1:10090 http://127.0.0.1:10091 http://127.0.0.1:10092 http://127.0.0.1:10093 http://127.0.0.1:10094 http://127.0.0.1:10095 http://127.0.0.1:10096 http://127.0.0.1:10097 http://127.0.0.1:10098 http://127.0.0.1:10099
```

### 4.2 Add `'unsafe-inline'` to `style-src`

**Same file**, same CSP line. Change `style-src 'self'` to:

```
style-src 'self' 'unsafe-inline'
```

**Why this is required (not optional)**:
- 19 frontend files use JSX `style={{...}}` attributes (33 total occurrences)
- Critical usage in SessionReplay: scene overlay elements use `style={{ left, top, width, height }}` for absolute positioning over screenshots
- Tauri's nonce injection only covers `<style>` elements and `<link>` tags, NOT runtime `style` attributes generated by React
- Without `'unsafe-inline'`, overlay elements stack at (0,0) — breaking the Action Assistant feature
- Note: When Tauri injects a nonce into `style-src`, `'unsafe-inline'` is normally ignored per CSP spec. However, Tauri v2 handles this via allowlisting, so adding `'unsafe-inline'` explicitly ensures inline style attributes work regardless of nonce behavior.

### 4.3 Rationale for This Approach

| Approach | Pros | Cons | Chosen |
|----------|------|------|--------|
| **Add `img-src` to CSP** | Minimal change, no code impact, works for all image endpoints | Ports hardcoded in CSP | ✅ |
| Tauri `asset://` protocol | Native, no HTTP round-trip | Major refactor: all image URLs change, need IPC for DB lookup, bypasses Axum middleware | ❌ |
| `convertFileSrc()` from Tauri API | Tauri-native file serving | Requires knowing file path in frontend (not just API URL), breaks web mode | ❌ |
| Proxy via IPC (base64) | No CSP issue | Performance hit (base64 encoding), memory overhead, poor UX for large images | ❌ |
| `data:` URLs | No CSP issue | Must fetch + encode before render, adds latency, no caching | ❌ |

### 4.4 Security Considerations

- `img-src` only allows loading images (not scripts, styles, or connections)
- Restricted to `127.0.0.1` loopback — no external network access
- Port range 10090-10099 matches the existing `connect-src` allowance
- `'self'` retained for any bundled images (icons, UI assets)
- `style-src 'unsafe-inline'` allows inline style attributes — acceptable risk since the app is local-only and no user-generated CSS injection is possible

### 4.5 Known Limitation: Hardcoded Port Range in CSP

The CSP port range (10090-10099) matches the default `web.port` config and `MAX_PORT_ATTEMPTS = 10` fallback range. If a user changes `web.port` to a non-default value (e.g., 8080), the web server may bind to a port outside the CSP-allowed range, causing images and API calls to fail.

**Mitigation**: This is a latent issue affecting non-default configurations only. A future enhancement could dynamically set CSP at runtime via Tauri's `webview.set_csp()` or generate `tauri.conf.json` at build time from config.

## 5. Verification Plan

1. Apply the CSP fix in `tauri.conf.json`
2. Build and launch via `cargo tauri dev`
3. Navigate to Timeline page — verify frame thumbnails load
4. Navigate to Session Replay — verify main screenshot loads
5. Click a frame thumbnail — verify lightbox image loads
6. Verify scene overlay elements are positioned correctly over screenshots
7. Verify browser dev console shows no CSP violations for `img-src` or `style-src-attr`
8. Verify API calls (fetch) still work (regression check)

## 6. Files to Modify

| File | Change |
|------|--------|
| `src-tauri/tauri.conf.json` | Add `img-src` directive + `'unsafe-inline'` to `style-src` in CSP string |

**Total**: 1 file, 1 line change (CSP string update).
