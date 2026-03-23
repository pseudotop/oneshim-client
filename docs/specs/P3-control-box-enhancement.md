# P3: Control Box Enhancement — Spec Document

**Status**: Analysis Complete
**Severity**: P3 — UX Enhancement
**Branch**: `fix/dmg-background`

---

## 1. Current State

**File**: `crates/oneshim-web/frontend/src/tracking-panel/App.tsx` (57 lines)
**Window**: `src-tauri/src/magic_overlay.rs:323-364` — 220x36px, top-center, non-draggable

Current features:
- Green/Yellow pulse dot (Capturing/Paused)
- Red dot if any service disconnected
- Pause/Resume button (▶/⏸)
- Hide button (✕)

### Limitations
- **Fixed position** — no drag, stuck at top-center
- **No expandable UI** — can't show more info without resize
- **No AI integration** — suggestions, scene analysis not accessible
- **No quick actions** — must open dashboard for everything

---

## 2. Enhancement Design

### 2.1 Layout: Collapsed + Expanded

**Collapsed** (default, ~250x36px): Drag-enabled compact bar
```
[drag] ● Capturing  [▷] [⊞] [✕]
```

**Expanded** (on ⊞ click, ~320x280px): Full control panel
```
┌─────────────────────────────────┐
│ [drag] ● Capturing   [━] [✕]   │
├─────────────────────────────────┤
│ 📷 Manual Capture               │
│ 🧠 Scene Analysis               │
│ 💡 AI Suggestions (3)           │
│ 🎯 Focus Mode                   │
│ 📊 Open Dashboard               │
├─────────────────────────────────┤
│ ● Server  ● LLM  ○ CLI         │
└─────────────────────────────────┘
```

### 2.2 Dragging

Use Tauri's `data-tauri-drag-region` attribute on the drag handle area. Already proven in `TitleBar.tsx`.

**Persistence**: Save last position via IPC to SQLite `app_meta` table (key: `tracking_panel_x`, `tracking_panel_y`). More reliable than `localStorage` for separate WebView windows. Restore on app launch via `get_meta` IPC.

### 2.3 Feature Buttons

| Button | IPC Command | Status | Notes |
|--------|-------------|--------|-------|
| **Pause/Resume** | `toggle_capture_pause` | ✅ Exists | Already wired |
| **Manual Capture** | `trigger_manual_capture` | ❌ New | Force high-importance screenshot |
| **Scene Analysis** | `trigger_scene_analysis` | ❌ New | Run object detection on current window |
| **AI Suggestions** | REST `/api/focus/suggestions` | ✅ Exists | Fetch from Axum, show in panel |
| **Focus Mode** | `set_focus_mode` | ❌ New | Suppress notifications + pause non-essential capture |
| **Open Dashboard** | `show_main_window` | ✅ Exists | Focus main window |
| **Expand/Collapse** | Frontend state | N/A | Toggle panel height |
| **Hide** | `set_indicator_visible` | ✅ Exists | Already wired |

### 2.4 New IPC Commands Required

#### `trigger_manual_capture`
Triggers `SmartCaptureTrigger` with forced high-importance override. Saves full frame + OCR.
```rust
#[command]
pub async fn trigger_manual_capture(state: tauri::State<'_, AppState>) -> Result<String, String>
```

#### `trigger_scene_analysis`
Runs scene analysis on the most recent full frame. Returns element count.
```rust
#[command]
pub async fn trigger_scene_analysis(state: tauri::State<'_, AppState>) -> Result<SceneAnalysisResult, String>
```

#### `set_focus_mode`
Pauses non-essential capture, suppresses coaching popups, mutes notifications.
```rust
#[command]
pub async fn set_focus_mode(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String>
```

#### `show_main_window`
Brings the main dashboard window to front.
```rust
#[command]
pub async fn show_main_window(app: tauri::AppHandle) -> Result<(), String>
```

### 2.5 Dynamic Window Resize

When toggling expanded/collapsed, call Tauri API to resize:
```typescript
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'

const win = getCurrentWindow()
await win.setSize(new LogicalSize(320, expanded ? 280 : 36))
```

Change `create_tracking_panel` to `resizable(true)` and add CSS `body { resize: none; }` in `tracking-panel.html` to prevent user drag-resize while allowing programmatic `set_size()`. This is more reliable than `resizable(false)` + programmatic resize.

---

## 3. Implementation Scope

### Phase 1 (This PR) — Core UX
1. **Draggable panel** with position persistence (SQLite `app_meta`)
2. **Expand/Collapse toggle** with dynamic window resize
3. **Show Dashboard button** + IPC command
4. **Connection status detail** (expanded view shows per-service status)
5. **Improved visual design** — icons, hover states, smooth transitions

### Phase 2 (Future) — AI + Capture Integration
6. Manual Capture button + `trigger_manual_capture` IPC (requires scheduler integration)
7. AI Suggestions panel (fetch + display from REST API)
8. Scene Analysis trigger + result overlay
9. Focus Mode toggle

### Rationale for phasing
- Phase 1 requires only frontend + 1 simple IPC command (`show_main_window`)
- Phase 2 requires backend logic: scheduler capture override, scene analysis from panel context, focus mode state management
- Ship Phase 1 now for immediate UX improvement

---

## 4. Files to Modify

### Phase 1
| File | Change |
|------|--------|
| `crates/oneshim-web/frontend/src/tracking-panel/App.tsx` | Rewrite: drag, expand/collapse, new buttons, connection detail |
| `src-tauri/src/magic_overlay.rs` | `resizable(true)` for programmatic resize |
| `src-tauri/src/commands/capture_status.rs` | Add `show_main_window` + `save_panel_position`/`get_panel_position` |
| `src-tauri/src/main.rs` | Register new IPC commands |

### Phase 2 (Future)
| File | Change |
|------|--------|
| `src-tauri/src/commands/capture_status.rs` | Add `trigger_manual_capture` (scheduler integration) |
| `src-tauri/src/commands/system.rs` | Add `set_focus_mode`, `trigger_scene_analysis` |
| `crates/oneshim-web/frontend/src/tracking-panel/App.tsx` | AI suggestion panel, scene analysis UI, manual capture |

---

## 5. Design Constraints

- **Always on top** — must stay above all windows
- **Minimal footprint** — collapsed state ≤ 250x36px
- **Non-intrusive** — no auto-expand, user-initiated only
- **Dark theme only** — panel uses `bg-black/70 backdrop-blur` (translucent dark)
- **Cross-platform** — macOS/Windows/Linux (Wayland graceful degrade)
- **Position restore** — remember last drag position across app restarts
