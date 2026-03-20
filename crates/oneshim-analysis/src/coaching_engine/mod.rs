mod guards;
mod triggers;

use chrono::{DateTime, Utc};
use oneshim_core::config::CoachingConfig;
use oneshim_core::models::coaching::{trigger_type_name, CoachingMessage, GoalProgressView};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::debug;

use crate::coaching_template::CoachingTemplateRegistry;
use crate::feedback_tracker::FeedbackTracker;
use crate::regime_goal_tracker::RegimeGoalTracker;

/// Central coaching orchestrator.
///
/// Evaluates triggers, matches profiles, applies guards (quiet hours,
/// cooldown, effectiveness gating), and produces `CoachingMessage` instances.
///
/// This is a **pure analysis component** — it does NOT take `Arc<dyn StorageService>`.
/// Persistence is handled by the caller (scheduler loop).
pub struct CoachingEngine {
    config: RwLock<CoachingConfig>,
    templates: CoachingTemplateRegistry,
    pub(super) goal_tracker: RwLock<RegimeGoalTracker>,
    pub(super) feedback_tracker: RwLock<FeedbackTracker>,

    /// Profile display name -> last alert timestamp (cooldown enforcement).
    pub(super) last_alert: RwLock<HashMap<String, DateTime<Utc>>>,
    /// Current regime ID for transition detection.
    pub(super) current_regime_id: RwLock<Option<String>>,
    /// Timestamp when the current regime was entered.
    pub(super) current_regime_entered: RwLock<Option<DateTime<Utc>>>,

    /// Tracks a snoozed profile: (profile_name, snooze_expiry_instant).
    /// When set, `evaluate()` skips triggers for this profile until the Instant passes.
    pub(super) snoozed_until: RwLock<Option<(String, Instant)>>,

    /// Per-regime-label EMA of dwell duration in seconds.
    /// Key: regime_label (not regime_id, since IDs are opaque).
    pub(super) regime_avg_duration: RwLock<HashMap<String, f64>>,

    /// Count of regime transitions today. Reset at midnight.
    pub(super) context_switch_count: RwLock<u32>,
    /// Date of last reset (for daily reset logic).
    pub(super) context_switch_date: RwLock<chrono::NaiveDate>,
}

impl CoachingEngine {
    /// Construct a new `CoachingEngine` with the given config.
    ///
    /// Initializes the goal tracker from `config.regime_goals`.
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
            snoozed_until: RwLock::new(None),
            regime_avg_duration: RwLock::new(HashMap::new()),
            context_switch_count: RwLock::new(0),
            context_switch_date: RwLock::new(chrono::Utc::now().date_naive()),
        }
    }

    /// Main entry point: evaluate whether a coaching message should be shown.
    ///
    /// Returns `Some(CoachingMessage)` when a trigger fires and all guards pass,
    /// or `None` when coaching is disabled, in quiet hours, on cooldown, or
    /// gated by low effectiveness.
    ///
    /// # Parameters
    /// - `regime_id`: opaque ID of the current regime (for transition detection)
    /// - `regime_label`: human-readable label (e.g., "Deep Work")
    /// - `regime_duration_secs`: seconds in the current regime
    /// - `avg_regime_duration_secs`: historical EMA average (from `avg_regime_duration_secs()`)
    /// - `drift_detected`: true if the drift detector flagged attention drift
    /// - `app_name`: currently focused application name
    pub async fn evaluate(
        &self,
        regime_id: Option<&str>,
        regime_label: &str,
        regime_duration_secs: u64,
        avg_regime_duration_secs: u64,
        drift_detected: bool,
        app_name: &str,
    ) -> Option<CoachingMessage> {
        // 1. Clone config and drop read guard immediately to prevent deadlock.
        //    evaluate() later acquires goal_tracker.write(); update_config() acquires
        //    config.write() then goal_tracker.write(). Holding config.read() here while
        //    waiting on goal_tracker.write() would deadlock if update_config() holds
        //    goal_tracker.write() and waits for config.write().
        let config = self.config.read().await.clone();
        if !config.enabled {
            return None;
        }

        // 2. Quiet hours check
        if Self::is_quiet_hour(&config) {
            debug!("coaching suppressed: quiet hours");
            return None;
        }

        // 2b. Clear expired snooze eagerly (before trigger detection)
        self.clear_expired_snooze().await;

        // 3. Detect trigger
        let trigger = self
            .detect_trigger(
                regime_id,
                regime_label,
                regime_duration_secs,
                avg_regime_duration_secs,
                drift_detected,
            )
            .await?;

        // 4. Match profile
        let profile = Self::match_profile(&config, &trigger, regime_label, app_name)?;

        // 4b. Snooze check
        if self.is_profile_snoozed(&profile).await {
            return None;
        }

        // 5. Cooldown check
        if !self.check_cooldown(&config, &profile).await {
            debug!(profile = ?profile, "coaching suppressed: cooldown");
            return None;
        }

        // 6. Effectiveness gate
        let profile_name = format!("{:?}", profile);
        let trigger_name = trigger_type_name(&trigger);
        {
            let mut ft = self.feedback_tracker.write().await;
            if !ft.should_show(&profile_name, &trigger_name) {
                debug!(profile = %profile_name, "coaching suppressed: low effectiveness");
                return None;
            }
        }

        // 7. Build variables
        let variables = self
            .build_variables(regime_label, regime_duration_secs, app_name)
            .await;

        // 8. Select template
        let template_text = self
            .templates
            .select(&profile, &trigger, &config.tone, &variables);

        // 9. Record alert timestamp
        self.record_alert(&profile).await;

        // 10. Produce message
        let message_id = uuid::Uuid::new_v4().to_string();
        Some(CoachingMessage {
            message_id,
            profile,
            trigger,
            template_text,
            personalized_text: None,
            variables,
            created_at: Utc::now(),
        })
    }

    // ── Public delegation methods ──────────────────────────────────────

    /// Hot-reload coaching config at runtime.
    ///
    /// Lock ordering: config -> goal_tracker (same as evaluate()) to prevent deadlock.
    pub async fn update_config(&self, config: CoachingConfig) {
        let mut current = self.config.write().await;
        let mut gt = self.goal_tracker.write().await;
        gt.update_goals(&config.regime_goals);
        *current = config;
    }

    /// Record additional minutes for goal tracking (delegates to RegimeGoalTracker).
    pub async fn record_minutes(&self, regime_label: &str, minutes: u32) {
        let mut gt = self.goal_tracker.write().await;
        gt.record_minutes(regime_label, minutes);
    }

    /// Get the EMA of dwell duration for a regime label, in seconds.
    /// Returns 1800 (30 min) as default when no history exists.
    pub async fn avg_regime_duration_secs(&self, regime_label: &str) -> u64 {
        let avgs = self.regime_avg_duration.read().await;
        avgs.get(regime_label).copied().unwrap_or(1800.0) as u64
    }

    /// Register a coaching message for pending feedback evaluation.
    pub async fn register_pending_feedback(
        &self,
        message_id: &str,
        profile: &str,
        trigger: &str,
        regime_id: Option<&str>,
        app_name: &str,
    ) {
        let mut ft = self.feedback_tracker.write().await;
        ft.register_pending(message_id, profile, trigger, regime_id, app_name);
    }

    /// Record explicit feedback (thumbs-up/down) for a coaching message.
    pub async fn record_explicit_feedback(&self, message_id: &str, positive: bool) {
        let mut ft = self.feedback_tracker.write().await;
        ft.record_explicit(message_id, positive);
    }

    /// Evaluate implicit feedback for messages past the 5-minute window.
    pub async fn evaluate_implicit_feedback(
        &self,
        current_regime_id: Option<&str>,
        current_app: &str,
        now: DateTime<Utc>,
    ) {
        let mut ft = self.feedback_tracker.write().await;
        ft.evaluate_implicit(current_regime_id, current_app, now);
    }

    // ── Phase 2 methods ──────────────────────────────────────────────

    /// Set a temporary cooldown override on the specified coaching profile.
    /// While snoozed, `evaluate()` skips that profile's triggers.
    /// The snooze expires after `duration` elapses.
    ///
    /// Called from the `dismiss_coaching_message` IPC command when the user
    /// clicks "Later". The profile name is read from the most recently
    /// shown coaching message.
    pub async fn snooze_current_profile(&self, profile_name: &str, duration: Duration) {
        let mut guard = self.snoozed_until.write().await;
        *guard = Some((profile_name.to_string(), Instant::now() + duration));
    }

    /// Return goal progress for all configured regimes.
    /// Delegates to `RegimeGoalTracker::all_progress()`, maps each `GoalProgress`
    /// to `GoalProgressView` with a deterministic display color assignment.
    ///
    /// Async because it reads from `RwLock<RegimeGoalTracker>`.
    pub async fn all_goal_progress(&self) -> Vec<GoalProgressView> {
        const COLORS: &[&str] = &[
            "#3b82f6", "#10b981", "#f59e0b", "#ef4444", "#8b5cf6", "#ec4899",
        ];
        let tracker = self.goal_tracker.read().await;
        tracker
            .all_progress()
            .into_iter()
            .enumerate()
            .map(|(i, gp)| GoalProgressView {
                regime_label: gp.regime_label,
                current_minutes: gp.current_minutes,
                target_minutes: gp.target_minutes,
                percentage: gp.percentage,
                display_color: COLORS[i % COLORS.len()].to_string(),
            })
            .collect()
    }

    /// Update the goal tracker's regime targets at runtime.
    /// Called from the IPC `update_regime_goals` command.
    pub async fn update_regime_goals(&self, goals: &HashMap<String, u32>) {
        let mut tracker = self.goal_tracker.write().await;
        tracker.update_goals(goals);
    }
}

#[async_trait::async_trait]
impl oneshim_core::ports::coaching::CoachingPort for CoachingEngine {
    fn all_goal_progress_blocking(&self) -> Vec<GoalProgressView> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.all_goal_progress())
        })
    }

    fn update_regime_goals_blocking(&self, goals: &HashMap<String, u32>) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.update_regime_goals(goals))
        })
    }

    async fn snooze_profile(&self, profile: &str, duration_secs: u64) {
        self.snooze_current_profile(profile, Duration::from_secs(duration_secs))
            .await;
    }

    async fn record_feedback(&self, message_id: &str, positive: bool) {
        self.record_explicit_feedback(message_id, positive).await;
    }

    async fn all_goal_progress(&self) -> Vec<GoalProgressView> {
        self.all_goal_progress().await
    }

    async fn update_regime_goals(&self, goals: &HashMap<String, u32>) {
        self.update_regime_goals(goals).await;
    }
}

/// Humanize a duration in seconds to a compact "Xh Ym" format.
fn humanize_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    if hours > 0 && minutes > 0 {
        format!("{}h {}m", hours, minutes)
    } else if hours > 0 {
        format!("{}h", hours)
    } else {
        format!("{}m", minutes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, Timelike};
    use oneshim_core::config::{ProfileConfig, TimeRange};
    use oneshim_core::models::coaching::{CoachingProfile, TriggerType};

    fn disabled_config() -> CoachingConfig {
        CoachingConfig {
            enabled: false,
            ..CoachingConfig::default()
        }
    }

    fn enabled_config() -> CoachingConfig {
        CoachingConfig {
            enabled: true,
            ..CoachingConfig::default()
        }
    }

    #[tokio::test]
    async fn evaluate_returns_none_when_disabled() {
        let engine = CoachingEngine::new(disabled_config());
        let result = engine
            .evaluate(Some("r1"), "Deep Work", 600, 1800, false, "VS Code")
            .await;
        assert!(result.is_none(), "disabled engine must return None");
    }

    #[tokio::test]
    async fn evaluate_returns_none_during_quiet_hours() {
        let now = Local::now();
        let start = format!("{:02}:{:02}", now.time().hour(), 0);
        let end_hour = (now.time().hour() + 1) % 24;
        let end = format!("{:02}:{:02}", end_hour, 0);

        let config = CoachingConfig {
            enabled: true,
            quiet_hours: vec![TimeRange { start, end }],
            ..CoachingConfig::default()
        };
        let engine = CoachingEngine::new(config);
        let result = engine
            .evaluate(Some("r1"), "Deep Work", 600, 1800, false, "VS Code")
            .await;
        assert!(result.is_none(), "quiet hours must suppress coaching");
    }

    #[tokio::test]
    async fn evaluate_fires_regime_transition() {
        let engine = CoachingEngine::new(enabled_config());

        // Set initial regime
        engine.on_regime_change(Some("regime-a")).await;

        // Evaluate with a different regime -> transition
        let result = engine
            .evaluate(Some("regime-b"), "Communication", 60, 1800, false, "Slack")
            .await;
        assert!(result.is_some(), "regime transition should fire");
        let msg = result.unwrap();
        match msg.trigger {
            TriggerType::RegimeTransition { .. } => {}
            other => panic!("expected RegimeTransition, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn evaluate_fires_overstay() {
        let engine = CoachingEngine::new(enabled_config());
        // Set regime so we don't get a transition trigger
        engine.on_regime_change(Some("regime-a")).await;

        // Duration > 1.2x average -> overstay
        let result = engine
            .evaluate(Some("regime-a"), "Email", 3600, 1800, false, "Outlook")
            .await;
        assert!(result.is_some(), "overstay should fire");
        let msg = result.unwrap();
        match msg.trigger {
            TriggerType::RegimeOverstay { .. } => {}
            other => panic!("expected RegimeOverstay, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn evaluate_respects_cooldown() {
        let config = CoachingConfig {
            enabled: true,
            profiles: {
                let mut p = HashMap::new();
                p.insert(
                    "FocusGuard".to_string(),
                    ProfileConfig {
                        enabled: true,
                        min_interval_secs: 600, // 10 minutes
                    },
                );
                p
            },
            ..CoachingConfig::default()
        };
        let engine = CoachingEngine::new(config);

        // First call: regime transition triggers FocusGuard
        engine.on_regime_change(Some("regime-a")).await;
        let first = engine
            .evaluate(Some("regime-b"), "Work", 60, 1800, false, "VS Code")
            .await;
        assert!(first.is_some(), "first call should fire");

        // Second call immediately: should be on cooldown
        engine.on_regime_change(Some("regime-b")).await;
        let second = engine
            .evaluate(Some("regime-c"), "Work", 60, 1800, false, "Chrome")
            .await;
        assert!(second.is_none(), "second call should be on cooldown");
    }

    #[tokio::test]
    async fn evaluate_fires_goal_threshold() {
        let mut goals = HashMap::new();
        goals.insert("Coding".to_string(), 100);
        let config = CoachingConfig {
            enabled: true,
            regime_goals: goals,
            ..CoachingConfig::default()
        };
        let engine = CoachingEngine::new(config);
        // Set regime so we don't get transition
        engine.on_regime_change(Some("regime-a")).await;

        // Record enough minutes to cross 25% threshold
        engine.record_minutes("Coding", 25).await;

        // Evaluate — should trigger GoalThreshold via check_threshold
        let result = engine
            .evaluate(Some("regime-a"), "Coding", 60, 1800, false, "VS Code")
            .await;
        assert!(result.is_some(), "goal threshold should fire");
        let msg = result.unwrap();
        match msg.trigger {
            TriggerType::GoalThreshold {
                threshold_percent, ..
            } => {
                assert_eq!(threshold_percent, 25);
            }
            other => panic!("expected GoalThreshold, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn profile_matching_context_restore() {
        let engine = CoachingEngine::new(enabled_config());

        // Simulate returning from "idle" regime
        engine.on_regime_change(Some("idle-regime")).await;
        {
            // Manually set the label to contain "idle" — the from_regime
            // in the transition carries the old regime ID, but match_profile
            // checks the from_regime string
            let mut rid = engine.current_regime_id.write().await;
            *rid = Some("idle".to_string());
        }

        let result = engine
            .evaluate(Some("work-regime"), "Work", 60, 1800, false, "VS Code")
            .await;
        assert!(result.is_some());
        let msg = result.unwrap();
        assert_eq!(
            msg.profile,
            CoachingProfile::ContextRestore,
            "transition from idle should map to ContextRestore"
        );
    }

    #[test]
    fn humanize_duration_formats_correctly() {
        assert_eq!(humanize_duration(3750), "1h 2m");
        assert_eq!(humanize_duration(7200), "2h");
        assert_eq!(humanize_duration(300), "5m");
        assert_eq!(humanize_duration(0), "0m");
        assert_eq!(humanize_duration(59), "0m");
        assert_eq!(humanize_duration(60), "1m");
        assert_eq!(humanize_duration(3600), "1h");
    }

    #[tokio::test]
    async fn build_variables_includes_goal_data() {
        let mut goals = HashMap::new();
        goals.insert("Deep Work".to_string(), 120);
        let config = CoachingConfig {
            enabled: true,
            regime_goals: goals,
            ..CoachingConfig::default()
        };
        let engine = CoachingEngine::new(config);

        // Record some minutes
        engine.record_minutes("Deep Work", 60).await;

        let vars = engine.build_variables("Deep Work", 3600, "VS Code").await;
        assert_eq!(vars.get("regime").unwrap(), "Deep Work");
        assert_eq!(vars.get("duration").unwrap(), "1h");
        assert_eq!(vars.get("app_name").unwrap(), "VS Code");
        assert_eq!(vars.get("goal_progress").unwrap(), "50");
        assert_eq!(vars.get("goal_minutes").unwrap(), "120");
        assert_eq!(vars.get("remaining_minutes").unwrap(), "60");
    }

    /// Smoke integration test: construct CoachingEngine, verify evaluate()
    /// returns None when disabled (per review fix instructions).
    #[tokio::test]
    async fn smoke_test_disabled_engine_returns_none() {
        let engine = CoachingEngine::new(CoachingConfig::default());
        // Default config has enabled=false
        let result = engine
            .evaluate(Some("r1"), "Label", 100, 200, true, "App")
            .await;
        assert!(
            result.is_none(),
            "evaluate() must return None when coaching is disabled"
        );
    }

    // ── Phase 2 method tests ─────────────────────────────────────

    #[tokio::test]
    async fn snooze_current_profile_suppresses_evaluation() {
        let engine = CoachingEngine::new(enabled_config());
        engine.on_regime_change(Some("regime-a")).await;

        // Snooze the "FocusGuard" profile for 60 seconds.
        // A regime transition from a non-idle regime maps to FocusGuard.
        engine
            .snooze_current_profile("FocusGuard", Duration::from_secs(60))
            .await;

        // Evaluate with a regime transition -> triggers FocusGuard profile
        let result = engine
            .evaluate(Some("regime-b"), "Communication", 60, 1800, false, "Slack")
            .await;

        // Snooze suppresses the matched FocusGuard profile
        assert!(
            result.is_none(),
            "snoozed profile should suppress evaluation"
        );
    }

    #[tokio::test]
    async fn all_goal_progress_returns_views_with_colors() {
        let mut goals = HashMap::new();
        goals.insert("Deep Work".to_string(), 120);
        goals.insert("Communication".to_string(), 60);
        let config = CoachingConfig {
            enabled: true,
            regime_goals: goals,
            ..CoachingConfig::default()
        };
        let engine = CoachingEngine::new(config);

        engine.record_minutes("Deep Work", 30).await;
        engine.record_minutes("Communication", 45).await;

        let views = engine.all_goal_progress().await;
        assert_eq!(views.len(), 2);
        // All views should have a non-empty display_color
        for view in &views {
            assert!(!view.display_color.is_empty());
            assert!(view.display_color.starts_with('#'));
        }
    }

    #[tokio::test]
    async fn update_regime_goals_changes_tracker() {
        let engine = CoachingEngine::new(enabled_config());

        let mut goals = HashMap::new();
        goals.insert("Coding".to_string(), 180);
        goals.insert("Email".to_string(), 30);
        engine.update_regime_goals(&goals).await;

        let views = engine.all_goal_progress().await;
        assert_eq!(views.len(), 2);
        let coding = views.iter().find(|v| v.regime_label == "Coding").unwrap();
        assert_eq!(coding.target_minutes, 180);
    }

    #[tokio::test]
    async fn avg_regime_duration_updates_on_transition() {
        let engine = CoachingEngine::new(enabled_config());
        engine.on_regime_change(Some("r-a")).await;
        // Simulate a short dwell in regime-a
        tokio::time::sleep(Duration::from_millis(50)).await;
        engine.on_regime_change(Some("r-b")).await;
        let avg = engine.avg_regime_duration_secs("r-a").await;
        // Should be > 0 (actual dwell) and < 1800 (default)
        assert!(
            avg < 1800,
            "avg should reflect actual short dwell, got {}",
            avg
        );
    }

    #[tokio::test]
    async fn context_switch_count_increments() {
        let engine = CoachingEngine::new(enabled_config());
        engine.on_regime_change(Some("r-a")).await;
        engine.on_regime_change(Some("r-b")).await;
        engine.on_regime_change(Some("r-c")).await;
        let vars = engine.build_variables("Test", 600, "VS Code").await;
        assert_eq!(vars.get("context_switches").unwrap(), "3");
    }
}
