# Control Box Features & Coaching Context — Technical Specification

> **Version**: 1.2 (2nd review pass — all Critical/Important resolved)
> **Date**: 2026-03-24
> **Scope**: A1 Manual Capture, A2 Scene Analysis, A3 AI Suggestions Panel, A4 Focus Mode, C1 Coaching Regime Context
> **Branch**: `fix/dmg-background`

---

## 1. Overview

Five medium-complexity features that extend the ONESHIM desktop client's **control box** (MagicOverlay + Tauri IPC) and **coaching pipeline**. All features share the same integration surface: Tauri IPC commands backed by existing scheduler/pipeline infrastructure.

### Feature Map

| ID | Feature | Layer | Key Dependency |
|----|---------|-------|----------------|
| A1 | Manual Capture | IPC → scheduler capture override | `SmartCaptureTrigger`, `EdgeFrameProcessor` |
| A2 | Scene Analysis | IPC → GUI + accessibility pipeline | `GuiPipelineState`, `AccessibilityExtractor` |
| A3 | AI Suggestions Panel | IPC → suggestion crate | `oneshim-suggestion` (Receiver, Queue, Presenter) |
| A4 | Focus Mode | IPC → capture policy + notification suppression | `NotificationManager`, `SmartCaptureTrigger`, `CoachingEngine` |
| C1 | Coaching Regime Context | Scheduler loop state sharing | `AdaptiveTriggerState`, coaching loop |

---

## 2. Feature A1: Manual Capture

### 2.1 Problem

Users cannot trigger an on-demand screenshot capture from the control box. The current capture pipeline is fully automatic — `SmartCaptureTrigger.should_capture()` fires based on event classification and importance scoring. There is no manual override path.

### 2.2 Solution

Add a Tauri IPC command `trigger_manual_capture` that bypasses the `SmartCaptureTrigger` decision logic and directly invokes `FrameProcessor::capture_and_process()` with maximum importance (1.0).

### 2.3 Design

#### IPC Command

```rust
// src-tauri/src/commands/capture.rs (NEW file)

#[tauri::command]
pub async fn trigger_manual_capture(
    state: tauri::State<'_, AppState>,
) -> Result<ManualCaptureResponse, String>
```

#### Response DTO

```rust
#[derive(Serialize)]
pub struct ManualCaptureResponse {
    pub success: bool,
    pub frame_id: Option<String>,
    pub timestamp: String,
    pub resolution: Option<(u32, u32)>,
    pub ocr_text: Option<String>,
}
```

#### Flow

```
Frontend button click
  → invoke("trigger_manual_capture")
  → AppState.frame_processor.capture_and_process(manual_request)
  → Full frame + OCR (importance = 1.0)
  → Persist to FrameFileStorage + SQLite
  → Return ManualCaptureResponse
```

#### Dependencies on AppState

New fields required in `AppState`:

```rust
pub frame_processor: Arc<dyn FrameProcessor>,    // Already exists in Scheduler, needs sharing
pub frame_storage: Option<Arc<FrameFileStorage>>, // Already exists in Scheduler, needs sharing
```

**Critical decision**: `frame_processor` and `frame_storage` are currently owned by `Scheduler` and not exposed to `AppState`. Two options:

- **Option A (Recommended)**: Add `Arc<dyn FrameProcessor>` and `Option<Arc<FrameFileStorage>>` to `AppState` during construction in `app_runtime_launch.rs`. Both are already `Arc`-wrapped — just clone the reference.
- **Option B**: Use a `tokio::mpsc` channel to send capture requests to the monitor loop. More complex, less direct.

Option A is preferred because `FrameProcessor` and `FrameFileStorage` are stateless (or use interior mutability) and safe to share.

#### CaptureRequest Construction

```rust
let manual_request = CaptureRequest {
    trigger_type: "manual".to_string(),
    importance: 1.0,  // Maximum — triggers Full + OCR pipeline
    app_name: current_app_name,
    window_title: current_window_title,
    window_bounds: None,  // Full primary monitor
};
```

#### Obtaining Current Window Context

The command must first query current window info for the `CaptureRequest`. This uses `ActivityMonitor::collect_context()` which returns `Result<UserContext, CoreError>`. The `app_name` and `window_title` are nested inside `UserContext.active_window: Option<WindowInfo>`:

```rust
let ctx = activity_monitor.collect_context().await.map_err(|e| e.to_string())?;
let (app_name, window_title) = match ctx.active_window {
    Some(ref w) => (w.app_name.clone(), w.title.clone()),
    None => ("unknown".to_string(), "".to_string()),
};
```

#### Storage Integration

After `capture_and_process()` returns `ProcessedFrame`:

1. Extract image bytes from `ImagePayload`: the `ImagePayload::Full { data, .. }` field contains **base64-encoded** WebP data. Decode via `base64::engine::general_purpose::STANDARD.decode(&data)` before passing to storage.
2. Save image via `FrameFileStorage::save_frame(timestamp, &webp_bytes)` (if available) — takes raw `&[u8]`, not `ProcessedFrame`.
3. Persist metadata to SQLite via `state.storage.save_frame_metadata_with_bounds()` (`SqliteStorage` implements this directly) — returns `Result<i64, CoreError>` (SQLite row ID). Note: the IPC command accesses `AppState.storage: Arc<SqliteStorage>`, not the `SchedulerStorage` trait.
4. Use the row ID as `frame_id` (convert to String): `frame_id = Some(row_id.to_string())`.
5. Return `ManualCaptureResponse` with the obtained `frame_id`.

#### Edge Cases

- **Capture paused** (`capture_paused` AtomicBool): Manual capture SHOULD work even when auto-capture is paused. This is intentional — user explicitly requested it.
- **No FrameProcessor**: Return `Err("Capture not available")` if `frame_processor` is None.
- **Concurrent captures**: `EdgeFrameProcessor` uses `Mutex<Option<DynamicImage>>` for prev_frame — concurrent calls are safe (mutex serializes access).

#### Tests

- Unit test: ManualCaptureResponse serialization
- Integration test: Command returns success with mock FrameProcessor

---

## 3. Feature A2: Scene Analysis

### 3.1 Problem

Users cannot trigger an on-demand scene analysis from the control box. The GUI pipeline and accessibility extraction run automatically each monitor tick but results are not exposed via IPC.

### 3.2 Solution

Add a Tauri IPC command `analyze_current_scene` that:
1. Captures current window context (app name, title, bounds)
2. Runs accessibility extraction (if available)
3. Runs GUI element detection on the current frame
4. Returns a structured scene analysis result

### 3.3 Design

#### IPC Command

```rust
// src-tauri/src/commands/capture.rs (same file as A1)

#[tauri::command]
pub async fn analyze_current_scene(
    state: tauri::State<'_, AppState>,
) -> Result<SceneAnalysisResponse, String>
```

#### Response DTO

```rust
#[derive(Serialize)]
pub struct SceneAnalysisResponse {
    pub app_name: String,
    pub window_title: String,
    pub timestamp: String,
    pub accessibility: Option<AccessibilitySnapshot>,
    pub ocr_regions: Vec<OcrRegionDto>,
    pub gui_elements: Vec<GuiElementDto>,
    pub work_type: Option<String>,
}

#[derive(Serialize)]
pub struct AccessibilitySnapshot {
    pub focused_element: Option<FocusedElementDto>,
    pub element_count: usize,
}

#[derive(Serialize)]
pub struct FocusedElementDto {
    pub role: String,
    pub label: Option<String>,
    pub value: Option<String>,
    pub bounds: Option<BoundsDto>,
}

#[derive(Serialize)]
pub struct OcrRegionDto {
    pub text: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub confidence: f32,
}

#[derive(Serialize)]
pub struct GuiElementDto {
    pub element_type: String,
    pub label: Option<String>,
    pub confidence: f32,
    pub bounds: Option<BoundsDto>,
}

#[derive(Serialize)]
pub struct BoundsDto {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
```

#### Flow

```
Frontend button click
  → invoke("analyze_current_scene")
  → 1. ActivityMonitor::collect_context() → UserContext
  →    Extract from UserContext.active_window: Option<WindowInfo> → app_name, window_title
  →    Handle None case (no active window) → return partial response
  → 2. AccessibilityExtractor::extract_focused_element(pii_level, has_full_text_consent) (if available)
  →    pii_level: from ConfigManager.get().privacy.pii_filter_level
  →    has_full_text_consent: from ConsentManager (if available) or false
  → 3. FrameProcessor::capture_and_process(importance=0.8) → OCR regions
  → 4. Classify work type from context
  → Return SceneAnalysisResponse
```

**Note on OcrRegionDto mapping**: The actual `OcrRegion` model nests coordinates inside `bbox: BoundingBox { x, y, width, height }`. The DTO flattens this: `OcrRegionDto.x = region.bbox.x`.

#### Dependencies on AppState

New fields required:

```rust
pub activity_monitor: Arc<dyn ActivityMonitor>,           // Already Arc in Scheduler
pub accessibility_extractor: Option<Arc<dyn AccessibilityExtractor>>, // Already Arc in Scheduler
pub consent_manager: Option<Arc<ConsentManager>>,         // Already Arc in Scheduler — needed for PII level
```

These follow the same sharing pattern as A1.

#### Edge Cases

- **No accessibility**: Return `accessibility: None`. This is normal on Linux/Wayland or when permission not granted on macOS. The `extract_focused_element()` method requires `pii_level` and `has_full_text_consent` params — source from `ConfigManager` and `ConsentManager`.
- **No active window**: `UserContext.active_window` is `Option<WindowInfo>`. When `None`, return `app_name: "unknown"`, `window_title: ""` and skip accessibility/OCR.
- **No OCR**: Return empty `ocr_regions` vec. OCR requires `#[cfg(feature = "ocr")]`.
- **Concurrent with monitor loop**: The `ActivityMonitor::collect_context()` and `AccessibilityExtractor::extract_focused_element()` are both `&self` (shared reference). Safe for concurrent access.

#### Tests

- Unit test: Response DTO serialization
- Unit test: Graceful degradation when accessibility unavailable

---

## 4. Feature A3: AI Suggestions Panel

### 4.1 Problem

The suggestion system (`oneshim-suggestion` crate) handles SSE reception and priority queuing, but there are no IPC commands to query pending suggestions, view history, or submit feedback from the control box overlay.

### 4.2 Solution

Add IPC commands for the suggestion lifecycle:
- `get_pending_suggestions` — fetch current priority queue
- `get_suggestion_history` — fetch recent history
- `submit_suggestion_feedback` — accept/reject/defer a suggestion

### 4.3 Design

#### IPC Commands

```rust
// src-tauri/src/commands/suggestions.rs (NEW file)

#[tauri::command]
pub async fn get_pending_suggestions(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SuggestionViewDto>, String>

#[tauri::command]
pub async fn get_suggestion_history(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<SuggestionViewDto>, String>

#[tauri::command]
pub async fn submit_suggestion_feedback(
    state: tauri::State<'_, AppState>,
    suggestion_id: String,
    action: String,  // "accept" | "reject" | "defer"
) -> Result<(), String>
```

#### Response DTO

```rust
#[derive(Serialize)]
pub struct SuggestionViewDto {
    pub id: String,
    pub title: String,
    pub body: String,
    pub priority: String,       // "critical" | "high" | "medium" | "low"
    pub category: Option<String>,
    pub source: String,         // "server" | "local"
    pub created_at: String,
    pub is_read: bool,
}
```

#### Dependencies on AppState

New field required:

```rust
pub suggestion_manager: Option<Arc<SuggestionManager>>,
```

**`SuggestionManager`** is a new thin wrapper that holds:
- `SuggestionQueue` (from `oneshim-suggestion`) — wrapped in `tokio::sync::Mutex` (consistent with `SuggestionReceiver`'s existing `Arc<Mutex<SuggestionQueue>>` pattern)
- `SuggestionHistory` (from `oneshim-suggestion`) — wrapped in `tokio::sync::Mutex` (non-Sync, uses `VecDeque` with `&mut self`)
- `FeedbackSender` (from `oneshim-suggestion`)

```rust
pub struct SuggestionManager {
    queue: Arc<tokio::sync::Mutex<SuggestionQueue>>,     // SHARED with SuggestionReceiver
    history: Arc<tokio::sync::Mutex<SuggestionHistory>>,  // SHARED with SuggestionReceiver
    feedback: FeedbackSender,
    read_ids: tokio::sync::Mutex<HashSet<String>>,        // is_read tracking (local only)
}
```

**CRITICAL: Queue sharing with SuggestionReceiver.** `SuggestionReceiver` already holds `Arc<Mutex<SuggestionQueue>>` and actively pushes incoming SSE suggestions into it. `SuggestionManager` MUST use the **same** `Arc<Mutex<SuggestionQueue>>` instance — NOT create its own. Otherwise IPC commands read from a disconnected empty queue.

**Initialization pattern:**
```rust
// During app construction:
let queue = Arc::new(tokio::sync::Mutex::new(SuggestionQueue::new()));
let history = Arc::new(tokio::sync::Mutex::new(SuggestionHistory::new()));

// Pass same Arc to both:
let receiver = SuggestionReceiver::new(queue.clone(), history.clone(), ...);
let manager = SuggestionManager::new(queue.clone(), history.clone(), feedback);
```

Thread-safety: Both `SuggestionQueue` and `SuggestionHistory` use `&mut self` methods (`BTreeSet`, `VecDeque`). They MUST be wrapped in `Mutex` for concurrent access from IPC commands and the SSE receiver.

**Alternative**: Instead of a new wrapper, expose each component individually on AppState. However, the wrapper approach is cleaner since they form a cohesive unit.

#### Flow

```
get_pending_suggestions:
  → SuggestionManager.queue.lock().await.iter()   // iter() not peek_all() — peek_all does not exist
  → Map to Vec<SuggestionViewDto> via SuggestionPresenter
  → Already sorted by priority (BTreeSet maintains order)

get_suggestion_history:
  → SuggestionManager.history.lock().await.recent(limit.unwrap_or(20))
  → Map to Vec<SuggestionViewDto>

submit_suggestion_feedback:
  → Parse action string → dispatch to correct FeedbackSender method:
    match action.as_str() {
        "accept" => feedback.accept(&suggestion_id, None).await,
        "reject" => feedback.reject(&suggestion_id, None).await,
        "defer"  => feedback.defer(&suggestion_id, None).await,
        _ => Err("unknown action"),
    }
  → Note: FeedbackSender does NOT have a generic send() method.
    It exposes accept(), reject(), defer() each taking (id: &str, comment: Option<String>).
  → If "accept"/"reject": Remove from queue, add to history
  → If "defer": Keep in queue, lower priority
```

#### Suggestion Source Mapping

Suggestions come from two sources. The `SuggestionSource` enum has variants `RuleBased`, `LlmLocal`, `LlmServer`. Map to DTO:
- `LlmServer` → `"server"`
- `LlmLocal` | `RuleBased` → `"local"`

The `source` field in `SuggestionViewDto` indicates origin. Both use the same priority queue.

**Note on `SuggestionViewDto` vs existing `SuggestionView`**: The existing `SuggestionView` (in `presenter.rs`) has different fields (`priority_label`, `priority_color`, `type_icon`, etc.) optimized for the web dashboard. The new `SuggestionViewDto` is optimized for the overlay panel and defines its own field set. `Suggestion.suggestion_type` maps to `SuggestionViewDto.title` via the existing `type_to_title()` helper.

**Note on `is_read` field**: Neither `Suggestion` nor `SuggestionView` tracks read state. This must be tracked in `SuggestionManager` via a `HashSet<String>` of read suggestion IDs (in-memory, resets on restart).

#### Edge Cases

- **No SuggestionManager**: Return empty vec / `Err("Suggestions not available")`
- **Stale suggestions**: Queue auto-evicts when exceeding 50 items. No manual cleanup needed.
- **Server disconnected**: Local suggestions still work. `source: "local"` indicates offline-generated suggestions.

#### Tests

- Unit test: DTO serialization
- Unit test: Feedback action parsing
- Unit test: Queue ordering in response

---

## 5. Feature A4: Focus Mode

### 5.1 Problem

Users cannot enter a "focus mode" that suppresses interruptions. Currently there's no unified mechanism to:
- Suppress coaching messages
- Suppress desktop notifications
- Reduce capture frequency
- Signal the overlay to show a minimal "focus" indicator

### 5.2 Solution

Add a `FocusMode` state with IPC commands to toggle it. When active, Focus Mode:
1. **Suppresses coaching**: via `CoachingEngine` quiet hours mechanism
2. **Suppresses notifications**: via `NotificationManager` config
3. **Reduces capture**: via `SmartCaptureTrigger` importance threshold elevation
4. **Updates overlay**: Shows a focus indicator, hides coaching messages

### 5.3 Design

#### State Model

```rust
// src-tauri/src/focus_mode.rs (NEW file)

pub struct FocusModeState {
    active: AtomicBool,
    activated_at: RwLock<Option<DateTime<Utc>>>,
    duration_minutes: AtomicU32,  // 0 = indefinite
}

impl FocusModeState {
    pub fn new() -> Self { ... }
    pub fn is_active(&self) -> bool { ... }
    pub fn activate(&self, duration_minutes: u32) { ... }
    pub fn deactivate(&self) { ... }
    pub fn remaining_minutes(&self) -> Option<u32> { ... }
    pub fn check_expiry(&self) -> bool { ... }  // Returns true if expired and auto-deactivated
}
```

#### IPC Commands

```rust
// src-tauri/src/commands/focus.rs (NEW file)

#[tauri::command]
pub async fn toggle_focus_mode(
    state: tauri::State<'_, AppState>,
    active: bool,
    duration_minutes: Option<u32>,
) -> Result<FocusModeResponse, String>

#[tauri::command]
pub async fn get_focus_mode_status(
    state: tauri::State<'_, AppState>,
) -> Result<FocusModeResponse, String>
```

#### Response DTO

```rust
#[derive(Serialize)]
pub struct FocusModeResponse {
    pub active: bool,
    pub remaining_minutes: Option<u32>,
    pub activated_at: Option<String>,
}
```

#### Dependencies on AppState

New field:

```rust
pub focus_mode: Arc<FocusModeState>,
```

#### Integration Points

##### 5.3.1 Coaching Suppression

**Important**: Coaching suppression happens in the **scheduler's monitor loop** (which owns `Arc<oneshim_analysis::CoachingEngine>` — the concrete type), NOT through `AppState.coaching_engine: Arc<dyn CoachingPort>`. The `CoachingPort` trait only exposes `snooze_profile`, `record_feedback`, `all_goal_progress`, `update_regime_goals` — it does NOT expose `evaluate()`. Therefore the check must be at the call site:

```rust
// In monitor loop coaching evaluation section (src-tauri/src/scheduler/loops/monitor.rs):
// The focus_mode Arc is passed to spawn_monitor_loop()
if focus_mode.is_active() {
    // Skip coaching evaluation entirely — no coaching messages in focus mode
    // Analysis pipeline and regime state STILL update (important for C1)
} else {
    // Normal coaching evaluation path
    if let Some(msg) = coaching.evaluate(...).await { ... }
}
```

This is simpler and more reliable than manipulating quiet hours config, which would need to be restored on deactivation.

##### 5.3.2 Notification Suppression

The `NotificationManager` has config-driven cooldowns. Focus Mode check happens at the **call site** (monitor loop and notification loop), NOT inside `NotificationManager` — this avoids coupling `NotificationManager` to the focus concept:

```rust
// In notification loop (src-tauri/src/scheduler/loops/events.rs):
if !focus_mode.is_active() {
    notification_manager.check_long_session().await;
}

// In monitor loop idle check:
if !focus_mode.is_active() {
    notification_manager.check_idle(idle_secs).await;
}
```

**Critical notifications** (e.g., app errors, update required) should NOT be suppressed. Only coaching and productivity notifications.

##### 5.3.3 Capture Policy

Focus Mode does NOT disable capture entirely (user data collection continues for post-focus analysis). Instead, it elevates the importance threshold:

```rust
// In SmartCaptureTrigger decision logic (or monitor loop):
let effective_threshold = if focus_mode.is_active() {
    0.7  // Only capture significant events (app switch, errors)
} else {
    0.0  // Normal: capture everything above noise floor
};

if importance < effective_threshold {
    // Skip capture in focus mode
}
```

This is implemented in the monitor loop's capture decision, NOT inside `SmartCaptureTrigger` itself, to avoid adding non-port state to the vision crate.

##### 5.3.4 Overlay Integration

**NEW method to implement** on `MagicOverlayHandle` (does not exist yet):

```rust
// MagicOverlayHandle — NEW method:
pub fn emit_focus_mode(&self, active: bool) {
    let _ = self.app_handle.emit("overlay:focus-mode", serde_json::json!({ "active": active }));
}

// When focus mode activates (in toggle_focus_mode IPC command):
magic_overlay.emit_focus_mode(true);
// → Emits "overlay:focus-mode" event → frontend shows focus indicator

// When focus mode deactivates:
magic_overlay.emit_focus_mode(false);
```

##### 5.3.5 Auto-Expiry

Focus Mode with a duration auto-expires. Checked in the monitor loop:

```rust
// At start of each monitor tick:
if focus_mode.check_expiry() {
    // Was active, now expired → deactivate
    magic_overlay.emit_focus_mode(false);
    // Coaching and notifications resume automatically (flag is now false)
}
```

#### Edge Cases

- **Duration = 0**: Indefinite focus mode. Only manual deactivation.
- **App restart**: Focus mode state is NOT persisted. Resets on restart. This is intentional — focus sessions are transient.
- **Concurrent toggle**: `AtomicBool` ensures thread-safe toggle without races.
- **Focus + Manual Capture (A1)**: Manual capture works in focus mode (user explicitly requested it).

#### Tests

- Unit test: FocusModeState activate/deactivate/expiry
- Unit test: IPC command response serialization
- Integration test: Coaching suppression when focus active

---

## 6. Feature C1: Coaching Regime Context Sharing

### 6.1 Problem

The coaching feedback evaluation loop (`spawn_coaching_loop` in `loops/events.rs`) currently uses **placeholder values** for `regime_id` and `app_name`:

```rust
// Current (Phase 1 placeholder) — note: 3 parameters, not 2:
engine.evaluate_implicit_feedback(None, "", Utc::now()).await;
// TODO(Phase 2): pass real current_regime_id and current_app
```

**Important**: `evaluate_implicit_feedback` is defined on the **concrete** `CoachingEngine` type, NOT on the `CoachingPort` trait. The coaching loop in `loops/intelligence.rs` holds `Option<Arc<oneshim_analysis::CoachingEngine>>` (concrete type), which is correct for this call.

This means implicit feedback classification cannot distinguish regime transitions (positive signal) from same-app continuation (neutral signal), reducing coaching effectiveness.

### 6.2 Solution

Share the monitor loop's current regime state with the coaching feedback loop via `Arc`-wrapped atomic/lock-free state.

### 6.3 Design

#### Shared State Structure

```rust
// src-tauri/src/scheduler/shared_regime_state.rs (NEW file)

use std::sync::Arc;
use parking_lot::RwLock;

#[derive(Debug, Clone)]
pub struct RegimeSnapshot {
    pub regime_id: Option<String>,
    pub regime_label: Option<String>,
    pub current_app: String,
    pub updated_at: DateTime<Utc>,
}

pub struct SharedRegimeState {
    inner: RwLock<RegimeSnapshot>,
}

impl SharedRegimeState {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(RegimeSnapshot {
                regime_id: None,
                regime_label: None,
                current_app: String::new(),
                updated_at: Utc::now(),
            }),
        }
    }

    /// Called by monitor loop after regime classification
    pub fn update(&self, regime_id: Option<&str>, label: Option<&str>, app: &str) {
        let mut guard = self.inner.write();
        guard.regime_id = regime_id.map(|s| s.to_string());
        guard.regime_label = label.map(|s| s.to_string());
        guard.current_app = app.to_string();
        guard.updated_at = Utc::now();
    }

    /// Called by coaching loop to get current state
    pub fn snapshot(&self) -> RegimeSnapshot {
        self.inner.read().clone()
    }
}
```

#### Integration: Monitor Loop (Writer)

```rust
// In monitor loop, after run_analysis_tick():
if let Some(ref shared_regime) = shared_regime_state {
    shared_regime.update(
        ts.current_regime_id.as_deref(),
        current_regime_label.as_deref(),
        &app_name,
    );
}
```

#### Integration: Coaching Loop (Reader)

```rust
// In spawn_coaching_loop() (loops/intelligence.rs), replace placeholder:
// BEFORE (3 params — note the Utc::now() third argument):
engine.evaluate_implicit_feedback(None, "", Utc::now()).await;

// AFTER:
let snap = shared_regime.snapshot();
engine.evaluate_implicit_feedback(
    snap.regime_id.as_deref(),
    &snap.current_app,
    Utc::now(),
).await;
```

#### Wiring

```rust
// In Scheduler construction or run_scheduler_loops():
let shared_regime = Arc::new(SharedRegimeState::new());

// Pass Arc clone to monitor loop
spawn_monitor_loop(..., shared_regime.clone(), ...);

// Pass Arc clone to coaching loop
spawn_coaching_loop(..., shared_regime.clone(), ...);
```

#### Why parking_lot::RwLock (not tokio::sync::RwLock, AtomicPtr, or channel)

- **parking_lot::RwLock**: Simple, correct, no poisoning (unlike std). Chosen intentionally over `tokio::sync::RwLock` because `SharedRegimeState` is accessed from both async (coaching loop) and potentially sync contexts. `parking_lot` is already a workspace dependency. Monitor writes ~1/sec, coaching reads ~1/30s — zero contention in practice.
- **tokio::sync::RwLock**: Would also work but requires `.await` on every read/write, adding unnecessary overhead for a <1μs operation.
- **AtomicPtr**: Unsafe, complex lifetime management for String fields.
- **Channel**: Coaching loop needs latest state, not every update. Channel would require draining.

#### Edge Cases

- **Monitor loop not yet started**: Coaching loop reads default snapshot (regime_id: None, app: ""). Same as current Phase 1 behavior — no regression.
- **Monitor loop crashed**: Snapshot becomes stale. `updated_at` field allows the coaching loop to detect staleness if needed (not required for initial implementation).
- **Ordering**: No strict ordering needed. Coaching loop tolerates reading slightly stale data (worst case: reads state from 1 second ago).

#### Tests

- Unit test: SharedRegimeState update + snapshot
- Unit test: Default snapshot when no updates
- Integration test: Writer/reader from different tokio tasks

---

## 7. Cross-Feature Interactions

### 7.1 A1 + A4: Manual Capture in Focus Mode

Manual capture (A1) MUST work when Focus Mode (A4) is active. This is explicit user intent and should not be suppressed.

### 7.2 A2 + A1: Scene Analysis Reuses Capture

Scene Analysis (A2) calls `FrameProcessor::capture_and_process()` internally. This is the same path as Manual Capture (A1) but with a different importance level (0.8 vs 1.0). The `prev_frame` mutex in `EdgeFrameProcessor` serializes concurrent calls — safe but potentially slow if both triggered simultaneously.

### 7.3 A4 + C1: Focus Mode Uses Coaching Engine

Focus Mode (A4) suppresses coaching at the monitor loop level (before `evaluate()` is called). C1's shared regime state still updates even in focus mode — the monitor loop's analysis pipeline runs regardless. This ensures that when focus mode ends, coaching has accurate regime context.

### 7.4 A3 + A4: Suggestions in Focus Mode

Pending suggestions (A3) are still queryable in Focus Mode (A4). The queue accumulates. Focus Mode only suppresses proactive notification delivery, not the data pipeline.

---

## 8. AppState Additions Summary

```rust
// All new fields in AppState (runtime_state.rs):
pub struct AppState {
    // ... existing fields ...

    // A1 + A2: Shared from Scheduler construction
    pub frame_processor: Option<Arc<dyn FrameProcessor>>,
    pub frame_storage: Option<Arc<FrameFileStorage>>,
    pub activity_monitor: Option<Arc<dyn ActivityMonitor>>,
    pub accessibility_extractor: Option<Arc<dyn AccessibilityExtractor>>,
    pub consent_manager: Option<Arc<ConsentManager>>,  // A2: needed for PII level in accessibility

    // A3: Suggestion access
    pub suggestion_manager: Option<Arc<SuggestionManager>>,

    // A4: Focus mode
    pub focus_mode: Arc<FocusModeState>,
}
```

All fields are `Option<Arc<...>>` to maintain the existing pattern where features degrade gracefully when components are unavailable.

---

## 9. New Files Summary

| File | Purpose | Feature |
|------|---------|---------|
| `src-tauri/src/commands/capture.rs` | Manual Capture + Scene Analysis IPC | A1, A2 |
| `src-tauri/src/commands/suggestions.rs` | Suggestion panel IPC | A3 |
| `src-tauri/src/commands/focus.rs` | Focus Mode IPC | A4 |
| `src-tauri/src/focus_mode.rs` | FocusModeState struct | A4 |
| `src-tauri/src/scheduler/shared_regime_state.rs` | SharedRegimeState for cross-loop sharing | C1 |
| `src-tauri/src/suggestion_manager.rs` | SuggestionManager wrapper | A3 |

---

## 10. Command & Module Registration

### 10.1 Module Registration

New modules must be added to `src-tauri/src/commands/mod.rs`:

```rust
pub mod capture;      // A1, A2
pub mod focus;        // A4
pub mod suggestions;  // A3
```

Current existing modules: `analysis`, `capture_status`, `coaching`, `dashboard`, `integration`, `onboarding`, `settings`, `system`.

### 10.2 Handler Registration

All new commands must be added to `tauri::generate_handler!` in `src-tauri/src/main.rs`:

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    commands::capture::trigger_manual_capture,
    commands::capture::analyze_current_scene,
    commands::suggestions::get_pending_suggestions,
    commands::suggestions::get_suggestion_history,
    commands::suggestions::submit_suggestion_feedback,
    commands::focus::toggle_focus_mode,
    commands::focus::get_focus_mode_status,
])
```

---

## 11. Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| AppState field explosion | Maintenance burden | Use Option<Arc<T>> consistently, document each field |
| Concurrent capture (A1+A2) | Mutex contention on prev_frame | Both complete in <100ms, acceptable for user-triggered actions |
| SuggestionManager coupling | New wrapper adds indirection | Keep wrapper thin — delegate to existing queue/history/feedback |
| Focus mode state loss on restart | User re-enters focus mode | Intentional — focus sessions are transient; persisting would need cleanup logic |
| SharedRegimeState stale reads | Coaching uses stale regime for feedback | 1-second staleness is acceptable; coaching operates on 30s intervals |

---

## 12. Out of Scope

- Frontend UI implementation (overlay HTML/CSS/JS) — separate task
- Server-side suggestion generation changes
- Persistent focus mode across app restarts
- Focus mode scheduling (automatic focus hours)
- Keyboard shortcuts for new features (can be added later)
