# Phase 3+4: YELLOW Domain Stabilization + Polish — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Stabilize 4 YELLOW domains (automation, embedding, sync, auto-update) and polish the suggestion system (offline mode, filtering, stats).

**Architecture:** Extend `AutomationPort` trait, add `NoOpEmbeddingProvider`, V23 SQLite migration for queue persistence, frontend filter+stats in overlay.

**Spec:** `docs/roadmap/phase3-4-stabilization-polish-spec.md`

---

## Task 1: NoOpEmbeddingProvider + Fallback Chain (Task 3.2)

**Files:**
- Modify: `crates/oneshim-core/src/ports/embedding_provider.rs`
- Modify: `src-tauri/src/agent_runtime/embedding_setup.rs`

- [ ] **Step 1: Add NoOpEmbeddingProvider to oneshim-core**

In `crates/oneshim-core/src/ports/embedding_provider.rs`, add after the trait definition:

```rust
/// No-op embedding provider that returns zero vectors.
/// Used as fallback when both local and remote embedding are unavailable.
pub struct NoOpEmbeddingProvider {
    dimensions: usize,
}

impl NoOpEmbeddingProvider {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

#[async_trait]
impl EmbeddingProvider for NoOpEmbeddingProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, CoreError> {
        Ok(vec![0.0; self.dimensions])
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        Ok(texts.iter().map(|_| vec![0.0; self.dimensions]).collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_id(&self) -> &str {
        "noop"
    }
}
```

- [ ] **Step 2: Wire fallback in embedding_setup.rs**

In `src-tauri/src/agent_runtime/embedding_setup.rs`, find the `build_embedding_components` function. After both local and remote attempts fail (the function currently returns `EmbeddingComponents` with `None` for each field), add a fallback that uses `NoOpEmbeddingProvider`:

```rust
// If both local and remote fail, use NoOp fallback
if embedding_provider.is_none() {
    tracing::warn!("both local and remote embedding unavailable — using no-op fallback (vector features degraded)");
    embedding_provider = Some(Arc::new(
        oneshim_core::ports::embedding_provider::NoOpEmbeddingProvider::new(384),
    ));
}
```

The key: instead of returning `None` for embedding_provider (which disables the entire pipeline), return a `NoOpEmbeddingProvider` that keeps the pipeline running with degraded accuracy.

- [ ] **Step 3: Build + test**

Run: `cargo check --workspace && cargo test -p oneshim-core -- --nocapture`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-core/src/ports/embedding_provider.rs src-tauri/src/agent_runtime/embedding_setup.rs
git commit -m "feat(embedding): add NoOpEmbeddingProvider fallback for graceful degradation"
```

---

## Task 2: V23 SQLite Migration + Queue Persistence Methods (Task 4.1a)

**Files:**
- Create: `crates/oneshim-storage/src/migration/v22_v23.rs`
- Modify: `crates/oneshim-storage/src/migration/mod.rs`
- Modify: `crates/oneshim-storage/src/sqlite/edge_intelligence/suggestions.rs`
- Modify: `crates/oneshim-core/src/models/storage_records.rs`

- [ ] **Step 1: Create V23 migration**

Create `crates/oneshim-storage/src/migration/v22_v23.rs`:

```rust
use rusqlite::Connection;
use crate::sqlite::StorageError;

/// Must return `rusqlite::Error` (not StorageError) — matches `run_migration_step` signature.
pub(crate) fn migrate_v23(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "ALTER TABLE suggestions ADD COLUMN resurface_at TEXT;
         ALTER TABLE suggestions ADD COLUMN state TEXT NOT NULL DEFAULT 'pending';
         CREATE INDEX IF NOT EXISTS idx_suggestions_state ON suggestions(state);
         INSERT OR IGNORE INTO schema_version (version) VALUES (23);",
    )?;
    Ok(())
}
```

- [ ] **Step 2: Register in migration/mod.rs**

Add module declaration and bump version:
```rust
mod v22_v23;
// Update CURRENT_VERSION from 22 to 23
pub(crate) const CURRENT_VERSION: u32 = 23;
```

Add migration step in `run_migrations`:
```rust
if current < 23 {
    run_migration_step(conn, 23, v22_v23::migrate_v23)?;
}
```

- [ ] **Step 3: Add SuggestionRecord::try_into_suggestion()**

In `crates/oneshim-core/src/models/storage_records.rs`, add a method to convert back to domain model:

```rust
impl SuggestionRecord {
    pub fn try_into_suggestion(self) -> Option<oneshim_core::models::suggestion::Suggestion> {
        use oneshim_core::models::suggestion::*;
        let suggestion_type = match self.suggestion_type.as_str() {
            "WORK_GUIDANCE" => SuggestionType::WorkGuidance,
            "EMAIL_DRAFT" => SuggestionType::EmailDraft,
            "PRODUCTIVITY_TIP" => SuggestionType::ProductivityTip,
            "WORKFLOW_OPTIMIZATION" => SuggestionType::WorkflowOptimization,
            "CONTEXT_BASED" => SuggestionType::ContextBased,
            _ => return None,
        };
        let priority = match self.priority.as_str() {
            "LOW" => Priority::Low,
            "HIGH" => Priority::High,
            "CRITICAL" => Priority::Critical,
            _ => Priority::Medium,
        };
        let source = match self.source.as_str() {
            "LLM_SERVER" => SuggestionSource::LlmServer,
            "LLM_LOCAL" => SuggestionSource::LlmLocal,
            _ => SuggestionSource::RuleBased,
        };
        Some(Suggestion {
            suggestion_id: self.suggestion_id,
            suggestion_type,
            content: self.content,
            priority,
            confidence_score: self.confidence_score,
            relevance_score: self.relevance_score,
            is_actionable: self.is_actionable,
            created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .ok()?
                .with_timezone(&chrono::Utc),
            expires_at: self.expires_at.as_ref().and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&chrono::Utc))
            }),
            source,
            reasoning: self.reasoning,
        })
    }
}
```

- [ ] **Step 4: Add save/list by state methods**

In `crates/oneshim-storage/src/sqlite/edge_intelligence/suggestions.rs`, add:

```rust
/// Save suggestion with explicit state for queue persistence.
pub fn save_suggestion_with_state(
    &self,
    suggestion: &Suggestion,
    state: &str,
    resurface_at: Option<&str>,
) -> Result<(), StorageError> {
    self.conn()?.execute(
        "INSERT OR REPLACE INTO suggestions \
         (suggestion_id, suggestion_type, source, content, priority, \
          confidence_score, relevance_score, is_actionable, reasoning, \
          created_at, expires_at, state, resurface_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            suggestion.suggestion_id,
            serde_json::to_string(&suggestion.suggestion_type).unwrap_or_default().trim_matches('"'),
            suggestion.source.as_sql_str(),
            suggestion.content,
            serde_json::to_string(&suggestion.priority).unwrap_or_default().trim_matches('"'),
            suggestion.confidence_score,
            suggestion.relevance_score,
            suggestion.is_actionable as i32,
            suggestion.reasoning,
            suggestion.created_at.to_rfc3339(),
            suggestion.expires_at.map(|d| d.to_rfc3339()),
            state,
            resurface_at,
        ],
    )?;
    Ok(())
}

/// List suggestions by state for queue restoration.
pub fn list_suggestions_by_state(&self, state: &str, limit: usize) -> Result<Vec<SuggestionRecord>, StorageError> {
    let conn = self.conn()?;
    let mut stmt = conn.prepare(
        "SELECT id, suggestion_id, suggestion_type, source, content, priority, \
         confidence_score, relevance_score, is_actionable, reasoning, \
         shown_at, dismissed_at, acted_at, created_at, expires_at \
         FROM suggestions WHERE state = ?1 \
         ORDER BY created_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![state, limit as i64], |row| {
        // Map to SuggestionRecord (reuse existing mapping logic)
        Ok(SuggestionRecord {
            id: row.get(0)?,
            suggestion_id: row.get(1)?,
            suggestion_type: row.get(2)?,
            source: row.get(3)?,
            content: row.get(4)?,
            priority: row.get(5)?,
            confidence_score: row.get(6)?,
            relevance_score: row.get(7)?,
            is_actionable: row.get::<_, i32>(8)? != 0,
            reasoning: row.get(9)?,
            shown_at: row.get(10)?,
            dismissed_at: row.get(11)?,
            acted_at: row.get(12)?,
            created_at: row.get(13)?,
            expires_at: row.get(14)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(StorageError::from)
}
```

- [ ] **Step 5: Build + test**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-storage/src/migration/ crates/oneshim-storage/src/sqlite/edge_intelligence/suggestions.rs crates/oneshim-core/src/models/storage_records.rs
git commit -m "feat(storage): V23 migration + queue persistence methods + SuggestionRecord converter"
```

---

## Task 3: Queue Save/Restore Wiring (Task 4.1b)

**Files:**
- Modify: `src-tauri/src/commands/suggestions.rs` — Add `save_suggestion_state` IPC
- Modify: `src-tauri/src/app_runtime_launch.rs` — Restore on startup
- Modify: `src-tauri/src/main.rs` — Register command

- [ ] **Step 1: Add save_suggestion_state IPC**

```rust
#[command]
pub async fn save_suggestion_state(
    suggestion_state: tauri::State<'_, SuggestionRuntimeState>,
    app_state: tauri::State<'_, crate::runtime_state::AppState>,
) -> Result<u32, String> {
    let mgr = suggestion_state.manager().ok_or("suggestions not available")?;
    let storage = app_state.storage().ok_or("storage not available")?;
    
    let mut saved = 0u32;
    
    // Save queue items
    let queue = mgr.queue().lock().await;
    for suggestion in queue.iter() {
        if let Err(e) = storage.save_suggestion_with_state(suggestion, "pending", None) {
            tracing::warn!(id = %suggestion.suggestion_id, "failed to persist suggestion: {e}");
        } else {
            saved += 1;
        }
    }
    drop(queue);
    
    // Save deferred items
    let deferred = mgr.deferred().lock().await;
    for entry in deferred.list_deferred() {
        let resurface = entry.resurface_at.to_rfc3339();
        if let Err(e) = storage.save_suggestion_with_state(
            &entry.suggestion, "deferred", Some(&resurface)
        ) {
            tracing::warn!(id = %entry.suggestion.suggestion_id, "failed to persist deferred: {e}");
        } else {
            saved += 1;
        }
    }
    
    Ok(saved)
}
```

- [ ] **Step 2: Add queue restore on startup**

In `app_runtime_launch.rs`, inside the `#[cfg(feature = "server")]` block, after `shared_suggestion_queue` is created (line ~123) but before it's moved into `AgentRuntimeBuilder`, add restoration logic using the actual variable names:

```rust
// Restore pending suggestions from SQLite (inside #[cfg(feature = "server")] block)
{
    let pending = sqlite_storage.list_suggestions_by_state("pending", 50).unwrap_or_default();
    let mut queue = shared_suggestion_queue.lock().await;
    for record in pending {
        if let Some(suggestion) = record.try_into_suggestion() {
            queue.push(suggestion);
        }
    }
    let count = queue.len();
    if count > 0 {
        tracing::info!(count, "restored suggestions from storage");
    }
}
```

Note: `sqlite_storage` is `Arc<SqliteStorage>` (always available, not an Option). `shared_suggestion_queue` is the queue Arc created under the feature gate.

- [ ] **Step 3: Register command + build**

Add `save_suggestion_state` to `generate_handler!` in `main.rs`.

Run: `cargo check -p oneshim-app`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/suggestions.rs src-tauri/src/app_runtime_launch.rs src-tauri/src/main.rs
git commit -m "feat(suggestion): queue save/restore for offline persistence"
```

---

## Task 4: Source Filtering UI (Task 4.2)

**Files:**
- Modify: `crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx`

- [ ] **Step 1: Add filter state and toggle UI**

In `SuggestionsPanel.tsx`, add state:
```typescript
const [sourceFilter, setSourceFilter] = useState<Set<string>>(() => {
  const saved = localStorage.getItem('suggestion-source-filter')
  return saved ? new Set(JSON.parse(saved)) : new Set(['server', 'local'])
})

const filteredSuggestions = useMemo(
  () => suggestions.filter(s => sourceFilter.has(s.source)),
  [suggestions, sourceFilter]
)

const toggleSource = (source: string) => {
  setSourceFilter(prev => {
    const next = new Set(prev)
    if (next.has(source)) next.delete(source)
    else next.add(source)
    localStorage.setItem('suggestion-source-filter', JSON.stringify([...next]))
    return next
  })
}
```

Add filter toggles above the suggestion list (inside the Active tab):
```tsx
<div className="flex gap-1.5 px-3 py-1.5">
  {['server', 'local'].map(src => (
    <button
      key={src}
      type="button"
      className={cn(
        'px-2 py-0.5 rounded-full text-[10px] font-medium transition-colors',
        sourceFilter.has(src)
          ? 'bg-brand/20 text-brand'
          : 'bg-content-inverse/5 text-content-tertiary',
      )}
      onClick={() => toggleSource(src)}
    >
      {src === 'server' ? 'Server' : 'Local'}
    </button>
  ))}
</div>
```

Replace `suggestions` with `filteredSuggestions` in the list rendering.

- [ ] **Step 2: Build check**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx
git commit -m "feat(overlay): add source filter toggles (Server/Local) with localStorage persistence"
```

---

## Task 5: Statistics Tab (Task 4.3)

**Files:**
- Modify: `src-tauri/src/commands/suggestions.rs` — Add `get_suggestion_stats` IPC
- Create: `crates/oneshim-web/frontend/src/overlay/components/SuggestionStats.tsx`
- Modify: `crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx` — Add Stats tab

- [ ] **Step 1: Add get_suggestion_stats IPC**

```rust
#[derive(Serialize)]
pub struct SuggestionStatsDto {
    pub total: u32,
    pub accepted: u32,
    pub rejected: u32,
    pub deferred: u32,
    pub pending: u32,
    pub acceptance_rate: f64,
}

#[command]
pub async fn get_suggestion_stats(
    state: tauri::State<'_, SuggestionRuntimeState>,
) -> Result<SuggestionStatsDto, String> {
    let mgr = state.manager().ok_or("suggestions not available")?;
    let stats = mgr.history().lock().await.stats();
    let rate = if stats.total > 0 {
        (stats.accepted as f64 / stats.total as f64) * 100.0
    } else {
        0.0
    };
    Ok(SuggestionStatsDto {
        total: stats.total,
        accepted: stats.accepted,
        rejected: stats.rejected,
        deferred: stats.deferred,
        pending: stats.pending,
        acceptance_rate: (rate * 10.0).round() / 10.0,
    })
}
```

Register in `main.rs`.

- [ ] **Step 2: Create SuggestionStats component**

Create `crates/oneshim-web/frontend/src/overlay/components/SuggestionStats.tsx`:

```tsx
import { useEffect, useState } from 'react'
import { cn } from '../../utils/cn'

interface StatsData {
  total: number
  accepted: number
  rejected: number
  deferred: number
  pending: number
  acceptance_rate: number
}

const barColors: Record<string, string> = {
  accepted: 'bg-semantic-success',
  rejected: 'bg-semantic-error',
  deferred: 'bg-semantic-warning',
  pending: 'bg-content-secondary',
}

export function SuggestionStats() {
  const [stats, setStats] = useState<StatsData | null>(null)

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core')
        const data = await invoke<StatsData>('get_suggestion_stats')
        if (!cancelled) setStats(data)
      } catch (e) {
        console.warn('Failed to load stats:', e)
      }
    })()
    return () => { cancelled = true }
  }, [])

  if (!stats) return <p className="text-content-secondary text-xs p-4">Loading...</p>
  if (stats.total === 0) return <p className="text-content-secondary text-xs p-4">No data yet</p>

  const entries = [
    { key: 'accepted', label: 'Accepted', count: stats.accepted },
    { key: 'rejected', label: 'Rejected', count: stats.rejected },
    { key: 'deferred', label: 'Snoozed', count: stats.deferred },
    { key: 'pending', label: 'Pending', count: stats.pending },
  ]

  return (
    <div className="flex flex-col gap-3 p-3">
      <div className="text-center">
        <div className="text-2xl font-bold text-brand">{stats.acceptance_rate}%</div>
        <div className="text-[10px] text-content-secondary">Acceptance Rate</div>
      </div>
      <div className="text-[10px] text-content-secondary text-center">{stats.total} total suggestions</div>
      <div className="flex flex-col gap-1.5">
        {entries.map(({ key, label, count }) => (
          <div key={key} className="flex items-center gap-2">
            <span className="text-[10px] text-content-secondary w-14">{label}</span>
            <div className="flex-1 h-3 rounded-full bg-content-inverse/5 overflow-hidden">
              <div
                className={cn('h-full rounded-full transition-all', barColors[key])}
                style={{ width: `${stats.total > 0 ? (count / stats.total) * 100 : 0}%` }}
              />
            </div>
            <span className="text-[10px] text-content-primary w-6 text-right">{count}</span>
          </div>
        ))}
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Add Stats tab to SuggestionsPanel**

Add `'stats'` to the `activeTab` state type. Add a third tab button. Add conditional rendering:

```tsx
{activeTab === 'stats' && <SuggestionStats />}
```

- [ ] **Step 4: Build check**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/suggestions.rs src-tauri/src/main.rs crates/oneshim-web/frontend/src/overlay/components/SuggestionStats.tsx crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx
git commit -m "feat(overlay): add suggestion stats tab with acceptance rate + breakdown bars"
```

---

## Task 6: Automation Confirmation Flow (Task 3.1)

**Files:**
- Modify: `crates/oneshim-core/src/ports/automation.rs` — Extend trait
- Modify: `crates/oneshim-automation/src/controller/mod.rs` — Implement new methods + pending queue
- Modify: `src-tauri/src/commands/automation.rs` — Add IPC commands
- Modify: `src-tauri/src/main.rs` — Register commands
- Create: `crates/oneshim-web/frontend/src/overlay/components/AutomationConfirmModal.tsx`
- Modify: `crates/oneshim-web/frontend/src/overlay/types.ts` — Add types
- Modify: `crates/oneshim-web/frontend/src/overlay/hooks/useOverlayEvents.ts` — Add listener
- Modify: `crates/oneshim-web/frontend/src/overlay/App.tsx` — Mount modal

This is the largest task. Implementation must be done carefully to maintain hexagonal architecture.

- [ ] **Step 1: Extend AutomationPort trait**

Add to `crates/oneshim-core/src/ports/automation.rs`:

```rust
    /// List pending automation confirmations awaiting user response.
    async fn list_pending_confirmations(&self) -> Result<Vec<PendingConfirmation>, CoreError>;

    /// Submit user's confirmation decision for a pending command.
    async fn submit_confirmation(&self, command_id: &str, approved: bool) -> Result<(), CoreError>;
```

Add the `PendingConfirmation` model to `crates/oneshim-core/src/models/automation.rs` (or wherever automation models live):

```rust
#[derive(Debug, Clone, Serialize)]
pub struct PendingConfirmation {
    pub command_id: String,
    pub process_name: String,
    pub args: Vec<String>,
    pub audit_level: String,
    pub requested_at: DateTime<Utc>,
}
```

- [ ] **Step 2: Implement in AutomationController**

Add to `AutomationController` struct:
```rust
pending_confirmations: Arc<tokio::sync::Mutex<HashMap<String, (PendingConfirmation, tokio::sync::oneshot::Sender<bool>)>>>,
```

Implement the trait methods. For `submit_confirmation`: look up the command_id, send `approved` through the oneshot channel, remove from map.

For the confirmation trigger: modify the `execute_command` impl so that when `audit_level >= Basic`, it:
1. Creates a `PendingConfirmation` entry with a `oneshot::Sender<bool>`
2. Returns a new `CoreError::ConfirmationRequired { command_id, process_name, args, audit_level }` variant
3. The IPC layer in `src-tauri/src/commands/automation.rs` catches this error and emits the `automation:confirm-request` Tauri event (hexagonal-correct — the crate doesn't know about Tauri)
4. The IPC handler then awaits a response via a second IPC call from the frontend

**Important:** `AutomationController` lives in `oneshim-automation` which has NO Tauri dependency. It CANNOT emit Tauri events directly. The IPC layer handles event emission.

Also: search for ALL `impl AutomationPort` blocks in the codebase and update them with the 2 new methods (including any test mocks).

- [ ] **Step 3: Add IPC commands**

In `automation.rs`:
```rust
#[command]
pub async fn get_pending_confirmations(
    state: tauri::State<'_, AutomationRuntimeState>,
) -> Result<Vec<PendingConfirmationDto>, String>

#[command]
pub async fn confirm_automation_command(
    state: tauri::State<'_, AutomationRuntimeState>,
    command_id: String,
    approved: bool,
) -> Result<(), String>
```

- [ ] **Step 4: Create overlay modal component**

Create `AutomationConfirmModal.tsx` with:
- Process name + args display
- Audit level badge
- Approve (green) + Deny (red) buttons
- 30s countdown timer
- Auto-dismiss on timeout

- [ ] **Step 5: Wire overlay events**

Add to `useOverlayEvents`:
- Listen for `automation:confirm-request` event
- Add `pendingConfirmation: PendingConfirmationDto | null` to `OverlayState`
- Mount `AutomationConfirmModal` in App.tsx

- [ ] **Step 6: Build + test**

Run: `cargo check --workspace && cd crates/oneshim-web/frontend && pnpm build`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/oneshim-core/ crates/oneshim-automation/ src-tauri/src/commands/automation.rs src-tauri/src/main.rs crates/oneshim-web/frontend/src/overlay/
git commit -m "feat(automation): confirmation modal with pending queue + 30s auto-deny"
```

---

## Task 7: Sync Verification Tests (Task 3.3)

**Files:**
- Modify: `crates/oneshim-network/src/sync/lan_transport/tests.rs`

- [ ] **Step 1: Add multi-peer + bidirectional tests**

Add to the existing test module:

```rust
#[tokio::test]
async fn three_peers_discover_each_other() {
    // Create 3 transports with same passphrase on loopback
    // Start all 3, wait for discovery
    // Each should discover the other 2
}

#[tokio::test]
async fn bidirectional_sync_roundtrip() {
    // A pushes changeset, B pulls it — verify content
    // B pushes different changeset, A pulls it — verify content
    // Both have all data
}

#[tokio::test]
async fn watermark_filtering_skips_old_data() {
    // Push changeset at T1
    // Pull with since=T0 (before T1) — returns data
    // Pull with since=T1+1 — returns None (no newer data)
}

#[tokio::test]
async fn concurrent_push_pull_no_data_loss() {
    // A and B push simultaneously (tokio::join!)
    // Both pull — both have all changesets
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p oneshim-network sync -- --nocapture`
Expected: All existing + new tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-network/src/sync/
git commit -m "test(sync): add multi-peer, bidirectional, watermark, concurrent sync tests"
```

---

## Task 8: Auto-Update Verification Tests (Task 3.4)

**Files:**
- Modify: `src-tauri/src/updater/install.rs`
- Modify: `src-tauri/src/updater/mod.rs`

- [ ] **Step 1: Add checksum + extraction + URL tests**

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn sha256_verification_correct_file() {
        // Create temp file, compute hash, verify match
    }

    #[test]
    fn sha256_verification_corrupt_file() {
        // Create temp file, provide wrong hash, verify mismatch detected
    }

    #[test]
    fn safe_archive_path_rejects_traversal() {
        // "../../../etc/passwd" → rejected
        // "bin/oneshim" → accepted
    }

    #[test]
    fn url_allowlist_accepts_github() {
        // "https://github.com/..." → accepted
        // "https://evil.com/..." → rejected
    }

    #[test]
    fn binary_backup_and_replace() {
        // Create temp exe, backup, replace, verify rollback exists
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p oneshim-app updater -- --nocapture`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/updater/
git commit -m "test(updater): add checksum, extraction safety, URL allowlist, backup tests"
```

---

## Task 9: Full Verification

- [ ] **Step 1:** `cargo check --workspace` — PASS
- [ ] **Step 2:** `cargo test --workspace` — ALL PASS
- [ ] **Step 3:** `cargo clippy --workspace` — 0 warnings
- [ ] **Step 4:** `cargo fmt --check` — PASS
- [ ] **Step 5:** `cd crates/oneshim-web/frontend && pnpm build` — PASS
