# P3: Control Box Enhancement — Spec Document

**Status**: Verified (2026-03-25) — Phase 1+2 implemented, 1 bug remaining
**Severity**: P3 — UX Enhancement

---

## Status Summary

### Phase 1 — COMPLETED (v0.4.2, session 2026-03-24)

All Phase 1 features are implemented:
- Draggable panel with `data-tauri-drag-region` + position persistence (`app_meta` table)
- Expand/collapse toggle (260x36 ↔ 320x260) with `getCurrentWindow().setSize()`
- Show Dashboard (`show_main_window` IPC)
- Connection status detail (server/llm/cli per-service)
- Pause/Resume, Hide buttons

### Phase 2 — COMPLETED (v0.4.2, session 2026-03-24)

All Phase 2 features are implemented:
- Manual Capture (`trigger_manual_capture` IPC)
- Scene Analysis (`analyze_current_scene` IPC)
- Focus Mode (`toggle_focus_mode` IPC)
- AI Suggestions panel

---

## Remaining Bug: `resizable(false)` Blocks Programmatic Resize

### Problem

`create_tracking_panel` in `src-tauri/src/magic_overlay.rs:267` sets `.resizable(false)`.
The frontend `toggleExpanded` calls `getCurrentWindow().setSize()` to resize the window,
but this **silently fails** on macOS/Windows because the OS window manager respects
the non-resizable constraint.

Current workaround: try-catch at `App.tsx:87` suppresses the error. React state
changes so the expanded content renders, but the **Tauri window frame stays at
260x36px**, clipping the expanded content.

### Fix

**Step 1**: `src-tauri/src/magic_overlay.rs` — change `.resizable(false)` to `.resizable(true)`

**Step 2**: `crates/oneshim-web/frontend/src/tracking-panel/tracking-panel.html` — add
`body { resize: none; }` CSS to prevent user drag-resize while allowing programmatic
`setSize()`.

### Files

| File | Change |
|------|--------|
| `src-tauri/src/magic_overlay.rs` | `.resizable(false)` → `.resizable(true)` |
| `crates/oneshim-web/frontend/src/tracking-panel/tracking-panel.html` | Add `body { resize: none }` CSS |

### Verification

1. `cargo check --workspace` — pass
2. Launch app → tracking panel visible
3. Click expand button → window resizes to 320x260 (not clipped)
4. Click collapse button → window resizes back to 260x36
5. User cannot manually drag-resize the window edges

### Risk

- Low: single property change + CSS override
- Cross-platform: `resize: none` is universally supported
