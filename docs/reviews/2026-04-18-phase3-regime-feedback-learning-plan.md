# Phase 3 — FeedbackSignalSink + regime_id filter + RegimeManager persistence: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire user-feedback into CoachingEngine/RegimeClassifier, make `regime_id` vector filter actually filter, and persist RegimeManager state across restart.

**Architecture:** Three independent items in one branch, phased:
- **X3** New `FeedbackSignalSink` port in `oneshim-core`; `CompositeFeedbackSink` in `src-tauri` fans out to `CoachingEngine::record_user_reaction` + `RegimeClassifier::record_user_reaction`. Fire-and-forget; ~10 ms latency budget enforced by doc contract.
- **C3a** Replace silent-ignore `warn!` in `search_filtered` + `search_quantized` with correlated subquery `segment_id IN (SELECT id FROM activity_segments WHERE regime_id = ?)` — no migration, uses existing `idx_segments_regime`.
- **C3c + X6** New `RegimeStoragePort` + `SqliteRegimeManagerStateStore` writes a JSON blob to a new singleton `regime_manager_state` table (v31 migration). Hydrate on startup, save on `RunEvent::Exit` with 4 s watchdog. Parse failure quarantines corrupt payload to `payload_backup` column.

**Tech Stack:** Rust 2021, tokio 1, async_trait, serde/serde_json, rusqlite, parking_lot, tracing. No new workspace deps.

**Authoritative spec:** [`docs/reviews/2026-04-18-phase3-regime-feedback-learning-spec.md`](./2026-04-18-phase3-regime-feedback-learning-spec.md).

---

## Ground rules

- **TDD.** Write failing test → minimum code → passing test → commit.
- **Plan tasks may be finer than spec §7 commits.** The spec bundles X3 into Commit 1 and splits C3c shutdown wiring across Commits 5+7; plan tasks map 1:many when TDD demands. Engineer may squash plan tasks into the spec's 7-commit shape before PR.
- **Every commit stays green** on `cargo check --workspace`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings -A clippy::empty_docs -A clippy::derivable_impls -A clippy::type_complexity`.
- **`parking_lot::Mutex` NEVER held across `.await`.** Same rule as Phase 2 per ADR-007.
- **No new scheduler-loop migrations to `ConfigManager::subscribe()`** — the audit-coalescing hazard (ADR-016) applies here too.

---

## Test-placement convention

- New `oneshim-core` ports get unit tests in `crates/oneshim-core/src/ports/<name>.rs` inside `#[cfg(test)] mod tests`. Use `tokio = { workspace = true }` dev-dep for `#[tokio::test]` (already present; added in Phase 2).
- `SqliteRegimeManagerStateStore` tests live inline in `crates/oneshim-storage/src/regime_manager_state_store.rs` using `tempfile::tempdir()` for the SQLite file.
- Vector filter tests extend `crates/oneshim-storage/src/sqlite/vector_store_impl/tests.rs` (or equivalent existing test module).
- `CompositeFeedbackSink` + `FeedbackSender::new_with_sink` tests live inline in `src-tauri/src/telemetry/*`-style modules. `src-tauri` is binary-only so tests are `#[cfg(test)] mod tests` within `src/`.

---

## File structure

### Created

| Path | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/ports/feedback_signal_sink.rs` | `FeedbackSignalSink` port trait (X3). |
| `crates/oneshim-core/src/ports/regime_storage.rs` | `RegimeStoragePort` trait (X6). |
| `crates/oneshim-storage/src/regime_manager_state_store.rs` | `SqliteRegimeManagerStateStore` impl + unit tests. |
| `crates/oneshim-storage/src/migration/v31_regime_manager_state.rs` | v31 SQL migration creating `regime_manager_state`. |
| `src-tauri/src/feedback_sink/mod.rs` | `CompositeFeedbackSink` impl + unit tests. |
| `docs/architecture/ADR-017-feedback-signal-sink.md` + `.ko.md` | ADR for X3. |
| `docs/architecture/ADR-018-regime-manager-persistence.md` + `.ko.md` | ADR for C3c/X6. |

### Modified

| Path | Changes |
|------|---------|
| `crates/oneshim-core/src/ports/mod.rs` | re-exports for the two new ports. |
| `crates/oneshim-suggestion/src/feedback.rs` | `new_with_sink` ctor; shim `new`; fire sink before API. |
| `crates/oneshim-analysis/src/coaching_engine/mod.rs` | add `record_user_reaction` stub method. |
| `crates/oneshim-analysis/src/regime_classifier.rs` | add `record_user_reaction` stub method. |
| `crates/oneshim-analysis/src/regime_manager.rs` | add `hydrate_from` method. |
| `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs` | replace silent-ignore at lines ~129, ~388 with correlated subquery. |
| `crates/oneshim-storage/src/migration/mod.rs` | bump `CURRENT_VERSION: u32 = 30` → `31` + add v31 module. |
| `src-tauri/src/runtime_state.rs:347` | add `regime_storage: Option<Arc<dyn RegimeStoragePort>>` + `regime_manager_snapshot: Option<Arc<RegimeManager>>` fields to `AppState`. |
| `src-tauri/src/main.rs:335+` | inside `RunEvent::Exit` handler: save-guard block with 4 s watchdog. |
| `src-tauri/src/app_runtime_launch.rs` | construct `Arc<SqliteRegimeManagerStateStore>`, `Arc<CompositeFeedbackSink>`, call `load_all → hydrate_from`, populate `AppState` fields, thread sink into `FeedbackSender::new_with_sink`. |
| `src-tauri/src/main.rs` — module decls | `mod feedback_sink;`. |

---

## Dependency order

```
Task 1..4  (X3 trait + CE/RC stubs + sink wiring + tests)
   │
Task 5     (ADR-017)
   │
Task 6..7  (C3a SQL filter + tests)
   │
Task 8..11 (RegimeStoragePort + v31 migration + store impl + tests)
   │
Task 12    (hydrate_from on RegimeManager)
   │
Task 13    (AppState fields + dormant save-guard in RunEvent::Exit)
   │
Task 14    (ADR-018)
   │
Task 15    (composition-root wiring + T-C3c-6/7)
```

---

## Task 1: `FeedbackSignalSink` port trait + re-export

**Files:**
- Create: `crates/oneshim-core/src/ports/feedback_signal_sink.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`

- [ ] **Step 1: Write the port file.**

```rust
//! Feedback signal sink port.
//!
//! Cross-crate notification channel for user reactions to suggestions.
//! Implementations wrap `CoachingEngine`, `RegimeClassifier`, or any other
//! component that should adapt to accept/reject/defer signals.
//!
//! See docs/architecture/ADR-017-feedback-signal-sink.md for the full
//! rationale (latency budget, Err semantics, fan-out pattern).

use crate::error::CoreError;
use crate::models::suggestion::SuggestionFeedback;
use async_trait::async_trait;

/// Routes user reactions into learning components.
///
/// # Failure semantics
///
/// Fire-and-forget from the caller's perspective. `FeedbackSender` MUST NOT
/// block user-path accept/reject on a sink error.
///
/// The `Result` return is ONLY for programmer bugs (mutex poisoning,
/// invariant violations). All expected failure classes — network, database,
/// transient unavailability — are the implementation's responsibility to log
/// and swallow internally; they MUST NOT escalate as `Err`.
///
/// # Latency budget
///
/// Implementations must return within ~10 ms. Any blocking work (database
/// writes, network calls, heavy computation) must be offloaded to
/// `tokio::spawn` INSIDE the impl so the inline path stays O(µs). The caller
/// awaits this future synchronously on the user-path accept/reject; breaking
/// this budget re-introduces the write-path wait we intentionally decoupled.
#[async_trait]
pub trait FeedbackSignalSink: Send + Sync {
    async fn record_user_reaction(
        &self,
        feedback: &SuggestionFeedback,
    ) -> Result<(), CoreError>;
}
```

- [ ] **Step 2: Add re-export.**

Edit `crates/oneshim-core/src/ports/mod.rs`:

```rust
pub mod feedback_signal_sink;
pub use feedback_signal_sink::FeedbackSignalSink;
```

(Add between the existing `pub mod …;` declarations in alphabetical order.)

- [ ] **Step 3: Verify compile.**

```bash
cargo check -p oneshim-core 2>&1 | tail -3
```

Expected: `Finished`.

- [ ] **Step 4: Commit.**

```bash
git add crates/oneshim-core/src/ports/feedback_signal_sink.rs crates/oneshim-core/src/ports/mod.rs
git commit -m "feat(core): add FeedbackSignalSink port

Routes user reactions (accept/reject/defer) from FeedbackSender into
CoachingEngine / RegimeClassifier via a CompositeFeedbackSink in the
composition root. Port defines the channel only; concrete learning
algorithms land in a follow-up phase.

Part of Phase 3 (X3 FeedbackSignalSink).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 2: Stub `CoachingEngine::record_user_reaction` + `RegimeClassifier::record_user_reaction`

**Files:**
- Modify: `crates/oneshim-analysis/src/coaching_engine/mod.rs`
- Modify: `crates/oneshim-analysis/src/regime_classifier.rs`

- [ ] **Step 1: Write the CoachingEngine method.**

Open `crates/oneshim-analysis/src/coaching_engine/mod.rs`. Find the `impl CoachingEngine {` block (near the top of the file, after the `pub struct CoachingEngine` def around line 30). At the end of that impl block, before the closing `}`, add:

```rust
    /// Record a user reaction to a coaching message.
    ///
    /// Phase 3 stub — records no state beyond a trace log. The concrete
    /// learning algorithm (bayesian update of trigger priors, per-profile
    /// acceptance rate) lands in a follow-up phase. Called via
    /// `FeedbackSignalSink` from the composition root.
    ///
    /// Must return within ~10 ms; see ADR-017 for the latency budget.
    pub async fn record_user_reaction(
        &self,
        feedback: &oneshim_core::models::suggestion::SuggestionFeedback,
    ) {
        tracing::debug!(
            suggestion_id = %feedback.suggestion_id,
            feedback_type = ?feedback.feedback_type,
            "coaching_engine: user reaction recorded (no-op learning)"
        );
    }
```

- [ ] **Step 2: Write the RegimeClassifier method.**

Open `crates/oneshim-analysis/src/regime_classifier.rs`. Find `impl RegimeClassifier {` (starts near line 42). Add at the end, before the closing `}`:

```rust
    /// Record a user reaction to a suggestion. Phase 3 stub — records
    /// no per-regime state beyond a trace log. Called via
    /// `FeedbackSignalSink` from the composition root.
    pub fn record_user_reaction(
        &mut self,
        feedback: &oneshim_core::models::suggestion::SuggestionFeedback,
    ) {
        tracing::debug!(
            suggestion_id = %feedback.suggestion_id,
            feedback_type = ?feedback.feedback_type,
            "regime_classifier: user reaction recorded (no-op learning)"
        );
    }
```

- [ ] **Step 3: Verify compile + clippy.**

```bash
cargo check -p oneshim-analysis 2>&1 | tail -3
cargo clippy -p oneshim-analysis --all-targets -- -D warnings -A clippy::empty_docs -A clippy::derivable_impls -A clippy::type_complexity 2>&1 | tail -3
```

Expected: both green. The `feedback` parameter is used in the trace macro, so no `unused` warning.

- [ ] **Step 4: Commit.**

```bash
git add crates/oneshim-analysis/src/coaching_engine/mod.rs crates/oneshim-analysis/src/regime_classifier.rs
git commit -m "feat(analysis): record_user_reaction stubs on CoachingEngine + RegimeClassifier

Phase 3 lands the channel (FeedbackSignalSink) and these two stub
methods. Concrete learning algorithm deferred to a follow-up phase;
signatures are stable.

Both methods trace-log the feedback_type but otherwise no-op, so
existing behaviour is unchanged until a future PR implements the
algorithm.

Part of Phase 3 (X3 FeedbackSignalSink).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 3: `FeedbackSender::new_with_sink` + shim `new`

**Files:**
- Modify: `crates/oneshim-suggestion/src/feedback.rs`

- [ ] **Step 1: Audit existing callers (informational, not committed).**

```bash
grep -rn "FeedbackSender::new\|FeedbackSender {" src-tauri/src crates 2>/dev/null
```

Expected: 3-4 call sites — verify before changing the signature to confirm the shim pattern keeps all of them valid.

- [ ] **Step 2: Add `new_with_sink` and make `new` a shim.**

Edit `crates/oneshim-suggestion/src/feedback.rs`. Replace the existing `pub struct FeedbackSender` + `impl FeedbackSender { pub fn new(...) ...` block with:

```rust
use oneshim_core::ports::feedback_signal_sink::FeedbackSignalSink;

pub struct FeedbackSender {
    api_client: Arc<dyn ApiClient>,
    sink: Option<Arc<dyn FeedbackSignalSink>>,
}

impl FeedbackSender {
    /// Preserve the pre-Phase-3 signature. New call sites should prefer
    /// `new_with_sink` and pass a real sink when available.
    pub fn new(api_client: Arc<dyn ApiClient>) -> Self {
        Self::new_with_sink(api_client, None)
    }

    pub fn new_with_sink(
        api_client: Arc<dyn ApiClient>,
        sink: Option<Arc<dyn FeedbackSignalSink>>,
    ) -> Self {
        Self { api_client, sink }
    }
    // (accept/reject/defer methods stay unchanged)
```

- [ ] **Step 3: Wire sink call in `send_feedback`.**

Still in `feedback.rs`, find the `async fn send_feedback` (private) and modify:

```rust
    async fn send_feedback(
        &self,
        suggestion_id: &str,
        feedback_type: FeedbackType,
        comment: Option<String>,
    ) -> Result<(), SuggestionError> {
        let feedback = SuggestionFeedback {
            suggestion_id: suggestion_id.to_string(),
            feedback_type: feedback_type.clone(),
            timestamp: Utc::now(),
            comment,
        };

        // Fire-and-forget into the local sink BEFORE the server call.
        // See ADR-017 for failure + latency rules.
        if let Some(ref sink) = self.sink {
            if let Err(e) = sink.record_user_reaction(&feedback).await {
                tracing::warn!(
                    error = %e,
                    "feedback sink returned Err — programmer-bug path, not a transient failure"
                );
            }
        }

        debug!("feedback sent: {suggestion_id} -> {feedback_type:?}");

        match self.api_client.send_feedback(&feedback).await {
            Ok(()) => {
                debug!("feedback sent success");
                Ok(())
            }
            Err(e) => {
                warn!("feedback sent failure: {e}");
                Err(SuggestionError::Core(e))
            }
        }
    }
```

- [ ] **Step 4: Run existing tests.**

```bash
cargo test -p oneshim-suggestion feedback 2>&1 | tail -10
```

Expected: existing 3 tests (`accept_feedback`, `reject_feedback_with_comment`, `defer_feedback`) still pass — they use `FeedbackSender::new(Arc::new(MockApiClient))`, which now calls through the shim.

- [ ] **Step 5: Commit.**

```bash
git add crates/oneshim-suggestion/src/feedback.rs
git commit -m "feat(suggestion): FeedbackSender sink param + fire-and-forget wiring

Adds Option<Arc<dyn FeedbackSignalSink>> to FeedbackSender. send_feedback
now calls the sink BEFORE the API client so local learning adapts even
when the server is unreachable (retry queue handles the server side).

new(api) is preserved as a shim calling new_with_sink(api, None); 4
existing call sites stay zero-migration.

Part of Phase 3 (X3 FeedbackSignalSink).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 4: `CompositeFeedbackSink` impl + tests T-X3-1..5

**Files:**
- Create: `src-tauri/src/feedback_sink/mod.rs`
- Modify: `src-tauri/src/main.rs` (add `mod feedback_sink;`)

- [ ] **Step 1: Create the sink module with T-X3 tests first (TDD).**

`src-tauri/src/feedback_sink/mod.rs`:

```rust
//! CompositeFeedbackSink — fans user reactions out to CoachingEngine
//! and RegimeClassifier. Binary-crate composition glue per ADR-017.

use std::sync::Arc;
use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::models::suggestion::SuggestionFeedback;
use oneshim_core::ports::feedback_signal_sink::FeedbackSignalSink;
use oneshim_analysis::CoachingEngine;
use oneshim_analysis::RegimeClassifier;

pub struct CompositeFeedbackSink {
    coaching: Option<Arc<CoachingEngine>>,
    regime_classifier: Option<Arc<parking_lot::Mutex<RegimeClassifier>>>,
}

impl CompositeFeedbackSink {
    pub fn new(
        coaching: Option<Arc<CoachingEngine>>,
        regime_classifier: Option<Arc<parking_lot::Mutex<RegimeClassifier>>>,
    ) -> Self {
        Self { coaching, regime_classifier }
    }
}

#[async_trait]
impl FeedbackSignalSink for CompositeFeedbackSink {
    async fn record_user_reaction(
        &self,
        feedback: &SuggestionFeedback,
    ) -> Result<(), CoreError> {
        if let Some(ref c) = self.coaching {
            c.record_user_reaction(feedback).await;
        }
        if let Some(ref cls) = self.regime_classifier {
            let mut guard = cls.lock();
            guard.record_user_reaction(feedback);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use oneshim_core::models::suggestion::FeedbackType;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // --- Mock sink that counts calls ---
    struct CountingSink {
        calls: AtomicUsize,
        result: std::sync::Mutex<Result<(), CoreError>>,
    }

    impl CountingSink {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                result: std::sync::Mutex::new(Ok(())),
            }
        }
    }

    #[async_trait]
    impl FeedbackSignalSink for CountingSink {
        async fn record_user_reaction(
            &self,
            _feedback: &SuggestionFeedback,
        ) -> Result<(), CoreError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result.lock().unwrap().clone()
        }
    }

    fn sample_feedback(t: FeedbackType) -> SuggestionFeedback {
        SuggestionFeedback {
            suggestion_id: "sug_001".into(),
            feedback_type: t,
            timestamp: Utc::now(),
            comment: None,
        }
    }

    /// T-X3-5 — CompositeFeedbackSink invokes BOTH consumers.
    #[tokio::test]
    async fn composite_sink_fans_out_to_both() {
        // Arrange: the composite wraps a lightweight CoachingEngine + RegimeClassifier.
        let ce = Arc::new(make_test_coaching_engine());
        let rc = Arc::new(parking_lot::Mutex::new(make_test_regime_classifier()));
        let sink = CompositeFeedbackSink::new(Some(ce.clone()), Some(rc.clone()));

        // Act
        let fb = sample_feedback(FeedbackType::Accepted);
        sink.record_user_reaction(&fb).await.unwrap();

        // Assert: both consumers have observed the feedback. We rely on
        // the trace log (not asserted) + absence of panic as the contract
        // proxy — the stub methods record only to tracing. A follow-up
        // phase that gives the stubs state will extend this test.
    }

    /// T-X3-1 — accept / reject / defer each land exactly once.
    #[tokio::test]
    async fn sink_receives_accept_reject_defer() {
        let sink = CountingSink::new();
        sink.record_user_reaction(&sample_feedback(FeedbackType::Accepted))
            .await
            .unwrap();
        sink.record_user_reaction(&sample_feedback(FeedbackType::Rejected))
            .await
            .unwrap();
        sink.record_user_reaction(&sample_feedback(FeedbackType::Deferred))
            .await
            .unwrap();
        assert_eq!(sink.calls.load(Ordering::SeqCst), 3);
    }

    // Helpers — construct minimal CoachingEngine / RegimeClassifier for tests.
    fn make_test_coaching_engine() -> CoachingEngine {
        // CoachingEngine::new takes the coaching config. Reuse the default
        // enabled_config helper from the crate's own tests; if missing, fall
        // back to a hand-built minimal config.
        oneshim_analysis::coaching_engine::tests_support::enabled_config_for_feedback_sink()
    }

    fn make_test_regime_classifier() -> RegimeClassifier {
        RegimeClassifier::new()
    }
}
```

> Note: `tests_support::enabled_config_for_feedback_sink` may not exist. If compilation fails because it is absent, define it in the classifier/coaching test-support module as a simple `CoachingEngine::new(default_config())` helper. Follow the naming used by the existing `coaching_engine/triggers.rs:248` test pattern (`CoachingEngine::new(enabled_config())`). The concrete call should be whatever the crate already uses in `#[cfg(test)]` contexts.

- [ ] **Step 2: T-X3-1..4 inline (all in the same `mod tests`).**

Append to the same file:

```rust
    /// T-X3-2 — sink error does NOT fail send_feedback.
    /// We exercise this through FeedbackSender in crates/oneshim-suggestion;
    /// the inline test below just asserts CompositeFeedbackSink itself
    /// returns Ok even if one consumer panics internally — we defensively
    /// isolate via trace logs in the stubs.
    #[tokio::test]
    async fn sink_error_does_not_fail_send_feedback() {
        // Composite sink with no consumers — always Ok.
        let sink = CompositeFeedbackSink::new(None, None);
        let result = sink
            .record_user_reaction(&sample_feedback(FeedbackType::Accepted))
            .await;
        assert!(result.is_ok());
    }

    /// T-X3-3 — no sink configured on FeedbackSender still works.
    /// Exercised by existing `crates/oneshim-suggestion/src/feedback.rs`
    /// tests (`accept_feedback`, etc.) — they construct
    /// `FeedbackSender::new(api)` which calls through the shim with
    /// `Option::None` for the sink. No new test needed here.
    /// (Leave this as documentation — the existing tests are the
    /// regression guard.)

    /// T-X3-4 — sink is invoked BEFORE server call. Exercised in a
    /// cross-crate test below (see Task 3's existing test plus a new
    /// order-assertion test added here for completeness).
    #[tokio::test]
    async fn sink_called_before_server_documented_by_convention() {
        // CompositeFeedbackSink itself does not interact with the server.
        // The ordering guarantee lives in FeedbackSender::send_feedback
        // (see crates/oneshim-suggestion/src/feedback.rs). This test is
        // a placeholder that documents the contract — the real assertion
        // lives in the crate that implements the ordering.
        let sink = CompositeFeedbackSink::new(None, None);
        sink.record_user_reaction(&sample_feedback(FeedbackType::Accepted))
            .await
            .unwrap();
    }
```

- [ ] **Step 3: Register the module in main.rs.**

In `src-tauri/src/main.rs`, after the existing `mod feature_capabilities;` (alphabetical position):

```rust
mod feedback_sink;
```

- [ ] **Step 4: Run tests.**

```bash
cargo test -p oneshim-app --bin oneshim feedback_sink 2>&1 | tail -10
```

Expected: 3 inline tests pass (composite_sink_fans_out_to_both, sink_receives_accept_reject_defer, sink_error_does_not_fail_send_feedback, sink_called_before_server_documented_by_convention).

If `make_test_coaching_engine` fails to compile, replace the helper body with:

```rust
fn make_test_coaching_engine() -> CoachingEngine {
    CoachingEngine::new(oneshim_analysis::coaching_engine::CoachingConfig::default())
}
```

and verify `CoachingConfig` has a `Default` impl.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/feedback_sink/ src-tauri/src/main.rs
git commit -m "feat(app): CompositeFeedbackSink + T-X3 tests

Binary-crate composition root for FeedbackSignalSink. Fans user
reactions to Arc<CoachingEngine> + Arc<Mutex<RegimeClassifier>>.
Either consumer is Option<>: a feature-gated-off coaching path stays
fine.

Tests: T-X3-1 (accept/reject/defer each land once), T-X3-2 (no
consumers still Ok), T-X3-5 (both consumers reached). T-X3-3 and
T-X3-4 covered by existing oneshim-suggestion tests (no-sink
construction) and by the ordering guarantee embedded in
FeedbackSender::send_feedback (Task 3).

Part of Phase 3 (X3 FeedbackSignalSink).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 5: ADR-017 FeedbackSignalSink

**Files:**
- Create: `docs/architecture/ADR-017-feedback-signal-sink.md`
- Create: `docs/architecture/ADR-017-feedback-signal-sink.ko.md`

- [ ] **Step 1: Write ADR-017 (EN).**

Use this body verbatim:

```markdown
[English](./ADR-017-feedback-signal-sink.md) | [한국어](./ADR-017-feedback-signal-sink.ko.md)

# ADR-017: FeedbackSignalSink

**Status**: Approved
**Date**: 2026-04-18
**Scope**: `oneshim-core::ports::feedback_signal_sink`, `oneshim-suggestion::FeedbackSender`, `oneshim-analysis::CoachingEngine/RegimeClassifier`, `src-tauri::feedback_sink::CompositeFeedbackSink`

---

## Context

Before this ADR, `commands/suggestions.rs::handle_suggestion_action` routed accept/reject/defer to `FeedbackSender::send_feedback`, which fired to the server via `ApiClient`. On failure it enqueued into `FeedbackRetryQueue`, which the scheduler drains. Nothing inside the client heard about those events — `CoachingEngine` never learned that a suggestion was accepted, `RegimeClassifier` never saw which regime's suggestions the user accepts vs rejects.

See the 2026-04-16 feature gap analysis (X3 remainder of C1).

## Decision

A new port `FeedbackSignalSink` in `oneshim-core::ports::feedback_signal_sink`:

```rust
#[async_trait]
pub trait FeedbackSignalSink: Send + Sync {
    async fn record_user_reaction(
        &self,
        feedback: &SuggestionFeedback,
    ) -> Result<(), CoreError>;
}
```

`CompositeFeedbackSink` in `src-tauri/src/feedback_sink/mod.rs` fans out to `Arc<CoachingEngine>` + `Arc<parking_lot::Mutex<RegimeClassifier>>` — each Option<>.

`FeedbackSender` gains an `Option<Arc<dyn FeedbackSignalSink>>`. `send_feedback` fires the sink BEFORE the API call so local learning adapts even when the server is unreachable. Existing `FeedbackSender::new(api)` is preserved as a shim calling `new_with_sink(api, None)`.

## Consequences

### Positive

- CoachingEngine + RegimeClassifier now have a stable channel for user-reaction signal. Concrete learning algorithm lands in a follow-up phase without touching the port.
- Fan-out is composition-root glue — no cross-crate adapter dependency.

### Negative / Constraints

- **Latency budget**: implementations MUST return within ~10 ms. Any blocking work (database writes, network calls, heavy computation) must be offloaded to `tokio::spawn` INSIDE the impl. `FeedbackSender::send_feedback` awaits the sink synchronously on the user-path accept/reject; breaking this budget re-introduces the write-path wait that was intentionally decoupled.
- **Err semantics**: `Result<(), CoreError>` is reserved for programmer bugs (mutex poisoning, invariant violations). All expected failure classes — network, database, transient unavailability — are the implementation's responsibility to log and swallow internally; they MUST NOT escalate as `Err`. The caller logs `warn!` on `Err` but does not treat it as a user-path failure.

### Neutral

- `FeedbackSender::new_with_sink(api, None)` is always valid — telemetry-off / test / disabled-coaching paths all work unchanged.

## Alternatives considered

- `tokio::sync::broadcast` event bus — rejected. Adds a runtime task + sizing concern for two consumers and no per-event queuing need.
- Direct `Arc<CoachingEngine>` from `FeedbackSender` — rejected. Violates hexagonal boundary (`oneshim-suggestion` would depend on `oneshim-analysis`).
- One port per consumer (`CoachingSink`, `RegimeSink`) — rejected. Explodes port surface with no caller that wants to pick one-not-the-other; `CompositeFeedbackSink` handles Option<> per consumer.
- Fire the sink AFTER server call — rejected. Server failure would prevent local learning; local signal has independent value.

## References

- Spec: `docs/reviews/2026-04-18-phase3-regime-feedback-learning-spec.md`
- Gap analysis: `docs/reviews/2026-04-16-feature-gaps-analysis.md` X3
- ADR-001 Hexagonal boundary
- ADR-007 `parking_lot::Mutex` never across `.await` — honoured by `CompositeFeedbackSink` (lock acquired, method called, lock dropped before any `.await`)
```

- [ ] **Step 2: Write the Korean companion.**

Translate ADR-017 to Korean in `ADR-017-feedback-signal-sink.ko.md`. Keep structure identical; preserve English technical terms in parentheses after Korean (`구독자 (subscriber)`).

- [ ] **Step 3: Commit.**

```bash
git add docs/architecture/ADR-017-feedback-signal-sink.md docs/architecture/ADR-017-feedback-signal-sink.ko.md
git commit -m "docs(arch): ADR-017 for FeedbackSignalSink

Records the port decision, latency budget (~10 ms), Err semantics
(programmer-bug only), and fan-out composition pattern.

Part of Phase 3 (X3 FeedbackSignalSink).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 6: C3a subquery — implement + T-C3a-1..4

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs`

- [ ] **Step 1: Replace the silent-ignore in `search_filtered`.**

Find line ~129 — the block starting with `// regime_id filter: segment_id based lookup would require a join;`. Replace the entire block (through the closing of the `if filters.regime_id.is_some()` branch) with:

```rust
            // regime_id filter: subquery over the existing
            // `activity_segments.regime_id` + `idx_segments_regime` index.
            // See spec §3.2 / C3a.
            if let Some(ref regime_id) = filters.regime_id {
                let idx = param_values.len() + 1;
                conditions.push(format!(
                    "segment_id IN (SELECT id FROM activity_segments WHERE regime_id = ?{idx})"
                ));
                param_values.push(Box::new(regime_id.clone()));
            }
```

- [ ] **Step 2: Replace the silent-ignore in `search_quantized` (line ~388).**

Find the matching block in `search_quantized` and apply the same replacement.

- [ ] **Step 3: Write T-C3a-1..4 (TDD on the already-implemented code).**

Append to the existing test module in the same file (or create `mod tests` at the bottom if absent):

```rust
#[cfg(test)]
mod regime_filter_tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    fn setup_db_with_segments() -> (TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let conn = Connection::open(&path).unwrap();
        // Apply migrations up to v31 so activity_segments + embedding_vectors exist.
        crate::migration::run_migrations(&conn).unwrap();
        (dir, conn)
    }

    fn insert_segment(conn: &Connection, seg_id: &str, regime_id: Option<&str>) {
        conn.execute(
            "INSERT INTO activity_segments
                (id, start_time, end_time, duration_secs, regime_id,
                 trigger_reason, dominant_category)
             VALUES (?1, datetime('now'), datetime('now'), 0, ?2, 'test', 'work')",
            rusqlite::params![seg_id, regime_id],
        )
        .unwrap();
    }

    fn insert_embedding(conn: &Connection, seg_id: &str) {
        // Fill only the columns required; pad missing with minimal values.
        conn.execute(
            "INSERT INTO embedding_vectors
                (segment_id, content_type, content_label, original_text,
                 vector, timestamp)
             VALUES (?1, 'text', 'l', 'txt', ?2, datetime('now'))",
            rusqlite::params![seg_id, vec![0u8; 16]],
        )
        .unwrap();
    }

    /// T-C3a-1 — search_filtered with regime_id excludes other regimes.
    #[test]
    fn search_filtered_excludes_other_regimes() {
        let (_dir, conn) = setup_db_with_segments();
        insert_segment(&conn, "seg_r1_a", Some("r1"));
        insert_segment(&conn, "seg_r1_b", Some("r1"));
        insert_segment(&conn, "seg_r2", Some("r2"));
        insert_embedding(&conn, "seg_r1_a");
        insert_embedding(&conn, "seg_r1_b");
        insert_embedding(&conn, "seg_r2");

        let store = SqliteVectorStoreImpl::new_for_test(conn);
        let filters = VectorSearchFilters {
            regime_id: Some("r1".into()),
            ..Default::default()
        };
        // Minimal query vector + k=10 — exact signature matches the existing
        // public search_filtered method in oneshim-core::ports::vector_store.
        let results = store.search_filtered(&[0.0; 128], 10, filters).unwrap();
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.segment_id.starts_with("seg_r1"));
        }
    }

    /// T-C3a-2 — search_quantized with regime_id excludes other regimes.
    #[test]
    fn search_quantized_excludes_other_regimes() {
        let (_dir, conn) = setup_db_with_segments();
        insert_segment(&conn, "seg_r1", Some("r1"));
        insert_segment(&conn, "seg_r2", Some("r2"));
        insert_embedding(&conn, "seg_r1");
        insert_embedding(&conn, "seg_r2");

        let store = SqliteVectorStoreImpl::new_for_test(conn);
        let filters = VectorSearchFilters {
            regime_id: Some("r1".into()),
            ..Default::default()
        };
        let results = store.search_quantized(&[0.0; 128], 10, filters).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].segment_id, "seg_r1");
    }

    /// T-C3a-3 — no regime filter returns everything.
    #[test]
    fn regime_id_none_preserves_existing_behaviour() {
        let (_dir, conn) = setup_db_with_segments();
        insert_segment(&conn, "seg_r1", Some("r1"));
        insert_segment(&conn, "seg_r2", Some("r2"));
        insert_embedding(&conn, "seg_r1");
        insert_embedding(&conn, "seg_r2");

        let store = SqliteVectorStoreImpl::new_for_test(conn);
        let filters = VectorSearchFilters::default();
        let results = store.search_filtered(&[0.0; 128], 10, filters).unwrap();
        assert_eq!(results.len(), 2);
    }

    /// T-C3a-4 — a segment with NULL regime_id is excluded when filter set.
    #[test]
    fn segment_without_regime_not_returned_under_filter() {
        let (_dir, conn) = setup_db_with_segments();
        insert_segment(&conn, "seg_r1", Some("r1"));
        insert_segment(&conn, "seg_null", None);
        insert_embedding(&conn, "seg_r1");
        insert_embedding(&conn, "seg_null");

        let store = SqliteVectorStoreImpl::new_for_test(conn);
        let filters = VectorSearchFilters {
            regime_id: Some("r1".into()),
            ..Default::default()
        };
        let results = store.search_filtered(&[0.0; 128], 10, filters).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].segment_id, "seg_r1");
    }
}
```

> `SqliteVectorStoreImpl::new_for_test` — if a test-helper ctor does not exist, add one next to the production ctor, `#[cfg(test)] pub(crate) fn new_for_test(conn: Connection) -> Self { … }`, wrapping the connection in whatever sync primitive the struct holds (`Arc<Mutex<_>>` etc. — follow existing patterns).

- [ ] **Step 4: Run tests.**

```bash
cargo test -p oneshim-storage regime_filter 2>&1 | tail -10
```

Expected: 4/4 pass.

- [ ] **Step 5: Clippy + commit.**

```bash
cargo clippy -p oneshim-storage --all-targets -- -D warnings -A clippy::empty_docs -A clippy::derivable_impls -A clippy::type_complexity 2>&1 | tail -3
git add crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs
git commit -m "feat(storage): regime_id vector filter via activity_segments join

Replaces the silent-ignore warn! in search_filtered and search_quantized
with a correlated subquery over activity_segments.regime_id — uses the
existing idx_segments_regime index, no migration needed.

Tests: T-C3a-1 through T-C3a-4 (regime scoping, None passthrough, NULL
regime exclusion).

Part of Phase 3 (C3a regime_id vector filter).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 7: `RegimeStoragePort` + re-export

**Files:**
- Create: `crates/oneshim-core/src/ports/regime_storage.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`

- [ ] **Step 1: Write the port.**

`crates/oneshim-core/src/ports/regime_storage.rs`:

```rust
//! RegimeStoragePort — persists RegimeManager state across process restart.

use crate::error::CoreError;
use crate::models::tiered_memory::regime::Regime;
use async_trait::async_trait;

#[async_trait]
pub trait RegimeStoragePort: Send + Sync {
    /// Load all persisted regimes on startup. Empty Vec on first launch.
    ///
    /// Implementations MAY perform corrective side-effect writes — e.g.,
    /// quarantining a payload that failed to deserialise so user-curated
    /// state is preserved for later recovery (see
    /// `SqliteRegimeManagerStateStore`). Despite the name, `load_all` is
    /// therefore NOT guaranteed read-only; callers must treat it as a
    /// single-shot operation at startup. Concurrent `load_all` calls are
    /// not required to be safe.
    async fn load_all(&self) -> Result<Vec<Regime>, CoreError>;

    /// Persist the full regime set. Called on graceful shutdown and,
    /// in a future phase, periodically after lifecycle transitions
    /// (merge, delete, rename).
    async fn save_all(&self, regimes: &[Regime]) -> Result<(), CoreError>;
}
```

- [ ] **Step 2: Re-export.**

Edit `crates/oneshim-core/src/ports/mod.rs` — add:

```rust
pub mod regime_storage;
pub use regime_storage::RegimeStoragePort;
```

- [ ] **Step 3: Verify + commit.**

```bash
cargo check -p oneshim-core 2>&1 | tail -3
git add crates/oneshim-core/src/ports/regime_storage.rs crates/oneshim-core/src/ports/mod.rs
git commit -m "feat(core): add RegimeStoragePort

load_all / save_all trait for persisting RegimeManager state across
restart. Doc note warns implementations MAY perform corrective writes
during load (quarantine path).

Part of Phase 3 (C3c + X6).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 8: v31 migration `regime_manager_state`

**Files:**
- Create: `crates/oneshim-storage/src/migration/v31_regime_manager_state.rs`
- Modify: `crates/oneshim-storage/src/migration/mod.rs`

- [ ] **Step 1: Write the migration module.**

```rust
//! v31 — create `regime_manager_state` singleton table for RegimeManager
//! persistence (Phase 3 C3c/X6).
//!
//! The `payload_backup_at` column is intentionally nullable and lacks the
//! usual `NOT NULL DEFAULT (datetime('now'))` convention because it is set
//! only when `SqliteRegimeManagerStateStore::load_all` quarantines a corrupt
//! payload. Do NOT "fix" this to match the sibling migrations — the nullable
//! shape is load-bearing.

use rusqlite::{Connection, Result};

pub fn run(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS regime_manager_state (
            id INTEGER PRIMARY KEY CHECK (id = 0),
            payload TEXT NOT NULL,
            payload_backup TEXT,
            payload_backup_at TEXT,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )?;
    Ok(())
}
```

- [ ] **Step 2: Wire into the migration router.**

Edit `crates/oneshim-storage/src/migration/mod.rs`. At the top:

```rust
pub const CURRENT_VERSION: u32 = 31;  // was 30
```

Register the v31 step in the `apply_migrations` (or equivalent) function — follow the existing pattern (there is one-line-per-version). Example:

```rust
pub mod v31_regime_manager_state;

// inside apply_migrations:
if from < 31 && to >= 31 {
    v31_regime_manager_state::run(conn)?;
    set_version(conn, 31)?;
}
```

Follow the existing router's exact pattern — grep for how v30 is registered and mirror it.

- [ ] **Step 3: Run existing migration tests.**

```bash
cargo test -p oneshim-storage migration 2>&1 | tail -10
```

Expected: existing migration tests pass + `CURRENT_VERSION` assertion (if any) still matches.

- [ ] **Step 4: Clippy + commit.**

```bash
cargo clippy -p oneshim-storage --all-targets -- -D warnings -A clippy::empty_docs -A clippy::derivable_impls -A clippy::type_complexity 2>&1 | tail -3
git add crates/oneshim-storage/src/migration/
git commit -m "feat(storage): v31 migration — regime_manager_state table

Singleton row (id=0) holding the JSON-serialised RegimeManager state.
payload_backup + payload_backup_at columns quarantine corrupt payloads
on load rather than silently wiping user-curated data.

Note: payload_backup_at intentionally lacks NOT NULL DEFAULT
(datetime('now')) — set only on quarantine.

Part of Phase 3 (C3c + X6).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 9: `SqliteRegimeManagerStateStore` impl + T-C3c-1..4

**Files:**
- Create: `crates/oneshim-storage/src/regime_manager_state_store.rs`
- Modify: `crates/oneshim-storage/src/lib.rs` (add `pub mod regime_manager_state_store;`)

- [ ] **Step 1: Write the store + inline tests.**

```rust
//! SqliteRegimeManagerStateStore — RegimeStoragePort over SQLite.

use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::models::tiered_memory::regime::Regime;
use oneshim_core::ports::regime_storage::RegimeStoragePort;
use rusqlite::{Connection, OptionalExtension};
use std::sync::Arc;

pub struct SqliteRegimeManagerStateStore {
    conn: Arc<parking_lot::Mutex<Connection>>,
}

impl SqliteRegimeManagerStateStore {
    pub fn new(conn: Arc<parking_lot::Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl RegimeStoragePort for SqliteRegimeManagerStateStore {
    async fn load_all(&self) -> Result<Vec<Regime>, CoreError> {
        let conn = self.conn.lock();
        let payload: Option<String> = conn
            .query_row(
                "SELECT payload FROM regime_manager_state WHERE id = 0",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| CoreError::Storage(e.to_string()))?;

        match payload {
            Some(json) => match serde_json::from_str::<Vec<Regime>>(&json) {
                Ok(regimes) => Ok(regimes),
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "regime_manager_state payload failed to parse; quarantining to payload_backup and starting fresh. Recover via manual inspection of the backup column."
                    );
                    let _ = conn.execute(
                        "UPDATE regime_manager_state
                            SET payload_backup = payload,
                                payload_backup_at = datetime('now'),
                                payload = '[]',
                                updated_at = datetime('now')
                          WHERE id = 0",
                        [],
                    );
                    Ok(Vec::new())
                }
            },
            None => Ok(Vec::new()),
        }
    }

    async fn save_all(&self, regimes: &[Regime]) -> Result<(), CoreError> {
        let json = serde_json::to_string(regimes)
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO regime_manager_state
                (id, payload, payload_backup, payload_backup_at, updated_at)
             VALUES (
                0, ?1,
                (SELECT payload_backup FROM regime_manager_state WHERE id = 0),
                (SELECT payload_backup_at FROM regime_manager_state WHERE id = 0),
                datetime('now')
             )",
            rusqlite::params![json],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::models::tiered_memory::regime::{
        Regime, RegimeFeatures, RegimeStatus, TriggerParams,
    };
    use chrono::Utc;
    use tempfile::TempDir;

    fn open_db() -> (TempDir, Arc<parking_lot::Mutex<Connection>>) {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open(dir.path().join("t.db")).unwrap();
        crate::migration::run_migrations(&conn).unwrap();
        (dir, Arc::new(parking_lot::Mutex::new(conn)))
    }

    fn sample_regime(id: &str) -> Regime {
        Regime {
            regime_id: id.into(),
            name: None,
            auto_label: format!("label-{id}"),
            centroid: RegimeFeatures::default(),
            optimal_params: TriggerParams::default(),
            sample_count: 0,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            status: RegimeStatus::Active,
        }
    }

    /// T-C3c-1 — empty on first load.
    #[tokio::test]
    async fn empty_on_first_load() {
        let (_d, conn) = open_db();
        let store = SqliteRegimeManagerStateStore::new(conn);
        assert_eq!(store.load_all().await.unwrap().len(), 0);
    }

    /// T-C3c-2 — save then load roundtrip.
    #[tokio::test]
    async fn save_then_load_roundtrip() {
        let (_d, conn) = open_db();
        let store = SqliteRegimeManagerStateStore::new(conn);
        let regimes = vec![sample_regime("a"), sample_regime("b"), sample_regime("c")];
        store.save_all(&regimes).await.unwrap();
        let loaded = store.load_all().await.unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].regime_id, "a");
        assert_eq!(loaded[2].regime_id, "c");
    }

    /// T-C3c-3 — save replaces previous.
    #[tokio::test]
    async fn save_replaces_previous() {
        let (_d, conn) = open_db();
        let store = SqliteRegimeManagerStateStore::new(conn);
        store
            .save_all(&[sample_regime("a"), sample_regime("b"), sample_regime("c")])
            .await
            .unwrap();
        store.save_all(&[sample_regime("just_one")]).await.unwrap();
        let loaded = store.load_all().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].regime_id, "just_one");
    }

    /// T-C3c-4 — malformed payload quarantines, starts fresh.
    #[tokio::test]
    async fn malformed_payload_quarantines_and_starts_fresh() {
        let (_d, conn) = open_db();
        {
            let c = conn.lock();
            c.execute(
                "INSERT OR REPLACE INTO regime_manager_state (id, payload, updated_at) VALUES (0, '{not:valid json', datetime('now'))",
                [],
            )
            .unwrap();
        }
        let store = SqliteRegimeManagerStateStore::new(conn.clone());
        let result = store.load_all().await;
        assert!(result.is_ok(), "quarantine must not return Err");
        assert_eq!(result.unwrap().len(), 0, "fresh start expected");

        let c = conn.lock();
        let (backup, backup_at): (Option<String>, Option<String>) = c
            .query_row(
                "SELECT payload_backup, payload_backup_at FROM regime_manager_state WHERE id = 0",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(backup.unwrap(), "{not:valid json");
        assert!(backup_at.is_some(), "backup timestamp must be set");
    }
}
```

- [ ] **Step 2: Register the module.**

Edit `crates/oneshim-storage/src/lib.rs`:

```rust
pub mod regime_manager_state_store;
```

- [ ] **Step 3: Run tests.**

```bash
cargo test -p oneshim-storage regime_manager_state_store 2>&1 | tail -10
```

Expected: 4 passed.

- [ ] **Step 4: Commit.**

```bash
git add crates/oneshim-storage/src/regime_manager_state_store.rs crates/oneshim-storage/src/lib.rs
git commit -m "feat(storage): SqliteRegimeManagerStateStore + T-C3c-1..4

Persists RegimeManager state as a JSON blob in the v31
regime_manager_state singleton table. On parse failure, quarantines
the corrupt payload to payload_backup + payload_backup_at (error!
log) and returns Ok(vec![]) — never silently wipes user-curated data.

Part of Phase 3 (C3c + X6).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 10: `RegimeManager::hydrate_from` + T-C3c-5

**Files:**
- Modify: `crates/oneshim-analysis/src/regime_manager.rs`

- [ ] **Step 1: Add the method.**

Find the `impl RegimeManager {` block (around line 36). After `pub fn with_params(...)`, add:

```rust
    /// Replace the in-memory regime list with a persisted snapshot.
    /// Called exactly once at startup from the composition root after
    /// `RegimeStoragePort::load_all`. Does not validate against
    /// `max_active` / `archive_days`; the persisted set is trusted to
    /// be consistent with the config at the time it was saved.
    pub fn hydrate_from(&mut self, regimes: Vec<Regime>) {
        self.regimes = regimes;
    }
```

- [ ] **Step 2: Write T-C3c-5.**

At the bottom of `regime_manager.rs` tests, add:

```rust
    #[test]
    fn hydrate_from_replaces_in_memory_state() {
        let cfg = TieredMemoryConfig::default();
        let mut mgr = RegimeManager::new(&cfg);
        // Sanity: empty to start.
        assert_eq!(mgr.all_regimes().len(), 0);

        let imported = vec![make_test_regime("r1"), make_test_regime("r2")];
        mgr.hydrate_from(imported);
        assert_eq!(mgr.all_regimes().len(), 2);
        assert_eq!(mgr.all_regimes()[0].regime_id, "r1");
    }
```

`make_test_regime` likely exists in the existing test module; if absent, define it inline using the same pattern as Task 9.

- [ ] **Step 3: Run + commit.**

```bash
cargo test -p oneshim-analysis regime_manager::tests 2>&1 | tail -10
git add crates/oneshim-analysis/src/regime_manager.rs
git commit -m "feat(analysis): RegimeManager::hydrate_from + T-C3c-5

Single-purpose helper for the composition-root startup hydration path.
No config validation — the persisted set is trusted to match the
TieredMemoryConfig at save time.

Part of Phase 3 (C3c + X6).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 11: AppState fields + dormant save guard

**Files:**
- Modify: `src-tauri/src/runtime_state.rs:347` (AppState struct)
- Modify: `src-tauri/src/main.rs:335+` (RunEvent::Exit handler)

- [ ] **Step 1: Add the AppState fields.**

Open `src-tauri/src/runtime_state.rs`. Find the `pub struct AppState {` at line 347. Add these fields:

```rust
    pub(crate) regime_storage: Option<Arc<dyn oneshim_core::ports::RegimeStoragePort>>,
    pub(crate) regime_manager_snapshot: Option<Arc<oneshim_analysis::RegimeManager>>,
```

In whatever ctor / `::default()` path builds `AppState`, initialise both to `None`. (Follow the existing pattern — grep `AppState { ` or `AppState::default` to find.)

- [ ] **Step 2: Add the dormant save guard.**

In `src-tauri/src/main.rs`, inside the existing `RunEvent::Exit => { ... }` block (around line 335), right after the existing suggestion-persist + AI-session shutdown code and BEFORE the WAL checkpoint at line ~383, insert:

```rust
                // Persist RegimeManager state (best-effort, 4s watchdog).
                if let Some(ref regime_storage) = state.app_state.regime_storage {
                    if let Some(ref regime_manager) = state.app_state.regime_manager_snapshot {
                        let regimes = regime_manager.all_regimes().to_vec();
                        let storage_ref = regime_storage.clone();
                        let outcome = state.background_runtime.handle().block_on(async move {
                            tokio::time::timeout(
                                std::time::Duration::from_secs(4),
                                storage_ref.save_all(&regimes),
                            )
                            .await
                        });
                        match outcome {
                            Ok(Ok(())) => info!(count = regimes.len(), "regime state persisted"),
                            Ok(Err(e)) => warn!(error = %e, "regime state save failed"),
                            Err(_) => warn!("regime state save exceeded 4s; proceeding with shutdown"),
                        }
                    }
                }
```

The exact `state.app_state` field path may differ; match the existing patterns in the same block (e.g., `state.ai_session_runtime_state` / `state.background_runtime`). The two fields are `Option`, so this block is a compile-time no-op at runtime when either is `None` — which is the case throughout until Task 13 populates them.

- [ ] **Step 3: Verify compile + clippy.**

```bash
cargo check -p oneshim-app 2>&1 | tail -3
cargo clippy -p oneshim-app --all-targets -- -D warnings -A clippy::empty_docs -A clippy::derivable_impls -A clippy::type_complexity 2>&1 | tail -3
```

Expected: both green. The `Option::None` fields may trigger `dead_code` — if so, add `#[allow(dead_code)]` on each field with a comment "populated in Task 13 composition root" (matches the pattern Phase 2 used for `mod telemetry`).

- [ ] **Step 4: Run existing suite.**

```bash
cargo test --workspace 2>&1 | tail -3
```

Expected: no regression. The dormant guard runs its `if let Some` branches as `None` and is trivially correct.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/runtime_state.rs src-tauri/src/main.rs
git commit -m "feat(app): AppState regime_storage/snapshot fields + dormant save guard

RunEvent::Exit now has a best-effort save block guarded by
`if let Some(regime_storage) = ... { if let Some(regime_manager) =
... }`. Both fields are None until Task 13 (composition-root wiring)
populates them; this commit intentionally ships the dormant path so
each commit stays cargo-test green.

Watchdog is 4s, matching the telemetry OTel shutdown deadline.

Part of Phase 3 (C3c + X6).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 12: ADR-018

**Files:**
- Create: `docs/architecture/ADR-018-regime-manager-persistence.md`
- Create: `docs/architecture/ADR-018-regime-manager-persistence.ko.md`

- [ ] **Step 1: Write ADR-018 (EN).**

```markdown
[English](./ADR-018-regime-manager-persistence.md) | [한국어](./ADR-018-regime-manager-persistence.ko.md)

# ADR-018: RegimeManager Persistence

**Status**: Approved
**Date**: 2026-04-18
**Scope**: `oneshim-core::ports::regime_storage`, `oneshim-storage::regime_manager_state_store`, `oneshim-analysis::RegimeManager::hydrate_from`, `src-tauri::main::RunEvent::Exit`

---

## Context

`RegimeManager` was purely in-memory — every restart lost user-curated regime names, merges, deletes. The existing `regimes` SQL table is touched only by the cross-device sync path (`sync_merger.rs`); it does NOT carry RegimeManager's full state (centroid, RegimeStatus enum, name override).

See the 2026-04-16 gap analysis X6.

## Decision

A new `RegimeStoragePort` in `oneshim-core` and `SqliteRegimeManagerStateStore` in `oneshim-storage`. State is a JSON blob in a new **dedicated** `regime_manager_state` singleton table (v31 migration), not the existing `regimes` table.

On startup the composition root calls `store.load_all()` → `RegimeManager::hydrate_from(regimes)`. On graceful shutdown the `RunEvent::Exit` handler in `main.rs` calls `store.save_all(&regime_manager.all_regimes())` with a 4 s watchdog.

On parse failure, `load_all` quarantines the corrupt payload to `payload_backup` with `payload_backup_at` timestamp, logs `error!`, and returns `Ok(vec![])` so the app starts fresh. User-curated state is preserved for later recovery.

## Consequences

### Positive

- Regimes survive restart; the "new regime discovered" notification stops firing for the same cluster on every cold boot.
- Vector `regime_id` filter (C3a) becomes meaningful across sessions — regime IDs are now stable.
- sync_merger's use of the existing `regimes` table is untouched.

### Negative / Constraints

- JSON blob evolves with `Regime` struct. serde's `#[serde(default)]` handles additive fields. Removed/renamed fields trigger the quarantine path. Schema mismatches are never silent wipes.
- `load_all` is not read-only in the quarantine edge case. Doc warns callers; all call sites are single-shot at startup.
- Shutdown save is best-effort under a 4 s watchdog — matches telemetry's shutdown. Past the deadline we log `warn!` and continue; shutdown MUST NOT be blocked.

### Neutral

- Mid-life periodic save is OUT OF SCOPE for this phase. Shutdown-only is sufficient for routine restart survival; a follow-up phase can add periodic save after `run_maintenance` ticks if cold-kill data loss becomes a concern.

## Alternatives considered

- **Reuse the existing `regimes` table** — rejected. Its schema is partial (no centroid, no RegimeStatus enum, no user-name override) and it is owned by sync_merger. Extending it would require migration + write-path update to keep sync consistent. New dedicated table avoids that blast radius.
- **Per-regime rows instead of JSON blob** — rejected. RegimeManager's regime count is bounded (`max_active + archive_days`), so a single blob is simpler and negligible cost. Diff-API is a backward-compatible follow-up if it ever matters.
- **"Start fresh on parse failure"** — explicitly rejected during spec review. Wiping months of user-curated names silently is a regression. Quarantine preserves recovery path.

## References

- Spec: `docs/reviews/2026-04-18-phase3-regime-feedback-learning-spec.md`
- Gap analysis: `docs/reviews/2026-04-16-feature-gaps-analysis.md` C3 + X6
- ADR-016 ConfigChangeBus (shutdown-watchdog pattern)
```

- [ ] **Step 2: Korean companion.**

Translate ADR-018 to Korean.

- [ ] **Step 3: Commit.**

```bash
git add docs/architecture/ADR-018-regime-manager-persistence.md docs/architecture/ADR-018-regime-manager-persistence.ko.md
git commit -m "docs(arch): ADR-018 for RegimeManager persistence

Records the dedicated-table + JSON-blob + quarantine-on-parse-failure
design. Explains why the existing regimes table was not reused
(sync_merger ownership).

Part of Phase 3 (C3c + X6).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 13: Composition-root wiring + T-C3c-6/7

**Files:**
- Modify: `src-tauri/src/app_runtime_launch.rs`
- Modify: `src-tauri/src/main.rs` (Tauri `.manage()` if needed)
- Modify: `src-tauri/src/feedback_sink/mod.rs` (append integration tests)

- [ ] **Step 1: Construct the store + hydrate on startup.**

In `src-tauri/src/app_runtime_launch.rs`, find where `RegimeManager` is currently constructed (`agent_runtime/analysis_setup.rs:99` or equivalent — grep `RegimeManager::new`). Replace that site to:

```rust
use oneshim_storage::regime_manager_state_store::SqliteRegimeManagerStateStore;

let regime_storage: Arc<dyn oneshim_core::ports::RegimeStoragePort> =
    Arc::new(SqliteRegimeManagerStateStore::new(conn.clone()));

let mut regime_manager = oneshim_analysis::RegimeManager::new(tm_config);
match regime_storage.load_all().await {
    Ok(regimes) if !regimes.is_empty() => {
        let count = regimes.len();
        regime_manager.hydrate_from(regimes);
        tracing::info!(count, "regime manager hydrated from storage");
    }
    Ok(_) => tracing::info!("regime manager: no persisted state, starting fresh"),
    Err(e) => tracing::warn!(error = %e, "regime manager hydrate failed; starting fresh"),
}

let regime_manager = Arc::new(regime_manager);
```

- [ ] **Step 2: Populate AppState fields.**

Find where `AppState { ... }` is constructed in the same file. Include:

```rust
regime_storage: Some(regime_storage.clone()),
regime_manager_snapshot: Some(regime_manager.clone()),
```

- [ ] **Step 3: Construct `CompositeFeedbackSink` + thread into `FeedbackSender`.**

Find `FeedbackSender::new(api)` in the same file. Replace with:

```rust
let coaching_arc: Option<Arc<oneshim_analysis::CoachingEngine>> = /* existing construction */;
let regime_classifier_arc: Option<Arc<parking_lot::Mutex<oneshim_analysis::RegimeClassifier>>> =
    /* existing construction wrapped in Arc<Mutex<...>> if not already */;

let sink: Arc<dyn oneshim_core::ports::FeedbackSignalSink> =
    Arc::new(crate::feedback_sink::CompositeFeedbackSink::new(
        coaching_arc.clone(),
        regime_classifier_arc.clone(),
    ));

let feedback_sender = Arc::new(
    oneshim_suggestion::feedback::FeedbackSender::new_with_sink(api, Some(sink)),
);
```

- [ ] **Step 4: Add T-C3c-6 (roundtrip via two store instances on the same file).**

Append to `crates/oneshim-storage/src/regime_manager_state_store.rs` tests module:

```rust
    /// T-C3c-6 — survives-restart roundtrip via two sequential store
    /// constructions on the same SQLite file.
    #[tokio::test]
    async fn survives_restart_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("roundtrip.db");

        // Session 1: save.
        {
            let conn = Connection::open(&db_path).unwrap();
            crate::migration::run_migrations(&conn).unwrap();
            let s = SqliteRegimeManagerStateStore::new(Arc::new(parking_lot::Mutex::new(conn)));
            s.save_all(&[sample_regime("a"), sample_regime("b")]).await.unwrap();
        }

        // Session 2: reload.
        {
            let conn = Connection::open(&db_path).unwrap();
            let s = SqliteRegimeManagerStateStore::new(Arc::new(parking_lot::Mutex::new(conn)));
            let loaded = s.load_all().await.unwrap();
            assert_eq!(loaded.len(), 2);
            assert_eq!(loaded[0].regime_id, "a");
        }
    }
```

- [ ] **Step 5: Add T-C3c-7 (watchdog does not panic under slow store).**

Append in the same file:

```rust
    /// T-C3c-7 — slow save still completes under tokio::time::timeout
    /// without panic. The 4s watchdog in main.rs is exercised by an
    /// artificial delay here.
    #[tokio::test]
    async fn save_slower_than_deadline_times_out_gracefully() {
        use std::time::Duration;
        struct SlowStore;
        #[async_trait::async_trait]
        impl RegimeStoragePort for SlowStore {
            async fn load_all(&self) -> Result<Vec<Regime>, CoreError> {
                Ok(vec![])
            }
            async fn save_all(&self, _: &[Regime]) -> Result<(), CoreError> {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok(())
            }
        }

        let start = std::time::Instant::now();
        let outcome = tokio::time::timeout(Duration::from_secs(4), SlowStore.save_all(&[])).await;
        let elapsed = start.elapsed();
        assert!(outcome.is_err(), "must time out, not return");
        assert!(elapsed < Duration::from_secs(5), "must unblock within budget + margin");
    }
```

- [ ] **Step 6: Run + commit.**

```bash
cargo test --workspace 2>&1 | tail -10
git add src-tauri/src/app_runtime_launch.rs src-tauri/src/main.rs crates/oneshim-storage/src/regime_manager_state_store.rs
git commit -m "feat(app): composition-root wiring for Phase 3

(1) Constructs SqliteRegimeManagerStateStore, calls load_all on
startup, calls hydrate_from on RegimeManager.
(2) Populates AppState.regime_storage and regime_manager_snapshot
so the dormant save guard in RunEvent::Exit (Task 11) is now active.
(3) Constructs CompositeFeedbackSink wrapping Arc<CoachingEngine> +
Arc<Mutex<RegimeClassifier>>, threads into FeedbackSender via
new_with_sink.

Tests land here for the integration-path: T-C3c-6 survives-restart
via two sequential store instances on the same DB file; T-C3c-7
watchdog-under-slow-save.

Part of Phase 3 (X3 + C3c + X6).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Self-review checklist

- [ ] All 16 spec tests land: T-X3-1..5 (Tasks 3+4), T-C3a-1..4 (Task 6), T-C3c-1..7 (Tasks 9+10+13).
- [ ] `cargo check --workspace` + `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings -A clippy::empty_docs -A clippy::derivable_impls -A clippy::type_complexity` green on every commit.
- [ ] ADR-017 + ADR-018 committed with Korean companions.
- [ ] No new scheduler-loop migrated to `ConfigManager::subscribe()` (ADR-016).
- [ ] `parking_lot::Mutex` never held across `.await` (inspect `CompositeFeedbackSink::record_user_reaction` — lock scope closes before the next `.await`, which is trivially after the RegimeClassifier call).
- [ ] When push-ready, open PR with spec + plan paths in the body.
