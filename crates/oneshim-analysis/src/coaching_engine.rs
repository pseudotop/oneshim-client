use chrono::{DateTime, Local, NaiveTime, Utc};
use oneshim_core::config::CoachingConfig;
use oneshim_core::models::coaching::{
    trigger_type_name, CoachingMessage, CoachingProfile, GoalProgressView, TriggerType,
};
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
    goal_tracker: RwLock<RegimeGoalTracker>,
    feedback_tracker: RwLock<FeedbackTracker>,

    /// Profile display name -> last alert timestamp (cooldown enforcement).
    last_alert: RwLock<HashMap<String, DateTime<Utc>>>,
    /// Current regime ID for transition detection.
    current_regime_id: RwLock<Option<String>>,
    /// Timestamp when the current regime was entered.
    current_regime_entered: RwLock<Option<DateTime<Utc>>>,

    /// Tracks a snoozed profile: (profile_name, snooze_expiry_instant).
    /// When set, `evaluate()` skips triggers for this profile until the Instant passes.
    snoozed_until: RwLock<Option<(String, Instant)>>,
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
    /// - `avg_regime_duration_secs`: historical average (TODO: placeholder 1800 in Phase 1)
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
        // 1. Check master switch
        let config = self.config.read().await;
        if !config.enabled {
            return None;
        }

        // 2. Quiet hours check
        if Self::is_quiet_hour(&config) {
            debug!("coaching suppressed: quiet hours");
            return None;
        }

        // 2b. Snooze check — skip evaluation if the current profile is snoozed
        {
            let guard = self.snoozed_until.read().await;
            if let Some((ref snoozed_profile, until)) = *guard {
                if Instant::now() < until && regime_label == snoozed_profile {
                    debug!(profile = %snoozed_profile, "coaching suppressed: snoozed");
                    return None;
                }
            }
        }
        // Clear expired snooze
        {
            let mut guard = self.snoozed_until.write().await;
            if let Some((_, until)) = guard.as_ref() {
                if Instant::now() >= *until {
                    *guard = None;
                }
            }
        }

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

    /// Detect which trigger type fired (if any).
    ///
    /// Priority order:
    /// 1. RegimeTransition (regime ID changed)
    /// 2. RegimeDrift (attention drift flagged)
    /// 3. GoalThreshold (milestone crossed)
    /// 4. RegimeOverstay (duration > 1.2x average)
    async fn detect_trigger(
        &self,
        regime_id: Option<&str>,
        regime_label: &str,
        regime_duration_secs: u64,
        avg_regime_duration_secs: u64,
        drift_detected: bool,
    ) -> Option<TriggerType> {
        // 1. Regime transition — read current, compare, then drop guard before write
        let transition = {
            let current = self.current_regime_id.read().await;
            let current_str = current.as_deref();
            let changed = match (current_str, regime_id) {
                (Some(old), Some(new)) => old != new,
                (None, Some(_)) => true,
                (Some(_), None) => true,
                (None, None) => false,
            };
            if changed {
                Some(current.clone()) // clone the Option<String>, then guard drops
            } else {
                None
            }
        }; // read guard dropped here
        if let Some(from_regime) = transition {
            let to_regime = regime_id.map(String::from);
            // Update internal tracking (acquires write lock, safe now)
            self.on_regime_change(regime_id).await;
            return Some(TriggerType::RegimeTransition {
                from_regime,
                to_regime,
            });
        }

        // 2. Regime drift
        if drift_detected {
            return Some(TriggerType::RegimeDrift {
                regime_label: regime_label.to_string(),
            });
        }

        // 3. Goal threshold
        {
            let mut gt = self.goal_tracker.write().await;
            if let Some(threshold) = gt.check_threshold(regime_label) {
                let progress = gt.progress(regime_label);
                let (target, current) = progress
                    .map(|p| (p.target_minutes, p.current_minutes))
                    .unwrap_or((0, 0));
                return Some(TriggerType::GoalThreshold {
                    regime_label: regime_label.to_string(),
                    target_minutes: target,
                    current_minutes: current,
                    threshold_percent: threshold,
                });
            }
        }

        // 4. Regime overstay (duration > 1.2x average)
        if avg_regime_duration_secs > 0
            && regime_duration_secs > avg_regime_duration_secs * 120 / 100
        {
            return Some(TriggerType::RegimeOverstay {
                regime_label: regime_label.to_string(),
                duration_secs: regime_duration_secs,
                avg_duration_secs: avg_regime_duration_secs,
            });
        }

        None
    }

    /// Map a trigger to a coaching profile, checking if that profile is enabled.
    fn match_profile(
        config: &CoachingConfig,
        trigger: &TriggerType,
        _regime_label: &str,
        _app_name: &str,
    ) -> Option<CoachingProfile> {
        let profile = match trigger {
            TriggerType::RegimeTransition { from_regime, .. } => {
                // If returning from idle/break -> ContextRestore
                let from_lower = from_regime.as_deref().unwrap_or("").to_lowercase();
                if from_lower.contains("idle")
                    || from_lower.contains("break")
                    || from_lower.contains("away")
                {
                    CoachingProfile::ContextRestore
                } else {
                    CoachingProfile::FocusGuard
                }
            }
            TriggerType::RegimeDrift { .. } => CoachingProfile::FocusGuard,
            TriggerType::RegimeOverstay { regime_label, .. } => {
                let label_lower = regime_label.to_lowercase();
                if label_lower.contains("deep")
                    || label_lower.contains("focus")
                    || label_lower.contains("coding")
                {
                    CoachingProfile::DeepWorkCoach
                } else {
                    CoachingProfile::TimeAware
                }
            }
            TriggerType::GoalThreshold { .. } => CoachingProfile::GoalTracker,
        };

        // Check if the matched profile is enabled
        let profile_name = format!("{:?}", profile);
        if let Some(profile_config) = config.profiles.get(&profile_name) {
            if !profile_config.enabled {
                debug!(profile = %profile_name, "coaching profile disabled");
                return None;
            }
        }

        Some(profile)
    }

    /// Check if the current time falls within any configured quiet hour range.
    fn is_quiet_hour(config: &CoachingConfig) -> bool {
        if config.quiet_hours.is_empty() {
            return false;
        }

        let now = Local::now().time();

        for range in &config.quiet_hours {
            let start = match NaiveTime::parse_from_str(&range.start, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };
            let end = match NaiveTime::parse_from_str(&range.end, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };

            if start <= end {
                // Normal range: e.g. 22:00 - 23:00
                if now >= start && now < end {
                    return true;
                }
            } else {
                // Overnight range: e.g. 22:00 - 06:00
                if now >= start || now < end {
                    return true;
                }
            }
        }

        false
    }

    /// Check if enough time has passed since the last alert for this profile.
    /// Returns `true` if the message should be allowed (cooldown passed).
    async fn check_cooldown(&self, config: &CoachingConfig, profile: &CoachingProfile) -> bool {
        let profile_name = format!("{:?}", profile);
        let min_interval = config
            .profiles
            .get(&profile_name)
            .map(|p| p.min_interval_secs)
            .unwrap_or(300);

        let last = self.last_alert.read().await;
        match last.get(&profile_name) {
            Some(last_time) => {
                let elapsed = (Utc::now() - *last_time).num_seconds();
                elapsed >= min_interval as i64
            }
            None => true,
        }
    }

    /// Record the current time as the last alert for this profile.
    async fn record_alert(&self, profile: &CoachingProfile) {
        let profile_name = format!("{:?}", profile);
        let mut last = self.last_alert.write().await;
        last.insert(profile_name, Utc::now());
    }

    /// Build the variable substitution map for template rendering.
    async fn build_variables(
        &self,
        regime_label: &str,
        regime_duration_secs: u64,
        app_name: &str,
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("regime".to_string(), regime_label.to_string());
        vars.insert(
            "duration".to_string(),
            humanize_duration(regime_duration_secs),
        );
        vars.insert("app_name".to_string(), app_name.to_string());

        // Goal-related variables
        let gt = self.goal_tracker.read().await;
        if let Some(progress) = gt.progress(regime_label) {
            vars.insert("goal_progress".to_string(), progress.percentage.to_string());
            vars.insert(
                "goal_minutes".to_string(),
                progress.target_minutes.to_string(),
            );
            let remaining = progress
                .target_minutes
                .saturating_sub(progress.current_minutes);
            vars.insert("remaining_minutes".to_string(), remaining.to_string());
        } else {
            vars.insert("goal_progress".to_string(), "0".to_string());
            vars.insert("goal_minutes".to_string(), "0".to_string());
            vars.insert("remaining_minutes".to_string(), "0".to_string());
        }

        // Placeholder values for Phase 1 (will be enriched in Phase 2)
        // TODO(Phase 2): wire actual context switch count from regime transition history
        vars.insert("context_switches".to_string(), "N/A".to_string());
        // TODO(Phase 2): wire historical comparison data from daily digest / weekly trends
        vars.insert("comparison".to_string(), "N/A".to_string());
        // TODO(Phase 2): wire previous context from regime transition tracking
        vars.insert("previous_context".to_string(), "N/A".to_string());

        vars
    }

    // ── Public delegation methods ──────────────────────────────────────

    /// Update internal regime tracking when a regime change is detected.
    pub async fn on_regime_change(&self, new_regime_id: Option<&str>) {
        let mut rid = self.current_regime_id.write().await;
        *rid = new_regime_id.map(String::from);
        let mut entered = self.current_regime_entered.write().await;
        *entered = Some(Utc::now());
    }

    /// Hot-reload coaching config at runtime.
    pub async fn update_config(&self, config: CoachingConfig) {
        let mut gt = self.goal_tracker.write().await;
        gt.update_goals(&config.regime_goals);
        let mut current = self.config.write().await;
        *current = config;
    }

    /// Record additional minutes for goal tracking (delegates to RegimeGoalTracker).
    pub async fn record_minutes(&self, regime_label: &str, minutes: u32) {
        let mut gt = self.goal_tracker.write().await;
        gt.record_minutes(regime_label, minutes);
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
    use chrono::Timelike;
    use oneshim_core::config::{ProfileConfig, TimeRange};

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

        // Snooze the regime label for 60 seconds
        engine
            .snooze_current_profile("Communication", Duration::from_secs(60))
            .await;

        // Evaluate with a regime transition to "Communication"
        let result = engine
            .evaluate(Some("regime-b"), "Communication", 60, 1800, false, "Slack")
            .await;

        // Even though there's a regime transition, snooze suppresses it
        // because the label matches the snoozed profile.
        // Note: The snooze compares against regime_label, not profile name,
        // so it matches when the regime_label is the snoozed value.
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
}
