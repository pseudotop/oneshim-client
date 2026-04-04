# Phase 3+4 Spec: YELLOW Domain Stabilization + Polish

**Date:** 2026-04-04
**Baseline:** v0.4.18 + Phase 1 (`875ca36d`) + Phase 2 (`3a600343`)
**Branch:** `feat/analysis-wiring-v2`
**Scope:** 7 tasks — 4 YELLOW domain stabilization + 3 suggestion polish

---

## 1. Scope Assessment

After codebase analysis, the 7 tasks have very different sizes and natures:

| Task | Nature | Size | Approach |
|------|--------|------|----------|
| 3.1 Automation policy UI | New UI + IPC | Large | Confirmation modal + policy list |
| 3.2 Embedding remote fallback | Architecture change | Medium | Graceful degradation (no server endpoint exists) |
| 3.3 Sync verification | Testing only | Medium | Integration tests for existing code |
| 3.4 Auto-update verification | Testing only | Medium | Integration tests for download/install |
| 4.1 Offline mode | Persistence wiring | Medium | Queue save/restore using existing SQLite |
| 4.2 Source filtering | Frontend UI | Small | Filter toggles in SuggestionsPanel |
| 4.3 Statistics dashboard | Frontend UI | Small | Stats tab with Recharts |

**Key decision:** Tasks 3.3 and 3.4 are verification/testing tasks — they add integration tests, not features. Tasks 3.1 and 3.2 add real functionality.

---

## 2. Task Specifications

### 2.1 Automation Confirmation Policy UI (Task 3.1)

**Goal:** Desktop users can see, manage, and respond to automation policy confirmations.

#### Current State

- `PolicyClient` backend is complete (10 methods, token validation, 6-step command verification)
- `ExecutionPolicy` model has `audit_level`, `allowed_args`, `sandbox_profile`
- IPC commands are stubbed: `check_automation_available`, `list_automation_presets` (returns empty), `run_automation_preset` (returns error), `execute_automation_hint`, `analyze_automation_scene`
- `NoOpOverlayDriver` — no actual confirmation UI

#### Design

**Architecture constraint:** `AutomationRuntimeState` holds `Option<Arc<dyn AutomationPort>>`. The `AutomationPort` trait does NOT expose `PolicyClient` or policy listing methods. The concrete `AutomationController` owns `PolicyClient` internally but doesn't expose it through the port.

**Solution:** Extend `AutomationPort` with policy query methods (hexagonal-correct — port defines the capability, controller implements it):

```rust
// Add to AutomationPort trait in oneshim-core:
async fn list_policies(&self) -> Result<Vec<PolicySummary>, CoreError>;
async fn submit_confirmation(&self, command_id: &str, approved: bool) -> Result<(), CoreError>;
```

**New pending-confirmation channel:** The current `PolicyClient` validates synchronously — no queue. Add a `tokio::sync::mpsc` channel to `AutomationController` that holds commands awaiting user approval:

```rust
// In AutomationController:
pending_confirmations: Arc<Mutex<HashMap<String, PendingConfirmation>>>,
confirm_tx: tokio::sync::mpsc::Sender<ConfirmationResponse>,
```

When `validate_command()` encounters `audit_level >= Basic`, instead of returning immediately, it:
1. Creates a `PendingConfirmation` entry with a `oneshot::Sender<bool>`
2. Emits `automation:confirm-request` Tauri event
3. Awaits the oneshot (with 30s timeout)
4. Returns approved/denied based on response

**New IPC commands:**

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

**PendingConfirmationDto:**
```rust
pub struct PendingConfirmationDto {
    pub command_id: String,
    pub process_name: String,
    pub args: Vec<String>,
    pub audit_level: String,
    pub requested_at: String,
}
```

**Overlay confirmation modal:** When an automation command requires confirmation:
1. Backend emits `automation:confirm-request` Tauri event with `PendingConfirmationDto`
2. Overlay shows a modal: process name, arguments preview, audit level badge
3. User clicks Approve or Deny
4. Frontend calls `confirm_automation_command` IPC
5. Backend resolves the oneshot channel → command proceeds or is denied
6. 30s timeout → auto-deny

**Frontend components:**
- `AutomationConfirmModal.tsx` — NEW modal in overlay
- Add `automation:confirm-request` listener to `useOverlayEvents`
- Add `pendingConfirmation` to `OverlayState`

#### Files Modified
- `crates/oneshim-core/src/ports/automation.rs` — Add `list_policies`, `submit_confirmation` to trait
- `crates/oneshim-automation/src/controller/mod.rs` — Add pending confirmation channel + implementations
- `src-tauri/src/commands/automation.rs` — Add 2 new IPC commands
- `src-tauri/src/main.rs` — Register commands
- `crates/oneshim-web/frontend/src/overlay/types.ts` — Add DTO types + state field
- `crates/oneshim-web/frontend/src/overlay/hooks/useOverlayEvents.ts` — Add event listener
- `crates/oneshim-web/frontend/src/overlay/components/AutomationConfirmModal.tsx` — NEW
- `crates/oneshim-web/frontend/src/overlay/App.tsx` — Mount modal

#### Acceptance Criteria
- [ ] Confirmation modal appears when automation command requires approval
- [ ] Approve resolves confirmation → command executes
- [ ] Deny blocks execution
- [ ] Audit level displayed (none/basic/detailed/full)
- [ ] Modal auto-dismisses after 30s timeout (deny by default)
- [ ] `AutomationPort` trait extended (hexagonal-correct)

---

### 2.2 Embedding Graceful Degradation (Task 3.2)

**Goal:** When local embedding is unavailable, the system degrades gracefully instead of erroring.

#### Current State

- `LocalEmbeddingProvider` works when `fastembed-local` feature enabled (default)
- When disabled, stub returns `CoreError::Internal` immediately
- **No server API endpoint exists for remote embedding** — cannot implement full remote fallback
- Calling code (`oneshim-analysis`) does not handle embedding failures gracefully

#### Design Decision

**Scope reduction:** Since no server endpoint exists, implement **graceful degradation** rather than remote fallback:
1. When embedding fails, skip the embedding step (don't crash)
2. Analysis pipeline continues without vector similarity features
3. Log a warning on first failure, suppress subsequent warnings
4. Add a config flag to explicitly disable embedding

**Implementation:** Add `NoOpEmbeddingProvider` next to the `EmbeddingProvider` port trait (in `oneshim-core`, not `oneshim-embedding` — it has no fastembed dependency):

```rust
// In crates/oneshim-core/src/ports/embedding_provider.rs
pub struct NoOpEmbeddingProvider {
    dimensions: usize,
}

impl NoOpEmbeddingProvider {
    pub fn new(dimensions: usize) -> Self { Self { dimensions } }
}

#[async_trait]
impl EmbeddingProvider for NoOpEmbeddingProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, CoreError> {
        Ok(vec![0.0; self.dimensions])
    }
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        Ok(texts.iter().map(|_| vec![0.0; self.dimensions]).collect())
    }
    fn dimensions(&self) -> usize { self.dimensions }
    fn model_id(&self) -> &str { "noop" }
}
```

**Fallback chain in DI wiring** (actual wiring is in `src-tauri/src/agent_runtime/embedding_setup.rs`, `build_embedding_components()`):
```
Try LocalEmbeddingProvider::new()
  → Success: use local
  → Failure: log warning, use NoOpEmbeddingProvider(384)  // 384 = AllMiniLML6V2 dimensions
```

#### Files Modified
- `crates/oneshim-core/src/ports/embedding_provider.rs` — Add `NoOpEmbeddingProvider`
- `src-tauri/src/agent_runtime/embedding_setup.rs` — Fallback chain

#### Acceptance Criteria
- [ ] App starts successfully when fastembed fails to initialize
- [ ] Warning logged when falling back to no-op
- [ ] Analysis pipeline functions (returns degraded results, not errors)
- [ ] No crashes or panics on embedding failure

---

### 2.3 Cross-Device Sync Verification (Task 3.3)

**Goal:** Validate that LAN sync works end-to-end through integration tests.

#### Current State

- 23 unit tests exist (9 transport + 14 server)
- Tests cover: start/stop, auth flow, push/pull roundtrip, wrong passphrase, offline peer
- **Gap:** No multi-device scenarios, no bidirectional sync, no watermark conflict tests

#### Design

Add integration tests to `crates/oneshim-network/src/sync/lan_transport/tests.rs`:

1. **Multi-peer discovery test** — 3 transports start, discover each other via loopback
2. **Bidirectional sync** — A pushes, B pulls; B pushes, A pulls; verify consistency
3. **Watermark filtering** — Push at T1, pull with since=T0 (get data), pull with since=T2 (get nothing)
4. **Concurrent push/pull** — Two peers push simultaneously, both pull all data
5. **Session token expiry** — Force-expire token, verify re-auth on next operation

#### Files Modified
- `crates/oneshim-network/src/sync/lan_transport/tests.rs` — Add ~5 integration tests
- `crates/oneshim-network/src/sync/lan_server/tests.rs` — Add watermark edge case tests

#### Acceptance Criteria
- [ ] Multi-peer discovery verified (3 transports on loopback)
- [ ] Bidirectional push/pull roundtrip verified
- [ ] Watermark filtering verified (since parameter works)
- [ ] All new tests pass alongside existing 23 tests

---

### 2.4 Auto-Update Verification (Task 3.4)

**Goal:** Validate the download and install pipeline through integration tests.

#### Current State

- Check logic tested (GitHub API, semver, asset selection)
- Download/install: custom implementation (not Tauri updater)
- **Gap:** No tests for actual file download, checksum verification, archive extraction, binary replacement

#### Design

Add integration tests to `src-tauri/src/updater/`:

1. **Checksum verification** — Create temp file, compute SHA-256, verify match/mismatch
2. **Archive extraction safety** — Create `.tar.gz` with normal paths (success) and path traversal (rejected)
3. **URL validation** — Verify allowlisted hosts accepted, others rejected
4. **Download flow** (mock server) — Serve a small file via local HTTP, download + verify checksum
5. **Binary replacement simulation** — Write temp exe, backup, replace, verify rollback file exists

#### Files Modified
- `src-tauri/src/updater/install.rs` — Add integration test module (~5 tests)
- `src-tauri/src/updater/mod.rs` — Add checksum/URL validation tests

#### Acceptance Criteria
- [ ] SHA-256 verification tested (correct + corrupt file)
- [ ] Archive path traversal rejected
- [ ] URL allowlist enforced
- [ ] Binary backup/replacement verified
- [ ] All new tests pass

---

### 2.5 Offline Mode — Queue Persistence (Task 4.1)

**Goal:** Suggestion queue and deferred items survive app restarts.

#### Current State

- SQLite `suggestions` table exists (V8 migration) with all needed columns
- `save_rule_suggestion_sync()` and `list_suggestions()` already implemented in storage
- Queue, history, and deferred are all in-memory only — lost on restart

#### Design

**Save on shutdown:**
- In the app shutdown handler, iterate the `SuggestionQueue` and `DeferredManager`, persist each to SQLite via `save_rule_suggestion_sync()`
- Mark feedback-completed items with `acted_at` so they're not re-loaded

**Restore on startup:**
- In `app_runtime_launch.rs`, after creating `SuggestionManager`, call `list_suggestions()` from storage
- **Conversion:** `SuggestionRecord` (flat strings) → `Suggestion` (typed enums). Add a `SuggestionRecord::try_into_suggestion()` method to `oneshim-storage` or `oneshim-core` that parses `suggestion_type`, `priority`, `source` strings back to enums.
- Filter out expired items (`expires_at <= now`), push remaining into `SuggestionQueue`
- For deferred items: read from new `resurface_at` column, restore to `DeferredManager`

**Schema change (V23 migration):** Add `resurface_at` and `state` columns to `suggestions` table (do NOT repurpose `expires_at` — it has different semantics):

```sql
-- V23 migration
ALTER TABLE suggestions ADD COLUMN resurface_at TEXT;
ALTER TABLE suggestions ADD COLUMN state TEXT NOT NULL DEFAULT 'pending';
-- state: 'pending' | 'deferred' | 'accepted' | 'rejected' | 'dismissed'
CREATE INDEX IF NOT EXISTS idx_suggestions_state ON suggestions(state);
```

**New IPC command:**
```rust
#[command]
pub async fn save_suggestion_state(
    suggestion_state: tauri::State<'_, SuggestionRuntimeState>,
    storage_state: tauri::State<'_, AppState>,
) -> Result<(), String>
```

This can be called by the Tauri exit handler.

**State mapping for persistence:**
- Queue items: `state = 'pending'`, `resurface_at IS NULL`
- Deferred items: `state = 'deferred'`, `resurface_at = <timestamp>`
- Feedback given: `state = 'accepted'/'rejected'`, `acted_at IS NOT NULL`
- Dismissed/expired: `state = 'dismissed'`

#### Files Modified
- `crates/oneshim-storage/src/migration/` — V23 migration (add `resurface_at`, `state` columns)
- `crates/oneshim-storage/src/sqlite/edge_intelligence/suggestions.rs` — Add `save_with_state()`, `list_by_state()` methods
- `crates/oneshim-core/src/models/storage_records.rs` — Add `SuggestionRecord::try_into_suggestion()` conversion
- `src-tauri/src/commands/suggestions.rs` — Add `save_suggestion_state` IPC
- `src-tauri/src/app_runtime_launch.rs` — Restore queue from SQLite on startup
- `src-tauri/src/main.rs` — Register command, add shutdown hook

#### Acceptance Criteria
- [ ] Queue state saved to SQLite on app shutdown
- [ ] Queue restored from SQLite on app startup
- [ ] Expired suggestions filtered on restore
- [ ] Deferred items restored with correct resurface time
- [ ] No duplicate suggestions after restart (idempotent save/restore)

---

### 2.6 Source Filtering (Task 4.2)

**Goal:** Users can filter suggestions by source (Server, Local AI, Rule-Based).

#### Current State

- `SuggestionViewDto.source` already exposed: `"server"` | `"local"`
- Currently "local" covers both `LlmLocal` and `RuleBased` — mapped in `source_label()`
- No filter UI exists

#### Design

**Frontend-only filter** (simplest, queue max 50 items):

Add to `SuggestionsPanel`:
```typescript
const [sourceFilter, setSourceFilter] = useState<Set<string>>(new Set(['server', 'local']))
const filteredSuggestions = useMemo(
    () => suggestions.filter(s => sourceFilter.has(s.source)),
    [suggestions, sourceFilter]
)
```

**Filter UI:** Toggle buttons above the suggestion list (inside the Active tab):
```
[Server ✓] [Local ✓]
```

Clicking toggles the filter. Persist to `localStorage`.

#### Files Modified
- `crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx` — Add filter toggles + state
- No backend changes needed

#### Acceptance Criteria
- [ ] Toggle buttons for Server and Local sources
- [ ] Filtering applied to active suggestion list
- [ ] Filter state persisted in localStorage
- [ ] Empty state when all sources filtered out

---

### 2.7 Statistics Dashboard (Task 4.3)

**Goal:** Users can view suggestion acceptance/rejection stats.

#### Current State

- `SuggestionHistory::stats()` returns `HistoryStats { total, accepted, rejected, deferred, pending }`
- `get_suggestion_history` IPC returns entries with `feedback` field
- Recharts (`^2.14.1`) available in frontend
- `SuggestionsPanel` has 2 tabs (Active, History)

#### Design

**Third tab "Stats"** in `SuggestionsPanel`:

```
[Active (5)] [History] [Stats]
```

**Stats component:** `SuggestionStats.tsx`

Content:
1. **Feedback breakdown bar chart** — accepted/rejected/snoozed/pending counts
2. **Acceptance rate** — `(accepted / total) * 100%` as large number
3. **Source breakdown** — pie chart of server vs local

**Data source:** Reuse `get_suggestion_history` IPC (already returns feedback + source). Compute stats in frontend `useMemo`.

**New IPC (optional, for richer stats):** `get_suggestion_stats` that returns `HistoryStats` directly from Rust. This avoids fetching all history entries just for counts.

```rust
#[command]
pub async fn get_suggestion_stats(
    state: tauri::State<'_, SuggestionRuntimeState>,
) -> Result<SuggestionStatsDto, String>
```

#### Files Modified
- `src-tauri/src/commands/suggestions.rs` — Add `get_suggestion_stats` IPC
- `crates/oneshim-web/frontend/src/overlay/components/SuggestionStats.tsx` — NEW
- `crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx` — Add Stats tab

#### Acceptance Criteria
- [ ] Stats tab visible in SuggestionsPanel
- [ ] Feedback breakdown displayed (accepted/rejected/snoozed/pending)
- [ ] Acceptance rate percentage shown
- [ ] Source breakdown shown (server vs local)
- [ ] Stats update when switching to the tab

---

## 3. Cross-Cutting Concerns

### 3.1 Testing Tasks vs Feature Tasks

Tasks 3.3 and 3.4 produce tests only — no runtime code changes. They can be done in parallel with all other tasks.

### 3.2 No New Crate Dependencies

- Recharts already available for stats charts
- SQLite storage already has suggestion methods
- All new IPC commands use existing state types

### 3.3 Implementation Order

```
Independent (parallel):
  Task 3.2 (embedding fallback) — standalone crate change
  Task 3.3 (sync tests) — test-only
  Task 3.4 (update tests) — test-only
  Task 4.2 (source filtering) — frontend-only

Sequential:
  Task 3.1 (automation UI) — needs overlay + IPC
  Task 4.1 (offline mode) — needs storage wiring
  Task 4.3 (stats dashboard) — needs IPC + frontend
```

---

## 4. Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| Automation confirmation timeout → dead command | Medium | 30s auto-deny, log + notify |
| SQLite queue restore loads stale data | Low | Filter expired on restore, UNIQUE constraint prevents dupes |
| Embedding NoOp produces meaningless similarity scores | Low | Return zero vectors, caller handles gracefully |
| Sync integration tests flaky on CI (port binding) | Medium | Use random ports, retry logic |
| Recharts bundle size increase | Low | Already a dependency, no new import cost |

---

## 5. Out of Scope

- Server-side remote embedding API endpoint (no server changes in v0.4)
- Real OS-native overlay adapters (macOS AXUIElement, Windows UIA) for automation — deferred to ADR-002 Phase 4
- Persistent sync state across restarts (sync is session-scoped)
- Cross-platform auto-update E2E (requires actual installers)
