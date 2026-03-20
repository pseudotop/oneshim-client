# Proactive Coaching Engine Phase 1 — Backend Core Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the backend core of the Proactive Coaching Engine: regime-aware trigger evaluation, template-based messaging, goal tracking, effectiveness feedback, and desktop notification delivery. Phase 1 produces all data models, config, storage migration, the coaching engine, and scheduler integration. The MagicOverlay UI is NOT in scope (Phase 2).

**Architecture:** `CoachingEngine` is a concrete struct in `oneshim-analysis` (same pattern as `ContextAnalyzer`). `CoachingConfig`, coaching models, and template types live in `oneshim-core`. Storage migration V17 adds three tables in `oneshim-storage`. Scheduler integration adds a `coaching_loop` spawned from `run_scheduler_loops()`. Desktop notification delivery extends the existing `NotificationManager` in `src-tauri`.

**Tech Stack:** Rust, serde, chrono, tokio (RwLock), uuid, oneshim-core models/config, oneshim-analysis, oneshim-storage (SQLite V17), src-tauri scheduler

**Spec:** `docs/superpowers/specs/2026-03-20-proactive-coaching-engine-design.md`

---

## File Map

### New files

| File | Content |
|------|---------|
| `crates/oneshim-core/src/models/coaching.rs` | `CoachingMessage`, `CoachingProfile`, `TriggerType`, `DismissAction`, `FeedbackSignal`, `GoalProgress`, `GoalProgressView` |
| `crates/oneshim-core/src/config/sections/coaching.rs` | `CoachingConfig`, `ProfileConfig`, `TimeRange` |
| `crates/oneshim-analysis/src/coaching_engine.rs` | `CoachingEngine` struct, trigger detection, profile matching, cooldown, quiet hours |
| `crates/oneshim-analysis/src/coaching_template.rs` | `CoachingTemplateRegistry`, 50+ `const` templates, variable substitution |
| `crates/oneshim-analysis/src/regime_goal_tracker.rs` | `RegimeGoalTracker`, `GoalProgress`, date rollover, threshold detection |
| `crates/oneshim-analysis/src/feedback_tracker.rs` | `FeedbackTracker`, `EffectivenessScore`, implicit/explicit feedback, should_show gating |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-core/src/models/mod.rs` | Add `pub mod coaching;` |
| `crates/oneshim-core/src/config/enums.rs` | Add `CoachingTone`, `DataLookback`, `OverlayMode` enums |
| `crates/oneshim-core/src/config/sections/mod.rs` | Add `mod coaching;` and re-export |
| `crates/oneshim-core/src/config/mod.rs` | Add `coaching: CoachingConfig` field to `AppConfig`, add `CoachingConfig::default()` to `default_config()` |
| `crates/oneshim-analysis/src/lib.rs` | Add `pub mod coaching_engine; pub mod coaching_template; pub mod regime_goal_tracker; pub mod feedback_tracker;` and re-exports |
| `crates/oneshim-storage/src/migration.rs` | Bump `CURRENT_VERSION` to 17; add `migrate_v17()` with 3 tables; add V17 branch in `run_migrations()`; extend tests |
| `src-tauri/src/scheduler/config.rs` | Add `COACHING_INTERVAL_SECS` constant |
| `src-tauri/src/scheduler/mod.rs` | Add `coaching_engine` field to `AdaptiveTriggerState` (or as standalone `Option<CoachingEngine>` on `Scheduler`) |
| `src-tauri/src/scheduler/loops.rs` | Add `spawn_coaching_loop()` method; wire it in `run_scheduler_loops()` |
| `src-tauri/src/notification_manager.rs` | Add `notify_coaching()` method for coaching-specific desktop notifications |

---

## Task 1: Coaching enums in `oneshim-core` config

**Why:** The config section and models both depend on `CoachingTone`, `DataLookback`, and `OverlayMode` enums. These must exist in `enums.rs` first since both `sections/coaching.rs` and `models/coaching.rs` import them.

**Files:**
- Modify: `crates/oneshim-core/src/config/enums.rs`

- [ ] **Step 1.1: Add `CoachingTone` enum**

Append after the `ExternalDataPolicy` enum at the end of the file:

```rust
/// Message tone style for coaching messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CoachingTone {
    /// Short, actionable statements.
    Direct,
    /// Softer, encouraging language.
    #[default]
    Gentle,
    /// Statistics-focused with numbers and comparisons.
    DataDriven,
}
```

```
cargo check -p oneshim-core
```

- [ ] **Step 1.2: Add `DataLookback` enum**

Append after `CoachingTone`:

```rust
/// Historical comparison window for coaching baselines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataLookback {
    /// Compare against today's data only.
    #[default]
    Today,
    /// Rolling 7-day comparison.
    Week,
    /// Rolling 30-day comparison.
    Month,
}
```

```
cargo check -p oneshim-core
```

- [ ] **Step 1.3: Add `OverlayMode` enum**

Append after `DataLookback`:

```rust
/// Overlay display mode (Phase 2 — MagicOverlay). Stored in config for forward compatibility.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayMode {
    /// Focus area border + coaching popup only.
    #[default]
    Minimal,
    /// + bottom progress bar + attention heatmap ghost.
    Rich,
    /// Auto-switches based on regime (deep work → Minimal, transition → Rich).
    Adaptive,
}
```

```
cargo check -p oneshim-core
```

---

## Task 2: Coaching models in `oneshim-core`

**Why:** All other components (`CoachingEngine`, templates, scheduler) depend on these shared model types. They must exist first.

**Files:**
- Create: `crates/oneshim-core/src/models/coaching.rs`
- Modify: `crates/oneshim-core/src/models/mod.rs`

- [ ] **Step 2.1: Create `coaching.rs` with all model types**

Create `crates/oneshim-core/src/models/coaching.rs` with these types:

- `CoachingProfile` enum: `FocusGuard`, `TimeAware`, `DeepWorkCoach`, `ContextRestore`, `GoalTracker`. Derive `Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize`.
- `TriggerType` enum with four variants: `RegimeTransition { from_regime: Option<String>, to_regime: Option<String> }`, `RegimeOverstay { regime_label: String, duration_secs: u64, avg_duration_secs: u64 }`, `RegimeDrift { regime_label: String }`, `GoalThreshold { regime_label: String, target_minutes: u32, current_minutes: u32, threshold_percent: u8 }`. Derive `Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize`.
- `CoachingMessage` struct: `message_id: String`, `profile: CoachingProfile`, `trigger: TriggerType`, `template_text: String`, `personalized_text: Option<String>`, `variables: HashMap<String, String>`, `created_at: DateTime<Utc>`. Derive `Debug, Clone, Serialize, Deserialize`. Add `pub fn display_text(&self) -> &str` method returning personalized text or template text.
- `DismissAction` enum: `Ok`, `Later`, `Timeout`. Derive `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`.
- `FeedbackSignal` enum: `ExplicitPositive`, `ExplicitNegative`, `ImplicitPositive`, `ImplicitNeutral`, `ImplicitNegative`. Derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`.
- `GoalProgress` struct: `regime_label: String`, `current_minutes: u32`, `target_minutes: u32`, `percentage: u8`. Derive `Debug, Clone, Serialize, Deserialize`.
- `GoalProgressView` struct: same as `GoalProgress` plus `display_color: String`. Derive `Debug, Clone, Serialize, Deserialize`.

Add a helper function:

```rust
/// Extract the variant name of a TriggerType for template matching and storage keys.
pub fn trigger_type_name(trigger: &TriggerType) -> String {
    match trigger {
        TriggerType::RegimeTransition { .. } => "RegimeTransition".to_string(),
        TriggerType::RegimeOverstay { .. } => "RegimeOverstay".to_string(),
        TriggerType::RegimeDrift { .. } => "RegimeDrift".to_string(),
        TriggerType::GoalThreshold { .. } => "GoalThreshold".to_string(),
    }
}
```

```
cargo check -p oneshim-core
```

- [ ] **Step 2.2: Register module in `models/mod.rs`**

Add `pub mod coaching;` to `crates/oneshim-core/src/models/mod.rs`.

```
cargo check -p oneshim-core
```

- [ ] **Step 2.3: Add serde round-trip tests**

At the bottom of `coaching.rs`, add `#[cfg(test)] mod tests` with:
- `coaching_message_serde_roundtrip`: serialize and deserialize a `CoachingMessage` with all fields populated, assert equality via re-serialization.
- `display_text_prefers_personalized`: create a message with `personalized_text = Some(...)`, assert `display_text()` returns the personalized text.
- `display_text_falls_back_to_template`: create a message with `personalized_text = None`, assert `display_text()` returns the template text.
- `trigger_type_name_variants`: test all four variants of `trigger_type_name()`.

```
cargo test -p oneshim-core -- coaching
```

---

## Task 3: CoachingConfig section in `oneshim-core`

**Why:** The engine, scheduler, and templates all read from `CoachingConfig`. It must be wired into `AppConfig` before the engine can be constructed.

**Files:**
- Create: `crates/oneshim-core/src/config/sections/coaching.rs`
- Modify: `crates/oneshim-core/src/config/sections/mod.rs`
- Modify: `crates/oneshim-core/src/config/mod.rs`

- [ ] **Step 3.1: Create `coaching.rs` config section**

Create `crates/oneshim-core/src/config/sections/coaching.rs` with:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use super::super::enums::{CoachingTone, DataLookback, OverlayMode};

/// Coaching engine configuration.
///
/// All fields use `#[serde(default)]` for backward-compatible deserialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachingConfig {
    /// Master switch. Default: false (opt-in only).
    #[serde(default)]
    pub enabled: bool,

    /// Per-profile configuration.
    #[serde(default = "default_profile_configs")]
    pub profiles: HashMap<String, ProfileConfig>,

    /// Lookback window for historical comparisons.
    #[serde(default)]
    pub data_lookback: DataLookback,

    /// Message tone preference.
    #[serde(default)]
    pub tone: CoachingTone,

    /// Manual quiet hours (coaching suppressed during these ranges).
    #[serde(default)]
    pub quiet_hours: Vec<TimeRange>,

    /// Per-regime daily time goals (regime_label -> target minutes).
    #[serde(default)]
    pub regime_goals: HashMap<String, u32>,

    /// Overlay display mode (Phase 2 — stored for forward compatibility).
    #[serde(default)]
    pub overlay_mode: OverlayMode,

    /// Overlay toggle hotkey (Phase 2 — stored for forward compatibility).
    #[serde(default = "default_overlay_hotkey")]
    pub overlay_hotkey: String,
}

impl Default for CoachingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            profiles: default_profile_configs(),
            data_lookback: DataLookback::default(),
            tone: CoachingTone::default(),
            quiet_hours: vec![],
            regime_goals: HashMap::new(),
            overlay_mode: OverlayMode::default(),
            overlay_hotkey: default_overlay_hotkey(),
        }
    }
}

fn default_overlay_hotkey() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+Shift+O".to_string()
    } else {
        "Ctrl+Shift+O".to_string()
    }
}

fn default_profile_configs() -> HashMap<String, ProfileConfig> {
    let mut profiles = HashMap::new();
    for name in [
        "FocusGuard",
        "TimeAware",
        "DeepWorkCoach",
        "ContextRestore",
        "GoalTracker",
    ] {
        profiles.insert(name.to_string(), ProfileConfig::default());
    }
    profiles
}

/// Per-profile settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// Whether this profile is active.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Minimum seconds between alerts for this profile.
    #[serde(default = "default_min_interval_secs")]
    pub min_interval_secs: u64,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_interval_secs: 300,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_min_interval_secs() -> u64 {
    300
}

/// Time range for quiet hours.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    /// Start time in "HH:MM" format.
    pub start: String,
    /// End time in "HH:MM" format.
    pub end: String,
}
```

```
cargo check -p oneshim-core
```

- [ ] **Step 3.2: Register in `sections/mod.rs`**

Add `mod coaching;` and add to the public re-export list: `pub use coaching::*;`

Follow the existing pattern (look at how `sync.rs`, `analysis.rs`, etc. are registered).

```
cargo check -p oneshim-core
```

- [ ] **Step 3.3: Add `coaching` field to `AppConfig`**

In `crates/oneshim-core/src/config/mod.rs`, add to the `AppConfig` struct:

```rust
    #[serde(default)]
    pub coaching: CoachingConfig,
```

In `AppConfig::default_config()`, add:

```rust
    coaching: CoachingConfig::default(),
```

```
cargo check -p oneshim-core
```

- [ ] **Step 3.4: Add config round-trip test**

In the `#[cfg(test)] mod tests` at the bottom of `coaching.rs`, add tests:
- `coaching_config_default_disabled`: assert `CoachingConfig::default().enabled == false`.
- `coaching_config_has_five_profiles`: assert default profiles map has 5 entries.
- `coaching_config_serde_roundtrip`: serialize to JSON, deserialize back, re-serialize, assert JSON strings match.
- `coaching_config_unknown_fields_ignored`: add an unknown field to JSON, deserialize, confirm no error.
- `profile_config_default_values`: assert `ProfileConfig::default().enabled == true` and `min_interval_secs == 300`.

```
cargo test -p oneshim-core -- coaching
```

---

## Task 4: V17 storage migration

**Why:** Coaching events, regime goals, and effectiveness scores need persistent storage. The migration must be in place before the engine can persist data.

**Files:**
- Modify: `crates/oneshim-storage/src/migration.rs`

- [ ] **Step 4.1: Bump `CURRENT_VERSION` to 17**

Change line 4:

```rust
const CURRENT_VERSION: u32 = 17;
```

```
cargo check -p oneshim-storage
```

- [ ] **Step 4.2: Add V17 branch in `run_migrations()`**

After the `if current < 16` block (line 80), add:

```rust
    if current < 17 {
        migrate_v17(conn)?;
    }
```

```
cargo check -p oneshim-storage
```

- [ ] **Step 4.3: Implement `migrate_v17()`**

Add the function after `migrate_v16()`:

```rust
fn migrate_v17(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V17 execution: coaching engine tables");

    conn.execute_batch(
        "
        -- Coaching event log: every coaching message shown
        CREATE TABLE IF NOT EXISTS coaching_events (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id              TEXT NOT NULL UNIQUE,
            trigger_type          TEXT NOT NULL,
            profile_name          TEXT NOT NULL,
            regime_id             TEXT,
            message_template      TEXT NOT NULL,
            personalized_message  TEXT,
            shown_at              TEXT NOT NULL,
            dismissed_at          TEXT,
            dismiss_action        TEXT,
            feedback_type         TEXT,
            feedback_score        REAL,
            behavior_change_detected INTEGER DEFAULT 0,
            created_at            TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_coaching_events_profile
            ON coaching_events(profile_name, shown_at);
        CREATE INDEX IF NOT EXISTS idx_coaching_events_regime
            ON coaching_events(regime_id, shown_at);

        -- Per-regime daily time goals (user-configured)
        CREATE TABLE IF NOT EXISTS regime_goals (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,
            regime_label       TEXT NOT NULL UNIQUE,
            daily_target_minutes INTEGER NOT NULL,
            created_at         TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at         TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Aggregated coaching effectiveness scores
        CREATE TABLE IF NOT EXISTS coaching_effectiveness (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_name        TEXT NOT NULL,
            trigger_type        TEXT NOT NULL,
            total_shown         INTEGER NOT NULL DEFAULT 0,
            positive_feedback   REAL NOT NULL DEFAULT 0.0,
            negative_feedback   REAL NOT NULL DEFAULT 0.0,
            neutral_count       INTEGER NOT NULL DEFAULT 0,
            behavior_change_count INTEGER NOT NULL DEFAULT 0,
            updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(profile_name, trigger_type)
        );

        INSERT INTO schema_version (version) VALUES (17);
        ",
    )?;

    info!("migration V17 complete: coaching engine tables created");
    Ok(())
}
```

```
cargo check -p oneshim-storage
```

- [ ] **Step 4.4: Extend migration tests**

In the existing `migration_all_versions` test, add assertions after the V16 checks:

```rust
// V17 tables
let count: i64 = conn
    .query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='coaching_events'",
        [],
        |row| row.get(0),
    )
    .unwrap();
assert_eq!(count, 1);

let count: i64 = conn
    .query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='regime_goals'",
        [],
        |row| row.get(0),
    )
    .unwrap();
assert_eq!(count, 1);

let count: i64 = conn
    .query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='coaching_effectiveness'",
        [],
        |row| row.get(0),
    )
    .unwrap();
assert_eq!(count, 1);

// V17 indexes
let count: i64 = conn
    .query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_coaching_events_profile'",
        [],
        |row| row.get(0),
    )
    .unwrap();
assert_eq!(count, 1);
```

Update the final version assertion from `assert_eq!(version, 16)` to `assert_eq!(version, 17)` (both occurrences in `migration_all_versions` and `migration_idempotent`).

```
cargo test -p oneshim-storage -- migration
```

---

## Task 5: CoachingTemplateRegistry

**Why:** The engine needs a zero-dependency template lookup to produce instant messages (0ms). Templates are compiled into the binary as `const` arrays.

**Files:**
- Create: `crates/oneshim-analysis/src/coaching_template.rs`

- [ ] **Step 5.1: Define `CoachingTemplate` struct and substitution function**

Create the file with:

```rust
use oneshim_core::config::CoachingTone;
use oneshim_core::models::coaching::{CoachingProfile, TriggerType, trigger_type_name};
use std::collections::HashMap;

/// A coaching message template with variable placeholders.
///
/// Placeholders use `{variable_name}` syntax, resolved by
/// `CoachingTemplateRegistry::select()`.
#[derive(Debug, Clone)]
pub struct CoachingTemplate {
    pub profile: CoachingProfile,
    pub trigger_type: &'static str,
    pub tone: CoachingTone,
    pub text: &'static str,
}

/// Replace `{key}` placeholders with values from the variables map.
fn substitute(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    result
}
```

```
cargo check -p oneshim-analysis
```

- [ ] **Step 5.2: Define 50+ default templates as a `const` array**

Add a `const TEMPLATES` array. Organize by profile, then trigger type, then tone. At minimum provide:

| Profile | Trigger | Tone variants | Count |
|---------|---------|---------------|-------|
| FocusGuard | RegimeTransition | Direct, Gentle, DataDriven | 3 |
| FocusGuard | RegimeDrift | Direct, Gentle, DataDriven | 3 |
| TimeAware | RegimeOverstay | Direct, Gentle, DataDriven | 3 |
| TimeAware | GoalThreshold | Direct, Gentle, DataDriven | 3 |
| DeepWorkCoach | RegimeOverstay | Direct, Gentle, DataDriven | 3 |
| DeepWorkCoach | RegimeTransition | Direct, Gentle, DataDriven | 3 |
| ContextRestore | RegimeTransition | Direct, Gentle, DataDriven | 3 |
| GoalTracker | GoalThreshold (25%) | Direct, Gentle, DataDriven | 3 |
| GoalTracker | GoalThreshold (50%) | Direct, Gentle, DataDriven | 3 |
| GoalTracker | GoalThreshold (75%) | Direct, Gentle, DataDriven | 3 |
| GoalTracker | GoalThreshold (100%) | Direct, Gentle, DataDriven | 3 |
| GoalTracker | GoalThreshold (over) | Direct, Gentle, DataDriven | 3 |

That is 36 minimum. Add 14+ additional variant templates to reach 50+. Use these variable placeholders: `{regime}`, `{duration}`, `{goal_progress}`, `{goal_minutes}`, `{remaining_minutes}`, `{comparison}`, `{app_name}`, `{context_switches}`, `{previous_context}`.

Sample templates per spec section 4.4:

```rust
CoachingTemplate { profile: CoachingProfile::FocusGuard, trigger_type: "RegimeTransition", tone: CoachingTone::Direct, text: "You've switched from {regime} - {context_switches} switches in 30 min." },
CoachingTemplate { profile: CoachingProfile::FocusGuard, trigger_type: "RegimeTransition", tone: CoachingTone::Gentle, text: "Heads up: you've moved away from {regime}. Need to switch back?" },
CoachingTemplate { profile: CoachingProfile::FocusGuard, trigger_type: "RegimeTransition", tone: CoachingTone::DataDriven, text: "{context_switches} context switches today. Your average is {comparison}." },
CoachingTemplate { profile: CoachingProfile::DeepWorkCoach, trigger_type: "RegimeOverstay", tone: CoachingTone::Direct, text: "Deep work for {duration}. Take a 5-minute break." },
CoachingTemplate { profile: CoachingProfile::DeepWorkCoach, trigger_type: "RegimeOverstay", tone: CoachingTone::Gentle, text: "Nice focus session! {duration} in deep work. A short break might help." },
// ... etc.
```

```
cargo check -p oneshim-analysis
```

- [ ] **Step 5.3: Implement `CoachingTemplateRegistry`**

```rust
/// Registry holding 50+ templates. Selects by profile + trigger + tone.
pub struct CoachingTemplateRegistry {
    templates: Vec<CoachingTemplate>,
}

impl CoachingTemplateRegistry {
    pub fn new() -> Self {
        Self {
            templates: TEMPLATES.to_vec(),
        }
    }

    /// Select a template matching the profile, trigger, and config tone,
    /// then substitute variables.
    pub fn select(
        &self,
        profile: &CoachingProfile,
        trigger: &TriggerType,
        tone: &CoachingTone,
        variables: &HashMap<String, String>,
    ) -> String {
        let trigger_name = trigger_type_name(trigger);

        // Best match: profile + trigger + tone
        let template = self
            .templates
            .iter()
            .find(|t| t.profile == *profile && t.trigger_type == trigger_name && t.tone == *tone)
            // Fallback: profile + trigger (any tone)
            .or_else(|| {
                self.templates
                    .iter()
                    .find(|t| t.profile == *profile && t.trigger_type == trigger_name)
            })
            // Ultimate fallback: first template for this profile
            .or_else(|| self.templates.iter().find(|t| t.profile == *profile))
            // Should never happen — we have 50+ templates
            .unwrap_or(&self.templates[0]);

        substitute(template.text, variables)
    }

    /// Total number of templates (for metrics/testing).
    pub fn template_count(&self) -> usize {
        self.templates.len()
    }
}
```

```
cargo check -p oneshim-analysis
```

- [ ] **Step 5.4: Add template tests**

Add `#[cfg(test)] mod tests`:
- `registry_has_at_least_50_templates`: assert `template_count() >= 50`.
- `select_exact_match`: query with FocusGuard + RegimeTransition + Direct, confirm template text contains `{regime}` substitution result.
- `select_fallback_wrong_tone`: query with FocusGuard + RegimeTransition + DataDriven for a trigger that only has Gentle templates, confirm a result is still returned.
- `substitute_replaces_all_placeholders`: test `substitute()` with multiple variables, confirm no `{...}` remains.
- `all_profiles_have_templates`: for each variant of `CoachingProfile`, confirm at least one template exists.

```
cargo test -p oneshim-analysis -- coaching_template
```

---

## Task 6: RegimeGoalTracker

**Why:** Tracks per-regime daily time targets and fires `GoalThreshold` triggers at 25/50/75/100% milestones.

**Files:**
- Create: `crates/oneshim-analysis/src/regime_goal_tracker.rs`

- [ ] **Step 6.1: Implement `RegimeGoalTracker` struct**

Create the file following the spec (section 4.5). Key behavior:
- `goals: HashMap<String, u32>` — regime_label to target minutes (from config).
- `today_minutes: HashMap<String, u32>` — accumulated minutes today.
- `tracking_date: NaiveDate` — date for which `today_minutes` is valid.
- `notified_thresholds: HashMap<String, Vec<u8>>` — already-notified thresholds per regime.

Methods:
- `new() -> Self`
- `update_goals(&mut self, regime_goals: &HashMap<String, u32>)` — load from config.
- `record_minutes(&mut self, regime_label: &str, additional_minutes: u32)` — calls `ensure_date_rollover()` first.
- `check_threshold(&mut self, regime_label: &str) -> Option<u8>` — returns newly crossed threshold (25, 50, 75, 100), or None.
- `progress(&self, regime_label: &str) -> Option<GoalProgress>` — current progress snapshot.
- `all_progress(&self) -> Vec<GoalProgress>` — all regimes with goals.
- `fn ensure_date_rollover(&mut self)` — clears counters if date changed.

Use `chrono::Local::now().date_naive()` for date comparison, consistent with existing codebase (see aggregation loop in `loops.rs` line 893).

```
cargo check -p oneshim-analysis
```

- [ ] **Step 6.2: Add tests**

Add `#[cfg(test)] mod tests`:
- `record_and_check_threshold_25`: record enough minutes to cross 25%, assert `check_threshold()` returns `Some(25)`.
- `threshold_not_repeated`: after crossing 25%, additional `check_threshold()` calls return None until 50% crossed.
- `multiple_thresholds_sequential`: record incrementally, confirm 25, 50, 75, 100 each fire exactly once.
- `date_rollover_resets_counters`: simulate date change, confirm counters reset.
- `no_goal_returns_none`: query threshold for a regime with no goal set, confirm None.
- `zero_target_returns_none`: set target to 0, confirm None.
- `progress_returns_correct_values`: set goal=120, record 90 minutes, assert percentage=75.
- `all_progress_includes_all_goals`: set goals for 3 regimes, record minutes for 2, assert all 3 returned.

```
cargo test -p oneshim-analysis -- regime_goal_tracker
```

---

## Task 7: FeedbackTracker

**Why:** Tracks implicit (5-minute window behavior change) and explicit (thumbs-up/down) feedback to adaptively reduce coaching frequency for low-effectiveness triggers.

**Files:**
- Create: `crates/oneshim-analysis/src/feedback_tracker.rs`

- [ ] **Step 7.1: Implement `FeedbackTracker` and `EffectivenessScore`**

Create the file following the spec (section 4.6). Key types and behavior:

`EffectivenessScore`:
- `total_shown: u32`, `positive_signals: f32`, `negative_signals: f32`, `neutral_count: u32`
- `ratio() -> f32`: `positive_signals / (positive + negative + neutral)`. Returns 0.5 when no data.

`PendingEvaluation` (private):
- `shown_at: DateTime<Utc>`, `profile: String`, `trigger: String`, `regime_at_shown: Option<String>`, `app_at_shown: String`

`FeedbackTracker`:
- `scores: HashMap<(String, String), EffectivenessScore>`
- `pending: HashMap<String, PendingEvaluation>`

Methods:
- `new() -> Self`
- `register_pending(message_id, profile, trigger, regime_id, app_name)`
- `record_explicit(message_id, positive: bool)` — removes from pending, updates scores with weight 3.0.
- `evaluate_implicit(current_regime_id, current_app, now)` — processes all pending with elapsed >= 300s. Uses `classify_behavior_change()`.
- `should_show(profile, trigger) -> bool` — returns false (1-in-3) when effectiveness ratio < 0.2 AND total_shown >= 5.
- `classify_behavior_change(pending, current_regime_id, current_app) -> FeedbackSignal` — heuristic from spec section 4.6.
- `get_effectiveness(profile, trigger) -> Option<&EffectivenessScore>` — read-only accessor for storage persistence.
- `pending_count() -> usize` — for testing/metrics.

```
cargo check -p oneshim-analysis
```

- [ ] **Step 7.2: Add tests**

Add `#[cfg(test)] mod tests`:
- `explicit_positive_increases_score`: register pending, record explicit positive, assert positive_signals == 3.0.
- `explicit_negative_increases_negative`: register pending, record explicit negative, assert negative_signals == 3.0.
- `implicit_evaluation_after_5min`: register pending, call `evaluate_implicit()` with `now + 301s`, assert pending cleared and score updated.
- `implicit_not_evaluated_before_5min`: register pending, call `evaluate_implicit()` with `now + 200s`, assert pending still present.
- `should_show_always_true_when_no_data`: assert `should_show()` returns true for unknown profile+trigger.
- `should_show_reduces_for_low_effectiveness`: register 6 events with all-negative feedback, assert `should_show()` returns false for some calls (1-in-3 pattern).
- `classify_regime_change_is_positive`: context changed after coaching message, assert `ImplicitPositive`.
- `classify_no_change_is_neutral`: regime same after coaching message, assert `ImplicitNeutral`.

```
cargo test -p oneshim-analysis -- feedback_tracker
```

---

## Task 8: CoachingEngine

**Why:** Central orchestrator. Evaluates triggers, matches profiles, applies guards (quiet hours, cooldown, effectiveness), and produces `CoachingMessage` instances.

**Files:**
- Create: `crates/oneshim-analysis/src/coaching_engine.rs`

- [ ] **Step 8.1: Implement the `CoachingEngine` struct**

Create the file with the struct following spec section 4.1. Use `tokio::sync::RwLock` for async-safe mutable state (consistent with `NotificationManager` pattern).

```rust
pub struct CoachingEngine {
    config: RwLock<CoachingConfig>,
    templates: CoachingTemplateRegistry,
    goal_tracker: RwLock<RegimeGoalTracker>,
    feedback_tracker: RwLock<FeedbackTracker>,

    // Cooldown state: profile display name -> last alert timestamp
    last_alert: RwLock<HashMap<String, DateTime<Utc>>>,
    // Current regime tracking for transition detection
    current_regime_id: RwLock<Option<String>>,
    current_regime_entered: RwLock<Option<DateTime<Utc>>>,
}
```

Constructor:

```rust
pub fn new(config: CoachingConfig) -> Self {
    let mut goal_tracker = RegimeGoalTracker::new();
    goal_tracker.update_goals(&config.regime_goals);
    Self {
        config: RwLock::new(config),
        templates: CoachingTemplateRegistry::new(),
        goal_tracker: RwLock::new(goal_tracker),
        feedback_tracker: RwLock::new(FeedbackTracker::new()),
        last_alert: RwLock::new(HashMap::new()),
        current_regime_id: RwLock::new(None),
        current_regime_entered: RwLock::new(None),
    }
}
```

```
cargo check -p oneshim-analysis
```

- [ ] **Step 8.2: Implement `evaluate()` method**

The main entry point. Signature:

```rust
pub async fn evaluate(
    &self,
    regime_id: Option<&str>,
    regime_label: &str,
    regime_duration_secs: u64,
    avg_regime_duration_secs: u64,
    drift_detected: bool,
    app_name: &str,
) -> Option<CoachingMessage>
```

Logic (sequential guards):
1. Read config. If `!config.enabled`, return None.
2. `is_quiet_hour()` check. If true, return None.
3. `detect_trigger()` — determines which trigger type fired (if any).
4. `match_profile()` — maps trigger to a coaching profile, checking if that profile is enabled.
5. `check_cooldown()` — enforces `min_interval_secs` per profile.
6. `feedback_tracker.should_show()` — effectiveness gate.
7. `build_variables()` — constructs the HashMap of template variables.
8. `templates.select()` — picks and substitutes the template.
9. `record_alert()` — updates last_alert timestamp for the profile.
10. Return `Some(CoachingMessage { ... })` with `uuid::Uuid::new_v4().to_string()` as message_id.

```
cargo check -p oneshim-analysis
```

- [ ] **Step 8.3: Implement trigger detection**

Private method `detect_trigger()`:

```rust
async fn detect_trigger(
    &self,
    regime_id: Option<&str>,
    regime_duration_secs: u64,
    avg_regime_duration_secs: u64,
    drift_detected: bool,
) -> Option<TriggerType>
```

Logic (priority order):
1. **RegimeTransition**: Compare `regime_id` with `self.current_regime_id`. If different, return `RegimeTransition { from_regime, to_regime }` and update internal state via `on_regime_change()`.
2. **RegimeDrift**: If `drift_detected`, return `RegimeDrift { regime_label }`.
3. **GoalThreshold**: Call `self.goal_tracker.check_threshold(regime_label)`. If Some(threshold), return `GoalThreshold { ... }`.
4. **RegimeOverstay**: If `regime_duration_secs > avg_regime_duration_secs * 120 / 100` (1.2x), return `RegimeOverstay { ... }`.
5. Return None if no trigger fired.

```
cargo check -p oneshim-analysis
```

- [ ] **Step 8.4: Implement profile matching**

Private method `match_profile()`:

```rust
fn match_profile(
    &self,
    config: &CoachingConfig,
    trigger: &TriggerType,
    regime_label: &str,
    app_name: &str,
) -> Option<CoachingProfile>
```

Mapping table from spec section 4.3:
- `RegimeTransition` where from_regime is idle/break -> `ContextRestore`
- `RegimeTransition` other -> `FocusGuard`
- `RegimeDrift` -> `FocusGuard`
- `RegimeOverstay` where regime looks like deep work -> `DeepWorkCoach`
- `RegimeOverstay` other -> `TimeAware`
- `GoalThreshold` -> `GoalTracker`

Check if the matched profile is enabled in `config.profiles`. Return None if disabled.

```
cargo check -p oneshim-analysis
```

- [ ] **Step 8.5: Implement guard methods**

```rust
async fn is_quiet_hour(&self, config: &CoachingConfig) -> bool
```
Parse each `TimeRange` start/end as `NaiveTime` (HH:MM format). Compare against `Local::now().time()`. Return true if current time falls in any range.

```rust
async fn check_cooldown(&self, config: &CoachingConfig, profile: &CoachingProfile) -> bool
```
Look up profile display name in `last_alert`. Compare `Utc::now() - last_alert` against `min_interval_secs` from `config.profiles`. Return true if enough time has passed (or no prior alert).

```rust
async fn record_alert(&self, profile: &CoachingProfile)
```
Insert/update `last_alert[profile_name] = Utc::now()`.

```
cargo check -p oneshim-analysis
```

- [ ] **Step 8.6: Implement `build_variables()` and helper methods**

```rust
async fn build_variables(
    &self,
    regime_label: &str,
    regime_duration_secs: u64,
    app_name: &str,
) -> HashMap<String, String>
```

Build a HashMap with keys: `regime`, `duration`, `app_name`, `goal_progress`, `goal_minutes`, `remaining_minutes`, `comparison`, `context_switches`, `previous_context`.

`duration` formatting: humanize seconds into "Xh Ym" format. Use a private `humanize_duration()` helper.

Goal-related variables: read from `self.goal_tracker.progress(regime_label)`.

Placeholder values for variables not yet available (e.g., `context_switches`, `previous_context`): use empty string or "N/A" — these will be filled properly when the scheduler provides richer context.

Additional public methods:
- `pub async fn on_regime_change(&self, new_regime_id: Option<&str>)` — updates internal tracking.
- `pub async fn update_config(&self, config: CoachingConfig)` — hot-reload config.
- `pub async fn record_minutes(&self, regime_label: &str, minutes: u32)` — delegates to goal tracker.
- `pub async fn register_pending_feedback(...)` — delegates to feedback tracker.
- `pub async fn record_explicit_feedback(...)` — delegates to feedback tracker.
- `pub async fn evaluate_implicit_feedback(...)` — delegates to feedback tracker.

```
cargo check -p oneshim-analysis
```

- [ ] **Step 8.7: Add tests**

Add `#[cfg(test)] mod tests`:
- `evaluate_returns_none_when_disabled`: create engine with `enabled: false`, assert `evaluate()` returns None.
- `evaluate_returns_none_during_quiet_hours`: set quiet hours covering current time, assert None.
- `evaluate_fires_regime_transition`: call evaluate with a different regime_id than previous, assert `RegimeTransition` trigger.
- `evaluate_fires_overstay`: call with `duration > avg * 1.2`, assert `RegimeOverstay`.
- `evaluate_respects_cooldown`: fire evaluate twice rapidly, assert second call returns None.
- `evaluate_fires_goal_threshold`: set goal, record enough minutes, assert `GoalThreshold`.
- `profile_matching_context_restore`: trigger with from_regime containing "idle", assert `ContextRestore` profile.
- `humanize_duration_formats_correctly`: test `humanize_duration(3750)` returns "1h 2m".
- `build_variables_includes_goal_data`: set goal and progress, assert variables map contains goal keys.

```
cargo test -p oneshim-analysis -- coaching_engine
```

---

## Task 9: Register new modules in `oneshim-analysis`

**Why:** The four new files must be exported from the crate so `src-tauri` can use them.

**Files:**
- Modify: `crates/oneshim-analysis/src/lib.rs`

- [ ] **Step 9.1: Add module declarations and re-exports**

Add to `lib.rs`:

```rust
pub mod coaching_engine;
pub mod coaching_template;
pub mod regime_goal_tracker;
pub mod feedback_tracker;

// Re-exports for convenience
pub use coaching_engine::CoachingEngine;
pub use coaching_template::CoachingTemplateRegistry;
pub use regime_goal_tracker::RegimeGoalTracker;
pub use feedback_tracker::FeedbackTracker;
```

```
cargo check -p oneshim-analysis
```

- [ ] **Step 9.2: Verify workspace builds**

```
cargo check --workspace
```

---

## Task 10: Scheduler integration — coaching loop

**Why:** The coaching engine must be evaluated on a regular cadence (30s) within the existing scheduler infrastructure. The loop reads regime state from the existing `AdaptiveTriggerState` and delegates to `CoachingEngine.evaluate()`.

**Files:**
- Modify: `src-tauri/src/scheduler/config.rs`
- Modify: `src-tauri/src/scheduler/mod.rs`
- Modify: `src-tauri/src/scheduler/loops.rs`

- [ ] **Step 10.1: Add `COACHING_INTERVAL_SECS` constant**

In `src-tauri/src/scheduler/config.rs`, add after the `OAUTH_REFRESH_INTERVAL_SECS` constant (line 185):

```rust
/// Coaching evaluation interval — 30 seconds.
pub(super) const COACHING_INTERVAL_SECS: u64 = 30;
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 10.2: Add `coaching_engine` to `Scheduler` struct**

In `src-tauri/src/scheduler/mod.rs`, add a new field to the `Scheduler` struct:

```rust
coaching_engine: Option<Arc<oneshim_analysis::CoachingEngine>>,
```

Initialize it in the constructor from `CoachingConfig` (obtained from `ConfigManager`). The engine is constructed when the `coaching.enabled` flag is true OR `activity_pattern_learning` consent is granted. The engine itself checks `enabled` internally, so creating it always is safe — it will return None from `evaluate()` when disabled.

Alternatively, add `coaching_engine` as `Option<oneshim_analysis::CoachingEngine>` directly on `Scheduler` (not inside `AdaptiveTriggerState`) since the coaching engine needs its own `RwLock` state and does not require mutable access patterns of the analysis pipeline.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 10.3: Implement `spawn_coaching_loop()`**

Add to `src-tauri/src/scheduler/loops.rs`:

```rust
pub(super) fn spawn_coaching_loop(
    &self,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    let coaching = self.coaching_engine.clone();
    let notif = self.notification_manager.clone();

    tokio::spawn(async move {
        let engine = match coaching {
            Some(e) => e,
            None => {
                let _ = shutdown_rx.changed().await;
                return;
            }
        };

        let mut interval = tokio::time::interval(
            Duration::from_secs(super::config::COACHING_INTERVAL_SECS)
        );

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // For Phase 1, we use placeholder values for regime data.
                    // Full integration with AdaptiveTriggerState regime_classifier
                    // will be done when the coaching_engine is wired to consume
                    // shared regime state via the analysis pipeline.
                    //
                    // Evaluate implicit feedback for messages past the 5-min window
                    engine.evaluate_implicit_feedback(None, "", Utc::now()).await;
                }
                _ = shutdown_rx.changed() => {
                    info!("coaching loop ended");
                    break;
                }
            }
        }
    })
}
```

Note: In Phase 1, the coaching loop runs the feedback tracker's implicit evaluation cycle. The full `evaluate()` call with live regime data requires reading from `AdaptiveTriggerState.regime_classifier` and `drift_detector` which are owned by the monitor loop. There are two integration paths:

**Option A (recommended for Phase 1):** The monitor loop calls `coaching_engine.evaluate()` directly at the end of each tick (inside `spawn_monitor_loop()`), similar to how `focus_analyzer.on_app_switch_with_context()` is called. The separate `spawn_coaching_loop()` handles only implicit feedback evaluation.

**Option B (full separate loop):** Use a `tokio::sync::watch` channel to send regime snapshots from the monitor loop to the coaching loop. This is cleaner but more complex.

Implement Option A: add the coaching evaluation call inside `spawn_monitor_loop()` after the GUI pipeline section.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 10.4: Wire `spawn_coaching_loop()` in `run_scheduler_loops()`**

In `run_scheduler_loops()`, after the `cross_device_sync_task` spawn (line ~1616):

```rust
// 13. Coaching evaluation loop
let coaching_task = self.spawn_coaching_loop(shutdown_rx.clone());
```

Add `coaching_task.abort()` in the shutdown section.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 10.5: Wire coaching evaluation in monitor loop**

Inside `spawn_monitor_loop()`, after the GUI pipeline section (around line 490), add:

```rust
// ── Coaching evaluation ──
if let Some(ref coaching) = coaching_engine_ref {
    // Record elapsed minutes for goal tracking
    // (poll interval in seconds / 60 = minutes per tick, rounded)
    let elapsed_minutes = (poll.as_secs() as f32 / 60.0).max(0.0) as u32;
    if elapsed_minutes > 0 {
        coaching.record_minutes(&regime_label_for_coaching, elapsed_minutes).await;
    }

    // Evaluate coaching triggers
    if let Some(message) = coaching.evaluate(
        regime_id_for_coaching,
        &regime_label_for_coaching,
        regime_duration_secs,
        avg_regime_duration_secs,
        drift_detected,
        &app_name,
    ).await {
        // Send desktop notification (Phase 1 delivery)
        if let Some(ref notif) = notif1 {
            notif.notify_coaching(&message.template_text).await;
        }

        // Register for feedback tracking
        coaching.register_pending_feedback(
            &message.message_id,
            &format!("{:?}", message.profile),
            &oneshim_core::models::coaching::trigger_type_name(&message.trigger),
            regime_id_for_coaching,
            &app_name,
        ).await;

        info!(
            profile = ?message.profile,
            trigger = ?message.trigger,
            "coaching message: {}",
            message.template_text,
        );
    }
}
```

For Phase 1, use placeholder values derived from the analysis pipeline state:
- `regime_id` from `adaptive_trigger_state.current_regime_id`
- `regime_label` from the classifier output (or "Unknown" if no regime)
- `regime_duration_secs` estimated from `current_regime_entered` timestamp
- `avg_regime_duration_secs` = 1800 (30min default placeholder)
- `drift_detected` from `adaptive_trigger_state.drift_detector.observe()`

```
cargo check -p oneshim-tauri
```

---

## Task 11: Extend NotificationManager for coaching

**Why:** Phase 1 delivers coaching messages via desktop notifications. The existing `NotificationManager` has a `notify()` method but lacks coaching-specific formatting and cooldown awareness.

**Files:**
- Modify: `src-tauri/src/notification_manager.rs`

- [ ] **Step 11.1: Add `notify_coaching()` method**

Add a new method to `NotificationManager`:

```rust
/// Send a coaching notification through the desktop notification system.
///
/// Uses a "ONESHIM Coach" title prefix to distinguish coaching from system alerts.
/// Does not enforce its own cooldown — the CoachingEngine already applies per-profile cooldowns.
pub async fn notify_coaching(&self, body: &str) {
    let config = self.config.read().await;
    if !config.enabled {
        return;
    }

    if let Err(e) = self
        .notifier
        .show_notification("ONESHIM Coach", body)
        .await
    {
        debug!("coaching notification failure: {e}");
    }
}
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 11.2: Add test**

Add to the existing `#[cfg(test)] mod tests`:

```rust
#[tokio::test]
async fn notify_coaching_sends_when_enabled() {
    let config = NotificationConfig {
        enabled: true,
        ..Default::default()
    };
    let notifier = Arc::new(MockNotifier::new());
    let manager = NotificationManager::new(config, notifier.clone());

    manager.notify_coaching("Deep work for 2h. Take a break.").await;
    assert_eq!(notifier.calls(), 1);
}

#[tokio::test]
async fn notify_coaching_skips_when_disabled() {
    let config = NotificationConfig {
        enabled: false,
        ..Default::default()
    };
    let notifier = Arc::new(MockNotifier::new());
    let manager = NotificationManager::new(config, notifier.clone());

    manager.notify_coaching("Deep work for 2h. Take a break.").await;
    assert_eq!(notifier.calls(), 0);
}
```

```
cargo test -p oneshim-tauri -- notification
```

---

## Task 12: Workspace verification and cleanup

**Why:** Ensure the entire workspace builds, all tests pass, and lint is clean.

- [ ] **Step 12.1: Run `cargo check --workspace`**

```
cargo check --workspace
```

Fix any compilation errors.

- [ ] **Step 12.2: Run all tests**

```
cargo test --workspace
```

Fix any test failures.

- [ ] **Step 12.3: Run clippy**

```
cargo clippy --workspace
```

Fix any warnings (except allowed `dead_code` on future-use variants).

- [ ] **Step 12.4: Run format check**

```
cargo fmt --check
```

Fix any formatting issues.

---

## Summary

| Task | Files | Tests | Description |
|------|-------|-------|-------------|
| 1 | 1 modified | 0 | Coaching enums in config/enums.rs |
| 2 | 1 new + 1 modified | 4 | Coaching models (CoachingMessage, TriggerType, etc.) |
| 3 | 1 new + 2 modified | 5 | CoachingConfig section wired into AppConfig |
| 4 | 1 modified | 4+ | V17 migration: coaching_events, regime_goals, coaching_effectiveness |
| 5 | 1 new | 5 | CoachingTemplateRegistry with 50+ const templates |
| 6 | 1 new | 8 | RegimeGoalTracker with daily rollover and threshold detection |
| 7 | 1 new | 8 | FeedbackTracker with implicit/explicit scoring |
| 8 | 1 new | 9 | CoachingEngine: trigger evaluation, guards, message production |
| 9 | 1 modified | 0 | Module registration in oneshim-analysis lib.rs |
| 10 | 3 modified | 0 | Scheduler integration: coaching loop + monitor loop wiring |
| 11 | 1 modified | 2 | NotificationManager.notify_coaching() |
| 12 | 0 | 0 | Workspace-wide check, test, clippy, fmt |
| **Total** | **6 new + 9 modified** | **~45** | |

### Dependency Order

```
Task 1 (enums)
  └─→ Task 2 (models) + Task 3 (config)
        └─→ Task 4 (V17 migration) — can be parallel with Task 5
        └─→ Task 5 (templates) — depends on Task 2
        └─→ Task 6 (goal tracker) — depends on Task 2
        └─→ Task 7 (feedback tracker) — depends on Task 2
              └─→ Task 8 (engine) — depends on Tasks 5, 6, 7
                    └─→ Task 9 (lib.rs exports)
                          └─→ Task 10 (scheduler) + Task 11 (notifications)
                                └─→ Task 12 (verification)
```

### Phase 1 does NOT include

- MagicOverlay Tauri window (`src-tauri/src/magic_overlay.rs`)
- React frontend overlay components
- Tauri IPC commands (`show_coaching_message`, `upgrade_coaching_message`, etc.)
- LLM personalization background task (template-only in Phase 1)
- Focus area highlight rendering
- Overlay mode toggle hotkey
