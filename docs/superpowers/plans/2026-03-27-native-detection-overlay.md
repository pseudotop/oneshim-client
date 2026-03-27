# Native Detection Overlay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Visualize all detected GUI elements on the MagicOverlay with interactive inspection, toggled via Cmd+Shift+D.

**Architecture:** Extends the existing MagicOverlay WebView with a detection mode that renders UiScene elements as color-coded bounding boxes. Rust runs async scene analysis via `ElementFinder.analyze_scene()` and emits results as Tauri events. Frontend renders boxes with role-based colors and click-to-inspect tooltips.

**Tech Stack:** Rust (Tauri v2 IPC, async_trait), TypeScript/React (overlay components), Tailwind CSS

**Spec:** `docs/superpowers/specs/2026-03-27-native-detection-overlay-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/oneshim-core/src/ports/overlay_driver.rs` | Modify | Add `show_detection` / `clear_detection` to port trait |
| `crates/oneshim-automation/src/overlay.rs` | Modify | No-op stubs for new trait methods |
| `crates/oneshim-automation/src/controller/mod.rs` | Modify | Add `scene_finder()` public accessor |
| `src-tauri/src/runtime_state.rs` | Modify | Add `detection_active: Arc<AtomicBool>` |
| `src-tauri/src/magic_overlay.rs` | Modify | Detection state + emit/clear methods |
| `src-tauri/src/commands/detection.rs` | Create | IPC commands for toggle/refresh |
| `src-tauri/src/commands/mod.rs` | Modify | Register `detection` module |
| `src-tauri/src/setup.rs` | Modify | Register Cmd+Shift+D and Cmd+Shift+R shortcuts |
| `src-tauri/src/scheduler/mod.rs` | Modify | Add `detection_active` flag + builder |
| `src-tauri/src/scheduler/loops/monitor.rs` | Modify | Re-analyze on window change when detection active |
| `crates/oneshim-web/frontend/src/overlay/types.ts` | Modify | Detection payload types + state |
| `crates/oneshim-web/frontend/src/overlay/hooks/useOverlayEvents.ts` | Modify | Detection listeners + reducer |
| `crates/oneshim-web/frontend/src/overlay/components/DetectionOverlay.tsx` | Create | Element boxes + inspector tooltip |
| `crates/oneshim-web/frontend/src/overlay/components/DetectionHeader.tsx` | Create | Top header bar |
| `crates/oneshim-web/frontend/src/overlay/App.tsx` | Modify | Render detection components |

---

### Task 1: Extend OverlayDriver Port

**Files:**
- Modify: `crates/oneshim-core/src/ports/overlay_driver.rs`
- Modify: `crates/oneshim-automation/src/overlay.rs`

- [ ] **Step 1: Add detection methods to OverlayDriver trait**

```rust
// crates/oneshim-core/src/ports/overlay_driver.rs
//! Overlay driver port — defines the contract for rendering transparent
//! highlight overlays on screen elements (MagicOverlay, heatmap ghosts).
//! Implemented by Tauri WebView overlay in `src-tauri`.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::gui::{HighlightHandle, HighlightRequest};
use crate::models::ui_scene::UiScene;

#[async_trait]
pub trait OverlayDriver: Send + Sync {
    async fn show_highlights(&self, req: HighlightRequest) -> Result<HighlightHandle, CoreError>;

    async fn clear_highlights(&self, handle_id: &str) -> Result<(), CoreError>;

    /// Render all elements from a UiScene as detection overlay boxes.
    async fn show_detection(&self, scene: &UiScene) -> Result<(), CoreError>;

    /// Clear all detection overlay boxes.
    async fn clear_detection(&self) -> Result<(), CoreError>;
}
```

- [ ] **Step 2: Add no-op stubs to NoOpOverlayDriver**

```rust
// crates/oneshim-automation/src/overlay.rs
use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use oneshim_core::error::CoreError;
use oneshim_core::models::gui::{HighlightHandle, HighlightRequest};
use oneshim_core::models::ui_scene::UiScene;
use oneshim_core::ports::overlay_driver::OverlayDriver;

pub struct NoOpOverlayDriver;

#[async_trait]
impl OverlayDriver for NoOpOverlayDriver {
    async fn show_highlights(&self, req: HighlightRequest) -> Result<HighlightHandle, CoreError> {
        tracing::info!(
            session_id = %req.session_id,
            scene_id = %req.scene_id,
            target_count = req.targets.len(),
            "NoOpOverlayDriver accepted highlight request"
        );

        Ok(HighlightHandle {
            handle_id: Uuid::new_v4().to_string(),
            rendered_at: Utc::now(),
            target_count: req.targets.len(),
        })
    }

    async fn clear_highlights(&self, handle_id: &str) -> Result<(), CoreError> {
        tracing::debug!(handle_id, "NoOpOverlayDriver cleared highlight handle");
        Ok(())
    }

    async fn show_detection(&self, scene: &UiScene) -> Result<(), CoreError> {
        tracing::debug!(
            scene_id = %scene.scene_id,
            element_count = scene.elements.len(),
            "NoOpOverlayDriver accepted detection scene"
        );
        Ok(())
    }

    async fn clear_detection(&self) -> Result<(), CoreError> {
        tracing::debug!("NoOpOverlayDriver cleared detection overlay");
        Ok(())
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-core -p oneshim-automation`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-core/src/ports/overlay_driver.rs crates/oneshim-automation/src/overlay.rs
git commit -m "feat(core): extend OverlayDriver port with detection methods"
```

---

### Task 2: Add AutomationController scene_finder Accessor

**Files:**
- Modify: `crates/oneshim-automation/src/controller/mod.rs`

- [ ] **Step 1: Add public accessor method**

Add after `set_scene_finder` (around line 97):

```rust
    pub fn scene_finder(&self) -> Option<&Arc<dyn ElementFinder>> {
        self.scene_finder.as_ref()
    }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p oneshim-automation`
Expected: compiles without errors

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-automation/src/controller/mod.rs
git commit -m "feat(automation): add scene_finder() public accessor"
```

---

### Task 3: Add Detection State to AppState and Scheduler

**Files:**
- Modify: `src-tauri/src/runtime_state.rs`
- Modify: `src-tauri/src/scheduler/mod.rs`

- [ ] **Step 1: Add detection_active flag to AppState**

In `runtime_state.rs`, add after `indicator_visible` field (around line 80):

```rust
    /// Whether detection overlay mode is active (toggled via Cmd+Shift+D).
    pub detection_active: Arc<AtomicBool>,
```

Find where `AppState` is constructed (in `ManagedStateBuilder` or wherever `indicator_visible` gets its value) and add:

```rust
detection_active: Arc::new(AtomicBool::new(false)),
```

- [ ] **Step 2: Add detection_active to Scheduler struct**

In `scheduler/mod.rs`, add after `capture_paused` field (around line 163):

```rust
    /// Whether detection overlay is active. When `true`, the monitor loop
    /// re-triggers scene analysis on window focus changes.
    pub(super) detection_active: Arc<std::sync::atomic::AtomicBool>,
```

- [ ] **Step 3: Add builder method for detection_active**

Add a builder method alongside the existing `with_capture_paused` (around line 386):

```rust
    pub fn with_detection_active(mut self, flag: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.detection_active = flag;
        self
    }
```

- [ ] **Step 4: Update Scheduler::new default**

In the `Scheduler::new()` constructor, add default value:

```rust
detection_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
```

- [ ] **Step 5: Wire the flag in main.rs (or app_runtime.rs)**

Find where `.with_capture_paused(state.capture_paused.clone())` is called and add alongside:

```rust
.with_detection_active(state.detection_active.clone())
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p oneshim-app`
Expected: compiles without errors

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/runtime_state.rs src-tauri/src/scheduler/mod.rs
git commit -m "feat(state): add detection_active shared flag"
```

---

### Task 4: Add Detection Methods to MagicOverlayHandle

**Files:**
- Modify: `src-tauri/src/magic_overlay.rs`

- [ ] **Step 1: Add DetectionScenePayload struct**

Add after `OverlayModePayload` (around line 47):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionElementPayload {
    pub element_id: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub label: String,
    pub role: Option<String>,
    pub confidence: f64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionScenePayload {
    pub scene_id: String,
    pub app_name: Option<String>,
    pub screen_width: u32,
    pub screen_height: u32,
    pub element_count: usize,
    pub elements: Vec<DetectionElementPayload>,
}
```

- [ ] **Step 2: Add detection_active to OverlayState**

In `OverlayState` struct (around line 50), add:

```rust
    detection_active: bool,
```

And in wherever `OverlayState` is initialized, add `detection_active: false`.

- [ ] **Step 3: Add emit_detection_scene method**

Add after `clear_focus_highlight` method:

```rust
    /// Emit a full UiScene to the detection overlay. Clears any active
    /// focus highlight first (mutual exclusion).
    pub async fn emit_detection_scene(&self, scene: &oneshim_core::models::ui_scene::UiScene) {
        // Clear focus highlight (mutual exclusion)
        self.clear_focus_highlight();

        let elements: Vec<DetectionElementPayload> = scene
            .elements
            .iter()
            .take(200) // cap at 200 elements
            .map(|el| DetectionElementPayload {
                element_id: el.element_id.clone(),
                x: el.bbox_abs.x,
                y: el.bbox_abs.y,
                width: el.bbox_abs.width,
                height: el.bbox_abs.height,
                label: el.label.clone(),
                role: el.role.clone(),
                confidence: el.confidence,
                source: "composite".to_string(),
            })
            .collect();

        let payload = DetectionScenePayload {
            scene_id: scene.scene_id.clone(),
            app_name: scene.app_name.clone(),
            screen_width: scene.screen_width,
            screen_height: scene.screen_height,
            element_count: elements.len(),
            elements,
        };

        self.ensure_window().await;
        if let Err(e) = self.app_handle.emit("overlay:detection-update", &payload) {
            warn!("failed to emit overlay:detection-update: {e}");
        }

        let mut state = self.state.write().await;
        state.detection_active = true;
        info!(
            scene_id = %scene.scene_id,
            element_count = payload.element_count,
            "detection overlay updated"
        );
    }

    /// Clear the detection overlay.
    pub async fn clear_detection_scene(&self) {
        let _ = self.app_handle.emit("overlay:detection-clear", ());
        let mut state = self.state.write().await;
        state.detection_active = false;
        debug!("detection overlay cleared");
    }
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p oneshim-app`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/magic_overlay.rs
git commit -m "feat(overlay): add detection scene emit/clear methods"
```

---

### Task 5: Create IPC Detection Commands

**Files:**
- Create: `src-tauri/src/commands/detection.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Create detection.rs with toggle and refresh commands**

Note: `AppState` does NOT implement `Clone`. Tauri v2 `State<'_, T>` dereferences directly — no `.inner()` method. Clone individual `Arc` fields for spawned tasks.

```rust
// src-tauri/src/commands/detection.rs
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::State;
use tracing::{info, warn};

use oneshim_core::ports::element_finder::ElementFinder;

use crate::magic_overlay::MagicOverlayHandle;
use crate::runtime_state::AppState;

#[derive(Debug, Serialize)]
pub struct ToggleDetectionResponse {
    pub active: bool,
}

#[tauri::command]
pub async fn toggle_detection_overlay(
    active: bool,
    state: State<'_, AppState>,
) -> Result<ToggleDetectionResponse, String> {
    state
        .detection_active
        .store(active, Ordering::Relaxed);

    if active {
        info!("detection overlay activated — running scene analysis");
        spawn_detection_analysis_from_state(&state).await;
    } else {
        info!("detection overlay deactivated");
        if let Some(ref overlay) = state.magic_overlay {
            overlay.clear_detection_scene().await;
        }
    }

    Ok(ToggleDetectionResponse { active })
}

#[tauri::command]
pub async fn refresh_detection_overlay(
    state: State<'_, AppState>,
) -> Result<(), String> {
    if !state.detection_active.load(Ordering::Relaxed) {
        return Err("detection overlay is not active".to_string());
    }
    info!("detection overlay manual refresh");
    spawn_detection_analysis_from_state(&state).await;
    Ok(())
}

/// Helper for global shortcut handler (setup.rs) and IPC commands.
/// Clones only the Arc fields needed, not the entire AppState.
pub async fn spawn_detection_analysis_from_state(state: &AppState) {
    let finder: Arc<dyn ElementFinder> = match state.automation_controller.as_ref() {
        Some(controller) => match controller.scene_finder() {
            Some(finder) => finder.clone(),
            None => {
                warn!("scene_finder not configured — cannot run detection");
                return;
            }
        },
        None => {
            warn!("automation_controller not configured — cannot run detection");
            return;
        }
    };

    let overlay: MagicOverlayHandle = match state.magic_overlay.as_ref() {
        Some(overlay) => overlay.clone(),
        None => {
            warn!("magic_overlay not available — cannot show detection");
            return;
        }
    };

    // Run analysis in background (non-blocking)
    tokio::spawn(async move {
        match finder.analyze_scene(None, None).await {
            Ok(scene) => {
                overlay.emit_detection_scene(&scene).await;
            }
            Err(e) => {
                warn!("detection scene analysis failed: {e}");
            }
        }
    });
}
```

- [ ] **Step 2: Register module in commands/mod.rs**

Add after `pub(crate) mod dashboard;`:

```rust
pub(crate) mod detection;
```

- [ ] **Step 3: Register IPC commands in Tauri invoke handler**

In `src-tauri/src/main.rs` at the `tauri::generate_handler![...]` block (around line 182-248), add before the closing `])`:

```rust
commands::detection::toggle_detection_overlay,
commands::detection::refresh_detection_overlay,
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p oneshim-app`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/detection.rs src-tauri/src/commands/mod.rs src-tauri/src/main.rs
git commit -m "feat(commands): add detection overlay IPC commands"
```

---

### Task 6: Register Global Shortcuts

**Files:**
- Modify: `src-tauri/src/setup.rs`

- [ ] **Step 1: Add detection shortcut registration function**

Add after `register_suggestions_shortcut` function:

```rust
fn register_detection_shortcut(app: &App) {
    if let Err(e) = app
        .global_shortcut()
        .on_shortcut("CmdOrCtrl+Shift+D", |app_handle, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state: tauri::State<'_, crate::runtime_state::AppState> = handle.state();
                    let was_active = state
                        .detection_active
                        .fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
                    let now_active = !was_active;

                    if now_active {
                        tracing::info!("detection overlay toggled ON via shortcut");
                        if let Some(ref overlay) = state.magic_overlay {
                            overlay.set_interactive(true).await;
                        }
                        crate::commands::detection::spawn_detection_analysis_from_state(&state).await;
                    } else {
                        tracing::info!("detection overlay toggled OFF via shortcut");
                        if let Some(ref overlay) = state.magic_overlay {
                            overlay.clear_detection_scene().await;
                            overlay.set_interactive(false).await;
                        }
                    }
                });
            }
        })
    {
        tracing::warn!("failed to register detection shortcut: {e}");
    }
}

fn register_detection_refresh_shortcut(app: &App) {
    if let Err(e) = app
        .global_shortcut()
        .on_shortcut("CmdOrCtrl+Shift+R", |app_handle, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state: tauri::State<'_, crate::runtime_state::AppState> = handle.state();
                    if state
                        .detection_active
                        .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        tracing::info!("detection overlay refresh via shortcut");
                        crate::commands::detection::spawn_detection_analysis_from_state(&state).await;
                    }
                });
            }
        })
    {
        tracing::warn!("failed to register detection refresh shortcut: {e}");
    }
}
```

- [ ] **Step 2: Call registration in init()**

Add alongside existing shortcut registrations:

```rust
register_detection_shortcut(app);
register_detection_refresh_shortcut(app);
```

Note: `spawn_detection_analysis_from_state` is already defined in Task 5 Step 1 as a public function. Shortcuts call it directly.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-app`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/setup.rs src-tauri/src/commands/detection.rs
git commit -m "feat(shortcuts): register Cmd+Shift+D/R for detection overlay"
```

---

### Task 7: Monitor Loop Re-analysis on Window Change

**Files:**
- Modify: `src-tauri/src/scheduler/loops/monitor.rs`

- [ ] **Step 1: Add detection re-analysis logic**

In the monitor loop, find where window/app change is detected (where `app_name` or `window_title` changes between ticks). Add after the change detection:

```rust
// Re-trigger detection analysis if detection mode is active
if detection_active.load(std::sync::atomic::Ordering::Relaxed) && app_changed {
    if let Some(ref overlay) = magic_overlay {
        if let Some(ref finder) = scene_finder {
            let finder = finder.clone();
            let overlay = overlay.clone();
            tokio::spawn(async move {
                match finder.analyze_scene(None, None).await {
                    Ok(scene) => overlay.emit_detection_scene(&scene).await,
                    Err(e) => tracing::warn!("detection re-analysis failed: {e}"),
                }
            });
        }
    }
}
```

The `detection_active`, `magic_overlay`, and `scene_finder` references need to be extracted from the Scheduler fields at the start of the loop, alongside existing fields like `capture_paused`.

- [ ] **Step 2: Pass scene_finder to monitor loop**

The monitor loop needs access to `ElementFinder`. Add to the Scheduler struct if not already present, or pass through the AppState. The simplest approach: add `scene_finder: Option<Arc<dyn ElementFinder>>` to Scheduler and wire it via a builder.

In `scheduler/mod.rs`, add field:

```rust
    /// ElementFinder for detection overlay re-analysis on window change.
    pub(super) scene_finder: Option<Arc<dyn oneshim_core::ports::element_finder::ElementFinder>>,
```

Add builder:

```rust
    pub fn with_scene_finder(mut self, finder: Arc<dyn oneshim_core::ports::element_finder::ElementFinder>) -> Self {
        self.scene_finder = Some(finder);
        self
    }
```

Wire in `app_runtime.rs` where the Scheduler is built, pulling from `automation_controller.scene_finder()`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-app`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/scheduler/loops/monitor.rs src-tauri/src/scheduler/mod.rs
git commit -m "feat(scheduler): re-analyze detection on window change"
```

---

### Task 8: Frontend Types and Event Listeners

**Files:**
- Modify: `crates/oneshim-web/frontend/src/overlay/types.ts`
- Modify: `crates/oneshim-web/frontend/src/overlay/hooks/useOverlayEvents.ts`

- [ ] **Step 1: Add detection types to types.ts**

Add before `OverlayState` interface:

```typescript
export interface DetectionElementPayload {
  element_id: string
  x: number
  y: number
  width: number
  height: number
  label: string
  role: string | null
  confidence: number
  source: string
}

export interface DetectionScenePayload {
  scene_id: string
  app_name: string | null
  screen_width: number
  screen_height: number
  element_count: number
  elements: DetectionElementPayload[]
}
```

- [ ] **Step 2: Extend OverlayState with detection fields**

```typescript
export interface OverlayState {
  mode: OverlayMode
  coaching: CoachingPayload | null
  focusHighlight: FocusHighlightPayload | null
  focusMode: boolean
  goals: GoalProgressItem[]
  captureState: CaptureStatePayload
  suggestionsPanelOpen: boolean
  suggestions: SuggestionViewDto[]
  captureFlashTimestamp: string | null
  detectionScene: DetectionScenePayload | null
  detectionSelectedId: string | null
}
```

- [ ] **Step 3: Add detection reducer actions in useOverlayEvents.ts**

Add to `OverlayAction` union type:

```typescript
  | { type: 'detection-update'; payload: DetectionScenePayload }
  | { type: 'detection-clear' }
  | { type: 'detection-select'; payload: string | null }
```

Add to `initialState`:

```typescript
  detectionScene: null,
  detectionSelectedId: null,
```

Add to `reducer` switch:

```typescript
    case 'detection-update':
      return {
        ...state,
        detectionScene: action.payload,
        detectionSelectedId: null,
        focusHighlight: null, // mutual exclusion
      }
    case 'detection-clear':
      return { ...state, detectionScene: null, detectionSelectedId: null }
    case 'detection-select':
      return { ...state, detectionSelectedId: action.payload }
```

- [ ] **Step 4: Add event listeners in setup()**

Add after `u12` listener:

```typescript
      // u13: Detection overlay scene update
      const u13 = await listen<DetectionScenePayload>('overlay:detection-update', (e) => {
        dispatch({ type: 'detection-update', payload: e.payload })
      })

      // u14: Detection overlay clear
      const u14 = await listen('overlay:detection-clear', () => {
        dispatch({ type: 'detection-clear' })
      })

      unlisten = [u1, u2, u3, u4, u5, u6, u7, u8, u9, u10, u11, u12, u13, u14]
```

Add `DetectionScenePayload` to the imports from `../types`.

- [ ] **Step 5: Also block update-focus when detection is active**

In the reducer, modify `update-focus` case:

```typescript
    case 'update-focus':
      // Mutual exclusion: ignore focus highlights while detection is active
      if (state.detectionScene) return state
      return { ...state, focusHighlight: action.payload }
```

- [ ] **Step 6: Verify frontend builds**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: builds without errors (or `pnpm tsc --noEmit` if build is not configured)

- [ ] **Step 7: Commit**

```bash
git add crates/oneshim-web/frontend/src/overlay/types.ts crates/oneshim-web/frontend/src/overlay/hooks/useOverlayEvents.ts
git commit -m "feat(overlay-frontend): add detection types, events, reducer"
```

---

### Task 9: DetectionOverlay Component

**Files:**
- Create: `crates/oneshim-web/frontend/src/overlay/components/DetectionOverlay.tsx`

- [ ] **Step 1: Create DetectionOverlay component**

```tsx
// crates/oneshim-web/frontend/src/overlay/components/DetectionOverlay.tsx
import { useCallback, useEffect, useRef, useState } from 'react'
import type { DetectionElementPayload, DetectionScenePayload } from '../types'

const ROLE_COLORS: Record<string, string> = {
  AXButton: '#3B82F6',
  button: '#3B82F6',
  Button: '#3B82F6',
  AXTextField: '#22C55E',
  AXTextArea: '#22C55E',
  edit: '#22C55E',
  Edit: '#22C55E',
  TextInput: '#22C55E',
  AXLink: '#A855F7',
  link: '#A855F7',
  Link: '#A855F7',
  AXMenuItem: '#F97316',
  menuitem: '#F97316',
  MenuItem: '#F97316',
  AXTabGroup: '#06B6D4',
  tab: '#06B6D4',
  TabLabel: '#06B6D4',
  AXOutlineRow: '#F59E0B',
  treeitem: '#F59E0B',
  TreeItem: '#F59E0B',
  AXImage: '#EC4899',
  image: '#EC4899',
  Image: '#EC4899',
}

const DEFAULT_COLOR = '#6B7280'

function getRoleColor(role: string | null): string {
  if (!role) return DEFAULT_COLOR
  return ROLE_COLORS[role] ?? DEFAULT_COLOR
}

function getRoleLabel(role: string | null): string {
  if (!role) return '?'
  // Shorten AX-prefixed roles
  return role.replace(/^AX/, '')
}

interface DetectionOverlayProps {
  scene: DetectionScenePayload
  selectedId: string | null
  onSelect: (id: string | null) => void
}

export default function DetectionOverlay({ scene, selectedId, onSelect }: DetectionOverlayProps) {
  const inspectorRef = useRef<HTMLDivElement>(null)

  const selected = selectedId ? scene.elements.find((el) => el.element_id === selectedId) : null

  const handleBoxClick = useCallback(
    (el: DetectionElementPayload) => {
      onSelect(selectedId === el.element_id ? null : el.element_id)
    },
    [selectedId, onSelect],
  )

  // Close inspector on Escape
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        if (selectedId) {
          onSelect(null)
        } else {
          // Deactivate detection mode
          import('@tauri-apps/api/core').then(({ invoke }) => {
            invoke('toggle_detection_overlay', { active: false })
          })
        }
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [selectedId, onSelect])

  return (
    <>
      {scene.elements.map((el) => {
        const color = getRoleColor(el.role)
        const opacity = el.confidence * 0.6 + 0.4
        const isSelected = el.element_id === selectedId

        return (
          <div
            key={el.element_id}
            className="fixed cursor-pointer"
            style={{
              left: el.x,
              top: el.y,
              width: el.width,
              height: el.height,
              border: `${isSelected ? 2 : 1.5}px solid ${color}`,
              borderRadius: '2px',
              backgroundColor: `${color}${isSelected ? '20' : '14'}`,
              opacity,
              zIndex: isSelected ? 10001 : 10000,
              transition: 'border-width 0.1s, background-color 0.1s',
            }}
            onClick={(e) => {
              e.stopPropagation()
              handleBoxClick(el)
            }}
          >
            {/* Role badge */}
            <span
              className="absolute left-0.5 top-0.5 rounded px-0.5 text-[8px] font-medium leading-3"
              style={{
                backgroundColor: `${color}CC`,
                color: '#fff',
                whiteSpace: 'nowrap',
              }}
            >
              {getRoleLabel(el.role)}
            </span>
          </div>
        )
      })}

      {/* Inspector tooltip */}
      {selected && <Inspector element={selected} ref={inspectorRef} />}
    </>
  )
}

interface InspectorProps {
  element: DetectionElementPayload
}

import { forwardRef } from 'react'

const Inspector = forwardRef<HTMLDivElement, InspectorProps>(function Inspector({ element }, ref) {
  const color = getRoleColor(element.role)

  // Position: prefer below-right, flip if near screen edge
  const left = Math.min(element.x + element.width + 8, window.innerWidth - 300)
  const top = Math.min(element.y, window.innerHeight - 200)

  return (
    <div
      ref={ref}
      className="fixed rounded-lg border border-white/20 bg-black/85 p-3 text-xs text-white shadow-2xl backdrop-blur-sm"
      style={{
        left,
        top,
        width: 260,
        zIndex: 10002,
      }}
    >
      <div className="mb-1.5 flex items-center justify-between">
        <span className="rounded px-1.5 py-0.5 text-[10px] font-semibold" style={{ backgroundColor: `${color}CC` }}>
          {getRoleLabel(element.role)}
        </span>
        <span className="text-white/60">{(element.confidence * 100).toFixed(0)}%</span>
      </div>
      <div className="space-y-1 text-[11px]">
        <Row label="label" value={element.label || '(empty)'} />
        <Row label="role" value={element.role ?? 'unknown'} />
        <Row label="bounds" value={`(${element.x}, ${element.y}, ${element.width}, ${element.height})`} />
        <Row label="source" value={element.source} />
        <Row label="id" value={element.element_id} mono />
      </div>
    </div>
  )
})

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex justify-between gap-2">
      <span className="text-white/50">{label}</span>
      <span className={`text-right ${mono ? 'font-mono' : ''} truncate`} title={value}>
        {value}
      </span>
    </div>
  )
}
```

- [ ] **Step 2: Verify frontend builds**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: builds without errors

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/src/overlay/components/DetectionOverlay.tsx
git commit -m "feat(overlay-frontend): add DetectionOverlay component with inspector"
```

---

### Task 10: DetectionHeader Component

**Files:**
- Create: `crates/oneshim-web/frontend/src/overlay/components/DetectionHeader.tsx`

- [ ] **Step 1: Create DetectionHeader component**

```tsx
// crates/oneshim-web/frontend/src/overlay/components/DetectionHeader.tsx

interface DetectionHeaderProps {
  elementCount: number
  onRefresh: () => void
  onClose: () => void
}

export default function DetectionHeader({ elementCount, onRefresh, onClose }: DetectionHeaderProps) {
  const isMac = navigator.platform.startsWith('Mac')
  const refreshKey = isMac ? '\u2318\u21e7R' : 'Ctrl+Shift+R'
  const closeKey = isMac ? '\u2318\u21e7D' : 'Ctrl+Shift+D'

  return (
    <div
      className="fixed left-0 right-0 top-0 z-[10003] flex items-center justify-between px-4 text-[11px] text-white backdrop-blur-md"
      style={{
        height: 28,
        backgroundColor: 'rgba(0, 0, 0, 0.75)',
      }}
    >
      <div className="flex items-center gap-3">
        <span className="font-medium">Detection Mode</span>
        <span className="text-white/50">{elementCount} elements</span>
      </div>
      <div className="flex items-center gap-3">
        <button
          type="button"
          className="rounded px-1.5 py-0.5 text-white/60 transition-colors hover:bg-white/10 hover:text-white"
          onClick={onRefresh}
          title={`Refresh (${refreshKey})`}
        >
          Refresh {refreshKey}
        </button>
        <button
          type="button"
          className="rounded px-1.5 py-0.5 text-white/60 transition-colors hover:bg-white/10 hover:text-white"
          onClick={onClose}
          title={`Close (${closeKey})`}
        >
          Close
        </button>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/oneshim-web/frontend/src/overlay/components/DetectionHeader.tsx
git commit -m "feat(overlay-frontend): add DetectionHeader component"
```

---

### Task 11: Wire Detection Into Overlay App

**Files:**
- Modify: `crates/oneshim-web/frontend/src/overlay/App.tsx`

- [ ] **Step 1: Import and render detection components**

```tsx
// crates/oneshim-web/frontend/src/overlay/App.tsx
import { useCallback } from 'react'
import { CaptureFlash } from './components/CaptureFlash'
import CoachingPopup from './components/CoachingPopup'
import DetectionHeader from './components/DetectionHeader'
import DetectionOverlay from './components/DetectionOverlay'
import FocusHighlight from './components/FocusHighlight'
import { FocusModeIndicator } from './components/FocusModeIndicator'
import GoalProgressBar from './components/GoalProgressBar'
import HeatmapGhost from './components/HeatmapGhost'
import { SuggestionsPanel } from './components/SuggestionsPanel'
import { useOverlayEvents } from './hooks/useOverlayEvents'
import type { SuggestionViewDto } from './types'

export default function OverlayApp() {
  const { state, dispatch } = useOverlayEvents()
  const isRich = state.mode === 'rich' || state.mode === 'adaptive'

  async function handleClosePanel() {
    dispatch({ type: 'toggle-suggestions-panel', payload: false })
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('toggle_overlay_interactive', { interactive: false })
  }

  const handleRefreshSuggestions = useCallback(async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    try {
      const suggestions = await invoke<SuggestionViewDto[]>('get_pending_suggestions')
      dispatch({ type: 'set-suggestions', payload: suggestions })
    } catch {
      /* ignore */
    }
  }, [dispatch])

  const handleDetectionSelect = useCallback(
    (id: string | null) => {
      dispatch({ type: 'detection-select', payload: id })
    },
    [dispatch],
  )

  const handleDetectionRefresh = useCallback(async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    try {
      await invoke('refresh_detection_overlay')
    } catch {
      /* ignore */
    }
  }, [])

  const handleDetectionClose = useCallback(async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    try {
      await invoke('toggle_detection_overlay', { active: false })
    } catch {
      /* ignore */
    }
  }, [])

  return (
    <div className="relative h-screen w-screen overflow-hidden">
      {/* Detection mode header */}
      {state.detectionScene && (
        <DetectionHeader
          elementCount={state.detectionScene.element_count}
          onRefresh={handleDetectionRefresh}
          onClose={handleDetectionClose}
        />
      )}

      {/* Detection overlay boxes */}
      {state.detectionScene && (
        <DetectionOverlay
          scene={state.detectionScene}
          selectedId={state.detectionSelectedId}
          onSelect={handleDetectionSelect}
        />
      )}

      {/* Focus mode pill indicator (top center) */}
      <FocusModeIndicator active={state.focusMode} />

      {/* Focus area highlight (when no detection mode) */}
      {!state.detectionScene && state.focusHighlight && <FocusHighlight highlight={state.focusHighlight} />}

      {/* Coaching popup (shown when a message is active) */}
      {state.coaching && <CoachingPopup message={state.coaching} autoDismissSecs={state.coaching.auto_dismiss_secs} />}

      {/* Suggestions panel (right side, slide in/out) */}
      <SuggestionsPanel
        open={state.suggestionsPanelOpen}
        suggestions={state.suggestions}
        onClose={handleClosePanel}
        onRefresh={handleRefreshSuggestions}
      />

      {/* Rich mode: goal progress bar at bottom */}
      {isRich && state.goals.length > 0 && <GoalProgressBar goals={state.goals} />}

      {/* Rich mode: attention heatmap ghost */}
      {isRich && <HeatmapGhost />}

      {/* Manual capture feedback flash */}
      <CaptureFlash timestamp={state.captureFlashTimestamp} />
    </div>
  )
}
```

- [ ] **Step 2: Verify frontend builds**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: builds without errors

- [ ] **Step 3: Run full workspace check**

Run: `cargo check --workspace && cargo test --workspace`
Expected: all checks pass

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-web/frontend/src/overlay/App.tsx
git commit -m "feat(overlay): wire detection overlay into App"
```

---

### Task 12: Add overlay capability for detection IPC

**Files:**
- Modify: `src-tauri/capabilities/overlay.json` (if exists) or wherever IPC permissions are defined

- [ ] **Step 1: Check existing capability file**

Look for IPC command allowlists in `src-tauri/capabilities/` or `src-tauri/tauri.conf.json`. Add `toggle_detection_overlay` and `refresh_detection_overlay` to the overlay window's allowed commands.

- [ ] **Step 2: Commit**

```bash
git add src-tauri/capabilities/
git commit -m "feat(capabilities): allow detection IPC commands in overlay window"
```

---

## Self-Review Checklist

1. **Spec coverage**: All spec sections covered — data flow, visual design (role colors in Task 9), events (Task 8), IPC (Task 5), shortcuts (Task 6), state (Task 3), mutual exclusion (Tasks 4, 8), performance cap (Task 4 — `take(200)`), monitor re-analysis (Task 7).
2. **Placeholder scan**: No TBD/TODO — all steps have code blocks.
3. **Type consistency**: `DetectionScenePayload` and `DetectionElementPayload` match between Rust (Task 4) and TypeScript (Task 8). Reducer actions match component props (Tasks 8, 9, 11). `scene_finder()` accessor (Task 2) matches usage in Tasks 5, 6, 7.
