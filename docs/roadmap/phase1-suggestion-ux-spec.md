# Phase 1 Spec: Suggestion Real-World UX

**Date:** 2026-04-04
**Baseline:** v0.4.18 stable
**Branch:** `feat/analysis-wiring-v2`
**Scope:** 5 tasks — SSE reconnection, notification badge, history UI, deferred management, feedback error handling

---

## 1. Current State Analysis

### Architecture Overview

```
Server SSE Stream
    |
    v
SseStreamClient::connect()          # Has backoff (1s→30s), Last-Event-ID
    |  mpsc::Sender<SseEvent>
    v
SuggestionReceiver::run()           # NO reconnection — breaks on Close/Error
    |
    +---> FeedbackScorer::adjust()  # Relevance adjustment
    +---> SuggestionQueue::push()   # In-memory BTreeSet (max 50)
    +---> DesktopNotifier           # OS-native notification
    |
    v
spawn_suggestion_loop()             # Spawns once, no restart
    |
    v
Tauri Event: overlay:suggestions-changed
    |
    v
useOverlayEvents → IPC → SuggestionsPanel → SuggestionItem
```

### Identified Gaps

| Component | Current | Gap |
|-----------|---------|-----|
| SSE reconnection | `SseStreamClient` has backoff, but `SuggestionReceiver::run()` exits on Close with no retry | Receiver-level reconnection loop missing |
| New suggestion event | `emit_suggestions_changed` called only after feedback, not on arrival | No push notification to frontend on new arrival |
| Badge count | No badge UI element | Frontend has no visual indicator of unread count |
| Desktop notification | `DesktopNotifier::show_suggestion()` fires for ALL priorities (no filter) | May want to gate Low-priority from OS notifications |
| History UI | `get_suggestion_history` IPC exists, `SuggestionHistory` in-memory (max 100) | No History tab component in overlay |
| History feedback | `history.record_feedback()` is NEVER called in IPC layer — all entries have `feedback: None` | **BUG**: Must fix before History UI can show feedback badges |
| Deferred re-queue | `defer` action keeps item in active queue, does NOT move to history | No snooze/re-queue mechanism; behavior change needed |
| Feedback retry | `FeedbackSender` single attempt | No retry queue, no visual feedback on failure |
| Queue persistence | In-memory only | Lost on app restart |

---

## 2. Task Specifications

### 2.1 SSE Reconnection (Task 1.1)

**Goal:** `SuggestionReceiver` automatically reconnects when SSE stream closes or errors, preserving queue state.

#### Design

The reconnection loop belongs in `spawn_suggestion_loop` (scheduler level), not inside `SuggestionReceiver::run()`. Rationale:
- `SuggestionReceiver::run()` is a clean single-connection lifecycle
- The scheduler already owns shutdown coordination via `watch::Receiver<bool>`
- Backoff state belongs to the loop controller, not the business logic

```
spawn_suggestion_loop:
    loop {
        match receiver.run(&session_id).await {
            Ok(()) => {
                // Clean close — reconnect after delay
                info!("SSE stream closed, reconnecting...");
            }
            Err(e) => {
                warn!("SSE error: {e}, reconnecting...");
            }
        }
        
        // Check shutdown before sleeping
        if *shutdown_rx.borrow() { break; }
        
        // Exponential backoff: 1s, 2s, 4s, 8s, 16s, 30s (cap)
        tokio::select! {
            _ = tokio::time::sleep(backoff_delay) => {}
            _ = shutdown_rx.changed() => { break; }
        }
        
        backoff_delay = (backoff_delay * 2).min(MAX_BACKOFF);
        consecutive_failures += 1;
    }
```

**Backoff reset:** Use a time-based heuristic at the scheduler level — no change to `SuggestionReceiver::run()` return type needed. Keep `Result<(), SuggestionError>`:

```
// In spawn_suggestion_loop:
let start = Instant::now();
let result = receiver.run(&session_id).await;
let ran_for = start.elapsed();

// If the session ran for >10s, it connected successfully → reset backoff
if ran_for > Duration::from_secs(10) {
    backoff_delay = INITIAL_BACKOFF;  // 1s
}
```

Rationale: `SseEvent::Connected` is received inside `run()` and not visible to the scheduler. A time-based heuristic avoids adding a new return type while achieving the same goal — if the receiver ran for a meaningful duration, it had a live connection.

**Queue preservation:** No change needed — `SuggestionQueue` is `Arc<tokio::sync::Mutex<_>>` (async mutex) shared between receiver and manager. Reconnection creates a new `run()` call on the same receiver instance, which shares the same queue Arc.

#### Files Modified
- `src-tauri/src/scheduler/loops/suggestions.rs` — Add reconnection loop with backoff

#### Acceptance Criteria
- [ ] SSE stream auto-reconnects after server-initiated close
- [ ] Exponential backoff: 1s → 2s → 4s → 8s → 16s → 30s cap
- [ ] Backoff resets after successful connection + first event
- [ ] Queue state preserved across reconnections
- [ ] Clean shutdown via watch channel still works
- [ ] Existing tests pass; new tests for reconnection scenarios

---

### 2.2 New Suggestion Notification (Task 1.2)

**Goal:** Frontend receives real-time push when new suggestions arrive; badge shows unread count.

#### Design

**Rust side:** After `SuggestionReceiver::handle_suggestion()` successfully pushes to queue, emit a Tauri event with the updated count. The receiver needs access to `MagicOverlayHandle` (or a new lightweight event emitter trait).

**Problem:** `SuggestionReceiver` lives in `oneshim-suggestion` crate which has no Tauri dependency. We cannot pass `MagicOverlayHandle` directly.

**Solution:** Callback-based notification. Add an optional callback to `SuggestionReceiver`:

```rust
pub type OnNewSuggestion = Arc<dyn Fn(usize) + Send + Sync>;  // count

pub struct SuggestionReceiver {
    sse_client: Arc<dyn SseClient>,
    notifier: Option<Arc<dyn DesktopNotifier>>,
    queue: Arc<Mutex<SuggestionQueue>>,
    scorer: Arc<Mutex<FeedbackScorer>>,
    on_new: Option<OnNewSuggestion>,  // NEW
}
```

At wiring time in `src-tauri/src/app_runtime_launch.rs` (where `SuggestionReceiver` is constructed), the callback captures `MagicOverlayHandle` and calls `emit_suggestions_changed()`.

**Note on dual notification:** The `on_new` callback (overlay badge) and existing `DesktopNotifier::show_suggestion()` (OS notification) serve different channels:
- `on_new` → overlay badge count update (always fires on accepted suggestion)
- `DesktopNotifier` → OS-native notification (currently fires for ALL priorities, no filter)
Both fire after successful `queue.push()`. This is intentional — badge is visual indicator, OS notification is attention-grabbing alert. No double-notification risk since they target different surfaces.

**Frontend side:** The `overlay:suggestions-changed` event listener already exists and fetches suggestions via IPC. The handler re-fetches all suggestions (ignores the event's `count` field). No change needed for data flow — just need badge UI.

**Badge component:** Add a floating badge indicator near the suggestions panel toggle area.

```typescript
// In useOverlayEvents reducer, track unread count
interface OverlayState {
    // ... existing fields
    suggestionBadgeCount: number;  // NEW — derived in reducer, cleared on panel open
}
```

**Badge behavior:**
- Computed in reducer: when `set-suggestions` action arrives, compare new array length to previous — if increased, bump badge count by the delta
- Resets to 0 when `toggle-suggestions-panel` opens the panel (not on close)
- Animate (pulse) on increment
- **No race condition:** badge count derived from reducer state transition, not from event payload

#### Files Modified
- `crates/oneshim-suggestion/src/receiver.rs` — Add `on_new` callback, invoke after successful push
- `src-tauri/src/agent_runtime_support.rs` — Wire callback to `emit_suggestions_changed`
- `src-tauri/src/magic_overlay.rs` — Ensure emission works from callback context
- `crates/oneshim-web/frontend/src/overlay/hooks/useOverlayEvents.ts` — Add `suggestionBadgeCount` to state
- `crates/oneshim-web/frontend/src/overlay/components/SuggestionBadge.tsx` — NEW badge component
- `crates/oneshim-web/frontend/src/overlay/types.ts` — Add badge count to OverlayState

#### Acceptance Criteria
- [ ] New suggestion arrival emits `overlay:suggestions-changed` event immediately
- [ ] Badge shows unread count when panel is closed
- [ ] Badge clears when panel opens
- [ ] Desktop notification fires for all accepted suggestions (existing behavior — no priority filter)
- [ ] Badge animates on new arrival (CSS pulse)
- [ ] No Tauri dependency added to `oneshim-suggestion` crate

---

### 2.3 History UI (Task 1.3)

**Goal:** Users can view past suggestions with their feedback status in a History tab.

#### Design

**Tab navigation:** Add tabs to `SuggestionsPanel` — "Active" (current queue) and "History" (past suggestions).

**Data source:** `get_suggestion_history` IPC already exists. It reads from `SuggestionHistory` (in-memory `VecDeque<HistoryEntry>`, max 100 entries). Each entry has `suggestion: Suggestion` and `feedback: Option<FeedbackType>`.

**Prerequisite bug fix:** `submit_suggestion_feedback` currently calls `history.add(suggestion)` (which sets `feedback: None`) but NEVER calls `history.record_feedback(id, feedback_type)`. All history entries have `feedback: None`. **Must fix:** after `history.add()`, immediately call `record_feedback()` with the appropriate `FeedbackType` in the accept/reject arms of `submit_suggestion_feedback`.

**Problem:** `SuggestionHistory` is in-memory only (max 100 entries). App restart loses all history.

**Solution for Phase 1:** Accept in-memory limitation. Phase 4 (offline mode) will add SQLite persistence. The unified `suggestions` table already has `shown_at`, `dismissed_at`, `acted_at` columns that can back persistent history later.

**Frontend component:**

```
SuggestionsPanel
├── TabBar: [Active | History]
├── ActiveTab (existing SuggestionItem list)
└── HistoryTab (NEW)
    ├── HistoryItem
    │   ├── SuggestionItem (compact variant)
    │   └── FeedbackBadge (Accepted ✓ / Rejected ✗ / Deferred ⏸ / Pending ?)
    └── HistoryStats (accepted/rejected/deferred counts)
```

**IPC response augmentation:** The current `SuggestionViewDto` doesn't include feedback status. Need to add:

```rust
#[derive(Serialize)]
pub struct SuggestionHistoryDto {
    pub suggestion: SuggestionViewDto,
    pub feedback: Option<String>,      // "accepted" | "rejected" | "deferred" | null
    pub feedback_at: Option<String>,   // RFC3339 timestamp
}
```

**New IPC command:** `get_suggestion_history_with_feedback` — or modify existing `get_suggestion_history` to return `SuggestionHistoryDto[]`.

#### Files Modified
- `src-tauri/src/commands/suggestions.rs` — Update `get_suggestion_history` return type to include feedback
- `crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx` — Add tab navigation
- `crates/oneshim-web/frontend/src/overlay/components/SuggestionHistory.tsx` — NEW history list component
- `crates/oneshim-web/frontend/src/overlay/components/HistoryItem.tsx` — NEW history item with feedback badge
- `crates/oneshim-web/frontend/src/overlay/types.ts` — Add `SuggestionHistoryDto` type

#### Acceptance Criteria
- [ ] **Prerequisite:** `record_feedback()` called after `add()` in accept/reject arms — history entries have correct feedback
- [ ] Tab bar switches between Active and History views
- [ ] History shows suggestions with feedback status badges
- [ ] History sorted newest-first
- [ ] Stats summary (accepted/rejected/deferred counts) displayed
- [ ] Empty state when no history
- [ ] History loads via existing IPC with augmented DTO (includes feedback field)

---

### 2.4 Deferred Management (Task 1.4)

**Goal:** Deferred suggestions re-surface after a configurable delay with snooze options.

#### Design

**Snooze model:**

```rust
pub struct DeferredEntry {
    pub suggestion: Suggestion,
    pub deferred_at: DateTime<Utc>,
    pub resurface_at: DateTime<Utc>,
}
```

**Snooze durations:** 30m, 1h, 2h, 4h, Tomorrow 9AM (5 options).

**Where to manage:** Add `DeferredManager` to `oneshim-suggestion` crate:

```rust
pub struct DeferredManager {
    items: VecDeque<DeferredEntry>,
    max_size: usize,  // 50
}

impl DeferredManager {
    pub fn defer(&mut self, suggestion: Suggestion, duration: Duration) -> bool;
    pub fn collect_resurfaced(&mut self) -> Vec<Suggestion>;
    pub fn pending_count(&self) -> usize;
    pub fn list_deferred(&self) -> Vec<&DeferredEntry>;
    pub fn cancel(&mut self, suggestion_id: &str) -> Option<Suggestion>;
}
```

**Re-queue mechanism:** The maintenance scheduler loop (every 30s, shared with Task 1.5 retry processing) calls `collect_resurfaced()` and pushes results back into `SuggestionQueue`.

**Integration with feedback flow — behavioral change from current defer:**

Current behavior (`suggestions.rs:118-144`): `defer` action sends server feedback + records scorer feedback, but **keeps item in active queue** and does NOT move to history.

New behavior with snooze:
1. Send server feedback via `FeedbackSender::defer()` (unchanged)
2. Record scorer feedback (unchanged)
3. **Remove from active queue** (`queue.remove_by_id()`) — behavioral change
4. **Push to DeferredManager** with user-selected duration (new)
5. Emit `suggestions-changed` with updated count

The `submit_suggestion_feedback` IPC command needs a `snooze_minutes: Option<u32>` parameter. Tauri v2 IPC deserialization handles missing `Option` fields as `None` — if JS omits `snoozeMinutes`, Rust receives `None`. For backward compat, `None` means default snooze (2 hours).

**Frontend snooze UI:** When user clicks "Later" on a suggestion, show a dropdown/popover with duration options instead of immediate defer.

```
SuggestionItem "Later" button
    └── SnoozePopover
        ├── 30 minutes
        ├── 1 hour
        ├── 2 hours
        ├── 4 hours
        └── Tomorrow 9 AM
```

#### Files Modified
- `crates/oneshim-suggestion/src/deferred.rs` — NEW `DeferredManager`
- `crates/oneshim-suggestion/src/lib.rs` — Export deferred module
- `src-tauri/src/suggestion_manager.rs` — Add `DeferredManager` to managed state
- `src-tauri/src/runtime_state.rs` — Thread `DeferredManager` through `SuggestionRuntimeState`
- `src-tauri/src/commands/suggestions.rs` — Add `snooze_minutes` param to defer action
- `src-tauri/src/scheduler/loops/suggestions.rs` — Add periodic resurfacing check
- `crates/oneshim-web/frontend/src/overlay/components/SuggestionItem.tsx` — Snooze popover on "Later"
- `crates/oneshim-web/frontend/src/overlay/components/SnoozePopover.tsx` — NEW duration picker

#### Acceptance Criteria
- [ ] "Later" button shows snooze duration options
- [ ] Deferred suggestion re-appears in queue after selected duration
- [ ] Re-surfaced suggestion triggers badge count update
- [ ] Server-side defer feedback sent immediately (not delayed)
- [ ] Max 50 deferred items (FIFO eviction)
- [ ] Deferred list viewable in History tab (with "Snoozed until X" label)
- [ ] In-memory only (persistence deferred to Phase 4)

---

### 2.5 Feedback Error Handling (Task 1.5)

**Goal:** Feedback failures show user-visible toast, retry automatically, and show visual confirmation on success.

#### Design

**Retry queue:** Add `FeedbackRetryQueue` in a separate file `crates/oneshim-suggestion/src/feedback_retry.rs` (not in `feedback.rs` — different responsibility: scheduling vs. API calls):

```rust
pub struct PendingFeedback {
    pub suggestion_id: String,
    pub feedback_type: FeedbackType,
    pub comment: Option<String>,
    pub attempts: u32,
    pub next_retry_at: DateTime<Utc>,
}

pub struct FeedbackRetryQueue {
    items: VecDeque<PendingFeedback>,
    max_size: usize,      // 100
    max_attempts: u32,     // 5
}

impl FeedbackRetryQueue {
    pub fn enqueue(&mut self, feedback: PendingFeedback);
    pub fn collect_ready(&mut self) -> Vec<PendingFeedback>;
    pub fn retry_failed(&mut self, feedback: PendingFeedback);  // Re-enqueue with incremented attempt + backoff
    pub fn drop_exhausted(&mut self, suggestion_id: &str);       // Remove after max attempts
    pub fn pending_count(&self) -> usize;
}
```

**Retry schedule:** 5s, 15s, 45s, 2m, 5m (exponential with 3x multiplier, capped).

**Integration:** The scheduler loop that handles resurfacing (Task 1.4) also processes the retry queue every 30s.

**Frontend feedback states:**

```typescript
type FeedbackStatus = 'idle' | 'sending' | 'success' | 'failed' | 'retrying';
```

Per-suggestion feedback state tracked in component local state (not overlay reducer — ephemeral UI state).

**Toast notification:** Use a lightweight toast system. Add a `ToastContainer` to the overlay root, dispatch toasts from the feedback handler.

```
Feedback flow:
1. User clicks Accept/Reject/Later
2. UI shows "sending" spinner on the button
3. IPC call to submit_suggestion_feedback
4. Success → show green checkmark for 2s → remove from active list
5. Failure → show red X + toast "Feedback failed, will retry" → enqueue to retry queue
6. Retry success → silent (suggestion already removed from active)
7. Retry exhausted → toast "Could not send feedback for [title]"
```

**Visual confirmation on suggestion card:**
- Sending: button replaced with spinner
- Success: card slides out with green flash
- Failed: card stays, button shows retry icon, toast appears

#### Files Modified
- `crates/oneshim-suggestion/src/feedback_retry.rs` — NEW `FeedbackRetryQueue`, `PendingFeedback`
- `crates/oneshim-suggestion/src/lib.rs` — Export feedback_retry module
- `src-tauri/src/suggestion_manager.rs` — Add retry queue to managed state
- `src-tauri/src/runtime_state.rs` — Thread `FeedbackRetryQueue` through `SuggestionRuntimeState`
- `src-tauri/src/commands/suggestions.rs` — Return feedback status, handle retry enqueue
- `src-tauri/src/scheduler/loops/suggestions.rs` — Add retry processing to periodic loop
- `crates/oneshim-web/frontend/src/overlay/components/SuggestionItem.tsx` — Feedback state UI
- `crates/oneshim-web/frontend/src/overlay/components/Toast.tsx` — NEW toast component
- `crates/oneshim-web/frontend/src/overlay/components/ToastContainer.tsx` — NEW toast manager
- `crates/oneshim-web/frontend/src/overlay/App.tsx` — Mount ToastContainer

#### Acceptance Criteria
- [ ] Feedback failure shows toast notification
- [ ] Failed feedback auto-retries (5s, 15s, 45s, 2m, 5m)
- [ ] Max 5 retry attempts per feedback
- [ ] Visual spinner during send, checkmark on success, X on failure
- [ ] Card animates out on successful feedback
- [ ] Retry queue max 100 items
- [ ] Exhausted retries show final toast notification

---

## 3. Cross-Cutting Concerns

### 3.1 Scheduler Loop Design

Tasks 1.1, 1.4, and 1.5 all add work to the suggestion scheduler. Use **two separate spawn functions** (not nested spawns) to maintain the existing pattern where each `JoinHandle` is tracked in `run_scheduler_loops` for clean shutdown:

```rust
// Returns two handles — both tracked by run_scheduler_loops
pub(crate) fn spawn_suggestion_sse_loop(
    receiver: Arc<SuggestionReceiver>,
    session_id: String,
    shutdown_rx: watch::Receiver<bool>,
) -> JoinHandle<()>   // SSE reconnection loop (Task 1.1)

pub(crate) fn spawn_suggestion_maintenance_loop(
    queue: Arc<Mutex<SuggestionQueue>>,
    deferred: Arc<Mutex<DeferredManager>>,
    retry_queue: Arc<Mutex<FeedbackRetryQueue>>,
    feedback: Arc<FeedbackSender>,
    overlay: Option<MagicOverlayHandle>,
    shutdown_rx: watch::Receiver<bool>,
) -> JoinHandle<()>   // 30s periodic: resurfacing + retry (Task 1.4 + 1.5)
```

Each function receives its own `shutdown_rx` clone. Both handles are gated behind `Option<JoinHandle>` with `#[cfg(feature = "server")]` feature flag, matching the existing `suggestion_task` pattern in `sync.rs:416-424` (not in the main `tasks` vec).

### 3.2 No New Crate Dependencies

All features implementable with existing dependencies:
- `tokio::time` for backoff/timers
- `serde` for new DTOs
- `chrono` for timestamps
- No new Cargo.toml changes needed

### 3.3 Thread Safety Model

All new types (`DeferredManager`, `FeedbackRetryQueue`) will be wrapped in `Arc<tokio::sync::Mutex<_>>` (async mutex, matching existing pattern) and shared via `SuggestionRuntimeState` in Tauri managed state. Same pattern as existing `SuggestionQueue`.

### 3.4 Frontend Component Sizing

| Component | Estimated Lines | Complexity |
|-----------|----------------|------------|
| SuggestionBadge.tsx | ~40 | Low |
| SuggestionHistory.tsx | ~80 | Medium |
| HistoryItem.tsx | ~50 | Low |
| SnoozePopover.tsx | ~60 | Medium |
| Toast.tsx + ToastContainer.tsx | ~80 | Medium |
| Tab navigation in SuggestionsPanel | ~30 | Low |

Total frontend: ~340 lines new code.

### 3.5 Testing Strategy

| Layer | Test Type | Scope |
|-------|-----------|-------|
| `oneshim-suggestion` | Unit | DeferredManager, FeedbackRetryQueue, reconnection backoff |
| `src-tauri` | Unit | Scheduler reconnection logic, retry processing |
| Frontend | Manual | Badge, toast, history tab, snooze popover |

Estimated new test count: ~25-35 Rust tests.

---

## 4. Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| SSE backoff too aggressive → slow recovery | Medium | Reset on successful connection, not on connect attempt |
| Deferred items lost on restart | Low | Accepted for Phase 1; Phase 4 adds persistence |
| Retry queue memory growth | Low | Max 100 items with FIFO eviction |
| Badge count drift (race between events) | Low | Always re-fetch from source via IPC, not increment locally |
| Frontend toast spam | Medium | Deduplicate by suggestion_id, max 3 visible toasts |
| `on_new` callback panic safety | Medium | Catch panic in callback invocation with `std::panic::catch_unwind` |

---

## 5. Out of Scope (Phase 4+)

- SQLite persistence for queue/history/deferred
- Suggestion source filtering UI
- Statistics dashboard
- Chat integration (Phase 2)
