use chrono::Utc;
use oneshim_core::config::CoachingConfig;
use oneshim_core::models::coaching::{CoachingProfile, TriggerType};
use std::collections::HashMap;
use tracing::debug;

use super::CoachingEngine;

impl CoachingEngine {
    /// Detect which trigger type fired (if any).
    ///
    /// Priority order:
    /// 1. RegimeTransition (regime ID changed)
    /// 2. RegimeDrift (attention drift flagged)
    /// 3. GoalThreshold (milestone crossed)
    /// 4. RegimeOverstay (duration > 1.2x average)
    pub(super) async fn detect_trigger(
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
    pub(super) fn match_profile(
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

    /// Update internal regime tracking when a regime change is detected.
    ///
    /// Before overwriting the current regime, records the completed dwell time
    /// and updates the per-regime EMA duration tracker.
    ///
    /// Lock ordering: `current_regime_entered(R)` -> `current_regime_id(R)` ->
    /// `regime_avg_duration(W)` -> `current_regime_id(W)` -> `current_regime_entered(W)`.
    pub async fn on_regime_change(&self, new_regime_id: Option<&str>) {
        // Record completed dwell time into the EMA tracker
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
        drop(entered);

        let mut rid = self.current_regime_id.write().await;
        *rid = new_regime_id.map(String::from);
        let mut entered = self.current_regime_entered.write().await;
        *entered = Some(Utc::now());

        // Daily reset + increment context switch counter
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
    }

    /// Build the variable substitution map for template rendering.
    pub(super) async fn build_variables(
        &self,
        regime_label: &str,
        regime_duration_secs: u64,
        app_name: &str,
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("regime".to_string(), regime_label.to_string());
        vars.insert(
            "duration".to_string(),
            super::humanize_duration(regime_duration_secs),
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

        // Context switch count (daily)
        let switch_count = *self.context_switch_count.read().await;
        vars.insert("context_switches".to_string(), switch_count.to_string());

        // Historical comparison — EMA of regime dwell duration
        let avgs = self.regime_avg_duration.read().await;
        let avg_secs = avgs.get(regime_label).copied().unwrap_or(1800.0) as u64;
        vars.insert("comparison".to_string(), super::humanize_duration(avg_secs));

        // Previous context — last known regime before the current one
        let prev_regime = self.current_regime_id.read().await;
        vars.insert(
            "previous_context".to_string(),
            prev_regime.as_deref().unwrap_or("unknown").to_string(),
        );

        vars
    }
}
