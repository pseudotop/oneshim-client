# Phase 13 A1 + A3: Few-Shot Learning Prompts & Multi-LLM Orchestration

## Status: REVIEWED (R1)

**Branch**: `feature/phase13-ai-ml`
**Depends on**: A2 (Daily Digest) DONE, A4 (Embedding Hot-Reload) DONE

---

## A1: Few-Shot Learning Prompts

### Problem

The current analysis pipeline uses a single hardcoded system prompt (`ANALYSIS_SYSTEM_PROMPT` in `prompts.rs`, 20 lines). It gives generic instructions regardless of the user's work regime, past suggestion effectiveness, or historical context. The LLM receives rich structured context via `ContextAssembler` but the system prompt itself has no examples of good/bad suggestions.

### Goal

Enrich the system prompt with dynamically-selected few-shot examples drawn from the user's own suggestion history and feedback, so the LLM produces more relevant, personalized suggestions over time.

### Current Architecture

```
ContextAssembler.build_with_history()
  -> AnalysisContext { user_context_json, system_prompt }
     -> AnalysisProvider.analyze(context_json, system_prompt)
```

- `prompts.rs`: Single `ANALYSIS_SYSTEM_PROMPT` constant (21 lines, ~114 tokens)
- `assembler.rs`: `ContextAssembler` builds JSON context, always uses `ANALYSIS_SYSTEM_PROMPT`
- `analyzer.rs`: `ContextAnalyzer` orchestrates the pipeline
- `AnalysisProvider.analyze()` receives both context_json and system_prompt as `&str`

### Prerequisites: Schema Migration (V24)

**The current `local_suggestions` table has NO feedback column.** Schema (V8/V14/V28) stores: `id, suggestion_id, suggestion_type, source, content, priority, confidence_score, relevance_score, is_actionable, reasoning, shown_at, dismissed_at, acted_at, created_at, expires_at, hlc_ts, sync_state`.

A new migration **V28** must add feedback tracking:

```sql
-- V28: Add feedback tracking for few-shot learning
ALTER TABLE local_suggestions ADD COLUMN feedback_type TEXT;          -- 'accepted'|'rejected'|'deferred'|NULL
ALTER TABLE local_suggestions ADD COLUMN feedback_at TEXT;            -- ISO 8601 timestamp
ALTER TABLE local_suggestions ADD COLUMN context_app TEXT DEFAULT ''; -- app name at suggestion time
ALTER TABLE local_suggestions ADD COLUMN context_window TEXT DEFAULT ''; -- window title at suggestion time
ALTER TABLE local_suggestions ADD COLUMN regime_label TEXT;           -- regime label at suggestion time

CREATE INDEX IF NOT EXISTS idx_suggestions_feedback ON local_suggestions(feedback_type)
  WHERE feedback_type IS NOT NULL;
```

This adds columns to the existing table (no new table, no JOIN needed).

### Design

#### 1. Prompt Template System (`prompts.rs` expansion)

Replace the single constant with a builder that composes the system prompt dynamically:

```rust
/// Builds a system prompt with optional few-shot examples.
pub struct PromptBuilder {
    regime_hint: Option<String>,
    few_shot_examples: Vec<FewShotExample>,
    max_examples: usize, // default: 2
}

/// A single few-shot example for the system prompt.
pub struct FewShotExample {
    /// Condensed context summary (what the user was doing)
    pub context_summary: String,
    /// The suggestion that was given
    pub suggestion_content: String,
    /// The suggestion type
    pub suggestion_type: String,
    /// Whether the user accepted/rejected this
    pub outcome: FewShotOutcome,
}

pub enum FewShotOutcome {
    Accepted,
    Rejected,
}
```

The builder produces a system prompt like:
```
You are a productivity assistant analyzing desktop work patterns.
[...existing rules...]

## Current Work Mode
The user is currently in a "{regime_label}" work regime.

## Examples of past suggestions the user liked or disliked
### Accepted:
Context: Deep-coding in VSCode on auth.rs (45 min)
Suggestion: {"type": "ProductivityTip", "content": "Consider committing your progress", "confidence": 0.85}

### Rejected:
Context: Meeting on Zoom
Suggestion: {"type": "WorkflowOptimization", "content": "Try batching your emails", "confidence": 0.7}

Prefer patterns similar to accepted examples. Avoid patterns similar to rejected examples.
```

**Token budget**: Base prompt ~114 tokens + max 2 examples ~200 tokens + regime hint ~20 tokens = **~334 tokens total**. Well within any LLM's system prompt capacity. The `ANALYSIS_SYSTEM_PROMPT` constant is retained for backward compatibility.

#### 2. Few-Shot Example Selection (`few_shot_selector.rs`, new file in oneshim-analysis)

```rust
pub struct FewShotSelector {
    max_examples: usize, // default: 2
}

impl FewShotSelector {
    /// Select the best few-shot examples from suggestion history.
    /// Returns empty Vec if no feedback history exists (graceful degradation).
    pub fn select(
        &self,
        history: &[SuggestionHistoryEntry],
        current_regime: Option<&str>,
    ) -> Vec<FewShotExample> {
        // 1. Soft regime filter: prefer matching regime, relax if <2 candidates
        let mut candidates = self.filter_by_regime(history, current_regime);
        if candidates.len() < 2 {
            candidates = history.to_vec();
        }

        // 2. Partition by feedback
        let accepted: Vec<_> = candidates.iter()
            .filter(|h| h.feedback_type == "accepted")
            .collect();
        let rejected: Vec<_> = candidates.iter()
            .filter(|h| h.feedback_type == "rejected")
            .collect();

        // 3. Pick 1 accepted (most recent) + 1 rejected (most recent), up to max_examples
        let mut selected = Vec::new();
        if let Some(best_accepted) = accepted.first() {
            selected.push(to_few_shot(best_accepted, FewShotOutcome::Accepted));
        }
        if selected.len() < self.max_examples {
            if let Some(best_rejected) = rejected.first() {
                selected.push(to_few_shot(best_rejected, FewShotOutcome::Rejected));
            }
        }

        // 4. Fill remaining slots with accepted examples
        for entry in accepted.iter().skip(1) {
            if selected.len() >= self.max_examples { break; }
            selected.push(to_few_shot(entry, FewShotOutcome::Accepted));
        }

        selected
    }
}
```

Data source: Suggestions sorted by `created_at DESC` from SQLite `local_suggestions` table WHERE `feedback_type IS NOT NULL`.

#### 3. Integration into ContextAssembler + ContextAnalyzer

`ContextAssembler` gains a new method. Existing methods remain unchanged:

```rust
impl ContextAssembler {
    /// Build context with few-shot examples and optional regime hint.
    /// Internally delegates to a shared _build_internal() and uses PromptBuilder
    /// instead of the hardcoded ANALYSIS_SYSTEM_PROMPT.
    pub fn build_with_few_shot(
        &self,
        current: &CurrentActivity,
        events: &[Event],
        patterns: &[ActivityPattern],
        metrics: &SessionMetrics,
        segment_stats: Option<&SegmentStats>,
        relevant_history: &[RelevantHistoryEntry],
        few_shot_examples: &[FewShotExample],
        regime_hint: Option<&str>,
    ) -> AnalysisContext;
}
```

`ContextAnalyzer.analyze()` updated flow (the key integration point):

```rust
// In analyzer.rs — ContextAnalyzer.analyze()
async fn analyze(&self, ...) -> Result<Vec<Suggestion>, CoreError> {
    // ... existing: build current, events, patterns, metrics, segment_stats, relevant_history ...

    // NEW: Fetch few-shot history from storage
    let few_shot_history = self.storage.get_suggestions_with_feedback(10)?;
    let few_shot_examples = self.few_shot_selector.select(
        &few_shot_history,
        current_regime_label.as_deref(),
    );

    // NEW: Use build_with_few_shot instead of build_with_history
    let ctx = self.context_assembler.build_with_few_shot(
        &current, &events, &patterns, &metrics,
        seg_stats.as_ref(), &relevant_history,
        &few_shot_examples, current_regime_label.as_deref(),
    );

    self.analysis_provider.analyze(&ctx.user_context_json, &ctx.system_prompt).await
}
```

This means `ContextAnalyzer` must gain a `FewShotSelector` field (injected at construction).

#### 4. Storage Query for History

New port method on a **new trait** `FewShotStorage` (not on `StorageService`, which is already large):

```rust
// oneshim-core/src/ports/few_shot_storage.rs
// Synchronous trait — matches StorageService/FocusStorage/WebStorage pattern.
// SQLite operations are sync; callers use block_in_place if needed.
pub trait FewShotStorage: Send + Sync {
    /// Retrieve recent suggestions with user feedback for few-shot prompt construction.
    /// Returns suggestions ordered by created_at DESC, limited to `limit`.
    fn get_suggestions_with_feedback(&self, limit: usize) -> Result<Vec<SuggestionHistoryEntry>, CoreError>;

    /// Record user feedback on a suggestion.
    fn record_suggestion_feedback(
        &self,
        suggestion_id: &str,
        feedback_type: &str,
        context_app: &str,
        context_window: &str,
        regime_label: Option<&str>,
    ) -> Result<(), CoreError>;
}
```

`SuggestionHistoryEntry` (new model in `oneshim-core/src/models/suggestion.rs`):

```rust
/// Suggestion with feedback data, used for few-shot prompt construction.
/// Distinct from RelevantHistoryEntry (which is RAG-based activity history).
pub struct SuggestionHistoryEntry {
    pub suggestion_id: String,
    pub suggestion_type: String,
    pub content: String,
    pub confidence: f64,
    pub feedback_type: String,       // "accepted" | "rejected"
    pub regime_label: Option<String>,
    pub context_app: String,
    pub context_window: String,
    pub created_at: DateTime<Utc>,
}
```

SQLite implementation in `oneshim-storage` queries `local_suggestions` directly (no JOIN, feedback columns are on the same table after V24).

### Scope Boundaries

- **In scope**: V28 migration, system prompt enrichment, storage query, selector logic, ContextAnalyzer integration
- **Out of scope**: Prompt A/B testing, prompt versioning, separate prompt per regime type
- **Token budget**: ~334 tokens max (base 114 + 2 examples ~200 + regime ~20)

### Affected Crates

| Crate | Changes |
|-------|---------|
| `oneshim-core` | New `SuggestionHistoryEntry` model in `models/suggestion.rs`, new `FewShotStorage` port trait |
| `oneshim-analysis` | New `few_shot_selector.rs`, expanded `prompts.rs` (PromptBuilder), new `build_with_few_shot` in `assembler.rs`, updated `ContextAnalyzer` |
| `oneshim-storage` | V28 migration, implement `FewShotStorage` trait on `SqliteStorage` |
| `src-tauri` | Wire `FewShotSelector` into `ContextAnalyzer` construction |

---

## A3: Multi-LLM Orchestration for AnalysisProvider

### Problem

The analysis pipeline (`ContextAnalyzer`) currently uses a single `AnalysisProvider` instance (one `AnalysisClient` pointing to one LLM endpoint). If that endpoint is down or slow, the entire analysis pipeline fails silently. There is no fallback, no provider health tracking, and no ability to use different LLM providers for different tasks.

### Goal

Add a `FallbackAnalysisProvider` (following the proven `FallbackEmbeddingProvider` pattern) that chains a primary and fallback `AnalysisProvider`, with per-request health tracking and automatic failover.

### Current Architecture

```
config.ai_provider.llm_api -> AnalysisClient::new(llm_api)
  -> Arc<dyn AnalysisProvider>
     -> ContextAnalyzer (single provider)
     -> LlmSegmentSummarizer (single provider)
     -> LlmWorkTypeRefiner (single provider)
     -> DailyInsightGenerator (single provider)
     -> Scheduler coaching personalization (single provider)
```

Multiple `AnalysisClient` instances are created independently at 4 DI sites, all from the same `config.ai_provider.llm_api`. No fallback exists.

### Reference Pattern: FallbackEmbeddingProvider

Located in `oneshim-embedding/src/lib.rs:317-378`:
- Two providers: primary + fallback
- `AtomicBool` health tracking per request
- On primary error: log warning, try fallback
- `is_primary_healthy()` for UI/diagnostics

### Design

#### 1. FallbackAnalysisProvider (new, in oneshim-analysis)

```rust
// oneshim-analysis/src/fallback_analysis_provider.rs

/// Chains two AnalysisProviders with automatic failover.
/// Follows the same pattern as FallbackEmbeddingProvider.
pub struct FallbackAnalysisProvider {
    primary: Arc<dyn AnalysisProvider>,
    fallback: Arc<dyn AnalysisProvider>,
    primary_healthy: Arc<AtomicBool>,
}
```

Implements `AnalysisProvider` with the same try-primary-then-fallback logic for both `analyze()` and `summarize_text()`.

**Placement rationale**: Same pattern as `FallbackEmbeddingProvider` in `oneshim-embedding` — adapter crate holding its fallback wrapper. Must be exported from `oneshim-analysis/src/lib.rs`.

#### 2. NoOpAnalysisProvider (new, in oneshim-analysis)

```rust
/// Returns empty suggestions / error for summarize_text.
/// Used as the ultimate fallback when no LLM is configured.
pub struct NoOpAnalysisProvider;

#[async_trait]
impl AnalysisProvider for NoOpAnalysisProvider {
    async fn analyze(&self, _: &str, _: &str) -> Result<Vec<Suggestion>, CoreError> {
        tracing::debug!("NoOpAnalysisProvider: no LLM configured, returning empty");
        Ok(vec![])
    }
    async fn summarize_text(&self, _: &str, _: &str) -> Result<String, CoreError> {
        Err(CoreError::Analysis("No LLM provider configured".into()))
    }
    fn provider_name(&self) -> &str { "noop" }
}
```

#### 3. Config Extension

Add optional secondary LLM endpoint to `AiProviderConfig` in `oneshim-core/src/config/sections/ai.rs`:

```rust
pub struct AiProviderConfig {
    // ... existing fields ...
    pub llm_api: Option<ExternalApiEndpoint>,         // primary (existing)
    #[serde(default)]
    pub llm_api_fallback: Option<ExternalApiEndpoint>, // NEW: fallback endpoint
}
```

Backward compatible: `Option<T>` with `#[serde(default)]` deserializes to `None` for existing config files.

#### 4. DI Helper Function + Site Updates

Create a centralized helper to avoid repeating fallback logic at each DI site:

```rust
// src-tauri/src/agent_runtime/analysis_helpers.rs (NEW)

/// Build an AnalysisProvider with optional fallback chaining.
/// Returns (provider, health_flag) where health_flag can be stored for diagnostics.
pub fn build_analysis_provider(
    config: &AiProviderConfig,
) -> Option<(Arc<dyn AnalysisProvider>, Arc<AtomicBool>)> {
    let llm_api = config.llm_api.as_ref()?;
    let primary: Arc<dyn AnalysisProvider> = Arc::new(AnalysisClient::new(llm_api));
    let fallback: Arc<dyn AnalysisProvider> = match &config.llm_api_fallback {
        Some(api) => Arc::new(AnalysisClient::new(api)),
        None => Arc::new(NoOpAnalysisProvider),
    };
    let health_flag = Arc::new(AtomicBool::new(true));
    let provider = Arc::new(FallbackAnalysisProvider::new_with_flag(
        primary, fallback, health_flag.clone()
    ));
    Some((provider as Arc<dyn AnalysisProvider>, health_flag))
}
```

DI sites to update (4 locations, all in `src-tauri/src/`):
1. `agent_runtime/mod.rs:~299` — Scheduler coaching provider
2. `agent_runtime/embedding_setup.rs:~118` — LLM summarizer
3. `agent_runtime/analysis_setup.rs:~66` — WorkType refiner
4. `agent_runtime_support.rs:~146` — ContextAnalyzer

Each site replaces `Arc::new(AnalysisClient::new(llm_api))` with a call to `build_analysis_provider()`.

#### 5. Health Reporting via Shared AtomicBool

**Architecture constraint**: Providers are stored as `Arc<dyn AnalysisProvider>` in their consumers (ContextAnalyzer, Scheduler, etc.) — not accessible from AppState. Downcasting trait objects is fragile.

**Solution**: The `build_analysis_provider()` helper returns a separate `Arc<AtomicBool>` health flag. Store these flags in AppState:

```rust
// In AppState (runtime_state.rs)
pub struct AnalysisHealthFlags {
    /// Primary provider health for the main analysis pipeline.
    pub main_provider_healthy: Arc<AtomicBool>,
}
```

IPC command reads the flag directly:

```rust
#[tauri::command]
pub fn get_analysis_health(state: tauri::State<'_, AppState>) -> AnalysisHealthStatus {
    let healthy = state.analysis_health
        .as_ref()
        .map(|h| h.main_provider_healthy.load(Ordering::Relaxed))
        .unwrap_or(false);
    AnalysisHealthStatus {
        primary_healthy: healthy,
        provider_configured: state.analysis_health.is_some(),
    }
}
```

This avoids downcasting and keeps the health flag decoupled from the provider trait.

### Scope Boundaries

- **In scope**: Primary+fallback chaining, NoOp fallback, health tracking via shared AtomicBool, config extension, DI helper, IPC health endpoint
- **Out of scope**: Parallel execution (race), round-robin, latency-aware selection, cost tracking, per-task provider routing
- **Rationale for scope**: The FallbackEmbeddingProvider pattern is proven and simple. More complex orchestration strategies are Phase 14+ material.

### Affected Crates

| Crate | Changes |
|-------|---------|
| `oneshim-core` | `AiProviderConfig.llm_api_fallback` field |
| `oneshim-analysis` | `FallbackAnalysisProvider`, `NoOpAnalysisProvider` (new files, exported from lib.rs) |
| `oneshim-network` | No changes (AnalysisClient already generic) |
| `src-tauri` | New `analysis_helpers.rs`, update 4 DI sites, `AnalysisHealthFlags` in AppState, health IPC command |

---

## Cross-Cutting Concerns

### Token Budget

Base prompt ~114 tokens + max 2 few-shot examples ~200 tokens + regime hint ~20 tokens = ~334 tokens total. Well within LLM system prompt limits.

### Backward Compatibility

- `ANALYSIS_SYSTEM_PROMPT` constant remains for callers that don't use few-shot
- All existing `build()` / `build_with_segment()` / `build_with_history()` methods unchanged
- `FallbackAnalysisProvider` is transparent to callers (same trait)
- `llm_api_fallback` config field is `Option<_>`, defaults to `None`
- V28 migration adds nullable columns (existing rows get NULL feedback)

### Error Handling

- A1: If no suggestion history with feedback exists, `FewShotSelector.select()` returns empty Vec → base prompt used (graceful degradation)
- A3: If primary fails, fallback is tried. If both fail, error propagates normally
- NoOpAnalysisProvider: `analyze()` → empty Vec with debug log, `summarize_text()` → explicit error

### Test Strategy

| Component | Test Type | Count |
|-----------|-----------|-------|
| `PromptBuilder` | Unit | ~5 (empty, 1 example, 2 examples, regime hint, mixed outcomes) |
| `FewShotSelector` | Unit | ~6 (empty history, regime matching, soft regime filter relaxation, accepted-only history, rejected-only, limit) |
| `FallbackAnalysisProvider` | Unit | ~6 (primary ok, primary fail + fallback ok, both fail, health tracking, summarize_text fallback, health flag shared) |
| `NoOpAnalysisProvider` | Unit | ~2 (analyze returns empty, summarize_text returns error) |
| `FewShotStorage` impl | Unit | ~4 (empty, with data, limit respected, record_feedback roundtrip) |
| V28 migration | Unit | ~1 (migration applies cleanly) |

**Estimated: ~24 new tests**

### Dependencies Between A1 and A3

A1 and A3 are **independent** — they touch different parts of the pipeline:
- A1 modifies the **prompt** (what the LLM is asked)
- A3 modifies the **provider** (which LLM is asked)

They can be implemented in either order. **A3 first** (simpler, foundational).

---

## Implementation Order

1. **A3 first**: FallbackAnalysisProvider + NoOpAnalysisProvider + config + DI helper + DI site updates + health flags + IPC
2. **A1 second**: V28 migration + SuggestionHistoryEntry model + FewShotStorage port + SQLite impl + PromptBuilder + FewShotSelector + assembler method + ContextAnalyzer integration

## Estimated Impact

- **New files**: 5 (`fallback_analysis_provider.rs`, `noop_analysis_provider.rs`, `few_shot_selector.rs`, `analysis_helpers.rs`, `few_shot_storage.rs` port)
- **Modified files**: ~10 (config, assembler, prompts, analyzer, migration, lib.rs exports, 4 DI sites)
- **New tests**: ~24
- **Lines added**: ~500-600
- **Lines modified**: ~80-100

---

## Review History

### R1 (2026-04-06): 4 CRITICAL + 6 IMPORTANT issues found and resolved

| Issue | Resolution |
|-------|-----------|
| **CRITICAL**: Feedback storage missing in schema | Added V28 migration with ALTER TABLE columns |
| **CRITICAL**: SuggestionHistoryEntry undefined | Added explicit struct definition + placement in models/suggestion.rs |
| **CRITICAL**: Analyzer flow incompatible with few-shot injection | Added detailed ContextAnalyzer integration pseudocode |
| **CRITICAL**: Health IPC requires downcasting — impossible with trait objects | Redesigned: shared Arc<AtomicBool> returned from DI helper |
| **IMPORTANT**: Token budget unrealistic for 3 examples | Reduced to 2 examples, revised budget to ~334 tokens |
| **IMPORTANT**: FewShotSelector edge cases unspecified | Added full pseudocode with soft regime filter + partition logic |
| **IMPORTANT**: DI site updates lacked helper function | Added centralized `build_analysis_provider()` helper |
| **IMPORTANT**: NoOpAnalysisProvider semantics vague | Added explicit impl with debug logging |
| **IMPORTANT**: StorageService already large (don't add more methods) | Created separate `FewShotStorage` port trait |
| **IMPORTANT**: FallbackAnalysisProvider not exported from lib.rs | Noted: must export from oneshim-analysis/src/lib.rs |
