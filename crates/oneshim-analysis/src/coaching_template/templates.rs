use oneshim_core::config::CoachingTone;
use oneshim_core::models::coaching::CoachingProfile;

use super::CoachingTemplate;

// ── 54 built-in templates ──────────────────────────────────────────────

pub(super) const TEMPLATES: &[CoachingTemplate] = &[
    // ── FocusGuard x RegimeTransition ──
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
    // ── FocusGuard x RegimeDrift ──
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
    // ── TimeAware x RegimeOverstay ──
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
    // ── TimeAware x GoalThreshold ──
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
    // ── DeepWorkCoach x RegimeOverstay ──
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
    // ── DeepWorkCoach x RegimeTransition ──
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
    // ── ContextRestore x RegimeTransition ──
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
    // ── GoalTracker x GoalThreshold (25%) ──
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
    // ── GoalTracker x GoalThreshold (50%) ──
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
    // ── GoalTracker x GoalThreshold (75%) ──
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
    // ── GoalTracker x GoalThreshold (100%) ──
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
    // ── GoalTracker x GoalThreshold (over 100%) ──
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
    // FocusGuard x RegimeOverstay
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
    // ContextRestore x RegimeDrift
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
    // DeepWorkCoach x RegimeDrift
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
    // TimeAware x RegimeTransition
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
    // GoalTracker x RegimeOverstay
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
    // FocusGuard x GoalThreshold
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
