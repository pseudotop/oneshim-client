# LLM WorkType Classifier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an optional async LLM refinement layer that improves WorkType classification accuracy using AnalysisProvider.

**Architecture:** New `LlmWorkTypeRefiner` struct in `oneshim-analysis` wraps `AnalysisProvider.summarize_text()` to send classification prompts. Background prefetch + LRU cache ensures zero latency on the critical path. Wired into the analysis pipeline as step 4d.

**Tech Stack:** Rust (async_trait, lru, serde_json), AnalysisProvider port

**Spec:** `docs/superpowers/specs/2026-03-27-llm-worktype-classifier-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/oneshim-analysis/src/llm_work_type_refiner.rs` | Create | LLM classification + cache logic |
| `crates/oneshim-analysis/src/lib.rs` | Modify | Export `LlmWorkTypeRefiner` |
| `crates/oneshim-analysis/Cargo.toml` | Modify | Add `lru` dependency |
| `src-tauri/src/scheduler/mod.rs` | Modify | Add field to `AdaptiveTriggerState` |
| `src-tauri/src/scheduler/analysis_pipeline/mod.rs` | Modify | Add step 4d LLM refinement |
| `src-tauri/src/agent_runtime.rs` | Modify | Wire `LlmWorkTypeRefiner` creation |

---

### Task 0: Add Hash derive to WorkType

**Files:**
- Modify: `crates/oneshim-core/src/models/tiered_memory/content.rs`

`WorkType` currently derives `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default` but NOT `Hash`. The LRU cache key requires `Hash`. Add it:

- [ ] **Step 1: Add Hash to derive**

Change:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
```
to:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
```

- [ ] **Step 2: Verify**

Run: `cargo check -p oneshim-core`

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-core/src/models/tiered_memory/content.rs
git commit -m "feat(core): add Hash derive to WorkType for cache key usage"
```

---

### Task 1: Add lru Dependency to oneshim-analysis

**Files:**
- Modify: `crates/oneshim-analysis/Cargo.toml`

- [ ] **Step 1: Add lru to dependencies**

Add after `tracing.workspace = true`:

```toml
lru.workspace = true
```

The workspace already defines `lru = "0.16"` in the root `Cargo.toml`.

- [ ] **Step 2: Verify**

Run: `cargo check -p oneshim-analysis`

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-analysis/Cargo.toml
git commit -m "build(analysis): add lru dependency for classification cache"
```

---

### Task 2: Create LlmWorkTypeRefiner

**Files:**
- Create: `crates/oneshim-analysis/src/llm_work_type_refiner.rs`

- [ ] **Step 1: Create the refiner module**

```rust
// crates/oneshim-analysis/src/llm_work_type_refiner.rs

use std::sync::Arc;
use std::time::Instant;

use lru::LruCache;
use oneshim_core::models::tiered_memory::WorkType;
use oneshim_core::ports::analysis_provider::AnalysisProvider;
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, warn};

const CACHE_CAPACITY: usize = 64;
const CACHE_TTL_SECS: u64 = 300; // 5 minutes
const CONFIDENCE_THRESHOLD: f64 = 0.7;

const SYSTEM_PROMPT: &str = r#"You are a work activity classifier. Given the user's current app, window title, and engagement context, classify the activity into exactly one work type.

Work types: ACTIVE_CODING, CODE_REVIEW, WRITING, READING, DESIGNING, FORM_FILLING, BROWSING, PASSIVE_MEETING, ACTIVE_MEETING, NAVIGATION, TERMINAL_COMMANDS, LOG_READING, DOCUMENT_WRITING, DOCUMENT_READING, CHAT_COMPOSING, UNKNOWN

Respond with JSON only:
{"work_type": "ACTIVE_CODING", "confidence": 0.92}"#;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct CacheKey {
    app_name: String,
    window_title: String,
    baseline: WorkType,
}

#[derive(Debug, Clone)]
struct CachedResult {
    refined: WorkType,
    confidence: f64,
    cached_at: Instant,
}

impl CachedResult {
    fn is_expired(&self) -> bool {
        self.cached_at.elapsed().as_secs() > CACHE_TTL_SECS
    }
}

#[derive(Debug, Deserialize)]
struct ClassificationResponse {
    work_type: WorkType,
    confidence: f64,
}

pub struct LlmWorkTypeRefiner {
    provider: Arc<dyn AnalysisProvider>,
    cache: Mutex<LruCache<CacheKey, CachedResult>>,
}

impl LlmWorkTypeRefiner {
    pub fn new(provider: Arc<dyn AnalysisProvider>) -> Self {
        Self {
            provider,
            cache: Mutex::new(LruCache::new(
                std::num::NonZeroUsize::new(CACHE_CAPACITY).expect("nonzero"),
            )),
        }
    }

    /// Refine the rule-based WorkType using LLM.
    /// Returns `None` to keep the baseline (cache miss pending, LLM error, low confidence).
    pub async fn refine(
        &self,
        baseline: WorkType,
        app_name: &str,
        window_title: &str,
        focused_role: Option<&str>,
        ocr_sample: Option<&str>,
        keystrokes_per_min: f32,
    ) -> Option<WorkType> {
        let key = CacheKey {
            app_name: app_name.to_string(),
            window_title: window_title.to_string(),
            baseline,
        };

        // Check cache first
        {
            let mut cache = self.cache.lock().await;
            if let Some(cached) = cache.get(&key) {
                if !cached.is_expired() {
                    if cached.confidence >= CONFIDENCE_THRESHOLD && cached.refined != baseline {
                        debug!(
                            baseline = ?baseline,
                            refined = ?cached.refined,
                            confidence = cached.confidence,
                            "LLM work type refinement (cached)"
                        );
                        return Some(cached.refined);
                    }
                    return None; // cached but low confidence or same as baseline
                }
                // expired — fall through to re-query
            }
        }

        // Cache miss — spawn background prefetch
        let provider = self.provider.clone();
        let cache = self.cache.clone();
        let key_clone = key.clone();
        let context = build_context(app_name, window_title, focused_role, ocr_sample, keystrokes_per_min, baseline);

        tokio::spawn(async move {
            match provider.summarize_text(&context, SYSTEM_PROMPT).await {
                Ok(response) => {
                    if let Some(parsed) = parse_response(&response) {
                        let result = CachedResult {
                            refined: parsed.work_type,
                            confidence: parsed.confidence,
                            cached_at: Instant::now(),
                        };
                        debug!(
                            work_type = ?parsed.work_type,
                            confidence = parsed.confidence,
                            "LLM classification cached"
                        );
                        let mut cache = cache.lock().await;
                        cache.put(key_clone, result);
                    } else {
                        warn!("failed to parse LLM classification response");
                    }
                }
                Err(e) => {
                    debug!("LLM classification request failed: {e}");
                }
            }
        });

        None // current tick uses baseline; next tick picks up cached result
    }
}

fn build_context(
    app_name: &str,
    window_title: &str,
    focused_role: Option<&str>,
    ocr_sample: Option<&str>,
    keystrokes_per_min: f32,
    baseline: WorkType,
) -> String {
    let mut ctx = format!("App: {app_name}\nWindow: {window_title}\n");
    if let Some(role) = focused_role {
        ctx.push_str(&format!("Focused role: {role}\n"));
    }
    if let Some(sample) = ocr_sample {
        let truncated: String = sample.chars().take(200).collect();
        ctx.push_str(&format!("OCR sample: {truncated}\n"));
    }
    ctx.push_str(&format!("Keystrokes/min: {keystrokes_per_min:.0}\n"));
    ctx.push_str(&format!("Rule-based classification: {baseline:?}\n"));
    ctx
}

fn parse_response(response: &str) -> Option<ClassificationResponse> {
    // Try direct parse first
    if let Ok(parsed) = serde_json::from_str::<ClassificationResponse>(response) {
        return Some(parsed);
    }
    // Extract JSON object from potential preamble
    let start = response.find('{')?;
    let end = response.rfind('}')? + 1;
    serde_json::from_str(&response[start..end]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_clean_json() {
        let resp = r#"{"work_type": "ACTIVE_CODING", "confidence": 0.95}"#;
        let parsed = parse_response(resp).unwrap();
        assert_eq!(parsed.work_type, WorkType::ActiveCoding);
        assert!((parsed.confidence - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_json_with_preamble() {
        let resp = r#"Here is the classification:
{"work_type": "CODE_REVIEW", "confidence": 0.82}
"#;
        let parsed = parse_response(resp).unwrap();
        assert_eq!(parsed.work_type, WorkType::CodeReview);
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert!(parse_response("not json at all").is_none());
        assert!(parse_response("{}").is_none()); // missing required fields
    }

    #[test]
    fn parse_unknown_work_type_uses_default() {
        let resp = r#"{"work_type": "SOMETHING_NEW", "confidence": 0.9}"#;
        let parsed = parse_response(resp).unwrap();
        assert_eq!(parsed.work_type, WorkType::Unknown);
    }

    #[test]
    fn cache_key_equality() {
        let k1 = CacheKey {
            app_name: "VSCode".into(),
            window_title: "main.rs".into(),
            baseline: WorkType::ActiveCoding,
        };
        let k2 = CacheKey {
            app_name: "VSCode".into(),
            window_title: "main.rs".into(),
            baseline: WorkType::ActiveCoding,
        };
        assert_eq!(k1, k2);
    }

    #[test]
    fn cached_result_expiry() {
        let fresh = CachedResult {
            refined: WorkType::ActiveCoding,
            confidence: 0.9,
            cached_at: Instant::now(),
        };
        assert!(!fresh.is_expired());
    }

    #[test]
    fn build_context_includes_all_fields() {
        let ctx = build_context(
            "VSCode", "main.rs — VSCode",
            Some("AXTextArea"), Some("fn main()"),
            45.0, WorkType::ActiveCoding,
        );
        assert!(ctx.contains("App: VSCode"));
        assert!(ctx.contains("Window: main.rs"));
        assert!(ctx.contains("Focused role: AXTextArea"));
        assert!(ctx.contains("OCR sample: fn main()"));
        assert!(ctx.contains("Keystrokes/min: 45"));
        assert!(ctx.contains("Rule-based classification: ActiveCoding"));
    }

    #[test]
    fn build_context_omits_none_fields() {
        let ctx = build_context("Chrome", "Google", None, None, 0.0, WorkType::Browsing);
        assert!(!ctx.contains("Focused role"));
        assert!(!ctx.contains("OCR sample"));
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p oneshim-analysis`

- [ ] **Step 3: Run tests**

Run: `cargo test -p oneshim-analysis -- llm_work_type_refiner`

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-analysis/src/llm_work_type_refiner.rs
git commit -m "feat(analysis): add LlmWorkTypeRefiner with cache"
```

---

### Task 3: Export LlmWorkTypeRefiner from lib.rs

**Files:**
- Modify: `crates/oneshim-analysis/src/lib.rs`

- [ ] **Step 1: Add module declaration and re-export**

After line 18 (`pub mod gui_work_type_refiner;`), add:

```rust
pub mod llm_work_type_refiner;
```

After line 75 (`pub use gui_work_type_refiner::GuiWorkTypeRefiner;`), add:

```rust
pub use llm_work_type_refiner::LlmWorkTypeRefiner;
```

- [ ] **Step 2: Verify**

Run: `cargo check -p oneshim-analysis`

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-analysis/src/lib.rs
git commit -m "feat(analysis): export LlmWorkTypeRefiner"
```

---

### Task 4: Add LlmWorkTypeRefiner to AdaptiveTriggerState

**Files:**
- Modify: `src-tauri/src/scheduler/mod.rs`

- [ ] **Step 1: Add field to AdaptiveTriggerState**

After the `gui_work_type_refiner` field (line 93), add:

```rust
    /// Optional LLM-based work type refinement. When present, refines rule-based
    /// classification using AnalysisProvider. Background prefetch + LRU cache.
    pub(crate) llm_work_type_refiner: Option<Arc<oneshim_analysis::LlmWorkTypeRefiner>>,
```

- [ ] **Step 2: Verify**

Run: `cargo check -p oneshim-app`
Expected: error — field not initialized in `agent_runtime.rs`. Will fix in Task 6.

---

### Task 5: Add LLM Refinement Step to Analysis Pipeline

**Files:**
- Modify: `src-tauri/src/scheduler/analysis_pipeline/mod.rs`

- [ ] **Step 1: Add step 4d after accessibility refinement**

After line 127 (closing `};` of step 4c), add:

```rust
    // 4d. LLM refinement (async, optional — background prefetch + cached result)
    let work_type = if let Some(ref refiner) = ts.llm_work_type_refiner {
        let ocr_text: Option<String> = focused_element
            .and_then(|fe| fe.extracted_text.as_ref())
            .map(|s| s.to_string());
        refiner
            .refine(
                work_type,
                app_name,
                window_title,
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

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p oneshim-app`

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/scheduler/analysis_pipeline/mod.rs src-tauri/src/scheduler/mod.rs
git commit -m "feat(pipeline): add LLM work type refinement step 4d"
```

---

### Task 6: Wire LlmWorkTypeRefiner in agent_runtime.rs

**Files:**
- Modify: `src-tauri/src/agent_runtime.rs`

- [ ] **Step 1: Create LlmWorkTypeRefiner if AnalysisProvider is available**

The `analysis_provider` variable (line 208) is an `Arc<dyn AnalysisProvider>` that may be moved into `LlmSegmentSummarizer::new()` later. Clone it before that consumption:

Find where `analysis_provider` is created (around line 208-212) and add a clone for the refiner BEFORE it's consumed by the summarizer:

```rust
                // Clone for LLM work type refiner before analysis_provider is moved
                let llm_work_type_refiner_provider = analysis_provider.clone();
```

Then before the `AdaptiveTriggerState` struct literal (around line 279), add:

```rust
                let llm_work_type_refiner = Some(Arc::new(
                    oneshim_analysis::LlmWorkTypeRefiner::new(llm_work_type_refiner_provider),
                ));
```

Note: The refiner is created unconditionally when `analysis_provider` exists (inside the same `if let` block that creates the provider). When LLM is not configured, `AdaptiveTriggerState` won't be created at all (the whole block is conditional).

- [ ] **Step 2: Add field to struct literal**

In the `AdaptiveTriggerState { ... }` construction (around line 279-340), add after `gui_work_type_refiner`:

```rust
                    llm_work_type_refiner,
```

- [ ] **Step 3: Verify full compilation**

Run: `cargo check --workspace`

- [ ] **Step 4: Run tests**

Run: `cargo test -p oneshim-analysis`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent_runtime.rs
git commit -m "feat(runtime): wire LlmWorkTypeRefiner into analysis pipeline"
```

---

## Self-Review Checklist

1. **Spec coverage**: LLM refinement step (Task 5), cache with TTL (Task 2), confidence threshold (Task 2), background prefetch (Task 2), fallback to rule-based (Task 5), wiring via AnalysisProvider (Task 6).
2. **Placeholder scan**: No TBD/TODO — all steps have code blocks.
3. **Type consistency**: `LlmWorkTypeRefiner` used consistently across all tasks. `refine()` signature matches between Task 2 (definition) and Task 5 (call site). `AnalysisProvider` port used throughout (not `LlmProvider`).
4. **Dependency**: `lru` added in Task 1 before use in Task 2. `lib.rs` export in Task 3 before use in Task 4-6.
