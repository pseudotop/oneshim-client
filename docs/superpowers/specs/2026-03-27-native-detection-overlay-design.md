# Native Detection Overlay

**Date**: 2026-03-27
**Status**: Reviewed (revision 1)
**Scope**: `src-tauri/`, `crates/oneshim-web/frontend/src/overlay/`, `crates/oneshim-core/`

## Problem

The client detects GUI elements via OCR + accessibility APIs and produces `UiScene` with full element data (bounds, roles, labels, confidence). However, this data is invisible to users. There is no way to:

1. **Visualize** what the system "sees" on screen
2. **Inspect** individual elements (role, type, confidence, bounds)
3. **Validate** detection accuracy in real-time
4. **Debug** false positives/negatives in element classification

The existing `FocusHighlight` component handles a different use case: highlighting a small set of automation candidates selected by the system. Detection overlay needs to show ALL detected elements with interactive inspection.

## Design

### User Flow

1. User presses `Cmd+Shift+D` (macOS) / `Ctrl+Shift+D` (Windows/Linux)
2. System captures current screen → runs `ElementFinder.analyze_scene()` → emits scene to overlay
3. Overlay renders colored bounding boxes for every detected element
4. Overlay becomes interactive (click-through disabled)
5. User clicks an element box → inspector tooltip appears showing metadata
6. User presses shortcut again or `Escape` → overlay clears, returns to click-through

While active, re-analysis triggers on:
- Active window change (app/title change detected by monitor loop)
- Manual refresh via `Cmd+Shift+R` / `Ctrl+Shift+R`
- NOT on a timer (avoids continuous CPU load)

### Architecture

```
┌─────────────────────────────────────────────────────┐
│ src-tauri (Rust)                                    │
│                                                     │
│  commands/detection.rs (NEW)                        │
│  ├─ toggle_detection_overlay() → IPC command        │
│  ├─ refresh_detection_overlay() → IPC command       │
│  └─ inspect_detection_element(id) → IPC command     │
│                                                     │
│  magic_overlay.rs (EXTEND)                          │
│  ├─ emit_detection_scene(scene: UiScene)            │
│  ├─ clear_detection_scene()                         │
│  └─ detection state tracking (active: bool)         │
│                                                     │
│  scheduler/loops/monitor.rs (EXTEND)                │
│  └─ on window change → if detection active,         │
│     re-analyze and emit                             │
├─────────────────────────────────────────────────────┤
│ oneshim-core (ports)                                │
│                                                     │
│  ports/overlay_driver.rs (EXTEND)                   │
│  ├─ show_detection(&self, scene: &UiScene)          │
│  └─ clear_detection(&self)                          │
├─────────────────────────────────────────────────────┤
│ overlay frontend (TypeScript)                       │
│                                                     │
│  types.ts (EXTEND)                                  │
│  ├─ DetectionScenePayload                           │
│  ├─ DetectionElementPayload                         │
│  └─ OverlayState.detectionScene                     │
│                                                     │
│  hooks/useOverlayEvents.ts (EXTEND)                 │
│  ├─ overlay:detection-update listener               │
│  └─ overlay:detection-clear listener                │
│                                                     │
│  components/DetectionOverlay.tsx (NEW)              │
│  ├─ Element bounding boxes (color-coded by role)    │
│  ├─ Confidence opacity                              │
│  └─ Click → inspector tooltip                       │
│                                                     │
│  components/DetectionInspector.tsx (NEW)            │
│  └─ Metadata panel for selected element             │
│                                                     │
│  App.tsx (EXTEND)                                   │
│  └─ Render DetectionOverlay when scene active       │
└─────────────────────────────────────────────────────┘
```

### Data Flow

```
[User: Cmd+Shift+D]
       │
       ▼
[Global shortcut handler in setup.rs]
       │
       ▼
[AppState.automation_controller.scene_finder.analyze_scene(app_name, screen_id)]
       │  (runs on tokio::spawn — non-blocking)
       ▼
[UiScene { elements: Vec<UiSceneElement> }]
       │
       ▼
[MagicOverlayHandle.emit_detection_scene(scene)]
       │  (sets detection_active = true, clears any active FocusHighlight)
       ▼
[Tauri event: overlay:detection-update]
       │
       ▼
[useOverlayEvents reducer: set detectionScene, clear focusHighlight]
       │
       ▼
[DetectionOverlay component renders all elements]
       │
       ▼
[User clicks element box]
       │  (frontend-only — no IPC needed, all data in payload)
       ▼
[DetectionInspector shows: role, label, confidence, bounds, source]
```

#### ElementFinder Access Path

`ElementFinder` is not directly on `AppState`. Access path:

```rust
// AppState.automation_controller: Option<Arc<AutomationController>>
//   → AutomationController.scene_finder: Option<Arc<dyn ElementFinder>>
//     → CompositeElementFinder { accessibility_finder, ocr_finder }

// In the shortcut handler:
let state: State<'_, AppState> = handle.state();
if let Some(ref controller) = state.automation_controller {
    if let Some(ref finder) = controller.scene_finder() {
        let scene = finder.analyze_scene(app_name, screen_id).await?;
        // ...
    }
}
```

Note: `AutomationController.scene_finder` is `pub(super)`. Need to add a public accessor method `pub fn scene_finder(&self) -> Option<&Arc<dyn ElementFinder>>`.

### Visual Design

#### Element Boxes

Each detected `UiSceneElement` renders as a semi-transparent bordered rectangle:

| Property | Mapping |
|----------|---------|
| Position | `bbox_abs.{x, y, width, height}` (absolute pixels) |
| Border color | By role (see color map below) |
| Border width | 1.5px (subtle, non-intrusive) |
| Background | Border color at 8% opacity |
| Opacity | `confidence * 0.6 + 0.4` (range: 0.4–1.0) |
| Label | Compact role badge (top-left, inside box) |

#### Role Color Map

| Role | Color | Hex |
|------|-------|-----|
| Button / AXButton | Blue | `#3B82F6` |
| TextInput / AXTextField / Edit | Green | `#22C55E` |
| Link / AXLink | Purple | `#A855F7` |
| MenuItem / AXMenuItem | Orange | `#F97316` |
| TabLabel / AXTabGroup | Cyan | `#06B6D4` |
| TreeItem / AXOutlineRow | Amber | `#F59E0B` |
| Image / AXImage | Pink | `#EC4899` |
| Default (unknown role) | Gray | `#6B7280` |

#### Inspector Tooltip

When user clicks an element box, a floating tooltip appears anchored to the element:

```
┌──────────────────────────────┐
│ Button  confidence: 0.92     │
│ ─────────────────────────── │
│ label: "Save"                │
│ role:  AXButton              │
│ bounds: (420, 180, 80, 32)   │
│ source: Accessibility        │
│ id:    el-a3f2               │
└──────────────────────────────┘
```

- Max width: 280px
- Position: prefer below-right of element, flip if near screen edge
- Dismiss on: click elsewhere, Escape, or click same element again

#### Overlay Header Bar

When detection mode is active, a thin header bar appears at the top:

```
┌─────────────────────────────────────────────────────────┐
│ 🔍 Detection Mode  │  42 elements  │  Refresh ⌘⇧R  │ ✕ │
└─────────────────────────────────────────────────────────┘
```

- Background: `rgba(0, 0, 0, 0.75)` with backdrop blur
- Height: 28px
- Shows element count, refresh shortcut, close button
- Close button = deactivate detection mode

### Tauri Events

| Event Name | Direction | Payload | Purpose |
|------------|-----------|---------|---------|
| `overlay:detection-update` | Rust → JS | `DetectionScenePayload` | Full scene data |
| `overlay:detection-clear` | Rust → JS | `{}` | Clear all detection visuals |

#### DetectionScenePayload

```typescript
interface DetectionScenePayload {
  scene_id: string
  app_name: string | null
  screen_width: number
  screen_height: number
  element_count: number
  elements: DetectionElementPayload[]
}

interface DetectionElementPayload {
  element_id: string
  x: number        // bbox_abs.x
  y: number        // bbox_abs.y
  width: number    // bbox_abs.width
  height: number   // bbox_abs.height
  label: string
  role: string | null
  confidence: number
  source: string   // "ocr" | "accessibility" | "template_matcher"
}
```

Note: `UiSceneElement` fields `bbox_norm`, `intent`, `state`, `text_masked`, `parent_id` are not sent to the overlay. The frontend only needs what it renders. This keeps the payload small (especially with 100+ elements).

### IPC Commands

| Command | Parameters | Returns | Purpose |
|---------|-----------|---------|---------|
| `toggle_detection_overlay` | `{ active: bool }` | `{ active: bool }` | Activate/deactivate (analysis runs async, result arrives via event) |
| `refresh_detection_overlay` | — | `{}` | Force re-analysis (result arrives via event) |

Both commands return immediately. The actual element data arrives asynchronously via `overlay:detection-update` event. This avoids blocking the IPC channel during scene analysis (which can take 100-500ms).

`inspect_detection_element` is NOT needed as an IPC command — all element metadata is already in the frontend payload.

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+Shift+D` / `Ctrl+Shift+D` | Toggle detection mode |
| `Cmd+Shift+R` / `Ctrl+Shift+R` | Refresh scene (while active) |
| `Escape` | Deactivate detection mode |

Registered via `app.global_shortcut().on_shortcut()` in `src-tauri/src/setup.rs`, following the existing pattern for `CmdOrCtrl+Shift+O` (overlay interactive) and `CmdOrCtrl+Shift+S` (suggestions toggle).

Note: `Escape` is handled in the frontend via `useEffect` keydown listener (not a global shortcut), because Escape should only deactivate detection when the overlay window has focus.

### State Management

#### Rust Side (MagicOverlayHandle)

Add to existing `OverlayState`:

```rust
pub detection_active: bool,
pub detection_scene_id: Option<String>,
```

#### Frontend Side (OverlayState)

Add to existing `OverlayState`:

```typescript
detectionScene: DetectionScenePayload | null
detectionSelectedId: string | null  // currently inspected element
```

New reducer actions:

```typescript
| { type: 'detection-update'; payload: DetectionScenePayload }
| { type: 'detection-clear' }
| { type: 'detection-select'; payload: string | null }
```

### Performance Considerations

1. **Element count cap**: Frontend caps rendering at 200 elements. If scene has more, show top 200 by confidence + "and N more" in header.
2. **No continuous polling**: Re-analysis only on window change or manual refresh.
3. **Lightweight payload**: Only render-necessary fields sent to frontend (no `bbox_norm`, `intent`, `state`).
4. **CSS-only rendering**: No canvas — pure CSS boxes with `position: fixed`. Browser handles compositing efficiently for 200 divs.
5. **Deferred analysis**: `analyze_scene()` runs on a background task, not blocking the IPC command response. Result emitted as event when ready.

### Integration with Existing Systems

1. **OverlayDriver port**: Extended with `show_detection` / `clear_detection` methods. `NoOpOverlayDriver` and `PlatformOverlayDriver` get no-op stubs.
2. **MagicOverlayHandle**: Gets new methods mirroring the port extension.
3. **Monitor loop**: Adds detection-aware window change detection — if `detection_active`, re-triggers analysis on focus change. The `detection_active` flag is an `Arc<AtomicBool>` shared between AppState and Scheduler (same pattern as `capture_paused`).
4. **Overlay capability**: Detection mode uses the same `magic-overlay` window. No new Tauri window.
5. **FocusHighlight mutual exclusion**: Detection overlay and FocusHighlight are mutually exclusive. Both Rust and frontend enforce this:
   - **Rust**: `MagicOverlayHandle.emit_detection_scene()` calls `clear_highlights()` first. `show_highlights()` checks `detection_active` flag and returns early with a warning log if true.
   - **Frontend**: `detection-update` reducer action sets `focusHighlight: null`. `update-focus` reducer action is no-op when `detectionScene` is non-null.
6. **AutomationController**: Add public accessor `pub fn scene_finder(&self) -> Option<&Arc<dyn ElementFinder>>` to expose the element finder for detection commands.

### Files Changed

| File | Change Type | Description |
|------|------------|-------------|
| `src-tauri/src/commands/detection.rs` | NEW | IPC commands for detection overlay |
| `src-tauri/src/commands/mod.rs` | MODIFY | Register detection module |
| `src-tauri/src/setup.rs` | MODIFY | Register `CmdOrCtrl+Shift+D` and `CmdOrCtrl+Shift+R` global shortcuts |
| `src-tauri/src/magic_overlay.rs` | MODIFY | `emit_detection_scene`, `clear_detection_scene`, state |
| `src-tauri/src/runtime_state.rs` | MODIFY | Add `detection_active: Arc<AtomicBool>` to AppState |
| `src-tauri/src/scheduler/mod.rs` | MODIFY | Add `detection_active` flag + `with_detection_active()` builder |
| `src-tauri/src/scheduler/loops/monitor.rs` | MODIFY | Window-change triggered re-analysis when detection active |
| `crates/oneshim-core/src/ports/overlay_driver.rs` | MODIFY | Add `show_detection` / `clear_detection` |
| `crates/oneshim-automation/src/controller/mod.rs` | MODIFY | Add `pub fn scene_finder()` accessor |
| `crates/oneshim-automation/src/overlay.rs` | MODIFY | No-op stubs for new port methods |
| `crates/oneshim-web/frontend/src/overlay/types.ts` | MODIFY | Detection payload types |
| `crates/oneshim-web/frontend/src/overlay/hooks/useOverlayEvents.ts` | MODIFY | Detection event listeners + reducer |
| `crates/oneshim-web/frontend/src/overlay/components/DetectionOverlay.tsx` | NEW | Element box rendering + inspector tooltip |
| `crates/oneshim-web/frontend/src/overlay/components/DetectionHeader.tsx` | NEW | Top header bar |
| `crates/oneshim-web/frontend/src/overlay/App.tsx` | MODIFY | Render detection components |

### Testing Strategy

1. **Rust unit tests**: `commands/detection.rs` — mock `ElementFinder`, verify event emission
2. **Frontend unit tests**: `DetectionOverlay.test.tsx` — render with mock payload, verify box count + colors
3. **Integration**: Manual test via `Cmd+Shift+D` on real desktop — verify element alignment with actual UI

### Out of Scope

- LLM-based element classification (separate task #2)
- Core ML / ONNX segmentation (separate task #3)
- Element interaction (clicking detected elements to perform actions)
- Persistent detection history / recording
- Multi-monitor detection (single primary screen only for v1)
