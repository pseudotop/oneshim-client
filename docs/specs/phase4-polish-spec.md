# Phase 4 — Polish Spec

**Date**: 2026-04-05
**Scope**: `oneshim-suggestion`, `src-tauri`, frontend overlay
**Prerequisite**: Phase 1-3 complete

## 1. Current State

| # | Item | Implemented | Missing |
|---|------|------------|---------|
| 13 | Offline mode | `save_suggestion_state` IPC + startup restoration | **No auto-save on shutdown** |
| 14 | Source filtering | Frontend Server/Local toggles + localStorage | **No "Rule-based" source, no backend filter** |
| 15 | Statistics | `get_suggestion_stats` + SuggestionStats component | **No type/source breakdowns** |

## 2. Goals

1. Suggestion queue persists automatically on app shutdown — zero data loss
2. Source filter includes all 3 sources (Server, Local, Rule-based)
3. Statistics show type distribution and source-based acceptance rates

### Non-Goals
- Time-series stats (daily/weekly trends) — requires new SQLite schema, deferred to v0.5
- Backend-side source filtering IPC parameter — frontend-only filter is sufficient for queue size ≤50
- Web dashboard stats page — overlay stats tab is sufficient

## 3. Design

### 3.1 Item #13: Auto-Save on Shutdown

**Problem**: `save_suggestion_state` exists but is only called manually by frontend. On app exit/crash, in-memory suggestions are lost.

**Solution**: Call save logic directly in the `RunEvent::Exit` handler.

**File**: `src-tauri/src/main.rs`

In the `RunEvent::Exit` handler (line ~306), before `shutdown_tx.send(true)`:

```rust
RunEvent::Exit => {
    // Persist suggestion queue before shutdown
    if let Some(ref mgr) = *suggestion_runtime_state_ref.manager_ref() {
        let storage = &app_state_ref.storage;
        // Save pending queue
        let queue = mgr.queue().blocking_lock();
        for suggestion in queue.iter() {
            let _ = storage.save_suggestion_with_state(suggestion, "pending", None);
        }
        drop(queue);
        // Save deferred items
        let deferred = mgr.deferred().blocking_lock();
        for entry in deferred.list_deferred() {
            let resurface = entry.resurface_at.to_rfc3339();
            let _ = storage.save_suggestion_with_state(
                &entry.suggestion, "deferred", Some(&resurface),
            );
        }
    }
    // existing shutdown logic...
}
```

**Key issue**: `RunEvent::Exit` is sync context (not async). Must use `blocking_lock()` on tokio::sync::Mutex.

**Alternative**: The `SuggestionRuntimeState` and `AppState` must be accessible in the `RunEvent::Exit` closure. Check if they are cloned into the closure.

### 3.2 Item #14: Extended Source Filter

**Problem**: Frontend only shows Server/Local. Suggestions from `RuleBased` source are grouped under "local".

**Solution**: 
A. Backend: Map `RuleBased` to "rule" (not "local") in `source_label()`
B. Frontend: Add third toggle button for "Rule-based"

#### 3.2.1 Backend source label

**File**: `src-tauri/src/commands/suggestions.rs`

Change `source_label()`:
```rust
fn source_label(source: &SuggestionSource) -> &'static str {
    match source {
        SuggestionSource::LlmServer => "server",
        SuggestionSource::LlmLocal => "local",
        SuggestionSource::RuleBased => "rule",  // was "local"
    }
}
```

#### 3.2.2 Frontend filter

**File**: `SuggestionsPanel.tsx`

- Default filter: `['server', 'local', 'rule']`
- Add third toggle button with label "Rules"
- **Migration**: On init, if saved filter is `['server', 'local']` (pre-upgrade default), append `'rule'` automatically to prevent invisible suggestions after upgrade

### 3.3 Item #15: Enhanced Statistics

**Problem**: `HistoryStats` only tracks total/accepted/rejected/deferred/pending counts. No breakdown by type or source.

#### 3.3.1 Extend `HistoryStats`

**File**: `crates/oneshim-suggestion/src/history.rs`

Add to `HistoryStats`:
```rust
pub struct HistoryStats {
    // existing
    pub total: u32,
    pub accepted: u32,
    pub rejected: u32,
    pub deferred: u32,
    pub pending: u32,
    // NEW
    pub by_type: Vec<(String, u32)>,        // [(type_name, count)]
    pub by_source: Vec<(String, u32, f64)>, // [(source, count, acceptance_rate)]
}
```

#### 3.3.2 Extend `stats()` method

Iterate history entries, group by `suggestion_type` and `source`:

```rust
let mut type_counts: HashMap<String, u32> = HashMap::new();
let mut source_stats: HashMap<String, (u32, u32)> = HashMap::new(); // (total, accepted)

for entry in &self.entries {
    // NOTE: fields are nested under entry.suggestion
    *type_counts.entry(format!("{:?}", entry.suggestion.suggestion_type)).or_default() += 1;
    let (total, accepted) = source_stats
        .entry(entry.suggestion.source.as_sql_str().to_string())
        .or_default();
    *total += 1;
    if entry.feedback == Some(FeedbackType::Accepted) { *accepted += 1; }
}
```

#### 3.3.3 Extend `SuggestionStatsDto`

**File**: `src-tauri/src/commands/suggestions.rs`

```rust
pub struct SuggestionStatsDto {
    // existing fields...
    pub by_type: Vec<TypeCountDto>,
    pub by_source: Vec<SourceStatsDto>,
}

#[derive(Serialize)]
pub struct TypeCountDto {
    pub suggestion_type: String,
    pub count: u32,
}

#[derive(Serialize)]
pub struct SourceStatsDto {
    pub source: String,
    pub count: u32,
    pub acceptance_rate: f64,
}
```

#### 3.3.4 Frontend stats component

**File**: `SuggestionStats.tsx`

Add two sections below existing acceptance rate bar:
1. **Type Distribution**: Horizontal bar chart showing count per type
2. **Source Quality**: Table with source name, count, acceptance rate

## 4. File Change Summary

| # | File | Change |
|---|------|--------|
| 1 | `src-tauri/src/main.rs` | Auto-save in RunEvent::Exit |
| 2 | `src-tauri/src/commands/suggestions.rs` | source_label change + extended StatsDto |
| 3 | `crates/oneshim-suggestion/src/history.rs` | Extended HistoryStats + stats() |
| 4 | `SuggestionsPanel.tsx` | Third source filter toggle |
| 5 | `SuggestionStats.tsx` | Type + source breakdown UI |

## 5. Edge Cases

| Scenario | Behavior |
|----------|----------|
| Exit during save_suggestion_state | Each save is individual INSERT OR REPLACE — partial save is safe |
| Empty history for stats | by_type and by_source are empty arrays |
| Source "rule" has 0 suggestions | Still shown in filter but count=0 |
| blocking_lock() in Exit handler | Sync context is fine — tokio runtime still alive at Exit event |
