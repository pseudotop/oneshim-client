# MagicOverlay Unwired Features — Spec Document

**Status**: Analysis Complete
**Scope**: Focus highlight wiring, mode toggle IPC, EventBus cleanup, stale dead_code removal

---

## 1. Focus Highlight Wiring

### Current State
- `MagicOverlayHandle::update_focus_highlight()` exists (line 225) — emits `overlay:update-focus` event
- `OverlayFocusPayload` struct exists (line 30) — x, y, width, height, border_color, opacity
- `FocusedElementInfo.position: Option<ElementRect>` has screen coordinates
- Monitor loop extracts `last_focused_element` every tick (line 192)
- Frontend `useOverlayEvents` listens for `overlay:update-focus` and `overlay:clear-focus` (already wired)
- Frontend `FocusHighlight` component renders the highlight rectangle (already implemented)

### Gap
Line 192 stores `last_focused_element` but never calls `overlay.update_focus_highlight()`.

### Fix
In `monitor.rs`, after line 192 where `last_focused_element` is updated, emit to overlay:

```rust
if let Some(ref fe) = last_focused_element {
    if let (Some(ref overlay), Some(ref pos)) = (&overlay_ref, &fe.position) {
        overlay.update_focus_highlight(OverlayFocusPayload {
            x: pos.x as i32,
            y: pos.y as i32,
            width: pos.width as u32,
            height: pos.height as u32,
            border_color: "#0d9488".to_string(), // brand teal
            opacity: 0.6,
        }).await;
    }
} else if let Some(ref overlay) = overlay_ref {
    let _ = overlay.app_handle.emit("overlay:clear-focus", ());
}
```

### Gate
Only emit when overlay mode is Rich or Adaptive (not Minimal). Check `overlay.get_mode()`.

### Files
- Modify: `src-tauri/src/scheduler/loops/monitor.rs` (~10 lines)
- Remove: `#[allow(dead_code)]` from `update_focus_highlight` and `OverlayFocusPayload`

---

## 2. Overlay Mode Toggle IPC

### Current State
- `MagicOverlayHandle::toggle_mode()` exists (line 264) — cycles Minimal→Rich→Adaptive→Minimal
- `set_overlay_mode` IPC exists (coaching.rs:74) — but sets specific mode, not toggle
- `Cmd+Shift+O` shortcut toggles interactive mode (setup.rs:107), not display mode
- No IPC for `toggle_mode()`

### Fix
Add `toggle_overlay_mode` IPC command in `commands/coaching.rs`:

```rust
#[command]
pub async fn toggle_overlay_mode(state: tauri::State<'_, AppState>) -> Result<String, String> {
    if let Some(ref overlay) = state.magic_overlay {
        overlay.toggle_mode().await;
        let mode = overlay.get_mode().await;
        Ok(format!("{:?}", mode))
    } else {
        Err("overlay not available".to_string())
    }
}
```

Register in `main.rs` invoke_handler.

### Files
- Modify: `src-tauri/src/commands/coaching.rs` (~10 lines)
- Modify: `src-tauri/src/main.rs` (1 line — register command)
- Remove: `#[allow(dead_code)]` from `toggle_mode`

---

## 3. EventBus Cleanup

### Current State
- `src-tauri/src/event_bus.rs` — 83 lines, declared in `main.rs` as `mod event_bus`
- Never instantiated or used in production code
- Duplicates `broadcast::Sender<RealtimeEvent>` already used by the web dashboard
- Has 2 passing tests

### Fix
Delete the module and remove `mod event_bus` from `main.rs`.

### Files
- Delete: `src-tauri/src/event_bus.rs`
- Modify: `src-tauri/src/main.rs` (remove `mod event_bus`)

---

## 4. Stale `#[allow(dead_code)]` Removal

### Candidates (verified used in production)

| File | Line | Item | Actually Used? |
|------|------|------|----------------|
| `runtime_state.rs:55` | `OAuthCoordinatorState` | ✅ Tauri managed state | Remove annotation |
| `runtime_state.rs:70` | `IntegrationSessionState` | ✅ Tauri managed state | Remove annotation |
| `runtime_state.rs:73` | `IntegrationAuthState` | ✅ Tauri managed state | Remove annotation |
| `scheduler/gui_pipeline.rs:30` | `GuiPipelineState` | ✅ Used in monitor loop | Remove annotation |
| `provider_adapters/types.rs:16` | `ProviderSource` | ✅ Used in production | Remove annotation |
| `magic_overlay.rs:28` | `OverlayFocusPayload` | Will be used after wiring | Remove annotation |
| `magic_overlay.rs:39` | `OverlayGoalPayload` | ✅ Already used (line 603) | Remove annotation |
| `magic_overlay.rs:224` | `update_focus_highlight` | Will be used after wiring | Remove annotation |
| `magic_overlay.rs:232` | `update_goal_progress` | ✅ Already used | Remove annotation |
| `magic_overlay.rs:263` | `toggle_mode` | Will be used after IPC | Remove annotation |

### Files
- Modify: `src-tauri/src/runtime_state.rs` (3 lines)
- Modify: `src-tauri/src/scheduler/gui_pipeline.rs` (1 line)
- Modify: `src-tauri/src/provider_adapters/types.rs` (1 line)
- Modify: `src-tauri/src/magic_overlay.rs` (5 lines)
