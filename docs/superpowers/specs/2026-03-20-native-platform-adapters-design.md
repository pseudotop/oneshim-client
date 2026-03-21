# Native Platform Adapters — Design Spec

> Created: 2026-03-20
> Status: Proposed
> Scope: oneshim-vision (accessibility), oneshim-automation (overlay), src-tauri (wiring)
> Prerequisite: ADR-002 GUI V2 (M0-M2 complete)

## 1. Goal

Replace script-based accessibility adapters (osascript/PowerShell/xdotool) with native OS APIs and wire the MagicOverlay for GUI interaction highlights. Add accessibility tree snapshots to dashcam frames.

## 2. Current State

### Accessibility
- macOS: `MacOsNativeAccessibility` in `crates/oneshim-vision/src/accessibility/macos.rs` — AXUIElement FFI, focused element only (role, title, position, size). Circuit breaker + PII filter + zeroize.
- Windows: `WindowsUiaAccessibility` in `crates/oneshim-vision/src/accessibility/windows.rs` — raw vtable COM via windows-sys, focused element only. Circuit breaker pattern.
- Linux: `LinuxAccessibility` stub in `crates/oneshim-vision/src/accessibility/linux.rs` — returns Ok(None).
- Platform dispatcher: `create_extractor()` in `crates/oneshim-vision/src/accessibility/mod.rs`.

### OverlayDriver
- Port: `crates/oneshim-core/src/ports/overlay_driver.rs` — `show_highlights()` + `clear_highlights()`.
- Impl: `NoOpOverlayDriver` in `crates/oneshim-automation/src/overlay.rs` — logs only.
- MagicOverlay: `src-tauri/src/magic_overlay.rs` — Tauri WebView window with `FocusHighlight` React component already exists.

### FocusProbe
- Port: `crates/oneshim-core/src/ports/focus_probe.rs`.
- Impl: `ProcessMonitorFocusProbe` in `crates/oneshim-app/src/focus_probe_adapter.rs` — already implemented with 30+ tests.

### Dashcam
- `CaptureRingBuffer` in `oneshim-vision::ring_buffer` — 6 slots (~18s), flush on importance >= 0.5, 2 post-event frames.
- Currently captures thumbnail images only, no accessibility metadata.

## 3. Architecture

### 3.1 Accessibility Tree Traversal (per OS)

Extend `AccessibilityExtractor` trait with:
```rust
async fn extract_window_elements(&self, max_depth: u32) -> Result<Vec<FocusedElementInfo>, CoreError> {
    // Default: single focused element (backward compatible)
    Ok(self.extract_focused_element().await?.into_iter().collect())
}
```

#### macOS
- Extend existing FFI in `ffi_macos.rs`: add `kAXChildrenAttribute` constant
- Use `AXUIElementCopyAttributeValue` recursively with depth limit
- Batch optimize with `AXUIElementCopyMultipleAttributeValues` for role+title+position in 1 IPC
- Performance target: <30ms for depth 3, 200 elements
- No new crate dependency (extend existing FFI)

#### Windows
- Extend existing `windows.rs`: add `CreateCacheRequest`, `TreeWalker` vtable calls
- CacheRequest pattern: AddProperty(Name, ControlType, BoundingRectangle), TreeScope_Children
- Single cross-process COM call fetches all properties for subtree
- Option to migrate from windows-sys raw vtable to `windows` crate (0.62) for type-safe COM
- Performance target: <30ms with CacheRequest

#### Linux
- Replace stub with `atspi` crate (0.29, tokio feature)
- Async-native (no spawn_blocking needed)
- `AccessibilityConnection::new()` → find active frame → `get_children()` → AccessibleProxy per child
- ComponentProxy for bounding box, get_role() for role
- No special permissions required (D-Bus based)
- Works on both X11 and Wayland
- Performance target: <50ms for focused window tree

### 3.2 Permission Gating

#### macOS
- `AXIsProcessTrustedWithOptions()` with prompt option
- On denial: return `CoreError::PermissionDenied` → maps to 403
- Circuit breaker already handles repeated failures

#### Windows
- UIA generally works without special permissions
- Some apps may require UIAccess manifest entry
- Graceful fallback to script-based adapter on COM errors

#### Linux
- Check AT-SPI daemon: `read_session_accessibility().await`
- Enable if needed: `set_session_accessibility(true).await`
- Fallback to xdotool for window metadata if AT-SPI unavailable

### 3.3 OverlayDriver → MagicOverlay Bridge

Instead of implementing native OS overlays, bridge `OverlayDriver` to the existing MagicOverlay Tauri WebView:

```rust
pub struct MagicOverlayDriver {
    app_handle: tauri::AppHandle,
}

impl OverlayDriver for MagicOverlayDriver {
    async fn show_highlights(&self, req: HighlightRequest) -> Result<HighlightHandle, CoreError> {
        // Emit "overlay:update-focus" Tauri event with element bounding boxes
        // FocusHighlight.tsx React component renders the highlights
    }
    async fn clear_highlights(&self, handle_id: &str) -> Result<(), CoreError> {
        // Emit "overlay:clear-focus" Tauri event
    }
}
```

- Reuses existing FocusHighlight React component
- Cross-platform (WebView works on all 3 OS)
- Click-through model already in place

### 3.4 Dashcam Accessibility Tagging

Extend `RingFrame` to carry accessibility context:
```rust
pub struct RingFrame {
    pub timestamp: DateTime<Utc>,
    pub thumbnail_data: Vec<u8>,
    pub app_name: String,
    pub window_title: String,
    // New: accessibility tree snapshot at capture time
    pub accessibility_elements: Vec<AccessibilityElement>,
}

pub struct AccessibilityElement {
    pub role: String,
    pub label: String,
    pub bounds: Option<(f32, f32, f32, f32)>,  // x, y, w, h
}
```

- Populated from `extract_window_elements()` result on each monitor tick
- Stored alongside thumbnail when dashcam flushes
- Enables post-hoc analysis of GUI state around events

## 4. Dependency Changes

| OS | Crate | Version | Scope |
|----|-------|---------|-------|
| Linux | `atspi` | 0.29 | `[target.'cfg(target_os = "linux")'.dependencies]` |
| Linux | (transitive: `zbus`) | via atspi | — |
| macOS | (none — extend existing FFI) | — | — |
| Windows | (none or optional `windows` 0.62) | — | Optional migration |

## 5. Testing Strategy

- macOS: test with mock AXUIElement responses (existing pattern in macos.rs tests)
- Windows: test with mock COM objects (existing pattern in windows.rs tests)
- Linux: test with mock D-Bus responses or in-process AT-SPI bus
- OverlayDriver: test Tauri event emission (unit test payload serialization)
- Dashcam: test RingFrame with accessibility data round-trip
- All: circuit breaker behavior, permission denial fallback, depth limit enforcement

## 6. Error Handling

- Permission denied → `CoreError::PermissionDenied` → 403 Forbidden
- Accessibility unavailable → graceful fallback to OCR-only (existing ChainedElementFinder)
- Tree traversal timeout → depth-limited, configurable max elements (default 300)
- Overlay window missing → silent skip (coaching fallback to desktop notification)

## 7. Performance Budget

| Operation | Budget | Approach |
|-----------|--------|----------|
| Focused element only | <5ms | Existing behavior (unchanged) |
| Window subtree (depth 3) | <30ms | Batch attributes, depth limit |
| Overlay highlight render | <16ms | WebView already handles |
| Dashcam tag overhead | <1ms | Snapshot already in memory |

## 8. Feature Flags

```toml
[features]
# Linux AT-SPI (requires atspi crate)
linux-atspi = ["atspi"]
```

macOS and Windows native adapters use existing platform APIs with no new feature flags (already behind `#[cfg(target_os)]`).
