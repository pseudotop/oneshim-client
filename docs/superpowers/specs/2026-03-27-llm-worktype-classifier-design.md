# LLM-based WorkType Classifier

**Date**: 2026-03-27
**Status**: Reviewed (revision 1)
**Scope**: `crates/oneshim-analysis/`, `src-tauri/src/scheduler/analysis_pipeline/`

## Problem

WorkType classification is rule-based — hardcoded thresholds on engagement metrics (keystrokes/min, click counts, scroll events) and app name matching. This causes:

1. **Misclassifications**: A developer reading documentation in VSCode is classified as `CodeReview` (because IDE + low keystrokes), not `Reading`
2. **Browser ambiguity**: Browser activities beyond GitHub/GitLab/Docs patterns default to `Unknown`
3. **No context awareness**: Window title semantic content is ignored (e.g., "Bug Triage - JIRA" could inform `Navigation` vs `FormFilling`)
4. **Rigid thresholds**: The 30 keystrokes/min boundary between `ActiveCoding` and `CodeReview` doesn't adapt to user typing speed

## Design

### Approach: LLM Refinement Layer

Add an **optional async LLM refinement step** after the existing rule-based classification. This preserves:
- Fast offline classification as the baseline (no latency impact)
- LLM enhancement when available (more accurate, context-aware)
- Graceful degradation (LLM unavailable → rule-based result stands)

```
[Engagement-based classify()] → WorkType (rule-based)
        │
        ▼
[GUI signal refinement]       → WorkType (refined)
        │
        ▼
[Accessibility role refinement] → WorkType (refined)
        │
        ▼
[LLM refinement (NEW, async, optional)] → WorkType (final)
```

### Architecture

```
crates/oneshim-analysis/
├── work_type_classifier.rs     (EXISTING — engagement-based, unchanged)
└── llm_work_type_refiner.rs    (NEW — LLM refinement)

src-tauri/src/scheduler/
├── analysis_pipeline/mod.rs    (MODIFY — add step 4d)
└── mod.rs                      (MODIFY — wire LlmProvider)
```

### LlmWorkTypeRefiner

New struct in `oneshim-analysis` that wraps an `AnalysisProvider` (not `LlmProvider` — the latter only supports intent interpretation, not arbitrary prompts). Uses `summarize_text(context_json, system_prompt) -> String` to send classification requests:

```rust
pub struct LlmWorkTypeRefiner {
    provider: Arc<dyn AnalysisProvider>,
    cache: Mutex<LruCache<CacheKey, CachedClassification>>,
}

struct CacheKey {
    app_name: String,
    window_title: String,
    work_type: WorkType,  // rule-based baseline
}

struct CachedClassification {
    refined_type: WorkType,
    confidence: f64,
    classified_at: Instant,
}
```

**Key design decisions:**

1. **Cache**: LRU cache (capacity: 64) keyed on `(app_name, window_title, baseline_work_type)`. Same context → same result without re-querying LLM. TTL: 5 minutes.

2. **Confidence threshold**: LLM must return confidence >= 0.7 to override the rule-based result. Below that, the rule-based classification stands.

3. **Non-blocking**: The refiner returns `Option<WorkType>` — `None` means "keep rule-based result" (cache miss pending LLM response, or LLM error).

4. **Background prefetch**: On cache miss, spawns a background LLM call and caches the result. The current tick uses rule-based; the next tick for the same context picks up the cached LLM result.

### LLM Prompt Design

**System prompt:**

```
You are a work activity classifier. Given the user's current app, window title,
and engagement metrics, classify the activity into exactly one work type.

Work types: ACTIVE_CODING, CODE_REVIEW, WRITING, READING, DESIGNING,
FORM_FILLING, BROWSING, PASSIVE_MEETING, ACTIVE_MEETING, NAVIGATION,
TERMINAL_COMMANDS, LOG_READING, DOCUMENT_WRITING, DOCUMENT_READING,
CHAT_COMPOSING, UNKNOWN

Respond with JSON only:
{"work_type": "ACTIVE_CODING", "confidence": 0.92, "reason": "brief reason"}
```

**User prompt:**

```
App: {app_name}
Window: {window_title}
Accessibility role: {focused_role}
OCR sample: {first 200 chars of ocr_text}
Keystrokes/min: {kpm}
Rule-based classification: {baseline_work_type}

Classify this activity.
```

### Response Parsing

```rust
#[derive(Deserialize)]
struct LlmClassificationResponse {
    work_type: WorkType,      // SCREAMING_SNAKE_CASE matches serde
    confidence: f64,
    reason: Option<String>,   // optional, for logging
}
```

Parsing is lenient:
- Extract first JSON object from response text (LLMs sometimes add preamble)
- If parsing fails → return `None` (keep rule-based)
- If `work_type` is unrecognized → return `None`

### Integration in Analysis Pipeline

In `analysis_pipeline/mod.rs`, add step 4d after accessibility refinement:

```rust
// 4d. LLM refinement (async, optional)
let work_type = if let Some(ref refiner) = llm_refiner {
    refiner
        .refine(
            work_type,
            app_name,
            &window_title,
            focused_element.map(|fe| fe.role.as_str()),
            ocr_text.as_deref(),
            engagement.keystrokes_per_min,
        )
        .await
        .unwrap_or(work_type)
} else {
    work_type
};
```

### Wiring

The `LlmWorkTypeRefiner` needs `Arc<dyn AnalysisProvider>`. This is already available in the Scheduler (wired as `analysis_provider` in `agent_runtime.rs` for coaching personalization). The refiner is:

1. Created in `agent_runtime.rs` if `AnalysisProvider` is configured
2. Stored in `AdaptiveTriggerState` as `llm_work_type_refiner: Option<Arc<LlmWorkTypeRefiner>>`
3. Accessed in `run_analysis_tick()` via `ts.llm_work_type_refiner`

No new dependency injection path — reuses the existing `AnalysisProvider` instance.

### Performance Considerations

1. **No added latency on critical path**: Background prefetch means LLM result is cached before the next tick
2. **Cache hit rate**: High — users typically stay in the same app/window for multiple ticks (10s intervals). Cache TTL of 5 min covers most work sessions.
3. **LLM call frequency**: At most 1 call per unique (app, title, baseline_type) combination per 5 minutes. Typical: 2-5 calls/hour.
4. **Prompt token budget**: ~150 tokens input, ~30 tokens output. Minimal cost.

### Files Changed

| File | Change Type | Description |
|------|------------|-------------|
| `crates/oneshim-analysis/src/llm_work_type_refiner.rs` | NEW | LLM refinement logic + cache |
| `crates/oneshim-analysis/src/lib.rs` | MODIFY | Export `LlmWorkTypeRefiner` |
| `src-tauri/src/scheduler/analysis_pipeline/mod.rs` | MODIFY | Add step 4d LLM refinement |
| `src-tauri/src/scheduler/mod.rs` | MODIFY | Store `LlmProvider` in `AdaptiveTriggerState` |
| `src-tauri/src/agent_runtime.rs` | MODIFY | Wire `LlmWorkTypeRefiner` creation |

### Testing Strategy

1. **Unit tests**: `llm_work_type_refiner.rs` — mock `LlmProvider`, verify cache behavior, confidence threshold, parsing
2. **Integration**: Verify pipeline still works when refiner is `None` (unchanged behavior)
3. **Edge cases**: Invalid JSON response, empty response, confidence below threshold, cache expiry

### Out of Scope

- Custom prompt tuning per user
- Training/fine-tuning a dedicated classification model
- Replacing the engagement-based classifier entirely
- UI for showing classification confidence to users
