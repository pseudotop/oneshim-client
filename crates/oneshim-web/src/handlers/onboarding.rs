use axum::{extract::State, Json};
use chrono::Utc;
use serde::Serialize;

use oneshim_automation::presets::builtin_presets;
use oneshim_core::models::intent::WorkflowPreset;

use crate::AppState;

const ONBOARDING_QUICKSTART_SCHEMA_VERSION: &str = "onboarding.quickstart.v1";

#[derive(Debug, Serialize)]
pub struct QuickstartStepDto {
    pub order: u8,
    pub title: String,
    pub action: String,
    pub expected_outcome: String,
}

#[derive(Debug, Serialize)]
pub struct OnboardingQuickstartDto {
    pub schema_version: String,
    pub generated_at: String,
    pub target_mode: String,
    pub dashboard_url: String,
    pub checklist: Vec<QuickstartStepDto>,
    pub recommended_presets: Vec<WorkflowPreset>,
    pub verification_commands: Vec<String>,
}

fn recommended_presets() -> Vec<WorkflowPreset> {
    let preferred = [
        "daily-priority-sync",
        "deep-work-start",
        "bug-triage-loop",
        "release-readiness",
    ];
    let presets = builtin_presets();
    preferred
        .iter()
        .filter_map(|id| presets.iter().find(|preset| preset.id == *id).cloned())
        .collect()
}

fn dashboard_url_from_state(state: &AppState) -> String {
    if let Some(config_manager) = state.config_manager.as_ref() {
        let config = config_manager.get();
        if config.web.allow_external {
            return format!("http://0.0.0.0:{}", config.web.port);
        }
        return format!("http://127.0.0.1:{}", config.web.port);
    }
    "http://127.0.0.1:9090".to_string()
}

pub async fn get_quickstart(State(state): State<AppState>) -> Json<OnboardingQuickstartDto> {
    let checklist = vec![
        QuickstartStepDto {
            order: 1,
            title: "Run Standalone Mode".to_string(),
            action: "Launch `cargo run -p oneshim-app -- --offline`.".to_string(),
            expected_outcome: "Agent starts without external server dependency.".to_string(),
        },
        QuickstartStepDto {
            order: 2,
            title: "Open Dashboard".to_string(),
            action: "Open local dashboard URL.".to_string(),
            expected_outcome: "Metrics and timeline panels load without errors.".to_string(),
        },
        QuickstartStepDto {
            order: 3,
            title: "Check Privacy Baseline".to_string(),
            action: "Keep sandbox enabled and `external_data_policy` at least `PiiFilterStandard`."
                .to_string(),
            expected_outcome: "Sensitive inputs are blocked unless explicit override exists."
                .to_string(),
        },
        QuickstartStepDto {
            order: 4,
            title: "Enable One Workflow".to_string(),
            action: "Run one recommended workflow preset for daily routine.".to_string(),
            expected_outcome: "Automation audit entries show success/blocked rate baseline."
                .to_string(),
        },
        QuickstartStepDto {
            order: 5,
            title: "Validate First Insight".to_string(),
            action: "Review focus suggestions and timeline interruption markers.".to_string(),
            expected_outcome: "At least one actionable local insight is produced.".to_string(),
        },
    ];

    Json(OnboardingQuickstartDto {
        schema_version: ONBOARDING_QUICKSTART_SCHEMA_VERSION.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        target_mode: "standalone".to_string(),
        dashboard_url: dashboard_url_from_state(&state),
        checklist,
        recommended_presets: recommended_presets(),
        verification_commands: vec![
            "cargo run -p oneshim-app -- --offline".to_string(),
            "cargo test --workspace".to_string(),
            "cargo test -p oneshim-automation perf_budget_ -- --nocapture".to_string(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommended_presets_contains_target_ids() {
        let presets = recommended_presets();
        let ids: Vec<String> = presets.into_iter().map(|preset| preset.id).collect();
        assert!(ids.iter().any(|id| id == "daily-priority-sync"));
        assert!(ids.iter().any(|id| id == "deep-work-start"));
    }
}
