# Text-Heavy App Intelligence Phase 1 — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce `AppRegistry` (unified app profile database with 50+ built-in profiles and JSON override loading), `InputPatternAnalyzer` (5 key-category AtomicU32 counters producing `KeystrokeProfile` ratios), 5 new `WorkType` variants (`TerminalCommands`, `LogReading`, `DocumentWriting`, `DocumentReading`, `ChatComposing`), extended `WorkTypeClassifier` rules using subcategory + keystroke profile, `TextIntelligenceConfig` config section, and scheduler wiring. Phase 1 uses simulated keystroke data — real platform hooks are Phase 1.5.

**Architecture:** Pure-algorithm additions to `oneshim-core` (models, config, registry), `oneshim-monitor` (key-category counters), and `oneshim-analysis` (classifier extension). No new crates, no new port traits, no OS permissions, no accessibility APIs. `AppRegistry` is a pure data lookup in the core crate — not behind a port trait. Counters remain at zero in production until Phase 1.5 wires platform hooks, so all new rules fall through to existing behavior (zero regression).

**Tech Stack:** Rust, serde, serde_json, chrono, AtomicU32

**Spec:** `docs/superpowers/specs/2026-03-19-text-heavy-app-intelligence-design.md` (Sections 4, 5, 7, 8, 12)

---

## File Map

### New files

| File | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/models/app_registry.rs` | `AppSubcategory` enum, `AppProfile` struct, `TitleParseHint`, `AccessibilityStrategy`, `KeyCategory` enum, `KeystrokeProfile` struct |
| `crates/oneshim-core/src/app_registry.rs` | `AppRegistry` struct — built-in profiles, JSON override loading, `lookup()`, `classify()`, `is_sensitive()` |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-core/src/models/mod.rs` | Add `pub mod app_registry;` |
| `crates/oneshim-core/src/models/tiered_memory/content.rs` | Add 5 new `WorkType` variants + `#[serde(other)]` on `Unknown` |
| `crates/oneshim-core/src/models/event.rs` | Add `keystroke_profile: Option<KeystrokeProfile>` to `InputActivityEvent` |
| `crates/oneshim-core/src/config/sections/analysis.rs` | Add `TextIntelligenceConfig` struct + `text_intelligence` field on `AnalysisConfig` |
| `crates/oneshim-core/src/lib.rs` | Add `pub mod app_registry;` for the registry module |
| `crates/oneshim-monitor/src/input_activity.rs` | Add 5 `AtomicU32` counters + `record_categorized_keystroke()` + snapshot extension |
| `crates/oneshim-analysis/src/work_type_classifier.rs` | Add `classify_extended()` + `infer_from_subcategory()` with 11 rule rows |
| `crates/oneshim-analysis/src/lib.rs` | Re-export new types from `work_type_classifier` |

---

## Task 1: Add AppSubcategory enum and KeystrokeProfile model (oneshim-core)

**Files:**
- New: `crates/oneshim-core/src/models/app_registry.rs`
- Modify: `crates/oneshim-core/src/models/mod.rs`

- [ ] **Step 1: Create `app_registry.rs` with `AppSubcategory` enum**

Create `crates/oneshim-core/src/models/app_registry.rs`:

```rust
use serde::{Deserialize, Serialize};

use super::tiered_memory::ContentType;
use super::work_session::AppCategory;

/// Fine-grained application subcategory within an AppCategory.
///
/// AppCategory remains unchanged (no breaking change). AppSubcategory
/// provides the additional granularity needed for text-heavy app
/// intelligence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AppSubcategory {
    // Development
    Terminal,
    Ide,
    TuiEditor,
    ApiTool,
    GitGui,
    // Documentation
    DocumentEditor,
    Spreadsheet,
    Presentation,
    // Communication
    Chat,
    Email,
    VideoCall,
    // Browser
    Browser,
    // Design
    Design,
    // Media
    Media,
    // System
    System,
    #[default]
    Other,
}
```

- [ ] **Step 2: Add `AppProfile`, `TitleParseHint`, `AccessibilityStrategy` structs**

Append to `app_registry.rs`:

```rust
/// Profile describing an application's characteristics for text intelligence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppProfile {
    pub name: String,
    pub name_patterns: Vec<String>,
    pub category: AppCategory,
    pub subcategory: AppSubcategory,
    #[serde(default)]
    pub title_hints: Vec<TitleParseHint>,
    #[serde(default)]
    pub accessibility_strategy: AccessibilityStrategy,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitleParseHint {
    pub separator: String,
    pub content_position: String,
    pub content_type: ContentType,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessibilityStrategy {
    #[default]
    None,
    Native,
    Osascript,
}
```

- [ ] **Step 3: Add `KeyCategory` enum and `KeystrokeProfile` struct**

Append to `app_registry.rs`:

```rust
/// Key category for input pattern analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCategory {
    Enter,
    Tab,
    Arrow,
    Backspace,
    Special,
    Regular,
}

/// Keystroke profile computed from per-category counters.
///
/// Each ratio is `category_count / total_keystrokes`. When total_keystrokes
/// is 0, all ratios are 0.0.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KeystrokeProfile {
    pub enter_ratio: f32,
    pub tab_ratio: f32,
    pub arrow_ratio: f32,
    pub backspace_ratio: f32,
    pub special_ratio: f32,
    pub total_keystrokes: u32,
}
```

- [ ] **Step 4: Register module in `models/mod.rs`**

Add `pub mod app_registry;` to `crates/oneshim-core/src/models/mod.rs`.

- [ ] **Step 5: Write unit tests for serde roundtrip**

Add `#[cfg(test)] mod tests` at bottom of `app_registry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcategory_serde_roundtrip() {
        let val = AppSubcategory::Terminal;
        let json = serde_json::to_string(&val).unwrap();
        assert_eq!(json, r#""terminal""#);
        let back: AppSubcategory = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AppSubcategory::Terminal);
    }

    #[test]
    fn subcategory_default_is_other() {
        assert_eq!(AppSubcategory::default(), AppSubcategory::Other);
    }

    #[test]
    fn accessibility_strategy_default_is_none() {
        assert_eq!(AccessibilityStrategy::default(), AccessibilityStrategy::None);
    }

    #[test]
    fn keystroke_profile_default_is_zero() {
        let p = KeystrokeProfile::default();
        assert_eq!(p.total_keystrokes, 0);
        assert!((p.enter_ratio).abs() < f32::EPSILON);
    }

    #[test]
    fn app_profile_serde_defaults() {
        let json = r#"{
            "name": "Test",
            "name_patterns": ["test"],
            "category": "development",
            "subcategory": "terminal"
        }"#;
        let profile: AppProfile = serde_json::from_str(json).unwrap();
        assert!(profile.enabled);
        assert!(!profile.sensitive);
        assert!(profile.title_hints.is_empty());
        assert_eq!(profile.accessibility_strategy, AccessibilityStrategy::None);
    }
}
```

**Verify:**
```bash
cargo test -p oneshim-core -- app_registry
```

**Commit:** `feat(core): add AppSubcategory, AppProfile, KeyCategory, KeystrokeProfile models`

---

## Task 2: Add 5 new WorkType variants (oneshim-core)

**Files:**
- Modify: `crates/oneshim-core/src/models/tiered_memory/content.rs`

- [ ] **Step 1: Add new variants and `#[serde(other)]` to WorkType**

In `content.rs`, extend the `WorkType` enum. Add 5 new variants before `Unknown`, and add `#[serde(other)]` on `Unknown`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkType {
    ActiveCoding,
    CodeReview,
    Writing,
    Reading,
    Designing,
    FormFilling,
    Browsing,
    PassiveMeeting,
    ActiveMeeting,
    Navigation,
    // Text-heavy app intelligence (Phase 1)
    TerminalCommands,
    LogReading,
    DocumentWriting,
    DocumentReading,
    ChatComposing,
    #[default]
    #[serde(other)]
    Unknown,
}
```

- [ ] **Step 2: Write serde forward-compat test**

Add test to the existing test module in `content.rs` (or create one if absent):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_type_new_variants_serde() {
        let val = WorkType::TerminalCommands;
        let json = serde_json::to_string(&val).unwrap();
        assert_eq!(json, r#""TERMINAL_COMMANDS""#);
        let back: WorkType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, WorkType::TerminalCommands);
    }

    #[test]
    fn work_type_unknown_variant_falls_back() {
        // Simulates a future variant name unknown to this build
        let back: WorkType = serde_json::from_str(r#""SOME_FUTURE_VARIANT""#).unwrap();
        assert_eq!(back, WorkType::Unknown);
    }

    #[test]
    fn work_type_all_new_variants_roundtrip() {
        for variant in [
            WorkType::TerminalCommands,
            WorkType::LogReading,
            WorkType::DocumentWriting,
            WorkType::DocumentReading,
            WorkType::ChatComposing,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let back: WorkType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }
}
```

**Verify:**
```bash
cargo test -p oneshim-core -- tiered_memory::content
```

**Commit:** `feat(core): add 5 new WorkType variants for text-heavy app intelligence`

---

## Task 3: Add KeystrokeProfile to InputActivityEvent (oneshim-core)

**Files:**
- Modify: `crates/oneshim-core/src/models/event.rs`

- [ ] **Step 1: Import KeystrokeProfile and add field**

Add import at top of `event.rs`:

```rust
use super::app_registry::KeystrokeProfile;
```

Add field to `InputActivityEvent`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputActivityEvent {
    pub timestamp: DateTime<Utc>,
    pub period_secs: u32,
    pub mouse: MouseActivity,
    pub keyboard: KeyboardActivity,
    pub app_name: String,
    /// Keystroke profile with key-category ratios.
    /// Present only when `input_pattern_detail` is enabled in config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keystroke_profile: Option<KeystrokeProfile>,
}
```

- [ ] **Step 2: Write backward-compat deserialization test**

```rust
#[test]
fn input_event_without_keystroke_profile_deserializes() {
    let json = r#"{
        "timestamp": "2026-03-19T00:00:00Z",
        "period_secs": 30,
        "mouse": {"click_count":0,"move_distance":0.0,"scroll_count":0,"last_position":null,"double_click_count":0,"right_click_count":0},
        "keyboard": {"keystrokes_per_min":0,"total_keystrokes":0,"typing_bursts":0,"shortcut_count":0,"correction_count":0},
        "app_name": "Code"
    }"#;
    let event: InputActivityEvent = serde_json::from_str(json).unwrap();
    assert!(event.keystroke_profile.is_none());
}
```

**Verify:**
```bash
cargo test -p oneshim-core -- event::tests
```

**Commit:** `feat(core): add optional KeystrokeProfile to InputActivityEvent`

---

## Task 4: Add TextIntelligenceConfig (oneshim-core)

**Files:**
- Modify: `crates/oneshim-core/src/config/sections/analysis.rs`

- [ ] **Step 1: Add TextIntelligenceConfig struct**

Add after `GuiIntelligenceConfig` impl block in `analysis.rs`:

```rust
// ---------------------------------------------------------------------------
// Text Intelligence configuration
// ---------------------------------------------------------------------------

/// Configuration for the Text-Heavy App Intelligence subsystem (Phase 1).
///
/// **Privacy**: input_pattern_detail requires `activity_pattern_learning`
/// consent (GDPR Tier 4). accessibility_extraction (Phase 2) requires the
/// same consent tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextIntelligenceConfig {
    /// Master switch for text-heavy app intelligence.
    /// When false, the system uses existing coarse classification only.
    #[serde(default)]
    pub enabled: bool,

    /// Enable key-category counters (Enter, Tab, Arrow, Backspace, Special).
    /// When false, only aggregate keystroke counts are tracked.
    #[serde(default = "default_input_pattern_detail")]
    pub input_pattern_detail: bool,

    /// Enable OS accessibility API extraction (Phase 2).
    /// Requires Accessibility permission on macOS.
    /// Requires `activity_pattern_learning` consent.
    #[serde(default)]
    pub accessibility_extraction: bool,

    /// PII filter level for accessibility-extracted text (Phase 2).
    #[serde(default = "default_pii_extraction_level")]
    pub pii_extraction_level: crate::config::enums::PiiFilterLevel,
}

impl Default for TextIntelligenceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            input_pattern_detail: default_input_pattern_detail(),
            accessibility_extraction: false,
            pii_extraction_level: default_pii_extraction_level(),
        }
    }
}

fn default_input_pattern_detail() -> bool {
    true
}

fn default_pii_extraction_level() -> crate::config::enums::PiiFilterLevel {
    crate::config::enums::PiiFilterLevel::Standard
}
```

- [ ] **Step 2: Add `text_intelligence` field to `AnalysisConfig`**

Add field and update `Default` impl:

```rust
// In AnalysisConfig struct:
    #[serde(default)]
    pub text_intelligence: TextIntelligenceConfig,

// In Default impl:
    text_intelligence: TextIntelligenceConfig::default(),
```

- [ ] **Step 3: Write config backward-compat test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_config_without_text_intelligence_deserializes() {
        let json = r#"{"enabled": true}"#;
        let config: AnalysisConfig = serde_json::from_str(json).unwrap();
        assert!(!config.text_intelligence.enabled);
        assert!(config.text_intelligence.input_pattern_detail);
    }

    #[test]
    fn text_intelligence_config_defaults() {
        let config = TextIntelligenceConfig::default();
        assert!(!config.enabled);
        assert!(config.input_pattern_detail);
        assert!(!config.accessibility_extraction);
    }
}
```

**Verify:**
```bash
cargo test -p oneshim-core -- config::sections::analysis
```

**Commit:** `feat(core): add TextIntelligenceConfig section to AnalysisConfig`

---

## Task 5: Add 5 key-category counters to InputActivityCollector (oneshim-monitor)

**Files:**
- Modify: `crates/oneshim-monitor/src/input_activity.rs`

- [ ] **Step 1: Add AtomicU32 counters to struct**

Add 5 new fields to `InputActivityCollector` after `correction_count`:

```rust
    // Key-category counters for text-heavy app intelligence (Phase 1).
    // Populated by record_categorized_keystroke(). Remain at zero until
    // Phase 1.5 wires platform key event hooks.
    enter_count: AtomicU32,
    tab_count: AtomicU32,
    arrow_count: AtomicU32,
    backspace_count: AtomicU32,
    special_count: AtomicU32,
```

- [ ] **Step 2: Initialize counters in `new()`**

Add to `new()`:

```rust
    enter_count: AtomicU32::new(0),
    tab_count: AtomicU32::new(0),
    arrow_count: AtomicU32::new(0),
    backspace_count: AtomicU32::new(0),
    special_count: AtomicU32::new(0),
```

- [ ] **Step 3: Add `record_categorized_keystroke()` method**

Add import at top of file:

```rust
use oneshim_core::models::app_registry::KeyCategory;
```

Add method to `impl InputActivityCollector`:

```rust
    /// Record a keystroke with key category classification.
    ///
    /// The caller (platform input hook) classifies each key into one of:
    /// Enter, Tab, Arrow, Backspace, Special, Regular.
    /// Regular keys increment only total_keystrokes.
    /// Category keys increment both their counter AND total_keystrokes.
    pub fn record_categorized_keystroke(
        &self,
        category: KeyCategory,
        is_shortcut: bool,
        is_correction: bool,
    ) {
        self.total_keystrokes.fetch_add(1, Ordering::Relaxed);

        match category {
            KeyCategory::Enter => {
                self.enter_count.fetch_add(1, Ordering::Relaxed);
            }
            KeyCategory::Tab => {
                self.tab_count.fetch_add(1, Ordering::Relaxed);
            }
            KeyCategory::Arrow => {
                self.arrow_count.fetch_add(1, Ordering::Relaxed);
            }
            KeyCategory::Backspace => {
                self.backspace_count.fetch_add(1, Ordering::Relaxed);
                self.correction_count.fetch_add(1, Ordering::Relaxed);
            }
            KeyCategory::Special => {
                self.special_count.fetch_add(1, Ordering::Relaxed);
            }
            KeyCategory::Regular => { /* only total_keystrokes */ }
        }

        // Handle non-Backspace corrections (e.g., Ctrl+Z undo)
        if is_correction && !matches!(category, KeyCategory::Backspace) {
            self.correction_count.fetch_add(1, Ordering::Relaxed);
        }

        if is_shortcut {
            self.shortcut_count.fetch_add(1, Ordering::Relaxed);
        }

        self.record_activity();
    }
```

- [ ] **Step 4: Extend `take_snapshot()` to include KeystrokeProfile**

Add import:

```rust
use oneshim_core::models::app_registry::KeystrokeProfile;
```

Inside `take_snapshot()`, after swapping existing counters, swap the 5 new ones and compute the profile:

```rust
        // Key-category counters
        let enters = self.enter_count.swap(0, Ordering::Relaxed);
        let tabs = self.tab_count.swap(0, Ordering::Relaxed);
        let arrows = self.arrow_count.swap(0, Ordering::Relaxed);
        let backspaces = self.backspace_count.swap(0, Ordering::Relaxed);
        let specials = self.special_count.swap(0, Ordering::Relaxed);

        let keystroke_profile = if enters + tabs + arrows + backspaces + specials > 0 {
            let total = keystrokes.max(1) as f32;
            Some(KeystrokeProfile {
                enter_ratio: enters as f32 / total,
                tab_ratio: tabs as f32 / total,
                arrow_ratio: arrows as f32 / total,
                backspace_ratio: backspaces as f32 / total,
                special_ratio: specials as f32 / total,
                total_keystrokes: keystrokes,
            })
        } else {
            None
        };
```

Then add `keystroke_profile` to the returned `InputActivityEvent`:

```rust
        InputActivityEvent {
            // ...existing fields...
            keystroke_profile,
        }
```

- [ ] **Step 5: Write tests for categorized keystroke recording**

Add tests to the existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn record_categorized_enter() {
        let collector = InputActivityCollector::new();
        collector.record_categorized_keystroke(KeyCategory::Enter, false, false);
        collector.record_categorized_keystroke(KeyCategory::Enter, false, false);
        collector.record_categorized_keystroke(KeyCategory::Regular, false, false);

        let snapshot = collector.take_snapshot();
        assert_eq!(snapshot.keyboard.total_keystrokes, 3);
        let profile = snapshot.keystroke_profile.unwrap();
        assert!((profile.enter_ratio - 2.0 / 3.0).abs() < 0.01);
        assert!((profile.tab_ratio).abs() < f32::EPSILON);
    }

    #[test]
    fn record_categorized_backspace_also_counts_correction() {
        let collector = InputActivityCollector::new();
        collector.record_categorized_keystroke(KeyCategory::Backspace, false, false);

        let snapshot = collector.take_snapshot();
        assert_eq!(snapshot.keyboard.correction_count, 1);
        assert_eq!(snapshot.keyboard.total_keystrokes, 1);
        let profile = snapshot.keystroke_profile.unwrap();
        assert!((profile.backspace_ratio - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn categorized_counters_reset_on_snapshot() {
        let collector = InputActivityCollector::new();
        collector.record_categorized_keystroke(KeyCategory::Tab, false, false);
        let _ = collector.take_snapshot();

        let second = collector.take_snapshot();
        assert!(second.keystroke_profile.is_none());
    }

    #[test]
    fn no_category_keys_yields_none_profile() {
        let collector = InputActivityCollector::new();
        // Only regular keystrokes via the old API
        collector.record_keystroke(false, false);
        collector.record_keystroke(false, false);

        let snapshot = collector.take_snapshot();
        assert!(snapshot.keystroke_profile.is_none());
    }

    #[test]
    fn mixed_categories_produce_correct_ratios() {
        let collector = InputActivityCollector::new();
        // 10 enters, 5 tabs, 5 arrows, 5 regular = 25 total
        for _ in 0..10 {
            collector.record_categorized_keystroke(KeyCategory::Enter, false, false);
        }
        for _ in 0..5 {
            collector.record_categorized_keystroke(KeyCategory::Tab, false, false);
        }
        for _ in 0..5 {
            collector.record_categorized_keystroke(KeyCategory::Arrow, false, false);
        }
        for _ in 0..5 {
            collector.record_categorized_keystroke(KeyCategory::Regular, false, false);
        }

        let snapshot = collector.take_snapshot();
        let profile = snapshot.keystroke_profile.unwrap();
        assert_eq!(profile.total_keystrokes, 25);
        assert!((profile.enter_ratio - 0.4).abs() < 0.01);
        assert!((profile.tab_ratio - 0.2).abs() < 0.01);
        assert!((profile.arrow_ratio - 0.2).abs() < 0.01);
    }
```

**Verify:**
```bash
cargo test -p oneshim-monitor -- input_activity
```

**Commit:** `feat(monitor): add 5 key-category counters and KeystrokeProfile to InputActivityCollector`

---

## Task 6: Build AppRegistry with built-in profiles (oneshim-core)

**Files:**
- New: `crates/oneshim-core/src/app_registry.rs`
- Modify: `crates/oneshim-core/src/lib.rs`

- [ ] **Step 1: Create `AppRegistry` struct with `new()` and built-in data**

Create `crates/oneshim-core/src/app_registry.rs`:

```rust
use std::path::Path;

use crate::error::CoreError;
use crate::models::app_registry::{
    AccessibilityStrategy, AppProfile, AppSubcategory, TitleParseHint,
};
use crate::models::tiered_memory::ContentType;
use crate::models::work_session::AppCategory;

/// Application profile registry.
///
/// Single source of truth for app identification, classification, and
/// behavioral hints. Replaces three scattered app lists.
///
/// Loading order:
/// 1. Built-in profiles (compiled into the binary, ~50 apps)
/// 2. User override file (~/.oneshim/app_profiles.json) merged on top
pub struct AppRegistry {
    profiles: Vec<AppProfile>,
}

impl AppRegistry {
    /// Create a registry with built-in profiles only.
    pub fn new() -> Self {
        Self {
            profiles: built_in_profiles(),
        }
    }

    /// Load user overrides from JSON file and merge with built-in profiles.
    ///
    /// User overrides can add new profiles, modify existing ones (matched
    /// by name_patterns overlap), or disable built-in profiles.
    pub fn load_user_overrides(&mut self, path: &Path) -> Result<(), CoreError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            CoreError::Config(format!(
                "Failed to read app profiles from {}: {}",
                path.display(),
                e
            ))
        })?;
        let overrides: Vec<AppProfile> = serde_json::from_str(&content).map_err(|e| {
            CoreError::Config(format!(
                "Failed to parse app profiles from {}: {}",
                path.display(),
                e
            ))
        })?;

        for override_profile in overrides {
            // Check if any existing profile shares a name_pattern
            let existing_idx = self.profiles.iter().position(|p| {
                p.name_patterns.iter().any(|pat| {
                    override_profile
                        .name_patterns
                        .iter()
                        .any(|op| op.eq_ignore_ascii_case(pat))
                })
            });

            if let Some(idx) = existing_idx {
                self.profiles[idx] = override_profile;
            } else {
                self.profiles.push(override_profile);
            }
        }

        Ok(())
    }

    /// Look up the profile for a given app name. Returns the first matching
    /// enabled profile. O(n) scan over ~100 entries.
    pub fn lookup(&self, app_name: &str) -> Option<&AppProfile> {
        let lower = app_name.to_lowercase();
        self.profiles.iter().find(|p| {
            p.enabled && p.name_patterns.iter().any(|pat| lower.contains(pat))
        })
    }

    /// Convenience: get category + subcategory for an app name.
    /// Falls back to (AppCategory::from_app_name, AppSubcategory::Other)
    /// when no profile matches.
    pub fn classify(&self, app_name: &str) -> (AppCategory, AppSubcategory) {
        match self.lookup(app_name) {
            Some(profile) => (profile.category, profile.subcategory),
            None => (AppCategory::from_app_name(app_name), AppSubcategory::Other),
        }
    }

    /// Check if an app is sensitive (should suppress capture).
    pub fn is_sensitive(&self, app_name: &str) -> bool {
        self.lookup(app_name).map_or(false, |p| p.sensitive)
    }

    /// Number of profiles in the registry.
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

impl Default for AppRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Add `built_in_profiles()` helper function**

Append to the same file. Order profiles most-specific first within each category to avoid shadowing (e.g., "xcode" before "code"). The spec calls for ~50 apps:

```rust
fn p(
    name: &str,
    patterns: &[&str],
    cat: AppCategory,
    sub: AppSubcategory,
    sensitive: bool,
) -> AppProfile {
    AppProfile {
        name: name.to_string(),
        name_patterns: patterns.iter().map(|s| s.to_string()).collect(),
        category: cat,
        subcategory: sub,
        title_hints: vec![],
        accessibility_strategy: AccessibilityStrategy::None,
        sensitive,
        enabled: true,
    }
}

fn built_in_profiles() -> Vec<AppProfile> {
    use AppCategory::*;
    use AppSubcategory::*;

    vec![
        // ── Sensitive (checked first by is_sensitive callers) ──
        p("1Password", &["1password"], System, AppSubcategory::System, true),
        p("LastPass", &["lastpass"], System, AppSubcategory::System, true),
        p("Bitwarden", &["bitwarden"], System, AppSubcategory::System, true),
        p("Dashlane", &["dashlane"], System, AppSubcategory::System, true),
        p("KeePass", &["keepass"], System, AppSubcategory::System, true),
        p("Enpass", &["enpass"], System, AppSubcategory::System, true),
        p("NordPass", &["nordpass"], System, AppSubcategory::System, true),

        // ── Development: Terminals ──
        p("iTerm2", &["iterm"], Development, Terminal, false),
        p("Warp", &["warp"], Development, Terminal, false),
        p("Alacritty", &["alacritty"], Development, Terminal, false),
        p("kitty", &["kitty"], Development, Terminal, false),
        p("Hyper", &["hyper"], Development, Terminal, false),
        p("Terminal", &["terminal.app", "terminal"], Development, Terminal, false),
        p("Konsole", &["konsole"], Development, Terminal, false),
        p("Windows Terminal", &["windows terminal", "windowsterminal", "wt.exe"], Development, Terminal, false),

        // ── Development: IDEs (order matters: xcode before code) ──
        p("Xcode", &["xcode"], Development, Ide, false),
        p("Android Studio", &["android studio"], Development, Ide, false),
        p("Visual Studio Code", &["visual studio code", "code"], Development, Ide, false),
        p("Cursor", &["cursor"], Development, Ide, false),
        p("IntelliJ IDEA", &["intellij"], Development, Ide, false),
        p("WebStorm", &["webstorm"], Development, Ide, false),
        p("PyCharm", &["pycharm"], Development, Ide, false),
        p("GoLand", &["goland"], Development, Ide, false),
        p("RustRover", &["rustrover"], Development, Ide, false),
        p("CLion", &["clion"], Development, Ide, false),
        p("Rider", &["rider"], Development, Ide, false),

        // ── Development: TUI Editors ──
        p("Neovim", &["neovim", "nvim"], Development, TuiEditor, false),
        p("Vim", &["vim"], Development, TuiEditor, false),
        p("Emacs", &["emacs"], Development, TuiEditor, false),

        // ── Development: API Tools ──
        p("Postman", &["postman"], Development, ApiTool, false),
        p("Insomnia", &["insomnia"], Development, ApiTool, false),
        p("Bruno", &["bruno"], Development, ApiTool, false),

        // ── Development: Git GUI ──
        p("SourceTree", &["sourcetree"], Development, GitGui, false),
        p("GitKraken", &["gitkraken"], Development, GitGui, false),
        p("Fork", &["fork"], Development, GitGui, false),

        // ── Documentation: Document Editors ──
        p("Notion", &["notion"], Documentation, DocumentEditor, false),
        p("Obsidian", &["obsidian"], Documentation, DocumentEditor, false),
        p("Typora", &["typora"], Documentation, DocumentEditor, false),
        p("Microsoft Word", &["word"], Documentation, DocumentEditor, false),
        p("Google Docs", &["google docs"], Documentation, DocumentEditor, false),
        p("Pages", &["pages"], Documentation, DocumentEditor, false),

        // ── Documentation: Spreadsheets ──
        p("Microsoft Excel", &["excel"], Documentation, Spreadsheet, false),
        p("Numbers", &["numbers"], Documentation, Spreadsheet, false),
        p("Google Sheets", &["sheets"], Documentation, Spreadsheet, false),

        // ── Documentation: Presentations ──
        p("PowerPoint", &["powerpoint"], Documentation, Presentation, false),
        p("Keynote", &["keynote"], Documentation, Presentation, false),

        // ── Communication: Chat ──
        p("Slack", &["slack"], Communication, Chat, false),
        p("Discord", &["discord"], Communication, Chat, false),
        p("Microsoft Teams", &["teams"], Communication, Chat, false),
        p("KakaoTalk", &["kakaotalk"], Communication, Chat, false),
        p("Telegram", &["telegram"], Communication, Chat, false),
        p("WhatsApp", &["whatsapp"], Communication, Chat, false),

        // ── Communication: Email ──
        p("Mail", &["mail"], Communication, Email, false),
        p("Outlook", &["outlook"], Communication, Email, false),
        p("Thunderbird", &["thunderbird"], Communication, Email, false),
        p("Gmail", &["gmail"], Communication, Email, false),

        // ── Communication: Video ──
        p("Zoom", &["zoom"], Communication, VideoCall, false),
        p("FaceTime", &["facetime"], Communication, VideoCall, false),

        // ── Browser ──
        p("Google Chrome", &["chrome"], Browser, AppSubcategory::Browser, false),
        p("Safari", &["safari"], Browser, AppSubcategory::Browser, false),
        p("Firefox", &["firefox"], Browser, AppSubcategory::Browser, false),
        p("Microsoft Edge", &["edge"], Browser, AppSubcategory::Browser, false),
        p("Arc", &["arc"], Browser, AppSubcategory::Browser, false),
        p("Brave", &["brave"], Browser, AppSubcategory::Browser, false),
        p("Opera", &["opera"], Browser, AppSubcategory::Browser, false),

        // ── Design ──
        p("Figma", &["figma"], Design, AppSubcategory::Design, false),
        p("Sketch", &["sketch"], Design, AppSubcategory::Design, false),
        p("Photoshop", &["photoshop"], Design, AppSubcategory::Design, false),
        p("Illustrator", &["illustrator"], Design, AppSubcategory::Design, false),
        p("Canva", &["canva"], Design, AppSubcategory::Design, false),

        // ── Media ──
        p("Spotify", &["spotify"], Media, AppSubcategory::Media, false),
        p("YouTube", &["youtube"], Media, AppSubcategory::Media, false),
        p("VLC", &["vlc"], Media, AppSubcategory::Media, false),

        // ── System ──
        p("Finder", &["finder"], System, AppSubcategory::System, false),
        p("Activity Monitor", &["activity monitor"], System, AppSubcategory::System, false),
    ]
}
```

- [ ] **Step 3: Register module in `lib.rs`**

Add `pub mod app_registry;` to `crates/oneshim-core/src/lib.rs`.

- [ ] **Step 4: Write AppRegistry unit tests**

Append `#[cfg(test)] mod tests` at bottom of `app_registry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_case_insensitive() {
        let registry = AppRegistry::new();
        let profile = registry.lookup("iTerm2").unwrap();
        assert_eq!(profile.subcategory, AppSubcategory::Terminal);

        let profile2 = registry.lookup("ITERM").unwrap();
        assert_eq!(profile2.subcategory, AppSubcategory::Terminal);
    }

    #[test]
    fn classify_known_app() {
        let registry = AppRegistry::new();
        let (cat, sub) = registry.classify("Visual Studio Code");
        assert_eq!(cat, AppCategory::Development);
        assert_eq!(sub, AppSubcategory::Ide);
    }

    #[test]
    fn classify_unknown_app_falls_back() {
        let registry = AppRegistry::new();
        let (cat, sub) = registry.classify("SomeRandomApp");
        assert_eq!(cat, AppCategory::Other);
        assert_eq!(sub, AppSubcategory::Other);
    }

    #[test]
    fn is_sensitive_password_manager() {
        let registry = AppRegistry::new();
        assert!(registry.is_sensitive("1Password"));
        assert!(registry.is_sensitive("Bitwarden"));
        assert!(!registry.is_sensitive("Visual Studio Code"));
    }

    #[test]
    fn xcode_before_code_ordering() {
        let registry = AppRegistry::new();
        // "xcode" should match Xcode IDE, not VSCode
        let profile = registry.lookup("Xcode").unwrap();
        assert_eq!(profile.name, "Xcode");
    }

    #[test]
    fn vscode_matches_code_pattern() {
        let registry = AppRegistry::new();
        let profile = registry.lookup("Code").unwrap();
        assert_eq!(profile.subcategory, AppSubcategory::Ide);
    }

    #[test]
    fn terminal_subcategories() {
        let registry = AppRegistry::new();
        for app in &["iTerm2", "Warp", "Alacritty", "kitty"] {
            let (_, sub) = registry.classify(app);
            assert_eq!(sub, AppSubcategory::Terminal, "failed for {app}");
        }
    }

    #[test]
    fn chat_subcategories() {
        let registry = AppRegistry::new();
        for app in &["Slack", "Discord", "Teams"] {
            let (_, sub) = registry.classify(app);
            assert_eq!(sub, AppSubcategory::Chat, "failed for {app}");
        }
    }

    #[test]
    fn document_editor_subcategories() {
        let registry = AppRegistry::new();
        for app in &["Notion", "Obsidian", "Word"] {
            let (_, sub) = registry.classify(app);
            assert_eq!(sub, AppSubcategory::DocumentEditor, "failed for {app}");
        }
    }

    #[test]
    fn spreadsheet_subcategories() {
        let registry = AppRegistry::new();
        let (_, sub) = registry.classify("Excel");
        assert_eq!(sub, AppSubcategory::Spreadsheet);
    }

    #[test]
    fn built_in_profile_count() {
        let registry = AppRegistry::new();
        assert!(registry.len() >= 50, "expected 50+ profiles, got {}", registry.len());
    }

    #[test]
    fn disabled_profile_skipped() {
        let mut registry = AppRegistry::new();
        // Simulate user override that disables iTerm2
        let json = r#"[{
            "name": "iTerm2",
            "name_patterns": ["iterm"],
            "category": "development",
            "subcategory": "terminal",
            "enabled": false
        }]"#;
        let tmp = std::env::temp_dir().join("test_app_profiles.json");
        std::fs::write(&tmp, json).unwrap();
        registry.load_user_overrides(&tmp).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert!(registry.lookup("iTerm2").is_none());
    }

    #[test]
    fn user_override_adds_new_profile() {
        let mut registry = AppRegistry::new();
        let json = r#"[{
            "name": "MyCustomApp",
            "name_patterns": ["mycustomapp"],
            "category": "development",
            "subcategory": "ide"
        }]"#;
        let tmp = std::env::temp_dir().join("test_custom_profiles.json");
        std::fs::write(&tmp, json).unwrap();
        registry.load_user_overrides(&tmp).unwrap();
        std::fs::remove_file(&tmp).ok();

        let profile = registry.lookup("MyCustomApp").unwrap();
        assert_eq!(profile.subcategory, AppSubcategory::Ide);
    }
}
```

**Verify:**
```bash
cargo test -p oneshim-core -- app_registry
```

**Commit:** `feat(core): add AppRegistry with 50+ built-in profiles and JSON override loading`

---

## Task 7: Extend WorkTypeClassifier with subcategory-aware rules (oneshim-analysis)

**Files:**
- Modify: `crates/oneshim-analysis/src/work_type_classifier.rs`
- Modify: `crates/oneshim-analysis/src/lib.rs`

- [ ] **Step 1: Add imports for new types**

Add to top of `work_type_classifier.rs`:

```rust
use oneshim_core::models::app_registry::{AppSubcategory, KeystrokeProfile};
```

- [ ] **Step 2: Add `classify_extended()` method**

Add to `impl WorkTypeClassifier`:

```rust
    /// Extended classify method that accepts app subcategory and keystroke profile.
    ///
    /// Falls back to the existing classify() behavior when subcategory is None
    /// or keystroke_profile is None.
    pub fn classify_extended(
        &self,
        keyboard: &KeyboardActivity,
        mouse: &MouseActivity,
        content_label: &str,
        app_category: AppCategory,
        app_subcategory: Option<AppSubcategory>,
        keystroke_profile: Option<&KeystrokeProfile>,
    ) -> (WorkType, EngagementMetrics) {
        let engagement = self.compute_engagement(keyboard, mouse);

        // Try subcategory-aware rules first
        if let (Some(subcategory), Some(profile)) = (app_subcategory, keystroke_profile) {
            if let Some(work_type) =
                self.infer_from_subcategory(&engagement, subcategory, profile)
            {
                return (work_type, engagement);
            }
        }

        // Fall back to existing rules
        let work_type = self.infer_work_type(&engagement, content_label, app_category);
        (work_type, engagement)
    }
```

- [ ] **Step 3: Add `infer_from_subcategory()` with 11 rule rows**

Add private method:

```rust
    /// Subcategory-aware classification rules.
    ///
    /// Rule table (spec Section 7.2):
    /// Terminal   + enter_ratio>0.15 + keys>5/min   -> TerminalCommands
    /// Terminal   + keys<5/min + scroll>5/min        -> LogReading
    /// Terminal   + keys>40/min + arrow_ratio>0.2    -> ActiveCoding (TUI)
    /// Terminal   + keys>40/min                      -> ActiveCoding
    /// DocEditor  + keys>40/min + enter_ratio<0.05   -> DocumentWriting
    /// DocEditor  + keys<5/min + scroll>3/min        -> DocumentReading
    /// DocEditor  + keys>20/min + enter_ratio>0.1    -> Writing (list/outline)
    /// Chat       + keys>20/min + enter_ratio>0.1    -> ChatComposing
    /// Chat       + keys<5/min                       -> Reading
    /// Spreadsheet+ tab_ratio>0.15 + keys>10/min     -> FormFilling
    /// Spreadsheet+ scroll>5/min + keys<5/min        -> Reading
    fn infer_from_subcategory(
        &self,
        engagement: &EngagementMetrics,
        subcategory: AppSubcategory,
        profile: &KeystrokeProfile,
    ) -> Option<WorkType> {
        let kpm = engagement.keystrokes_per_min;
        // NOTE: scroll_events_per_min is a raw count per snapshot period
        // (not a true per-minute rate). Thresholds are calibrated for a
        // 5-30s snapshot interval. Normalize if snapshot interval changes.
        let spm = engagement.scroll_events_per_min;

        match subcategory {
            AppSubcategory::Terminal => {
                if profile.enter_ratio > 0.15 && kpm > 5.0 {
                    return Some(WorkType::TerminalCommands);
                }
                if kpm < 5.0 && spm > 5.0 {
                    return Some(WorkType::LogReading);
                }
                if kpm > 40.0 && profile.arrow_ratio > 0.2 {
                    return Some(WorkType::ActiveCoding);
                }
                if kpm > 40.0 {
                    return Some(WorkType::ActiveCoding);
                }
                None
            }
            AppSubcategory::DocumentEditor => {
                if kpm > 40.0 && profile.enter_ratio < 0.05 {
                    return Some(WorkType::DocumentWriting);
                }
                if kpm < 5.0 && spm > 3.0 {
                    return Some(WorkType::DocumentReading);
                }
                if kpm > 20.0 && profile.enter_ratio > 0.1 {
                    return Some(WorkType::Writing);
                }
                None
            }
            AppSubcategory::Chat => {
                if kpm > 20.0 && profile.enter_ratio > 0.1 {
                    return Some(WorkType::ChatComposing);
                }
                if kpm < 5.0 {
                    return Some(WorkType::Reading);
                }
                None
            }
            AppSubcategory::Spreadsheet => {
                if profile.tab_ratio > 0.15 && kpm > 10.0 {
                    return Some(WorkType::FormFilling);
                }
                if spm > 5.0 && kpm < 5.0 {
                    return Some(WorkType::Reading);
                }
                None
            }
            AppSubcategory::TuiEditor => {
                if kpm > 40.0 {
                    return Some(WorkType::ActiveCoding);
                }
                None
            }
            // Ide and other subcategories fall through to existing rules
            _ => None,
        }
    }
```

- [ ] **Step 4: Write tests for all 11 subcategory rules**

Add to the existing `#[cfg(test)] mod tests`:

```rust
    use oneshim_core::models::app_registry::{AppSubcategory, KeystrokeProfile};

    fn profile(enter: f32, tab: f32, arrow: f32, backspace: f32, special: f32, total: u32) -> KeystrokeProfile {
        KeystrokeProfile {
            enter_ratio: enter,
            tab_ratio: tab,
            arrow_ratio: arrow,
            backspace_ratio: backspace,
            special_ratio: special,
            total_keystrokes: total,
        }
    }

    // ── Terminal rules ──

    #[test]
    fn terminal_commands_high_enter() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(30, 100, 0, 0), &mouse(0, 0), "", AppCategory::Development,
            Some(AppSubcategory::Terminal), Some(&profile(0.20, 0.0, 0.0, 0.0, 0.0, 100)),
        );
        assert_eq!(wt, WorkType::TerminalCommands);
    }

    #[test]
    fn terminal_log_reading_low_keys_high_scroll() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(2, 5, 0, 0), &mouse(0, 10), "", AppCategory::Development,
            Some(AppSubcategory::Terminal), Some(&profile(0.0, 0.0, 0.0, 0.0, 0.0, 5)),
        );
        assert_eq!(wt, WorkType::LogReading);
    }

    #[test]
    fn terminal_tui_editor_arrows() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(50, 200, 0, 0), &mouse(0, 0), "", AppCategory::Development,
            Some(AppSubcategory::Terminal), Some(&profile(0.05, 0.0, 0.25, 0.0, 0.0, 200)),
        );
        assert_eq!(wt, WorkType::ActiveCoding);
    }

    #[test]
    fn terminal_active_coding_fast_typing() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(50, 200, 0, 0), &mouse(0, 0), "", AppCategory::Development,
            Some(AppSubcategory::Terminal), Some(&profile(0.05, 0.0, 0.05, 0.1, 0.0, 200)),
        );
        assert_eq!(wt, WorkType::ActiveCoding);
    }

    // ── Document Editor rules ──

    #[test]
    fn doc_editor_writing_high_keys_low_enter() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(50, 200, 0, 2), &mouse(0, 0), "", AppCategory::Documentation,
            Some(AppSubcategory::DocumentEditor), Some(&profile(0.02, 0.0, 0.0, 0.05, 0.0, 200)),
        );
        assert_eq!(wt, WorkType::DocumentWriting);
    }

    #[test]
    fn doc_editor_reading_low_keys_scrolling() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(2, 5, 0, 0), &mouse(0, 5), "", AppCategory::Documentation,
            Some(AppSubcategory::DocumentEditor), Some(&profile(0.0, 0.0, 0.0, 0.0, 0.0, 5)),
        );
        assert_eq!(wt, WorkType::DocumentReading);
    }

    #[test]
    fn doc_editor_outline_writing() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(25, 100, 0, 1), &mouse(0, 0), "", AppCategory::Documentation,
            Some(AppSubcategory::DocumentEditor), Some(&profile(0.12, 0.0, 0.0, 0.0, 0.0, 100)),
        );
        assert_eq!(wt, WorkType::Writing);
    }

    // ── Chat rules ──

    #[test]
    fn chat_composing() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(30, 100, 0, 1), &mouse(2, 0), "", AppCategory::Communication,
            Some(AppSubcategory::Chat), Some(&profile(0.12, 0.0, 0.0, 0.05, 0.0, 100)),
        );
        assert_eq!(wt, WorkType::ChatComposing);
    }

    #[test]
    fn chat_reading() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(2, 5, 0, 0), &mouse(0, 3), "", AppCategory::Communication,
            Some(AppSubcategory::Chat), Some(&profile(0.0, 0.0, 0.0, 0.0, 0.0, 5)),
        );
        assert_eq!(wt, WorkType::Reading);
    }

    // ── Spreadsheet rules ──

    #[test]
    fn spreadsheet_form_filling() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(15, 60, 0, 0), &mouse(5, 0), "", AppCategory::Documentation,
            Some(AppSubcategory::Spreadsheet), Some(&profile(0.0, 0.20, 0.0, 0.0, 0.0, 60)),
        );
        assert_eq!(wt, WorkType::FormFilling);
    }

    #[test]
    fn spreadsheet_reading() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(2, 5, 0, 0), &mouse(0, 10), "", AppCategory::Documentation,
            Some(AppSubcategory::Spreadsheet), Some(&profile(0.0, 0.0, 0.0, 0.0, 0.0, 5)),
        );
        assert_eq!(wt, WorkType::Reading);
    }

    // ── TUI Editor ──

    #[test]
    fn tui_editor_active_coding() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(50, 200, 0, 0), &mouse(0, 0), "", AppCategory::Development,
            Some(AppSubcategory::TuiEditor), Some(&profile(0.0, 0.0, 0.3, 0.0, 0.1, 200)),
        );
        assert_eq!(wt, WorkType::ActiveCoding);
    }

    // ── Fallback ──

    #[test]
    fn ide_subcategory_falls_through() {
        let c = WorkTypeClassifier::new();
        let (wt, _) = c.classify_extended(
            &kb(65, 300, 10, 5), &mouse(5, 2), "main.rs", AppCategory::Development,
            Some(AppSubcategory::Ide), Some(&profile(0.02, 0.0, 0.0, 0.1, 0.0, 300)),
        );
        // Falls through to existing rules -> ActiveCoding
        assert_eq!(wt, WorkType::ActiveCoding);
    }

    #[test]
    fn no_subcategory_uses_existing_rules() {
        let c = WorkTypeClassifier::new();
        let (wt1, _) = c.classify(
            &kb(65, 300, 10, 5), &mouse(5, 2), "main.rs", AppCategory::Development,
        );
        let (wt2, _) = c.classify_extended(
            &kb(65, 300, 10, 5), &mouse(5, 2), "main.rs", AppCategory::Development,
            None, None,
        );
        assert_eq!(wt1, wt2);
    }
```

- [ ] **Step 5: Update `lib.rs` re-exports**

In `crates/oneshim-analysis/src/lib.rs`, the `work_type_classifier` module is already re-exported. No changes needed unless we want to also export the new types — the module visibility (`mod work_type_classifier` + `pub use`) is sufficient since `classify_extended` is a pub method on the already-exported `WorkTypeClassifier`.

**Verify:**
```bash
cargo test -p oneshim-analysis -- work_type_classifier
```

**Commit:** `feat(analysis): extend WorkTypeClassifier with subcategory-aware classify_extended()`

---

## Task 8: Final integration verification

- [ ] **Step 1: Run full workspace check**

```bash
cargo check --workspace
```

- [ ] **Step 2: Run full workspace tests**

```bash
cargo test --workspace
```

- [ ] **Step 3: Run clippy**

```bash
cargo clippy --workspace
```

- [ ] **Step 4: Run fmt check**

```bash
cargo fmt --check
```

- [ ] **Step 5: Fix any issues found in Steps 1-4**

Address compiler errors, test failures, lint warnings.

**Commit:** `chore: fix lint and test issues from text-heavy app intelligence Phase 1`

---

## Verification Criteria

After all tasks are complete:

1. `cargo test --workspace` passes with 0 failures
2. `cargo clippy --workspace` produces no warnings (except allowed `dead_code`)
3. `AppRegistry::new()` contains 50+ built-in profiles
4. `AppRegistry::classify("iTerm2")` returns `(Development, Terminal)`
5. `InputActivityCollector::record_categorized_keystroke()` correctly increments category counters
6. `take_snapshot()` produces `KeystrokeProfile` with correct ratios when category keys are recorded
7. `take_snapshot()` produces `keystroke_profile: None` when only `record_keystroke()` (old API) is used
8. `classify_extended()` with `None` subcategory produces identical output to `classify()`
9. Terminal + high enter_ratio classifies as `TerminalCommands`
10. DocumentEditor + high kpm + low enter classifies as `DocumentWriting`
11. Chat + moderate kpm + enter classifies as `ChatComposing`
12. `TextIntelligenceConfig` deserializes from JSON missing the field (backward compat)
13. `WorkType::Unknown` deserializes from unrecognized variant names (forward compat via `#[serde(other)]`)
14. All new config fields use `#[serde(default)]` (no breaking change to existing config files)

---

## What Phase 1 Does NOT Include

These are explicitly deferred to later phases:

| Item | Phase |
|------|-------|
| Platform key event hooks (`CGEventTap`, Raw Input) | Phase 1.5 |
| Accessibility API (macOS AXUIElement, Windows UIA) | Phase 2 |
| `FocusedElementInfo`, `ElementRect` models | Phase 2 |
| `AccessibilityExtractor` port trait | Phase 2 |
| `zeroize` dependency for raw text | Phase 3 |
| `full_text_extraction` consent field | Phase 3 |
| Scheduler wiring of `AppRegistry` into real pipeline | Phase 1.5 (when counters produce real data) |
| User override JSON file watching | Future (YAGNI for now) |
