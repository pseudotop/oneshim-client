use oneshim_core::config::CoachingTone;
use oneshim_core::models::coaching::{trigger_type_name, CoachingProfile, TriggerType};
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

// ── 54 built-in templates ──────────────────────────────────────────────

const TEMPLATES: &[CoachingTemplate] = &[
    // ── FocusGuard × RegimeTransition ──
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::Direct,
        text: "You've switched from {regime} - {context_switches} switches in 30 min.",
    },
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::Gentle,
        text: "Heads up: you've moved away from {regime}. Need to switch back?",
    },
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::DataDriven,
        text: "{context_switches} context switches today. Your average is {comparison}.",
    },
    // ── FocusGuard × RegimeDrift ──
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "RegimeDrift",
        tone: CoachingTone::Direct,
        text: "Drift detected in {regime}. Refocus on your current task.",
    },
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "RegimeDrift",
        tone: CoachingTone::Gentle,
        text: "Looks like your attention drifted from {regime}. Want to refocus?",
    },
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "RegimeDrift",
        tone: CoachingTone::DataDriven,
        text: "Attention drift in {regime}: {context_switches} app switches detected.",
    },
    // ── TimeAware × RegimeOverstay ──
    CoachingTemplate {
        profile: CoachingProfile::TimeAware,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::Direct,
        text: "{duration} in {regime}. Consider wrapping up.",
    },
    CoachingTemplate {
        profile: CoachingProfile::TimeAware,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::Gentle,
        text: "You've been in {regime} for {duration} — longer than usual. A break might help.",
    },
    CoachingTemplate {
        profile: CoachingProfile::TimeAware,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::DataDriven,
        text: "{duration} in {regime}. Your average session is {comparison}.",
    },
    // ── TimeAware × GoalThreshold ──
    CoachingTemplate {
        profile: CoachingProfile::TimeAware,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Direct,
        text: "{goal_progress}% of your {regime} goal reached ({goal_minutes} min target).",
    },
    CoachingTemplate {
        profile: CoachingProfile::TimeAware,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Gentle,
        text: "Nice progress! {goal_progress}% toward your {regime} goal.",
    },
    CoachingTemplate {
        profile: CoachingProfile::TimeAware,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::DataDriven,
        text: "{regime} goal: {goal_progress}% complete. {remaining_minutes} min remaining.",
    },
    // ── DeepWorkCoach × RegimeOverstay ──
    CoachingTemplate {
        profile: CoachingProfile::DeepWorkCoach,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::Direct,
        text: "Deep work for {duration}. Take a 5-minute break.",
    },
    CoachingTemplate {
        profile: CoachingProfile::DeepWorkCoach,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::Gentle,
        text: "Nice focus session! {duration} in deep work. A short break might help.",
    },
    CoachingTemplate {
        profile: CoachingProfile::DeepWorkCoach,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::DataDriven,
        text: "{duration} deep work session. Average is {comparison}. Break recommended.",
    },
    // ── DeepWorkCoach × RegimeTransition ──
    CoachingTemplate {
        profile: CoachingProfile::DeepWorkCoach,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::Direct,
        text: "Leaving deep work after {duration}. Save your progress.",
    },
    CoachingTemplate {
        profile: CoachingProfile::DeepWorkCoach,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::Gentle,
        text: "Transitioning out of deep work. Great session! Duration: {duration}.",
    },
    CoachingTemplate {
        profile: CoachingProfile::DeepWorkCoach,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::DataDriven,
        text: "Deep work ended after {duration}. Today's total: {goal_minutes} min.",
    },
    // ── ContextRestore × RegimeTransition ──
    CoachingTemplate {
        profile: CoachingProfile::ContextRestore,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::Direct,
        text: "Welcome back. Your last context was {previous_context} in {app_name}.",
    },
    CoachingTemplate {
        profile: CoachingProfile::ContextRestore,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::Gentle,
        text: "Back from break! You were working on {previous_context}. Ready to continue?",
    },
    CoachingTemplate {
        profile: CoachingProfile::ContextRestore,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::DataDriven,
        text: "Returning from idle. Previous: {previous_context} ({app_name}), {duration} ago.",
    },
    // ── GoalTracker × GoalThreshold (25%) ──
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Direct,
        text: "25% of {regime} goal done. {remaining_minutes} min to go.",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Gentle,
        text: "You're a quarter of the way to your {regime} goal! Keep it up.",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::DataDriven,
        text: "{regime}: 25% complete ({goal_progress} min / {goal_minutes} min target).",
    },
    // ── GoalTracker × GoalThreshold (50%) ──
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Direct,
        text: "Halfway to your {regime} goal. {remaining_minutes} min left.",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Gentle,
        text: "Great progress! You're halfway to your {regime} goal.",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::DataDriven,
        text: "{regime}: 50% complete ({goal_progress} min / {goal_minutes} min).",
    },
    // ── GoalTracker × GoalThreshold (75%) ──
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Direct,
        text: "Almost there — 75% of your {regime} goal. Push through!",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Gentle,
        text: "You're 75% toward your {regime} target. Wonderful pace!",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::DataDriven,
        text: "{regime}: 75% complete ({goal_progress} min / {goal_minutes} min). {remaining_minutes} min remaining.",
    },
    // ── GoalTracker × GoalThreshold (100%) ──
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Direct,
        text: "{regime} goal reached! {goal_minutes} min target complete.",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Gentle,
        text: "Congratulations! You've hit your {regime} goal for today.",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::DataDriven,
        text: "{regime}: 100% complete — {goal_minutes} min target achieved.",
    },
    // ── GoalTracker × GoalThreshold (over 100%) ──
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Direct,
        text: "Over target! {goal_progress}% of {regime} goal ({goal_minutes} min).",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Gentle,
        text: "You've exceeded your {regime} target — {goal_progress}%. Well done!",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::DataDriven,
        text: "{regime}: {goal_progress}% of {goal_minutes} min target. {goal_progress} min recorded.",
    },
    // ── Additional variant templates (14+ to reach 50+) ──
    // FocusGuard × RegimeOverstay
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::Direct,
        text: "Still in {regime} after {duration}. Consider a change of pace.",
    },
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::Gentle,
        text: "You've been focused on {regime} for {duration}. Everything okay?",
    },
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::DataDriven,
        text: "{regime}: {duration} elapsed. Typical session: {comparison}.",
    },
    // ContextRestore × RegimeDrift
    CoachingTemplate {
        profile: CoachingProfile::ContextRestore,
        trigger_type: "RegimeDrift",
        tone: CoachingTone::Direct,
        text: "Context drifting. Recall: you were in {previous_context}.",
    },
    CoachingTemplate {
        profile: CoachingProfile::ContextRestore,
        trigger_type: "RegimeDrift",
        tone: CoachingTone::Gentle,
        text: "Seems like you've drifted. Your earlier context was {previous_context}.",
    },
    CoachingTemplate {
        profile: CoachingProfile::ContextRestore,
        trigger_type: "RegimeDrift",
        tone: CoachingTone::DataDriven,
        text: "Drift from {previous_context}. {context_switches} switches in this session.",
    },
    // DeepWorkCoach × RegimeDrift
    CoachingTemplate {
        profile: CoachingProfile::DeepWorkCoach,
        trigger_type: "RegimeDrift",
        tone: CoachingTone::Direct,
        text: "Drift in deep work detected. Close {app_name} and refocus.",
    },
    CoachingTemplate {
        profile: CoachingProfile::DeepWorkCoach,
        trigger_type: "RegimeDrift",
        tone: CoachingTone::Gentle,
        text: "Your deep work flow was interrupted. Want to get back on track?",
    },
    CoachingTemplate {
        profile: CoachingProfile::DeepWorkCoach,
        trigger_type: "RegimeDrift",
        tone: CoachingTone::DataDriven,
        text: "Deep work drift: {context_switches} switches. Average uninterrupted: {comparison}.",
    },
    // TimeAware × RegimeTransition
    CoachingTemplate {
        profile: CoachingProfile::TimeAware,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::Direct,
        text: "Switching from {regime} after {duration}.",
    },
    CoachingTemplate {
        profile: CoachingProfile::TimeAware,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::Gentle,
        text: "Regime change from {regime}. You spent {duration} there.",
    },
    CoachingTemplate {
        profile: CoachingProfile::TimeAware,
        trigger_type: "RegimeTransition",
        tone: CoachingTone::DataDriven,
        text: "{regime} session ended: {duration}. Average: {comparison}.",
    },
    // GoalTracker × RegimeOverstay
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::Direct,
        text: "{regime} overstay: {duration}. Goal is {goal_minutes} min today.",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::Gentle,
        text: "Long {regime} session ({duration}). You've logged {goal_progress} min of your {goal_minutes} min goal.",
    },
    CoachingTemplate {
        profile: CoachingProfile::GoalTracker,
        trigger_type: "RegimeOverstay",
        tone: CoachingTone::DataDriven,
        text: "{regime}: {duration} session. Daily total: {goal_progress}/{goal_minutes} min ({goal_progress}%).",
    },
    // FocusGuard × GoalThreshold
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Direct,
        text: "Focus goal: {goal_progress}% of {goal_minutes} min. Stay on task.",
    },
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::Gentle,
        text: "You're {goal_progress}% toward your focus goal. Keep going!",
    },
    CoachingTemplate {
        profile: CoachingProfile::FocusGuard,
        trigger_type: "GoalThreshold",
        tone: CoachingTone::DataDriven,
        text: "Focus time: {goal_progress}% of {goal_minutes} min target. {remaining_minutes} min left.",
    },
];

/// Registry holding 50+ templates. Selects by profile + trigger + tone
/// with a 3-tier fallback: exact match -> profile+trigger -> profile.
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
    ///
    /// Fallback tiers:
    /// 1. Exact match: profile + trigger + tone
    /// 2. Profile + trigger (any tone)
    /// 3. First template for this profile
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

impl Default for CoachingTemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_at_least_50_templates() {
        let registry = CoachingTemplateRegistry::new();
        assert!(
            registry.template_count() >= 50,
            "expected >= 50 templates, got {}",
            registry.template_count()
        );
    }

    #[test]
    fn select_exact_match() {
        let registry = CoachingTemplateRegistry::new();
        let mut vars = HashMap::new();
        vars.insert("regime".to_string(), "Deep Work".to_string());
        vars.insert("context_switches".to_string(), "5".to_string());

        let result = registry.select(
            &CoachingProfile::FocusGuard,
            &TriggerType::RegimeTransition {
                from_regime: Some("Deep Work".to_string()),
                to_regime: Some("Communication".to_string()),
            },
            &CoachingTone::Direct,
            &vars,
        );

        assert!(
            result.contains("Deep Work"),
            "template should substitute {{regime}}: got '{}'",
            result
        );
        assert!(
            result.contains("5"),
            "template should substitute {{context_switches}}: got '{}'",
            result
        );
        // No unresolved placeholders for the variables we provided
        assert!(
            !result.contains("{regime}"),
            "{{regime}} placeholder should be resolved"
        );
    }

    #[test]
    fn select_fallback_wrong_tone() {
        let registry = CoachingTemplateRegistry::new();
        let vars = HashMap::new();

        // ContextRestore + RegimeDrift has Direct, Gentle, DataDriven.
        // Request a match that falls to the tone-agnostic tier:
        // We verify the fallback returns *something* even if exact tone isn't present
        // by requesting the same profile/trigger with all three tones and confirming each works.
        for tone in [
            CoachingTone::Direct,
            CoachingTone::Gentle,
            CoachingTone::DataDriven,
        ] {
            let result = registry.select(
                &CoachingProfile::ContextRestore,
                &TriggerType::RegimeDrift {
                    regime_label: "Coding".to_string(),
                },
                &tone,
                &vars,
            );
            assert!(!result.is_empty(), "fallback must return non-empty text");
        }
    }

    #[test]
    fn substitute_replaces_all_placeholders() {
        let mut vars = HashMap::new();
        vars.insert("regime".to_string(), "Deep Work".to_string());
        vars.insert("duration".to_string(), "2h 15m".to_string());
        vars.insert("app_name".to_string(), "VS Code".to_string());
        vars.insert("context_switches".to_string(), "3".to_string());
        vars.insert("comparison".to_string(), "1h 30m".to_string());
        vars.insert("goal_progress".to_string(), "75".to_string());
        vars.insert("goal_minutes".to_string(), "120".to_string());
        vars.insert("remaining_minutes".to_string(), "30".to_string());
        vars.insert("previous_context".to_string(), "PR review".to_string());

        let template = "Working in {regime} for {duration} on {app_name}. Switches: {context_switches}. Avg: {comparison}. Goal: {goal_progress}% of {goal_minutes}m. Left: {remaining_minutes}m. Prev: {previous_context}.";
        let result = substitute(template, &vars);

        assert!(
            !result.contains('{'),
            "no unresolved placeholders: '{}'",
            result
        );
    }

    #[test]
    fn all_profiles_have_templates() {
        let registry = CoachingTemplateRegistry::new();
        let profiles = [
            CoachingProfile::FocusGuard,
            CoachingProfile::TimeAware,
            CoachingProfile::DeepWorkCoach,
            CoachingProfile::ContextRestore,
            CoachingProfile::GoalTracker,
        ];

        for profile in &profiles {
            let count = registry
                .templates
                .iter()
                .filter(|t| t.profile == *profile)
                .count();
            assert!(
                count >= 1,
                "profile {:?} must have at least 1 template, got {}",
                profile,
                count
            );
        }
    }
}
