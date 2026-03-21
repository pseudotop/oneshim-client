# Proactive Productivity Coaching Engine — Design Spec

> Created: 2026-03-20
> Status: Implemented
> Depends on: Adaptive Tiered Memory (implemented), Standalone LLM Analysis Pipeline (ADR-011, implemented), MagicOverlay infrastructure (src-tauri, implemented)

## 1. Goal

Transform passive activity monitoring into proactive productivity coaching.
The system observes regime transitions, dwell time, drift signals, and goal
progress, then delivers contextual coaching messages through the MagicOverlay
and desktop notifications. Messages are instant (template-based, 0ms) with
optional background LLM personalization (1-3s upgrade). A behavioral feedback
loop tracks whether coaching interventions produce measurable changes,
automatically tuning coaching frequency to avoid nagging.

Key principle: the coaching engine is an **observer, not a blocker**. It never
interrupts workflows, only surfaces information. Users dismiss or ignore
messages at zero cost.

## 2. Design Decisions

| Item | Decision | Rationale |
|------|----------|-----------|
| Trigger source | Regime events + behavior signals | Reuses existing `RegimeManager`, `RegimeClassifier`, `DriftDetector` infrastructure |
| Message generation | Template-first, LLM-upgrade | 0ms initial response; LLM personalization is optional enhancement, not a dependency |
| Delivery channel | MagicOverlay (primary) + desktop notification (fallback) | Overlay is non-intrusive and contextual; desktop notification for when overlay is hidden |
| Feedback model | Implicit-primary, explicit-secondary | Minimizes user friction; explicit feedback weighted 3x when given |
| Coaching profiles | 5 independent profiles | Each addresses a distinct productivity concern; individually toggleable |
| Quiet hours | Auto-detect deep work regime OR manual time ranges | Respects flow state; avoids coaching during deep concentration |
| Storage | SQLite V17 migration | Consistent with existing migration pattern (V1-V16) |
| Consent | Requires existing `activity_pattern_learning` (Tier 4) | No new consent tier needed; coaching is an extension of pattern learning |
| Default state | `coaching.enabled: false` | Opt-in only; user must explicitly enable coaching |

## 3. Architecture

### 3.1 Component Overview

```
Monitor Loop → regime change / overstay / goal threshold
       │
       ▼
┌──────────────────────────────────────────────────────────┐
│  CoachingEngine (oneshim-analysis)                        │
│                                                           │
│  TriggerEvaluator                                        │
│  ├── RegimeTransitionTrigger  (regime A → B)             │
│  ├── RegimeOverstayTrigger    (duration > avg for regime) │
│  ├── RegimeDriftTrigger       (DriftDetector fires)       │
│  └── GoalThresholdTrigger     (progress crosses %)        │
│       │                                                   │
│       ▼                                                   │
│  ProfileMatcher                                          │
│  ├── FocusGuard       (context switch alerts)            │
│  ├── TimeAware        (category time limits)             │
│  ├── DeepWorkCoach    (session management)               │
│  ├── ContextRestore   (post-break context rebuild)       │
│  └── GoalTracker      (regime time goals)                │
│       │                                                   │
│       ▼                                                   │
│  Guards: quiet hours + cooldown + profile enabled         │
│       │                                                   │
│       ▼                                                   │
│  CoachingTemplate → variable substitution → message       │
│       │                                                   │
│       ├──→ MagicOverlay.show(template_message)            │
│       └──→ spawn: LLM personalization                     │
│                │                                          │
│                └──→ if overlay still visible:              │
│                     MagicOverlay.upgrade(personalized_msg) │
│                                                           │
│  FeedbackTracker (after 5min window)                     │
│  └──→ update coaching_effectiveness scores                │
└──────────────────────────────────────────────────────────┘
```

### 3.2 Data Flow (Detailed)

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  RegimeManager   │────→│  CoachingEngine   │←────│  RegimeGoal     │
│  (regime events) │     │  .evaluate()      │     │  Tracker        │
└─────────────────┘     └────────┬─────────┘     └─────────────────┘
                                 │
        ┌────────────────────────┤
        │                        │
        ▼                        ▼
┌───────────────┐      ┌────────────────────┐
│ DriftDetector  │      │ EmaStatsTracker    │
│ (drift signal) │      │ (category baselines)│
└───────────────┘      └────────────────────┘

CoachingEngine.evaluate() returns:
        │
        ▼
┌─────────────────────────────────────────────────────┐
│  CoachingMessage                                     │
│  ├── template_text: String      (instant, 0ms)      │
│  ├── personalized_text: Option  (LLM, 1-3s)         │
│  ├── profile: CoachingProfile                        │
│  ├── trigger: TriggerType                            │
│  └── variables: HashMap<String, String>              │
└──────────────────────┬──────────────────────────────┘
                       │
         ┌─────────────┴────────────┐
         ▼                          ▼
┌─────────────────┐     ┌──────────────────────┐
│  MagicOverlay    │     │  DesktopNotifier      │
│  (Tauri WebView) │     │  (fallback)           │
└─────────────────┘     └──────────────────────┘
         │
         │  after 5 min
         ▼
┌──────────────────────────────┐
│  FeedbackTracker             │
│  ├── implicit behavior scan  │
│  ├── explicit thumbs signal  │
│  └── effectiveness update    │
└──────────────────────────────┘
```

### 3.3 Crate Placement

| Component | Crate | Rationale |
|-----------|-------|-----------|
| `CoachingEngine` | `oneshim-analysis` | Consumes regime/drift data already in this crate |
| `CoachingTemplate` | `oneshim-core` | Pure data model, no I/O; needed by multiple crates |
| `CoachingConfig` | `oneshim-core` | Config section, consistent with existing pattern |
| `RegimeGoalTracker` | `oneshim-analysis` | Depends on regime state from `RegimeManager` |
| `FeedbackTracker` | `oneshim-analysis` | Depends on regime classification + storage |
| `MagicOverlay` | `src-tauri` | Tauri WebView window; platform adapter |
| Storage (V17 migration) | `oneshim-storage` | Consistent with existing migration chain |
| Scheduler integration | `src-tauri` (scheduler) | New `coaching_loop` added to 9-loop scheduler |

## 4. Components

### 4.1 CoachingEngine (`oneshim-analysis`)

Central orchestrator that evaluates triggers and produces coaching messages.
Concrete struct, not a port trait (same pattern as `ContextAnalyzer`).

```rust
use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, NaiveTime, Utc};
use tokio::sync::RwLock;

use oneshim_core::config::CoachingConfig;
use oneshim_core::models::coaching::{
    CoachingMessage, CoachingProfile, TriggerType,
};
use oneshim_core::ports::storage::StorageService;

use crate::auto_tuner::DriftDetector;
use crate::regime_classifier::RegimeClassifier;

/// Evaluates coaching triggers and produces contextual messages.
///
/// Not a port trait — concrete struct (ADR-011 section 3 pattern).
pub struct CoachingEngine {
    config: RwLock<CoachingConfig>,
    templates: CoachingTemplateRegistry,
    goal_tracker: RegimeGoalTracker,
    feedback_tracker: FeedbackTracker,
    storage: Arc<dyn StorageService>,

    // Cooldown state: profile_name → last alert timestamp
    last_alert: RwLock<HashMap<String, DateTime<Utc>>>,
    // Current regime tracking
    current_regime_id: RwLock<Option<String>>,
    current_regime_entered: RwLock<Option<DateTime<Utc>>>,
}

impl CoachingEngine {
    pub fn new(
        config: CoachingConfig,
        storage: Arc<dyn StorageService>,
    ) -> Self {
        Self {
            config: RwLock::new(config),
            templates: CoachingTemplateRegistry::new(),
            goal_tracker: RegimeGoalTracker::new(),
            feedback_tracker: FeedbackTracker::new(),
            storage,
            last_alert: RwLock::new(HashMap::new()),
            current_regime_id: RwLock::new(None),
            current_regime_entered: RwLock::new(None),
        }
    }

    /// Main evaluation entry point. Called from scheduler coaching_loop.
    ///
    /// Returns `Some(CoachingMessage)` if a coaching intervention should be
    /// shown, `None` if suppressed by guards.
    pub async fn evaluate(
        &self,
        regime_id: Option<&str>,
        regime_label: &str,
        regime_duration_secs: u64,
        avg_regime_duration_secs: u64,
        drift_detected: bool,
        app_name: &str,
    ) -> Option<CoachingMessage> {
        let config = self.config.read().await;
        if !config.enabled {
            return None;
        }

        // Guard: quiet hours
        if self.is_quiet_hour(&config).await {
            return None;
        }

        // Detect trigger type
        let trigger = self.detect_trigger(
            regime_id,
            regime_duration_secs,
            avg_regime_duration_secs,
            drift_detected,
        ).await;

        let trigger = trigger?;

        // Match against enabled profiles
        let profile = self.match_profile(&config, &trigger, regime_label, app_name);
        let profile = profile?;

        // Guard: cooldown per profile
        if !self.check_cooldown(&config, &profile).await {
            return None;
        }

        // Guard: effectiveness threshold (reduce frequency for low-effectiveness)
        if !self.feedback_tracker.should_show(&profile, &trigger).await {
            return None;
        }

        // Build template message
        let variables = self.build_variables(
            regime_label,
            regime_duration_secs,
            app_name,
        ).await;

        let template_text = self.templates.select(&profile, &trigger, &variables);

        // Record cooldown
        self.record_alert(&profile).await;

        Some(CoachingMessage {
            message_id: uuid::Uuid::new_v4().to_string(),
            profile: profile.clone(),
            trigger: trigger.clone(),
            template_text,
            personalized_text: None, // filled by background LLM task
            variables,
            created_at: Utc::now(),
        })
    }

    /// Called when regime changes. Updates internal tracking.
    pub async fn on_regime_change(
        &self,
        new_regime_id: Option<&str>,
    ) {
        let mut id = self.current_regime_id.write().await;
        let mut entered = self.current_regime_entered.write().await;
        *id = new_regime_id.map(String::from);
        *entered = Some(Utc::now());
    }
}
```

### 4.2 Trigger Types

Four trigger types drive coaching decisions:

```rust
use serde::{Deserialize, Serialize};

/// Trigger types that can produce coaching messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TriggerType {
    /// Regime changed from one activity mode to another.
    RegimeTransition {
        from_regime: Option<String>,
        to_regime: Option<String>,
    },
    /// User has been in the current regime longer than the historical average.
    RegimeOverstay {
        regime_label: String,
        duration_secs: u64,
        avg_duration_secs: u64,
    },
    /// DriftDetector flagged a behavioral shift within the current regime.
    RegimeDrift {
        regime_label: String,
    },
    /// Goal progress crossed a notable threshold (25%, 50%, 75%, 100%, over).
    GoalThreshold {
        regime_label: String,
        target_minutes: u32,
        current_minutes: u32,
        threshold_percent: u8,
    },
}
```

**Trigger detection logic:**

| Trigger | Condition | Source |
|---------|-----------|--------|
| `RegimeTransition` | `current_regime_id` differs from previous evaluation | `RegimeClassifier` output |
| `RegimeOverstay` | `regime_duration_secs > avg_regime_duration_secs * 1.2` | `RegimeManager.mark_seen()` timestamps + `EmaStatsTracker` baselines |
| `RegimeDrift` | `DriftDetector.observe()` returns `true` | `DriftDetector` (existing, `auto_tuner.rs`) |
| `GoalThreshold` | `current_minutes` crosses 25/50/75/100% of `target_minutes` | `RegimeGoalTracker` |

### 4.3 Coaching Profiles

Five profiles, each targeting a distinct productivity concern:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CoachingProfile {
    /// Alerts when context-switching exceeds user's baseline.
    FocusGuard,
    /// Warns when time in a category approaches user-set limits.
    TimeAware,
    /// Manages deep work sessions: encourages continuation, suggests breaks.
    DeepWorkCoach,
    /// After returning from a break/idle, rebuilds context with a summary.
    ContextRestore,
    /// Tracks per-regime daily time goals and celebrates milestones.
    GoalTracker,
}
```

**Profile-to-trigger mapping:**

| Profile | Triggers | Example message |
|---------|----------|-----------------|
| `FocusGuard` | `RegimeTransition`, `RegimeDrift` | "You've switched contexts 5 times in the last 30 minutes. Consider focusing on {regime} for a while." |
| `TimeAware` | `RegimeOverstay`, `GoalThreshold` | "You've spent {duration} in {regime} today. Your target was {goal_minutes} minutes." |
| `DeepWorkCoach` | `RegimeOverstay` (in deep-work regime) | "Great focus! You've been in deep work for {duration}. Consider a 5-minute break to recharge." |
| `ContextRestore` | `RegimeTransition` (from idle/break regime) | "Welcome back! Before the break you were working on {previous_context}." |
| `GoalTracker` | `GoalThreshold` | "You're 75% toward your {regime} goal today. {remaining_minutes} minutes to go!" |

### 4.4 CoachingTemplate (`oneshim-core`)

Pre-defined message templates with variable substitution. Zero LLM dependency
for instant display.

```rust
use std::collections::HashMap;

/// A coaching message template with variable placeholders.
///
/// Placeholders use `{variable_name}` syntax and are resolved
/// by `CoachingTemplateRegistry::select()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachingTemplate {
    pub template_id: String,
    pub profile: CoachingProfile,
    pub trigger_type: String,  // matches TriggerType variant name
    pub tone: CoachingTone,
    pub text: String,          // e.g., "You've been in {regime} for {duration}."
}

/// Registry holding 50+ templates per regime x trigger type.
pub struct CoachingTemplateRegistry {
    templates: Vec<CoachingTemplate>,
}

impl CoachingTemplateRegistry {
    pub fn new() -> Self {
        Self {
            templates: Self::load_defaults(),
        }
    }

    /// Select a template matching the profile, trigger, and substitute variables.
    pub fn select(
        &self,
        profile: &CoachingProfile,
        trigger: &TriggerType,
        variables: &HashMap<String, String>,
    ) -> String {
        let trigger_name = trigger_type_name(trigger);
        let template = self.templates.iter()
            .find(|t| t.profile == *profile && t.trigger_type == trigger_name)
            .unwrap_or(&self.templates[0]); // fallback to first

        substitute(&template.text, variables)
    }
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

**Template variable reference:**

| Variable | Source | Example |
|----------|--------|---------|
| `{regime}` | Current regime `name` or `auto_label` | "Deep Coding" |
| `{duration}` | Humanized dwell time | "2h 15m" |
| `{goal_progress}` | `current_minutes / target_minutes * 100` | "73%" |
| `{goal_minutes}` | `regime_goals[regime_label]` | "120" |
| `{remaining_minutes}` | `target_minutes - current_minutes` | "32" |
| `{comparison}` | vs. historical average | "25% longer than usual" |
| `{app_name}` | Current foreground app | "VSCode" |
| `{context_switches}` | Count in recent window | "5" |
| `{previous_context}` | Last regime before idle/break | "Coding in auth.rs" |

**Sample templates per profile and tone:**

```
# FocusGuard + RegimeTransition + Direct
"You've switched from {regime} — {context_switches} switches in 30 min."

# FocusGuard + RegimeTransition + Gentle
"Heads up: you've moved away from {regime}. Need to switch back?"

# FocusGuard + RegimeTransition + DataDriven
"{context_switches} context switches today. Your average is {comparison}."

# DeepWorkCoach + RegimeOverstay + Direct
"Deep work for {duration}. Take a 5-minute break."

# DeepWorkCoach + RegimeOverstay + Gentle
"Nice focus session! {duration} in deep work. A short break might help."

# GoalTracker + GoalThreshold + DataDriven
"{goal_progress} of your {regime} goal ({goal_minutes}min). {remaining_minutes}min left."

# ContextRestore + RegimeTransition + Gentle
"Welcome back! You were working in {previous_context} before the break."

# TimeAware + RegimeOverstay + Direct
"{duration} in {regime} today. You've exceeded your {goal_minutes}min target."
```

### 4.5 RegimeGoalTracker (`oneshim-analysis`)

Tracks per-regime daily time targets and real-time progress.

```rust
use std::collections::HashMap;

use chrono::{DateTime, Local, NaiveDate, Utc};

/// Per-regime daily time goal tracking with trend comparison.
pub struct RegimeGoalTracker {
    /// regime_label → daily target in minutes (from CoachingConfig)
    goals: HashMap<String, u32>,
    /// regime_label → accumulated minutes today
    today_minutes: HashMap<String, u32>,
    /// Date for which `today_minutes` is valid
    tracking_date: NaiveDate,
    /// Thresholds already notified today (regime_label → set of % thresholds)
    notified_thresholds: HashMap<String, Vec<u8>>,
}

impl RegimeGoalTracker {
    pub fn new() -> Self {
        Self {
            goals: HashMap::new(),
            today_minutes: HashMap::new(),
            tracking_date: Local::now().date_naive(),
            notified_thresholds: HashMap::new(),
        }
    }

    /// Load goals from CoachingConfig.
    pub fn update_goals(&mut self, regime_goals: &HashMap<String, u32>) {
        self.goals = regime_goals.clone();
    }

    /// Record time spent in a regime. Called on each regime dwell update.
    pub fn record_minutes(&mut self, regime_label: &str, additional_minutes: u32) {
        self.ensure_date_rollover();
        let entry = self.today_minutes
            .entry(regime_label.to_string())
            .or_insert(0);
        *entry += additional_minutes;
    }

    /// Check if a goal threshold was newly crossed.
    /// Returns the threshold percent if newly crossed (25, 50, 75, 100),
    /// or None if no new threshold crossed or no goal set.
    pub fn check_threshold(&mut self, regime_label: &str) -> Option<u8> {
        let target = self.goals.get(regime_label)?;
        let current = self.today_minutes.get(regime_label).copied().unwrap_or(0);

        if *target == 0 {
            return None;
        }

        let progress_pct = ((current as f64 / *target as f64) * 100.0) as u8;
        let thresholds = [25u8, 50, 75, 100];

        let notified = self.notified_thresholds
            .entry(regime_label.to_string())
            .or_default();

        for &threshold in &thresholds {
            if progress_pct >= threshold && !notified.contains(&threshold) {
                notified.push(threshold);
                return Some(threshold);
            }
        }

        None
    }

    /// Get current progress for a regime (minutes, target, percentage).
    pub fn progress(&self, regime_label: &str) -> Option<GoalProgress> {
        let target = self.goals.get(regime_label)?;
        let current = self.today_minutes.get(regime_label).copied().unwrap_or(0);
        Some(GoalProgress {
            current_minutes: current,
            target_minutes: *target,
            percentage: if *target > 0 {
                ((current as f64 / *target as f64) * 100.0).min(999.0) as u8
            } else {
                0
            },
        })
    }

    /// Reset counters on date change.
    fn ensure_date_rollover(&mut self) {
        let today = Local::now().date_naive();
        if today != self.tracking_date {
            self.today_minutes.clear();
            self.notified_thresholds.clear();
            self.tracking_date = today;
        }
    }
}

#[derive(Debug, Clone)]
pub struct GoalProgress {
    pub current_minutes: u32,
    pub target_minutes: u32,
    pub percentage: u8,
}
```

### 4.6 FeedbackTracker (`oneshim-analysis`)

Tracks implicit and explicit feedback to adjust coaching frequency.

```rust
use std::collections::HashMap;

use chrono::{DateTime, Utc};

/// Feedback signal type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackSignal {
    /// User clicked thumbs-up.
    ExplicitPositive,
    /// User clicked thumbs-down.
    ExplicitNegative,
    /// Behavior changed positively after suggestion (implicit).
    ImplicitPositive,
    /// No behavior change observed (implicit).
    ImplicitNeutral,
    /// Negative pattern continued or worsened (implicit).
    ImplicitNegative,
}

/// Effectiveness scores for a profile+trigger combination.
#[derive(Debug, Clone, Default)]
pub struct EffectivenessScore {
    pub total_shown: u32,
    pub positive_signals: f32,  // weighted sum (explicit=3x, implicit=1x)
    pub negative_signals: f32,
    pub neutral_count: u32,
}

impl EffectivenessScore {
    /// Effectiveness ratio in [0.0, 1.0].
    pub fn ratio(&self) -> f32 {
        let total_weight = self.positive_signals + self.negative_signals
            + self.neutral_count as f32;
        if total_weight < 1.0 {
            return 0.5; // no data — neutral default
        }
        self.positive_signals / total_weight
    }
}

pub struct FeedbackTracker {
    /// (profile_name, trigger_name) → effectiveness
    scores: HashMap<(String, String), EffectivenessScore>,
    /// Pending evaluation: message_id → (shown_at, profile, trigger, regime_snapshot)
    pending: HashMap<String, PendingEvaluation>,
}

struct PendingEvaluation {
    shown_at: DateTime<Utc>,
    profile: String,
    trigger: String,
    regime_at_shown: Option<String>,
    app_at_shown: String,
}

impl FeedbackTracker {
    pub fn new() -> Self {
        Self {
            scores: HashMap::new(),
            pending: HashMap::new(),
        }
    }

    /// Register a coaching message for later evaluation.
    pub fn register_pending(
        &mut self,
        message_id: &str,
        profile: &str,
        trigger: &str,
        regime_id: Option<&str>,
        app_name: &str,
    ) {
        self.pending.insert(message_id.to_string(), PendingEvaluation {
            shown_at: Utc::now(),
            profile: profile.to_string(),
            trigger: trigger.to_string(),
            regime_at_shown: regime_id.map(String::from),
            app_at_shown: app_name.to_string(),
        });
    }

    /// Record explicit feedback (thumbs-up/down from overlay).
    pub fn record_explicit(&mut self, message_id: &str, positive: bool) {
        if let Some(pending) = self.pending.remove(message_id) {
            let key = (pending.profile, pending.trigger);
            let score = self.scores.entry(key).or_default();
            score.total_shown += 1;
            if positive {
                score.positive_signals += 3.0; // explicit = 3x weight
            } else {
                score.negative_signals += 3.0;
            }
        }
    }

    /// Evaluate implicit feedback after the 5-minute observation window.
    ///
    /// Called periodically. Processes all pending evaluations whose
    /// observation window has elapsed.
    pub fn evaluate_implicit(
        &mut self,
        current_regime_id: Option<&str>,
        current_app: &str,
        now: DateTime<Utc>,
    ) {
        let expired: Vec<String> = self.pending.iter()
            .filter(|(_, p)| (now - p.shown_at).num_seconds() >= 300)
            .map(|(id, _)| id.clone())
            .collect();

        for message_id in expired {
            if let Some(pending) = self.pending.remove(&message_id) {
                let signal = self.classify_behavior_change(
                    &pending,
                    current_regime_id,
                    current_app,
                );

                let key = (pending.profile, pending.trigger);
                let score = self.scores.entry(key).or_default();
                score.total_shown += 1;
                match signal {
                    FeedbackSignal::ImplicitPositive => {
                        score.positive_signals += 1.0;
                    }
                    FeedbackSignal::ImplicitNeutral => {
                        score.neutral_count += 1;
                    }
                    FeedbackSignal::ImplicitNegative => {
                        score.negative_signals += 1.0;
                    }
                    _ => {}
                }
            }
        }
    }

    /// Determine whether to show a coaching message based on effectiveness.
    ///
    /// Low-effectiveness (<0.2) coaching types are shown at reduced frequency
    /// (1 in 3 times) to avoid nagging.
    pub async fn should_show(
        &self,
        profile: &CoachingProfile,
        trigger: &TriggerType,
    ) -> bool {
        let key = (format!("{:?}", profile), trigger_type_name(trigger));
        match self.scores.get(&key) {
            None => true, // no data — always show
            Some(score) => {
                let ratio = score.ratio();
                if ratio < 0.2 && score.total_shown >= 5 {
                    // Low effectiveness with enough data — reduce to 1-in-3
                    score.total_shown % 3 == 0
                } else {
                    true
                }
            }
        }
    }

    /// Classify behavior change for implicit feedback.
    fn classify_behavior_change(
        &self,
        pending: &PendingEvaluation,
        current_regime_id: Option<&str>,
        current_app: &str,
    ) -> FeedbackSignal {
        // If coaching was about focus and user returned to the target regime → positive
        if pending.regime_at_shown.as_deref() != current_regime_id
            && pending.app_at_shown != current_app
        {
            // Context changed — could be positive (acted on suggestion)
            // or negative (continued distraction)
            // Heuristic: if we suggested focus and they changed context, it's likely positive
            FeedbackSignal::ImplicitPositive
        } else if pending.regime_at_shown.as_deref() == current_regime_id {
            // Regime unchanged — neutral (they may have noted it)
            FeedbackSignal::ImplicitNeutral
        } else {
            FeedbackSignal::ImplicitNegative
        }
    }
}
```

### 4.7 MagicOverlay (`src-tauri` + React WebView)

A transparent always-on-top Tauri WebView window, separate from the main
dashboard. It renders coaching messages, goal progress, and optional
attention-heatmap ghosts.

#### 4.7.1 Window Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Main Tauri Window (Dashboard)                               │
│  ├── Regular WebView                                         │
│  └── Not always-on-top                                       │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  MagicOverlay Window (NEW)                                   │
│  ├── Transparent, always-on-top                              │
│  ├── Decorations: false, resizable: false                    │
│  ├── Click-through except for interactive elements           │
│  ├── Full-screen dimensions, transparent background          │
│  └── React components rendered in overlay context            │
└─────────────────────────────────────────────────────────────┘
```

#### 4.7.2 Adaptive Display Modes

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayMode {
    /// Focus area border + coaching popup only.
    #[default]
    Minimal,
    /// + bottom progress bar (regime goals) + attention heatmap ghost.
    Rich,
    /// deep work regime → Minimal, regime transition → momentarily Rich.
    Adaptive,
}
```

| Mode | Visual Elements | When |
|------|----------------|------|
| **Minimal** (default) | Translucent border around focus area + coaching popup bubble | Normal operation |
| **Rich** (hotkey toggle) | + bottom progress bar showing regime goals + attention heatmap ghost layer | User presses Cmd+Shift+O / Ctrl+Shift+O |
| **Adaptive** | Deep work regime auto-switches to Minimal; regime transition momentarily shows Rich for 10s then fades to Minimal | `coaching.overlay_mode: Adaptive` |

#### 4.7.3 Coaching Popup Component

```
┌──────────────────────────────────────────────┐
│  ┌──────────────────────────────────────────┐ │
│  │  💡 Nice focus session! 2h 15m in        │ │
│  │     Deep Coding. A short break might     │ │
│  │     help recharge.                       │ │
│  │                                          │ │
│  │         [OK]  [Later]      👍 👎         │ │
│  └──────────────────────────────────────────┘ │
│       ↑ coaching popup (corner positioned)     │
│                                                │
│  ═══════════════════════════════════════════   │
│  Deep Coding: ████████████░░ 73%  (87/120m)   │
│       ↑ bottom progress bar (Rich mode only)   │
└───────────────────────────────────────────────┘
```

- Popup appears in the top-right corner (configurable)
- Auto-dismiss after 15 seconds if no interaction
- `[OK]` dismisses immediately
- `[Later]` dismisses and snoozes that profile for 15 minutes
- Thumbs icons are subtle (low opacity), revealed fully on hover
- When LLM personalization arrives, the template text smoothly transitions to the personalized version

#### 4.7.4 Focus Area Highlight

Uses `FocusedElementInfo` position/size from `AccessibilityExtractor` (existing):

```rust
/// Focus area highlight specification for the overlay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusHighlight {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Translucent border color (CSS format).
    pub border_color: String,
    /// Border width in pixels.
    pub border_width: u32,
    /// Border opacity [0.0, 1.0].
    pub opacity: f32,
}
```

#### 4.7.5 Tauri IPC Commands

New commands registered in `src-tauri/src/commands.rs`:

```rust
#[tauri::command]
async fn show_coaching_message(
    state: tauri::State<'_, AppState>,
    message: CoachingMessage,
) -> Result<(), String> { /* ... */ }

#[tauri::command]
async fn upgrade_coaching_message(
    state: tauri::State<'_, AppState>,
    message_id: String,
    personalized_text: String,
) -> Result<(), String> { /* ... */ }

#[tauri::command]
async fn dismiss_coaching_message(
    state: tauri::State<'_, AppState>,
    message_id: String,
    action: DismissAction, // Ok, Later, Timeout
) -> Result<(), String> { /* ... */ }

#[tauri::command]
async fn submit_coaching_feedback(
    state: tauri::State<'_, AppState>,
    message_id: String,
    positive: bool,
) -> Result<(), String> { /* ... */ }

#[tauri::command]
async fn set_overlay_mode(
    state: tauri::State<'_, AppState>,
    mode: OverlayMode,
) -> Result<(), String> { /* ... */ }

#[tauri::command]
async fn get_goal_progress(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<GoalProgressView>, String> { /* ... */ }
```

### 4.8 LLM Integration

Template-first for instant response. Background LLM call for optional
personalization.

```rust
/// Build a personalization prompt for the LLM.
fn build_personalization_prompt(
    template_text: &str,
    regime_label: &str,
    regime_history_summary: &str,
    goal_progress: Option<&GoalProgress>,
    tone: CoachingTone,
) -> String {
    format!(
        "Rewrite this productivity coaching message to be more personalized \
         and contextual. Keep the same intent and information, but make it \
         feel natural.\n\n\
         Original: {template_text}\n\
         Current regime: {regime_label}\n\
         Recent history: {regime_history_summary}\n\
         {goal_section}\
         Tone: {tone:?}\n\
         Respond with ONLY the rewritten message, no preamble.",
        goal_section = match goal_progress {
            Some(gp) => format!(
                "Goal progress: {}min / {}min ({}%)\n",
                gp.current_minutes, gp.target_minutes, gp.percentage
            ),
            None => String::new(),
        },
    )
}
```

**LLM integration flow:**

1. `CoachingEngine.evaluate()` returns `CoachingMessage` with `template_text`.
2. Scheduler immediately sends `template_text` to MagicOverlay.
3. Scheduler spawns a background task:
   ```rust
   tokio::spawn(async move {
       let prompt = build_personalization_prompt(
           &message.template_text,
           regime_label,
           &history_summary,
           goal_progress.as_ref(),
           config.tone,
       );
       match analysis_provider.analyze(&prompt, COACHING_SYSTEM_PROMPT).await {
           Ok(suggestions) if !suggestions.is_empty() => {
               let personalized = &suggestions[0].content;
               // Upgrade overlay if still visible
               overlay_handle.upgrade_message(&message.message_id, personalized).await;
           }
           _ => { /* template text remains — fully functional without LLM */ }
       }
   });
   ```
4. If LLM succeeds within the 15-second display window, the overlay
   smoothly transitions to the personalized text.
5. **Offline fallback**: templates only. System is fully functional without LLM.

## 5. CoachingConfig (`oneshim-core`)

New config section added to `AppConfig`:

```rust
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Coaching engine configuration.
///
/// Added to `AppConfig.coaching` field with `#[serde(default)]`.
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

    /// Per-regime daily time goals (regime_label → target minutes).
    #[serde(default)]
    pub regime_goals: HashMap<String, u32>,

    /// Overlay display mode.
    #[serde(default)]
    pub overlay_mode: OverlayMode,

    /// Overlay toggle hotkey.
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
    for name in ["FocusGuard", "TimeAware", "DeepWorkCoach", "ContextRestore", "GoalTracker"] {
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
            min_interval_secs: 300, // 5 minutes
        }
    }
}

fn default_true() -> bool { true }
fn default_min_interval_secs() -> u64 { 300 }

/// Historical comparison window.
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

/// Message tone style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoachingTone {
    /// Short, actionable statements.
    Direct,
    /// Softer, encouraging language.
    #[default]
    Gentle,
    /// Statistics-focused with numbers and comparisons.
    DataDriven,
}

/// Time range for quiet hours.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: String, // "HH:MM" format
    pub end: String,   // "HH:MM" format
}

/// Overlay display mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayMode {
    #[default]
    Minimal,
    Rich,
    Adaptive,
}
```

**Integration with AppConfig:**

```rust
// In crates/oneshim-core/src/config/mod.rs
pub struct AppConfig {
    // ... existing fields ...
    #[serde(default)]
    pub coaching: CoachingConfig,
}
```

**Consent requirement:**

The coaching engine checks `ConsentManager` for the existing
`activity_pattern_learning` permission (Tier 4). No new consent tier is
needed because coaching is an extension of the same data that pattern
learning already requires.

```rust
// In CoachingEngine.evaluate()
if !consent_manager.has_permission("activity_pattern_learning") {
    return None;
}
```

## 6. Storage — V17 Migration

Three new tables added in `migrate_v17()`:

```sql
-- V17: Coaching engine tables

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
    dismiss_action        TEXT,           -- 'ok', 'later', 'timeout'
    feedback_type         TEXT,           -- 'explicit_positive', 'explicit_negative',
                                         -- 'implicit_positive', 'implicit_neutral',
                                         -- 'implicit_negative'
    feedback_score        REAL,           -- weighted score (explicit=3.0, implicit=1.0)
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
```

**Migration function** (`crates/oneshim-storage/src/migration.rs`):

```rust
// Bump CURRENT_VERSION from 16 to 17

fn migrate_v17(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V17: coaching engine tables");

    conn.execute_batch(
        "
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

        CREATE TABLE IF NOT EXISTS regime_goals (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,
            regime_label       TEXT NOT NULL UNIQUE,
            daily_target_minutes INTEGER NOT NULL,
            created_at         TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at         TEXT NOT NULL DEFAULT (datetime('now'))
        );

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

## 7. Scheduler Integration

A new `coaching_loop` is added to the existing 9-loop scheduler in
`src-tauri/src/scheduler/loops.rs`, making it a 10-loop scheduler.

```rust
/// Coaching evaluation loop — runs every 30 seconds.
///
/// Checks regime state against coaching triggers and delivers messages
/// through the MagicOverlay.
pub async fn coaching_loop(
    coaching_engine: &CoachingEngine,
    regime_classifier: &RegimeClassifier,
    drift_detector: &DriftDetector,
    overlay_handle: &MagicOverlayHandle,
    notifier: &Arc<dyn DesktopNotifier>,
    analysis_provider: &Arc<dyn AnalysisProvider>,
    config: &CoachingConfig,
) {
    // 1. Get current regime classification
    let current_regime = regime_classifier.classify(&current_features);
    let regime_id = current_regime.map(|r| r.regime_id.as_str());
    let regime_label = current_regime
        .map(|r| r.name.as_deref().unwrap_or(&r.auto_label))
        .unwrap_or("Unknown");

    // 2. Calculate regime dwell time
    let regime_duration = coaching_engine.current_regime_duration().await;

    // 3. Get historical average for this regime
    let avg_duration = coaching_engine.avg_regime_duration(regime_label).await;

    // 4. Check drift
    let drift_detected = drift_detector.observe(current_score);

    // 5. Update goal tracker with elapsed time
    coaching_engine.goal_tracker.record_minutes(regime_label, elapsed_minutes);

    // 6. Evaluate coaching triggers
    if let Some(message) = coaching_engine.evaluate(
        regime_id,
        regime_label,
        regime_duration,
        avg_duration,
        drift_detected,
        &current_app,
    ).await {
        // 7. Show template message immediately
        overlay_handle.show_coaching(&message).await;

        // 8. Register for feedback tracking
        coaching_engine.feedback_tracker.register_pending(
            &message.message_id,
            &format!("{:?}", message.profile),
            &trigger_type_name(&message.trigger),
            regime_id,
            &current_app,
        );

        // 9. Spawn background LLM personalization
        let msg_clone = message.clone();
        let provider = analysis_provider.clone();
        let overlay = overlay_handle.clone();
        tokio::spawn(async move {
            if let Ok(personalized) = personalize_message(&provider, &msg_clone).await {
                overlay.upgrade_message(&msg_clone.message_id, &personalized).await;
            }
        });

        // 10. Also send desktop notification as fallback
        let _ = notifier.show_notification(
            "ONESHIM Coach",
            &message.template_text,
        ).await;
    }

    // 11. Evaluate implicit feedback for messages past the 5-min window
    coaching_engine.feedback_tracker.evaluate_implicit(
        regime_id,
        &current_app,
        Utc::now(),
    );
}
```

**Scheduler config constant:**

```rust
// In src-tauri/src/scheduler/config.rs
pub const COACHING_INTERVAL_MS: u64 = 30_000; // 30 seconds
```

## 8. Coaching Models (`oneshim-core`)

New model file: `crates/oneshim-core/src/models/coaching.rs`

```rust
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A coaching message produced by CoachingEngine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachingMessage {
    /// Unique identifier for this message instance.
    pub message_id: String,
    /// Which coaching profile produced this message.
    pub profile: CoachingProfile,
    /// What trigger caused this message.
    pub trigger: TriggerType,
    /// Instant template-based message text (0ms).
    pub template_text: String,
    /// LLM-personalized text (filled async, 1-3s after template).
    pub personalized_text: Option<String>,
    /// Variable values used for template substitution.
    pub variables: HashMap<String, String>,
    /// When this message was created.
    pub created_at: DateTime<Utc>,
}

impl CoachingMessage {
    /// Get the best available message text (personalized if available,
    /// otherwise template).
    pub fn display_text(&self) -> &str {
        self.personalized_text
            .as_deref()
            .unwrap_or(&self.template_text)
    }
}

/// Coaching profiles — each addresses a distinct productivity concern.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CoachingProfile {
    /// Context switch frequency alerts.
    FocusGuard,
    /// Category time limit warnings.
    TimeAware,
    /// Deep work session management.
    DeepWorkCoach,
    /// Post-break context restoration.
    ContextRestore,
    /// Per-regime daily time goal tracking.
    GoalTracker,
}

/// Trigger types that initiate coaching.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TriggerType {
    RegimeTransition {
        from_regime: Option<String>,
        to_regime: Option<String>,
    },
    RegimeOverstay {
        regime_label: String,
        duration_secs: u64,
        avg_duration_secs: u64,
    },
    RegimeDrift {
        regime_label: String,
    },
    GoalThreshold {
        regime_label: String,
        target_minutes: u32,
        current_minutes: u32,
        threshold_percent: u8,
    },
}

/// Dismiss action from the coaching popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DismissAction {
    /// User clicked OK — acknowledged.
    Ok,
    /// User clicked Later — snooze this profile for 15 minutes.
    Later,
    /// Auto-dismissed after 15-second timeout.
    Timeout,
}

/// Feedback signal type for effectiveness tracking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackSignal {
    ExplicitPositive,
    ExplicitNegative,
    ImplicitPositive,
    ImplicitNeutral,
    ImplicitNegative,
}

/// Goal progress snapshot for a single regime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalProgress {
    pub regime_label: String,
    pub current_minutes: u32,
    pub target_minutes: u32,
    pub percentage: u8,
}

/// View model for the overlay progress bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalProgressView {
    pub regime_label: String,
    pub current_minutes: u32,
    pub target_minutes: u32,
    pub percentage: u8,
    pub display_color: String,
}
```

Register in `crates/oneshim-core/src/models/mod.rs`:

```rust
pub mod coaching;
```

## 9. Integration with Existing Codebase

### 9.1 Existing Infrastructure Reused

| Component | Location | How it's used |
|-----------|----------|---------------|
| `RegimeManager` | `oneshim-analysis/regime_manager.rs` | Source of regime lifecycle events; `active_regimes()` and `mark_seen()` provide regime state |
| `RegimeClassifier` | `oneshim-analysis/regime_classifier.rs` | `classify()` determines current regime each evaluation cycle |
| `DriftDetector` | `oneshim-analysis/auto_tuner.rs` | `observe()` returns `true` when behavioral drift detected — feeds `RegimeDrift` trigger |
| `EmaStatsTracker` | `oneshim-analysis/auto_tuner.rs` | `category_stats()` provides baselines for `RegimeOverstay` average duration calculations |
| `ContextAnalyzer` | `oneshim-analysis/analyzer.rs` | `AnalysisProvider` trait reused for LLM personalization (same `analyze()` method) |
| `NotificationManager` | `src-tauri/notification_manager.rs` | Fallback delivery channel; cooldown pattern borrowed for coaching cooldowns |
| `PlatformOverlayDriver` | `src-tauri/platform_overlay.rs` | Existing overlay infrastructure; MagicOverlay builds on this pattern but uses Tauri WebView instead of external process |
| `OverlayDriver` | `oneshim-core/ports/overlay_driver.rs` | Port trait pattern; coaching does not extend this (uses Tauri IPC directly) |
| `HighlightRequest` / `HighlightTarget` | `oneshim-core/models/gui.rs` | `FocusHighlight` follows same bounding-box pattern; `ElementBounds` reused |
| `Suggestion` model | `oneshim-core/models/suggestion.rs` | `CoachingMessage` follows the same ID + content + priority + source pattern |
| `ConsentManager` | `oneshim-core/consent.rs` | Gates coaching behind existing `activity_pattern_learning` permission |
| `StorageService` port | `oneshim-core/ports/storage.rs` | Coaching event persistence uses the same port |

### 9.2 New Files to Create

| File | Crate | Content |
|------|-------|---------|
| `crates/oneshim-core/src/models/coaching.rs` | `oneshim-core` | `CoachingMessage`, `CoachingProfile`, `TriggerType`, `DismissAction`, `FeedbackSignal`, `GoalProgress`, `GoalProgressView` |
| `crates/oneshim-core/src/config/sections.rs` (edit) | `oneshim-core` | Add `CoachingConfig`, `ProfileConfig`, `DataLookback`, `CoachingTone`, `TimeRange` |
| `crates/oneshim-core/src/config/enums.rs` (edit) | `oneshim-core` | Add `OverlayMode`, `CoachingTone`, `DataLookback` enums |
| `crates/oneshim-core/src/config/mod.rs` (edit) | `oneshim-core` | Add `coaching: CoachingConfig` field to `AppConfig` |
| `crates/oneshim-analysis/src/coaching_engine.rs` | `oneshim-analysis` | `CoachingEngine` struct + `evaluate()` + trigger detection |
| `crates/oneshim-analysis/src/coaching_template.rs` | `oneshim-analysis` | `CoachingTemplateRegistry` + 50+ default templates |
| `crates/oneshim-analysis/src/regime_goal_tracker.rs` | `oneshim-analysis` | `RegimeGoalTracker` + `GoalProgress` |
| `crates/oneshim-analysis/src/feedback_tracker.rs` | `oneshim-analysis` | `FeedbackTracker` + `EffectivenessScore` |
| `crates/oneshim-storage/src/migration.rs` (edit) | `oneshim-storage` | Add `migrate_v17()`, bump `CURRENT_VERSION` to 17 |
| `src-tauri/src/scheduler/loops.rs` (edit) | `src-tauri` | Add `coaching_loop` function |
| `src-tauri/src/scheduler/config.rs` (edit) | `src-tauri` | Add `COACHING_INTERVAL_MS` constant |
| `src-tauri/src/magic_overlay.rs` (new) | `src-tauri` | `MagicOverlayHandle` — Tauri WebView window management |
| `src-tauri/src/commands.rs` (edit) | `src-tauri` | Add coaching IPC commands |

### 9.3 Dependency Graph Updates

```
oneshim-core  (new models + config)
    ↑
oneshim-analysis  (CoachingEngine, templates, goals, feedback)
    ↑
oneshim-storage   (V17 migration)
    ↑
src-tauri         (MagicOverlay, scheduler loop, IPC commands)
```

No new cross-adapter dependencies are introduced. The dependency graph
remains compliant with Hexagonal Architecture rules (all adapter crates
depend on `oneshim-core`, never on each other directly).

### 9.4 Relationship to Existing Notification System

The existing `NotificationManager` (`src-tauri/src/notification_manager.rs`)
handles three concern types: idle, long session, high usage. The coaching
engine is a **separate system** that complements but does not replace
`NotificationManager`:

| Aspect | NotificationManager | CoachingEngine |
|--------|--------------------|-----------------|
| Trigger source | System metrics (idle time, CPU/memory) | Regime events + behavioral signals |
| Delivery | Desktop notifications only | MagicOverlay (primary) + desktop notification (fallback) |
| Feedback | None | Implicit + explicit feedback loop |
| Frequency control | Fixed cooldowns | Adaptive (effectiveness-based) |
| Profiles | None (flat checks) | 5 independently configurable profiles |

The two systems share the `DesktopNotifier` port for fallback delivery.

## 10. Testing Strategy

### 10.1 Unit Tests (in-module `#[cfg(test)]`)

| Module | Tests | Focus |
|--------|-------|-------|
| `coaching_engine.rs` | 15+ | Trigger detection, guard checks (quiet hours, cooldown, effectiveness), profile matching |
| `coaching_template.rs` | 10+ | Template selection, variable substitution, tone matching, fallback behavior |
| `regime_goal_tracker.rs` | 10+ | Goal recording, threshold detection, date rollover, edge cases (0 target, 100%+) |
| `feedback_tracker.rs` | 12+ | Implicit classification, explicit recording, effectiveness ratio, should_show gating, 5-min window |
| `coaching` models | 5+ | Serde roundtrip, `display_text()` fallback, enum exhaustiveness |
| `CoachingConfig` | 5+ | Default values, serde roundtrip, profile config merging |

### 10.2 Integration Tests (`crates/oneshim-app/tests/`)

| Test | Scope |
|------|-------|
| `coaching_full_cycle` | Engine evaluate → template → overlay show → LLM upgrade → feedback |
| `coaching_quiet_hours` | Verify suppression during quiet hours |
| `coaching_effectiveness_decay` | Low-effectiveness types reduce frequency after 5+ samples |
| `coaching_goal_rollover` | Date change resets goal counters |
| `coaching_storage_roundtrip` | V17 tables: insert → query coaching events and effectiveness |

### 10.3 Manual Test Scenarios

1. **Enable coaching** → Observe first coaching message within 5 minutes of regime detection
2. **Deep work session** → After 90+ minutes, see DeepWorkCoach break suggestion
3. **Context switching** → Switch apps 5+ times rapidly → FocusGuard alert
4. **Goal tracking** → Set 120min coding goal → See progress at 25%, 50%, 75%, 100%
5. **LLM upgrade** → Observe template text → Watch it smoothly transition to personalized text
6. **Thumbs feedback** → Click thumbs-down → Verify reduced frequency for that coaching type
7. **Quiet hours** → Set quiet hours → Verify no coaching during that window
8. **Overlay modes** → Press Cmd+Shift+O → Toggle between Minimal and Rich mode

## 11. Privacy and Consent

- Coaching data stays **local-only** (SQLite). No coaching events are uploaded
  to the server.
- Gated behind existing `activity_pattern_learning` consent (Tier 4).
- The LLM personalization call sends regime labels and aggregated statistics
  only — never raw app names, window titles, or PII. The same PII filtering
  used by `ContextAssembler` is applied.
- `coaching.enabled` defaults to `false`. Users must explicitly opt in.
- Goal data (`regime_goals` table) is user-configured and contains only regime
  labels and minute targets — no behavioral data.

## 12. Performance Considerations

| Concern | Mitigation |
|---------|------------|
| Coaching loop frequency | 30-second interval — lightweight HashMap lookups + cooldown checks |
| Template substitution | Pure string replacement, O(n) on template length, sub-microsecond |
| LLM personalization | Background `tokio::spawn`, does not block coaching display |
| Overlay rendering | Tauri WebView is already loaded; message push is a single IPC call |
| Feedback evaluation | Batch process expired pending items every 30s; HashMap is bounded (max ~50 pending) |
| SQLite writes | Coaching events written asynchronously; effectiveness table uses UPSERT (single row per profile+trigger combo) |
| Memory footprint | `CoachingTemplateRegistry` holds ~60 templates (~15KB); `FeedbackTracker` bounded by pending queue |

## 13. Future Extensions

- **Weekly digest**: aggregate coaching effectiveness into a weekly summary
  (rendered in web dashboard)
- **Peer comparison**: optional anonymized benchmarks ("You deep-work 2.5h/day,
  top 20% of users")
- **Calendar integration**: auto-set quiet hours from calendar events
  (via `oneshim-network` integration domain)
- **Custom profiles**: user-defined coaching profiles with custom trigger rules
- **Wearable integration**: heart rate variability as an additional signal
  for break suggestions
