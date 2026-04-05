# Phase 1 Gap Completion Spec — Deferred Hydration + Feedback Retry Wiring

**Date**: 2026-04-05
**Scope**: `oneshim-core`, `oneshim-suggestion`, `oneshim-storage`, `src-tauri`
**Prerequisite**: v0.4.20 (Phase 1 items #1-#3 complete)

## 1. Problem Statement

Phase 1 "Suggestion Real-World UX" has two incomplete items. Infrastructure exists but critical wiring is missing.

### Gap #4: Deferred Management — Restart Hydration

| Component | Current State | Issue |
|-----------|--------------|-------|
| `DeferredManager` | In-memory VecDeque, max 50 | Starts empty on every launch |
| `save_suggestion_state` IPC | Correctly persists `state="deferred"` + `resurface_at` to SQLite | Save path works |
| `list_suggestions_by_state` | Returns `SuggestionRecord` | **Does NOT return `resurface_at`** — SELECT omits it |
| `SuggestionRecord` | Has 15 fields | **Missing `resurface_at` field** |
| Startup (app_runtime_launch) | Restores `state="pending"` items to queue | **No code for `state="deferred"` items** |
| Frontend | SnoozePopover sends defer action | **No IPC to list currently deferred items** |

**Impact**: User snoozes a suggestion, restarts the app, suggestion is lost forever.

### Gap #5: Feedback Retry Queue — IPC Wiring

| Component | Current State | Issue |
|-----------|--------------|-------|
| `FeedbackRetryQueue` | Complete: enqueue, collect_ready, retry_failed, 11 tests | Infrastructure ready |
| `submit_suggestion_feedback` IPC | Calls `feedback.accept()/reject()/defer()` | **On error: returns Err immediately, never enqueues** |
| Maintenance loop | Processes retry_queue.collect_ready() every 30s | **Queue is always empty — nothing enqueues** |
| Persistence | None | In-memory only, lost on restart |

**Impact**: If server is temporarily unreachable, user clicks "Accept" → error toast → feedback permanently lost.

## 2. Goals

1. Deferred suggestions survive app restarts — hydrate from SQLite on startup
2. Frontend can query the list of currently deferred suggestions
3. Failed feedback submissions are automatically retried via the existing retry queue
4. Failed feedback survives app restarts (SQLite persistence)

### Non-Goals

- Changes to DeferredManager/FeedbackRetryQueue core logic — infrastructure is correct
- Frontend UI changes beyond consuming new IPC commands
- Server-side retry logic

## 3. Design

### 3.1 Gap #4: Deferred Hydration

#### 3.1.1 Extend `SuggestionRecord` with `resurface_at`

**File**: `crates/oneshim-core/src/models/storage_records.rs`

Add field:
```rust
pub struct SuggestionRecord {
    // ... existing 15 fields ...
    pub resurface_at: Option<String>,  // NEW — RFC3339 timestamp
}
```

#### 3.1.2 Extend `list_suggestions_by_state` SELECT

**File**: `crates/oneshim-storage/src/sqlite/edge_intelligence/suggestions.rs`

Add `resurface_at` to the SELECT clause (column index 15):
```sql
SELECT id, suggestion_id, suggestion_type, source, content, priority,
       confidence_score, relevance_score, is_actionable, reasoning,
       shown_at, dismissed_at, acted_at, created_at, expires_at,
       resurface_at                                    -- ADD
FROM suggestions WHERE state = ?1
ORDER BY created_at DESC LIMIT ?2
```

Add `resurface_at: row.get(15)?` to the row mapping.

#### 3.1.3 Add `DeferredManager::restore()` method

**File**: `crates/oneshim-suggestion/src/deferred.rs`

```rust
/// Bulk-restore deferred entries from storage records.
/// Skips items whose resurface_at has already passed (they go to resurfaced vec).
pub fn restore(
    &mut self,
    entries: Vec<(Suggestion, DateTime<Utc>, DateTime<Utc>)>, // (suggestion, deferred_at, resurface_at)
) -> Vec<Suggestion> {
    let now = Utc::now();
    let mut already_due = Vec::new();
    for (suggestion, deferred_at, resurface_at) in entries {
        if resurface_at <= now {
            already_due.push(suggestion);
        } else if self.items.len() < self.max_size {
            self.items.push_back(DeferredEntry {
                suggestion,
                deferred_at,
                resurface_at,
            });
        }
    }
    already_due
}
```

This returns suggestions that are already past their resurface time, so the caller can push them directly to the queue.

#### 3.1.4 Startup hydration in `app_runtime_launch.rs`

After the existing pending-restoration block (lines 127-147), add deferred restoration:

```rust
// Restore deferred suggestions from storage
let deferred_records = sqlite_storage
    .list_suggestions_by_state("deferred", 50)
    .unwrap_or_default();
if !deferred_records.is_empty() {
    let entries: Vec<_> = deferred_records
        .into_iter()
        .filter_map(|record| {
            let resurface_at = record.resurface_at.as_ref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))?;
            let created_at = DateTime::parse_from_rfc3339(&record.created_at).ok()
                .map(|dt| dt.with_timezone(&Utc))?;
            let suggestion = record.try_into_suggestion()?;
            Some((suggestion, created_at, resurface_at))
        })
        .collect();

    let mut deferred_mgr = handle.block_on(deferred.lock());
    let already_due = deferred_mgr.restore(entries);
    let deferred_count = deferred_mgr.pending_count();
    drop(deferred_mgr);

    // Push already-due items directly to queue
    if !already_due.is_empty() {
        let mut queue = handle.block_on(shared_suggestion_queue.lock());
        for suggestion in already_due {
            queue.push(suggestion);
        }
    }

    if deferred_count > 0 {
        tracing::info!(count = deferred_count, "restored deferred suggestions");
    }
}
```

#### 3.1.5 Add `get_deferred_suggestions` IPC command

**File**: `src-tauri/src/commands/suggestions.rs`

New Tauri command returning currently deferred items:

```rust
#[derive(Serialize)]
pub struct DeferredSuggestionDto {
    pub id: String,
    pub title: String,
    pub body: String,
    pub priority: String,
    pub source: String,
    pub deferred_at: String,
    pub resurface_at: String,
    pub remaining_minutes: i64,
}

#[command]
pub async fn get_deferred_suggestions(
    state: tauri::State<'_, SuggestionRuntimeState>,
) -> Result<Vec<DeferredSuggestionDto>, String> { ... }
```

Register in `main.rs` invoke_handler.

### 3.2 Gap #5: Feedback Retry Wiring

#### 3.2.1 Wire retry queue into `submit_suggestion_feedback`

**File**: `src-tauri/src/commands/suggestions.rs`

Change the accept/reject paths from fail-immediately to enqueue-on-failure:

**Before** (current):
```rust
"accept" => mgr.feedback().accept(&suggestion_id, None)
    .await.map_err(|e| e.to_string())?,
```

**After**:
```rust
"accept" => {
    if let Err(_e) = mgr.feedback().accept(&suggestion_id, None).await {
        mgr.retry_queue().lock().await.enqueue(PendingFeedback {
            suggestion_id: suggestion_id.clone(),
            feedback_type: FeedbackType::Accepted,
            comment: None,
            attempts: 0,
            next_retry_at: Utc::now(),
        });
        // Don't return error — feedback will be retried
    }
}
```

Same pattern for "reject".

**Defer path** requires special handling. Current flow:
1. `feedback.defer()` — server call (can fail)
2. `queue.remove_by_id()` — remove from active queue
3. `history.add()` + `scorer.record()`
4. `deferred.defer()` — add to deferred manager

The key insight: steps 2-4 are **local state changes** that should always succeed.
Step 1 is **server notification** that is best-effort. Restructure:

```rust
"defer" => {
    // 1. Server notification (best-effort — enqueue for retry on failure)
    if let Err(_e) = mgr.feedback().defer(&suggestion_id, None).await {
        mgr.retry_queue().lock().await.enqueue(PendingFeedback {
            suggestion_id: suggestion_id.clone(),
            feedback_type: FeedbackType::Deferred,
            comment: None,
            attempts: 0,
            next_retry_at: Utc::now(),
        });
    }
    // 2-4. Local state changes (always proceed regardless of server call)
    // ... existing queue removal, history, deferred.defer() ...
}
```

**Key design decision**: The IPC returns `Ok(())` even when server feedback fails, because:
1. The user's intent (accept/reject/defer) is captured in local state (queue removal, history, deferred manager)
2. Server notification is best-effort — will be retried automatically via maintenance loop
3. Returning an error creates a confusing UX (suggestion disappears but user sees error)

The frontend toast should only fire on truly unrecoverable errors (e.g., suggestion not found).

#### 3.2.2 Add `feedback_retries` SQLite table

**File**: `crates/oneshim-storage/src/migration/v23_v24.rs` (NEW)

```sql
CREATE TABLE IF NOT EXISTS feedback_retries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    suggestion_id TEXT NOT NULL,
    feedback_type TEXT NOT NULL,
    comment TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_retry_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(suggestion_id)
);
```

**Rationale**: A dedicated table is cleaner than overloading the suggestions table state column. The UNIQUE constraint on suggestion_id prevents duplicate entries.

#### 3.2.3 Storage methods for feedback retries

**File**: `crates/oneshim-storage/src/sqlite/edge_intelligence/suggestions.rs`

```rust
pub fn save_pending_feedback(&self, feedback: &PendingFeedbackRecord) -> Result<(), StorageError>
pub fn list_pending_feedbacks(&self, limit: usize) -> Result<Vec<PendingFeedbackRecord>, StorageError>
pub fn delete_pending_feedback(&self, suggestion_id: &str) -> Result<(), StorageError>
```

#### 3.2.4 Sync retry queue with SQLite — Three Touch Points

**A. On enqueue** (in IPC command `submit_suggestion_feedback`):
```rust
// After enqueuing to in-memory retry_queue, also persist
let record = PendingFeedbackRecord { suggestion_id, feedback_type, comment, attempts: 0 };
if let Err(e) = app_state.storage.save_pending_feedback(&record) {
    tracing::warn!("failed to persist pending feedback: {e}");
    // Non-fatal — in-memory queue still has it for this session
}
```

**B. On success** (in maintenance loop, after retry succeeds):
```rust
// Delete from SQLite on successful feedback send
if let Err(e) = storage.delete_pending_feedback(&pending.suggestion_id) {
    tracing::warn!("failed to clean up persisted feedback: {e}");
}
```

**C. On exhaustion** (in maintenance loop, when max attempts reached):
```rust
// Also clean up when retry exhausted
storage.delete_pending_feedback(&pending.suggestion_id);
```

**D. On startup** (in `app_runtime_launch.rs`, after deferred restoration):
```rust
let pending_feedbacks = sqlite_storage.list_pending_feedbacks(100).unwrap_or_default();
if !pending_feedbacks.is_empty() {
    let mut rq = handle.block_on(retry_queue.lock());
    let mut count = 0usize;
    for record in pending_feedbacks {
        if let Some(feedback) = record.try_into_pending_feedback() {
            rq.enqueue(feedback);
            count += 1;
        }
    }
    if count > 0 {
        tracing::info!(count, "restored pending feedbacks for retry");
    }
}
```

#### 3.2.5 Startup Ordering Constraint

All restoration must complete BEFORE the maintenance loop starts. Current flow in `app_runtime_launch.rs`:

```
1. Create shared_suggestion_queue        (line ~123)
2. Restore pending suggestions to queue  (line ~127)
3. Create deferred, retry_queue          (line ~217)
4. Create SuggestionManager              (line ~224)
   --- NEW: Insert here ---
5. Restore deferred suggestions          (NEW §3.1.4)
6. Restore pending feedbacks             (NEW §3.2.4.D)
   --- Then later ---
7. spawn_suggestion_maintenance_loop()   (in scheduler setup)
```

This ensures the maintenance loop's first 30-second tick sees fully populated deferred and retry queues.

#### 3.2.6 Pass `storage` to maintenance loop

The maintenance loop currently doesn't have access to `SqliteStorage`. Add it as a parameter:

```rust
pub(crate) fn spawn_suggestion_maintenance_loop(
    queue: Arc<Mutex<SuggestionQueue>>,
    deferred: Arc<Mutex<DeferredManager>>,
    retry_queue: Arc<Mutex<FeedbackRetryQueue>>,
    feedback: Arc<FeedbackSender>,
    storage: Arc<SqliteStorage>,          // ADD
    on_change: Option<Arc<dyn Fn(usize) + Send + Sync>>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()>
```

The call site in `scheduler/loops/sync.rs` (line ~363) must pass `self.storage.clone()`.
`SqliteStorage` is available via the scheduler's stored reference (check `SchedulerContext` or `AppState` reference).

### 3.3 Shared Data Types

#### 3.3.1 `PendingFeedbackRecord` (storage model)

**File**: `crates/oneshim-core/src/models/storage_records.rs`

```rust
#[derive(Debug, Clone)]
pub struct PendingFeedbackRecord {
    pub id: i64,
    pub suggestion_id: String,
    pub feedback_type: String,   // "Accepted" | "Rejected" | "Deferred"
    pub comment: Option<String>,
    pub attempts: u32,
    pub next_retry_at: String,   // RFC3339
    pub created_at: String,      // RFC3339
}

impl PendingFeedbackRecord {
    pub fn try_into_pending_feedback(self) -> Option<PendingFeedback> {
        // Parse feedback_type string → FeedbackType enum
        // Parse next_retry_at string → DateTime<Utc>
    }
}
```

### 3.4 Cleanup Policy

- **Restored deferred rows**: Left in SQLite after restore. `save_suggestion_state` uses INSERT OR REPLACE, so the next save cycle will update them naturally.
- **Feedback retries on success**: Deleted from SQLite immediately (§3.2.4.B).
- **Exhausted retries**: Deleted from SQLite + logged as warn (§3.2.4.C).
- **Orphaned rows** (no matching in-memory entry): Cleaned up on next startup restore cycle — if created_at > 7 days, skip during restore.

## 4. File Change Summary

| File | Change Type | Scope |
|------|-------------|-------|
| `oneshim-core/src/models/storage_records.rs` | MODIFY | Add `resurface_at` to `SuggestionRecord` + add `PendingFeedbackRecord` |
| `oneshim-suggestion/src/deferred.rs` | MODIFY | Add `restore()` method |
| `oneshim-storage/src/sqlite/edge_intelligence/suggestions.rs` | MODIFY | Extend SELECT + add 3 feedback retry methods |
| `oneshim-storage/src/migration/v23_v24.rs` | NEW | V24 migration: `feedback_retries` table |
| `oneshim-storage/src/migration/mod.rs` | MODIFY | Register V24 migration |
| `src-tauri/src/app_runtime_launch.rs` | MODIFY | Add deferred + retry + feedback hydration at startup |
| `src-tauri/src/commands/suggestions.rs` | MODIFY | Add `get_deferred_suggestions` IPC + retry wiring |
| `src-tauri/src/commands/mod.rs` | MODIFY | Export new command |
| `src-tauri/src/main.rs` | MODIFY | Register new IPC command |
| `src-tauri/src/scheduler/loops/suggestions.rs` | MODIFY | Add storage param + delete on success/exhaustion |
| `src-tauri/src/scheduler/loops/sync.rs` | MODIFY | Pass storage to maintenance loop call site |

## 5. Testing Strategy

### Unit Tests
- `DeferredManager::restore()` — items before/after current time, max_size boundary
- `SuggestionRecord` with `resurface_at` — round-trip save/load
- Feedback retry queue integration — enqueue on failure, persist, restore

### Integration Tests
- Startup with pre-seeded SQLite deferred rows → verify DeferredManager populated
- Startup with pre-seeded feedback_retries rows → verify retry_queue populated
- Full cycle: defer → save_state → restart → verify resurface timer active
- Full cycle: accept fails → enqueue → maintenance tick → retry succeeds → SQLite cleaned

## 6. Edge Cases

| Scenario | Expected Behavior |
|----------|------------------|
| App restarts after resurface_at has passed | Suggestion goes directly to queue, not DeferredManager |
| Duplicate feedback retry (same suggestion_id) | UNIQUE constraint → INSERT OR REPLACE overwrites |
| Retry exhausted (5 attempts) | Drop from queue + delete from SQLite + warn log |
| DeferredManager at max_size during restore | Excess items dropped (oldest first via FIFO) |
| SQLite lock contention on save_pending_feedback | Existing StorageError propagation, logged as warn |
| Network restored mid-retry-backoff | Next 30s tick picks up ready items naturally |
