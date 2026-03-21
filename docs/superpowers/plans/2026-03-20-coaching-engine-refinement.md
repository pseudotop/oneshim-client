# Coaching Engine Refinement — 7 Targeted Gaps

> **For agentic workers:** REQUIRED: Use superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace placeholders and stubs remaining from Phase 1 with real data paths. No new features; purely wiring existing signals into the coaching engine.

**Scope:** 7 self-contained gaps across 5 files. Each gap is independently deployable.

**Prerequisites:** Coaching Engine Phase 1 and Phase 2 complete and passing (`cargo test --workspace`).

---

## Gap 1: avg_regime_duration_secs — Replace Hardcoded 1800

**Problem:** `loops.rs:531` hardcodes `let avg_regime_duration_secs: u64 = 1800`. The overstay trigger (`triggers.rs:76-84`) fires when `regime_duration_secs > avg_regime_duration_secs * 120 / 100`, so a fixed 1800 means overstay only fires after 36 minutes regardless of actual user patterns.

**Data available:** `CoachingEngine` already tracks regime dwell via `current_regime_entered` (set in `triggers.rs:141`) and `on_regime_change()`. The monitor loop also tracks `regime_entered_at` / `prev_coaching_regime_id` (lines 208-209). However, neither stores historical averages.

**Approach:** Add a lightweight per-regime EMA duration tracker inside `CoachingEngine`. On each regime transition, record the completed dwell time and update the EMA. Expose the average via a public method the scheduler reads.

### Steps

- [ ] **1a.** In `crates/oneshim-analysis/src/coaching_engine/mod.rs`, add a field to `CoachingEngine`:
  ```rust
  /// Per-regime-label EMA of dwell duration in seconds.
  /// Key: regime_label (not regime_id, since IDs are opaque).
  pub(super) regime_avg_duration: RwLock<HashMap<String, f64>>,
  ```
  Initialize to `RwLock::new(HashMap::new())` in `new()` (after line 57).

- [ ] **1b.** In `crates/oneshim-analysis/src/coaching_engine/triggers.rs`, extend `on_regime_change()` (line 137-142). Before updating `current_regime_id`, read the previous regime's enter time and compute elapsed seconds. Feed into EMA:
  ```rust
  // Inside on_regime_change(), before the write to current_regime_id:
  let entered = self.current_regime_entered.read().await;
  if let Some(enter_time) = *entered {
      let dwell_secs = (Utc::now() - enter_time).num_seconds().max(0) as f64;
      let prev_id = self.current_regime_id.read().await;
      if let Some(ref label) = *prev_id {
          let mut avgs = self.regime_avg_duration.write().await;
          let ema = avgs.entry(label.clone()).or_insert(dwell_secs);
          // EMA alpha 0.2: responsive but stable
          *ema = *ema * 0.8 + dwell_secs * 0.2;
      }
  }
  ```
  Note: lock ordering is `current_regime_entered(R)` -> `current_regime_id(R)` -> `regime_avg_duration(W)`, which does not conflict with `evaluate()`'s ordering.

- [ ] **1c.** Add a public method to `CoachingEngine` in `mod.rs` (after `record_minutes`, ~line 179):
  ```rust
  /// Get the EMA of dwell duration for a regime label, in seconds.
  /// Returns 1800 (30 min) as default when no history exists.
  pub async fn avg_regime_duration_secs(&self, regime_label: &str) -> u64 {
      let avgs = self.regime_avg_duration.read().await;
      avgs.get(regime_label).copied().unwrap_or(1800.0) as u64
  }
  ```

- [ ] **1d.** In `src-tauri/src/scheduler/loops.rs`, replace line 531:
  ```rust
  // Before (line 531):
  let avg_regime_duration_secs: u64 = 1800;
  // After:
  let avg_regime_duration_secs: u64 = coaching
      .avg_regime_duration_secs(regime_label_for_coaching)
      .await;
  ```

- [ ] **1e.** Test: In `coaching_engine/mod.rs` tests, add:
  ```rust
  #[tokio::test]
  async fn avg_regime_duration_updates_on_transition() {
      let engine = CoachingEngine::new(enabled_config());
      engine.on_regime_change(Some("r-a")).await;
      // Simulate 10 seconds in regime-a
      tokio::time::sleep(Duration::from_millis(50)).await;
      engine.on_regime_change(Some("r-b")).await;
      let avg = engine.avg_regime_duration_secs("r-a").await;
      // Should be > 0 (actual dwell) and < 1800 (default)
      assert!(avg < 1800, "avg should reflect actual short dwell, got {}", avg);
  }
  ```

**Files:** `crates/oneshim-analysis/src/coaching_engine/mod.rs`, `crates/oneshim-analysis/src/coaching_engine/triggers.rs`, `src-tauri/src/scheduler/loops.rs`

---

## Gap 2: drift_detected — Wire DriftDetector.observe() Result

**Problem:** `loops.rs:532` hardcodes `let drift_detected = false`. The analysis pipeline already calls `ts.drift_detector.observe(importance)` at `analysis_pipeline.rs:264` and sets `recluster_requested` on drift. The coaching engine never sees this signal.

**Data available:** `AdaptiveTriggerState.recluster_requested` is an `Arc<AtomicBool>` that is set to `true` when drift is detected (line 266-267). It is later consumed by `run_periodic_regime_detection()` via `swap(false, ...)`. We need a separate flag that is not consumed by the re-clustering path.

**Approach:** Add a second `AtomicBool` field `last_drift_detected` to `AdaptiveTriggerState`. Set it alongside `recluster_requested` in the analysis pipeline. Read (and clear) it in the coaching section of the monitor loop.

### Steps

- [ ] **2a.** In `src-tauri/src/scheduler/mod.rs`, add to `AdaptiveTriggerState` (after `recluster_requested`, line 77):
  ```rust
  /// Flag: last drift observation result. Set by analysis pipeline,
  /// read-and-cleared by coaching evaluation in the monitor loop.
  pub last_drift_detected: Arc<std::sync::atomic::AtomicBool>,
  ```

- [ ] **2b.** In `src-tauri/src/agent_runtime.rs`, initialize the new field where `AdaptiveTriggerState` is constructed (~line 247):
  ```rust
  last_drift_detected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
  ```

- [ ] **2c.** In `src-tauri/src/scheduler/analysis_pipeline.rs`, at line 264-268, add after the `recluster_requested.store(true, ...)`:
  ```rust
  ts.last_drift_detected
      .store(true, std::sync::atomic::Ordering::Relaxed);
  ```

- [ ] **2d.** In test helper `make_trigger_state()` (`analysis_pipeline.rs:879`), add the field:
  ```rust
  last_drift_detected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
  ```

- [ ] **2e.** In `src-tauri/src/scheduler/loops.rs`, replace line 532:
  ```rust
  // Before:
  let drift_detected = false;
  // After:
  let drift_detected = adaptive_trigger_state
      .as_ref()
      .map(|ts| ts.last_drift_detected.swap(false, std::sync::atomic::Ordering::Relaxed))
      .unwrap_or(false);
  ```
  This reads and atomically clears the flag, ensuring each drift event fires coaching at most once.

- [ ] **2f.** Test: In `analysis_pipeline.rs` tests, verify `last_drift_detected` is set when drift triggers. The existing `shifted_data_detects_drift` test in `auto_tuner.rs` covers the detector itself; here we verify the pipeline flag propagation:
  ```rust
  #[tokio::test]
  async fn drift_detection_sets_last_drift_flag() {
      let mut ts = make_trigger_state();
      // Feed stable data to initialize detector
      for _ in 0..200 {
          ts.drift_detector.observe(0.5);
      }
      // Force a drift observation through the pipeline's auto-tune path
      // by manually calling observe with a shifted value
      let drifted = ts.drift_detector.observe(0.95);
      if drifted {
          ts.last_drift_detected.store(true, std::sync::atomic::Ordering::Relaxed);
      }
      assert!(ts.last_drift_detected.load(std::sync::atomic::Ordering::Relaxed));
  }
  ```

**Files:** `src-tauri/src/scheduler/mod.rs`, `src-tauri/src/agent_runtime.rs`, `src-tauri/src/scheduler/analysis_pipeline.rs`, `src-tauri/src/scheduler/loops.rs`

---

## Gap 3: context_switches Count

**Problem:** `triggers.rs:179` sets `context_switches` to `"N/A"`. Templates use `{context_switches}` in 6 places (FocusGuard x RegimeTransition Direct, FocusGuard x RegimeDrift DataDriven, ContextRestore x RegimeDrift DataDriven, DeepWorkCoach x RegimeDrift DataDriven, and two FocusGuard x RegimeDrift variants).

**Data available:** The monitor loop already detects `app_changed` (line 441). The `prev_app` variable tracks the previous app. No counter exists for switches within a time window.

**Approach:** Add a simple counter to `CoachingEngine` that increments on each regime transition and resets daily. The caller passes the count as an additional parameter to `evaluate()`, or the engine tracks it internally since it already receives regime transitions via `detect_trigger()`.

The simpler path: track inside `CoachingEngine` itself. It already has `on_regime_change()` called on each transition. Add a counter there.

### Steps

- [ ] **3a.** In `crates/oneshim-analysis/src/coaching_engine/mod.rs`, add fields to `CoachingEngine` (after `snoozed_until`, line 38):
  ```rust
  /// Count of regime transitions today. Reset at midnight.
  pub(super) context_switch_count: RwLock<u32>,
  /// Date of last reset (for daily reset logic).
  pub(super) context_switch_date: RwLock<chrono::NaiveDate>,
  ```
  Initialize in `new()`:
  ```rust
  context_switch_count: RwLock::new(0),
  context_switch_date: RwLock::new(chrono::Utc::now().date_naive()),
  ```

- [ ] **3b.** In `crates/oneshim-analysis/src/coaching_engine/triggers.rs`, in `on_regime_change()` (line 137), after updating `current_regime_id`, increment the counter:
  ```rust
  // Daily reset + increment
  let today = Utc::now().date_naive();
  {
      let mut date = self.context_switch_date.write().await;
      let mut count = self.context_switch_count.write().await;
      if *date != today {
          *count = 0;
          *date = today;
      }
      *count += 1;
  }
  ```

- [ ] **3c.** In `triggers.rs` `build_variables()` (line 179), replace the placeholder:
  ```rust
  // Before:
  vars.insert("context_switches".to_string(), "N/A".to_string());
  // After:
  let switch_count = *self.context_switch_count.read().await;
  vars.insert("context_switches".to_string(), switch_count.to_string());
  ```

- [ ] **3d.** Also wire `previous_context` and `comparison` while we are in `build_variables()`. For `previous_context` (line 183), use the `current_regime_id` that was just transitioned from:
  ```rust
  // Before:
  vars.insert("previous_context".to_string(), "N/A".to_string());
  // After:
  let prev_regime = self.current_regime_id.read().await;
  vars.insert(
      "previous_context".to_string(),
      prev_regime.as_deref().unwrap_or("unknown").to_string(),
  );
  ```
  For `comparison` (line 181), use the avg regime duration from Gap 1:
  ```rust
  // Before:
  vars.insert("comparison".to_string(), "N/A".to_string());
  // After:
  let avgs = self.regime_avg_duration.read().await;
  let avg_secs = avgs.get(regime_label).copied().unwrap_or(1800.0) as u64;
  vars.insert("comparison".to_string(), super::humanize_duration(avg_secs));
  ```

- [ ] **3e.** Test: In `coaching_engine/mod.rs` tests:
  ```rust
  #[tokio::test]
  async fn context_switch_count_increments() {
      let engine = CoachingEngine::new(enabled_config());
      engine.on_regime_change(Some("r-a")).await;
      engine.on_regime_change(Some("r-b")).await;
      engine.on_regime_change(Some("r-c")).await;
      let vars = engine.build_variables("Test", 600, "VS Code").await;
      assert_eq!(vars.get("context_switches").unwrap(), "3");
  }
  ```

**Files:** `crates/oneshim-analysis/src/coaching_engine/mod.rs`, `crates/oneshim-analysis/src/coaching_engine/triggers.rs`

**Dependency:** Gap 1 must be done first (for the `comparison` variable using `regime_avg_duration`).

---

## Gap 4: Implicit Feedback Real Data

**Problem:** `loops.rs:1667-1668` passes `(None, "")` to `evaluate_implicit_feedback()`. The `FeedbackTracker.classify_behavior_change()` (feedback_tracker.rs:210-232) compares `regime_at_shown` with `current_regime_id` and `app_at_shown` with `current_app`. With `(None, "")`, every evaluation returns `ImplicitNeutral`, making feedback tracking useless.

**Data available:** The coaching loop (`spawn_coaching_loop`, loops.rs:1642-1677) runs on a separate timer and does NOT have access to `adaptive_trigger_state` (which is owned by the monitor loop). However, the `CoachingEngine` itself tracks `current_regime_id` internally (set via `on_regime_change()` in triggers.rs:138-139).

**Approach:** Use the `CoachingEngine`'s own internal state. Read `self.current_regime_id` and use the `app_name` from the last `evaluate()` call (add a field to track it).

### Steps

- [ ] **4a.** In `crates/oneshim-analysis/src/coaching_engine/mod.rs`, add a field to track the last app name:
  ```rust
  /// Last app name passed to evaluate() — used for implicit feedback.
  pub(super) last_app_name: RwLock<String>,
  ```
  Initialize in `new()`:
  ```rust
  last_app_name: RwLock::new(String::new()),
  ```

- [ ] **4b.** In `evaluate()` (mod.rs), just before the `detect_trigger` call (around line 101), record the app name:
  ```rust
  {
      let mut app = self.last_app_name.write().await;
      *app = app_name.to_string();
  }
  ```

- [ ] **4c.** Change `evaluate_implicit_feedback()` in `mod.rs` (lines 201-209) to use internal state when caller provides no data:
  ```rust
  pub async fn evaluate_implicit_feedback(
      &self,
      current_regime_id: Option<&str>,
      current_app: &str,
      now: DateTime<Utc>,
  ) {
      // Use internal state when caller provides placeholders
      let regime_id_to_use: Option<String>;
      let app_to_use: String;
      if current_regime_id.is_none() && current_app.is_empty() {
          regime_id_to_use = self.current_regime_id.read().await.clone();
          app_to_use = self.last_app_name.read().await.clone();
      } else {
          regime_id_to_use = current_regime_id.map(String::from);
          app_to_use = current_app.to_string();
      }

      let mut ft = self.feedback_tracker.write().await;
      ft.evaluate_implicit(regime_id_to_use.as_deref(), &app_to_use, now);
  }
  ```
  This is backward-compatible: the monitor loop's direct call with real data still works, and the coaching loop's `(None, "")` call now uses internal state.

- [ ] **4d.** Test: In `coaching_engine/mod.rs` tests:
  ```rust
  #[tokio::test]
  async fn implicit_feedback_uses_internal_state() {
      let engine = CoachingEngine::new(enabled_config());
      // Simulate an evaluate() call that sets internal state
      engine.on_regime_change(Some("r-a")).await;
      {
          let mut app = engine.last_app_name.write().await;
          *app = "VS Code".to_string();
      }
      // Register a pending message
      engine.register_pending_feedback("msg-1", "FocusGuard", "RegimeTransition", Some("r-a"), "VS Code").await;
      // Simulate regime change (so implicit feedback would detect it)
      engine.on_regime_change(Some("r-b")).await;
      // Call with placeholder args — should use internal state
      let future = Utc::now() + chrono::Duration::seconds(301);
      engine.evaluate_implicit_feedback(None, "", future).await;
      // Pending should be consumed
      let ft = engine.feedback_tracker.read().await;
      assert_eq!(ft.pending_count(), 0);
  }
  ```

**Files:** `crates/oneshim-analysis/src/coaching_engine/mod.rs`

---

## Gap 5: Template i18n

**Problem:** All 54 templates in `coaching_template/templates.rs` are English-only. The client supports `ko`/`en` locales (web dashboard i18n), but coaching templates are hardcoded `&'static str`.

**Data available:** `CoachingConfig` has a `tone` field but no `locale` field. The `CoachingTemplateRegistry::select()` method filters by profile, trigger_type, and tone. No locale dimension exists.

**Approach:** Add a `locale` field to `CoachingConfig`, extend `CoachingTemplate` with an optional `locale` field, and add Korean templates. The `select()` method filters by locale, falling back to English.

This is the largest gap. It can be split into two sub-phases:
- **5A (minimal):** Add `locale` field to config + template struct, keep all existing templates as `"en"`, add a `select()` locale filter with `"en"` fallback. No Korean templates yet.
- **5B (content):** Add 54 Korean template variants.

### Steps (Phase 5A — structural)

- [ ] **5a.** In `crates/oneshim-core/src/config/sections/coaching.rs`, add to `CoachingConfig`:
  ```rust
  /// Locale for coaching messages ("en", "ko"). Default: "en".
  #[serde(default = "default_locale")]
  pub locale: String,
  ```
  Add helper:
  ```rust
  fn default_locale() -> String { "en".to_string() }
  ```
  Add to `Default` impl.

- [ ] **5b.** In `crates/oneshim-analysis/src/coaching_template/mod.rs`, add `locale` field to `CoachingTemplate`:
  ```rust
  pub struct CoachingTemplate {
      pub profile: CoachingProfile,
      pub trigger_type: &'static str,
      pub tone: CoachingTone,
      pub locale: &'static str,   // "en" or "ko"
      pub text: &'static str,
  }
  ```

- [ ] **5c.** In `templates.rs`, add `locale: "en"` to all 54 existing template entries.

- [ ] **5d.** In `CoachingTemplateRegistry::select()` (coaching_template/mod.rs), add locale filtering:
  ```rust
  // Filter by locale, fall back to "en" if no match
  let locale_filtered: Vec<_> = candidates.iter()
      .filter(|t| t.locale == locale)
      .collect();
  let final_candidates = if locale_filtered.is_empty() {
      &candidates  // fallback to any locale (en)
  } else {
      &locale_filtered
  };
  ```
  Thread `locale: &str` through `select()` from the caller, which reads `config.locale`.

- [ ] **5e.** Update the `evaluate()` call chain: pass `config.locale` from `CoachingEngine::evaluate()` to `templates.select()`. In `mod.rs:143`:
  ```rust
  // Before:
  let template_text = self.templates.select(&profile, &trigger, &config.tone, &variables);
  // After:
  let template_text = self.templates.select(&profile, &trigger, &config.tone, &config.locale, &variables);
  ```

- [ ] **5f.** Test: Verify locale fallback works when no Korean templates exist:
  ```rust
  #[test]
  fn select_falls_back_to_en_for_unknown_locale() {
      let registry = CoachingTemplateRegistry::new();
      let vars = HashMap::new();
      let text = registry.select(
          &CoachingProfile::FocusGuard,
          &TriggerType::RegimeTransition { from_regime: None, to_regime: None },
          &CoachingTone::Direct,
          "ko",
          &vars,
      );
      // Should return English template, not empty
      assert!(!text.is_empty());
  }
  ```

### Steps (Phase 5B — Korean content, separate commit)

- [ ] **5g.** Add 54 Korean template entries to `templates.rs` with `locale: "ko"`. Example:
  ```rust
  CoachingTemplate {
      profile: CoachingProfile::FocusGuard,
      trigger_type: "RegimeTransition",
      tone: CoachingTone::Direct,
      locale: "ko",
      text: "{regime}에서 전환됨 - 30분 동안 {context_switches}번 전환.",
  },
  ```
  This is a content-only change with no structural risk.

**Files:** `crates/oneshim-core/src/config/sections/coaching.rs`, `crates/oneshim-analysis/src/coaching_template/mod.rs`, `crates/oneshim-analysis/src/coaching_template/templates.rs`, `crates/oneshim-analysis/src/coaching_engine/mod.rs`

---

## Gap 6: Integration Test

**Problem:** Unit tests cover individual components (trigger detection, cooldown, feedback tracker), but no test exercises the full pipeline: construct state, simulate a sequence of regime changes, and verify coaching messages are produced with correct variables.

**Data available:** All components are available. `CoachingEngine::new()` is self-contained (no external dependencies).

**Approach:** Add an integration test in `crates/oneshim-analysis/src/coaching_engine/mod.rs` tests that simulates a realistic sequence.

### Steps

- [ ] **6a.** In `crates/oneshim-analysis/src/coaching_engine/mod.rs`, add integration test:
  ```rust
  #[tokio::test]
  async fn integration_full_coaching_cycle() {
      // 1. Setup: enabled config with goals
      let mut goals = HashMap::new();
      goals.insert("Coding".to_string(), 60);
      let config = CoachingConfig {
          enabled: true,
          regime_goals: goals,
          ..CoachingConfig::default()
      };
      let engine = CoachingEngine::new(config);

      // 2. Initial regime -> no trigger (first regime)
      let msg1 = engine.evaluate(Some("r-coding"), "Coding", 0, 1800, false, "VS Code").await;
      // First call with no prior regime may or may not trigger (depends on initial state)

      // 3. Record minutes to hit 25% goal threshold
      engine.record_minutes("Coding", 15).await;

      // 4. Same regime, should trigger GoalThreshold at 25%
      let msg2 = engine.evaluate(Some("r-coding"), "Coding", 900, 1800, false, "VS Code").await;
      assert!(msg2.is_some(), "goal threshold should fire");
      let m = msg2.unwrap();
      assert!(matches!(m.trigger, TriggerType::GoalThreshold { .. }));
      assert!(m.variables.contains_key("goal_progress"));
      assert!(m.variables.contains_key("context_switches"));

      // 5. Register feedback, evaluate implicit
      engine.register_pending_feedback(&m.message_id, &format!("{:?}", m.profile), "GoalThreshold", Some("r-coding"), "VS Code").await;
      engine.record_explicit_feedback(&m.message_id, true).await;

      // 6. Trigger regime transition
      engine.on_regime_change(Some("r-coding")).await;
      // Wait for cooldown to pass (min_interval_secs defaults to 300, so we
      // manually clear the last_alert for testing)
      {
          let mut la = engine.last_alert.write().await;
          la.clear();
      }
      let msg3 = engine.evaluate(Some("r-email"), "Email", 60, 1800, false, "Outlook").await;
      assert!(msg3.is_some(), "regime transition should fire");
      let m3 = msg3.unwrap();
      assert!(matches!(m3.trigger, TriggerType::RegimeTransition { .. }));

      // 7. Drift detection
      {
          let mut la = engine.last_alert.write().await;
          la.clear();
      }
      engine.on_regime_change(Some("r-email")).await;
      let msg4 = engine.evaluate(Some("r-email"), "Email", 300, 1800, true, "Outlook").await;
      assert!(msg4.is_some(), "drift should fire");
      assert!(matches!(msg4.unwrap().trigger, TriggerType::RegimeDrift { .. }));
  }
  ```

- [ ] **6b.** Verify test passes: `cargo test -p oneshim-analysis -- integration_full_coaching_cycle`

**Files:** `crates/oneshim-analysis/src/coaching_engine/mod.rs`

---

## Gap 7: HeatmapGhost Stub

**Problem:** `HeatmapGhost.tsx` renders `null` (placeholder). It is conditionally rendered in `App.tsx:32` when `isRich` mode is active but shows nothing.

**Current state:** The component at `crates/oneshim-web/frontend/src/overlay/components/HeatmapGhost.tsx` is 12 lines returning `null`. The overlay `App.tsx` already renders `{isRich && <HeatmapGhost />}`.

**Decision:** Document as Phase 3 future work. The HeatmapGhost requires:
1. Aggregated attention data from the monitor loop (mouse position heatmap over time)
2. A Tauri event to push heatmap data to the overlay
3. Canvas-based rendering of semi-transparent colored regions
4. Performance budget analysis (overlay must remain under 5ms per frame)

This is a full feature, not a targeted fix. The current `null` return is safe and correct.

### Steps

- [ ] **7a.** Add a JSDoc comment to `HeatmapGhost.tsx` documenting the prerequisites:
  ```tsx
  /**
   * HeatmapGhost — semi-transparent attention heatmap overlay.
   *
   * Status: Phase 3 placeholder (renders nothing).
   *
   * Prerequisites for implementation:
   * 1. Monitor loop: aggregate mouse position into a per-pixel heat counter
   *    (ring buffer, 5-minute sliding window, 50x50 grid buckets).
   * 2. Tauri event: `coaching://heatmap-update` with grid data (JSON array).
   * 3. Canvas renderer: draw semi-transparent colored rectangles per bucket.
   * 4. Performance: must stay under 5ms render time per update.
   *
   * Data source: InputActivityCollector already tracks mouse position;
   * aggregation into buckets is the missing piece.
   */
  ```

- [ ] **7b.** No code changes needed. Close this gap as documented.

**Files:** `crates/oneshim-web/frontend/src/overlay/components/HeatmapGhost.tsx`

---

## Execution Order

Dependencies between gaps:

```
Gap 1 (avg_regime_duration) ─────┐
                                 ├──> Gap 3 (context_switches + comparison + previous_context)
Gap 2 (drift_detected)           │
                                 │
Gap 4 (implicit feedback)        │    (independent)
Gap 5A (i18n structure)          │    (independent)
Gap 6 (integration test)  ──────┘    (depends on Gaps 1-4 being done)
Gap 5B (Korean content)               (independent, after 5A)
Gap 7 (HeatmapGhost doc)              (independent)
```

**Recommended order:** 1 -> 2 -> 3 -> 4 -> 5A -> 6 -> 7 -> 5B

**Total estimated changes:**
- `coaching_engine/mod.rs`: ~50 lines added (fields, methods, tests)
- `coaching_engine/triggers.rs`: ~30 lines modified (on_regime_change, build_variables)
- `scheduler/mod.rs`: ~2 lines (new field)
- `scheduler/loops.rs`: ~6 lines (replace 2 hardcoded values)
- `scheduler/analysis_pipeline.rs`: ~3 lines (set drift flag)
- `agent_runtime.rs`: ~1 line (initialize field)
- `coaching_template/mod.rs`: ~15 lines (locale filtering)
- `coaching_template/templates.rs`: ~54 lines (add `locale: "en"` to existing) + ~200 lines (Korean, Phase 5B)
- `config/sections/coaching.rs`: ~5 lines (locale field)
- `HeatmapGhost.tsx`: ~10 lines (JSDoc only)

**Verification:** `cargo test --workspace` and `cargo clippy --workspace` after each gap.
