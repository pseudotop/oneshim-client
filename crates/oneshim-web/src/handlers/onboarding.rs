use axum::{extract::State, Json};
use oneshim_api_contracts::onboarding::OnboardingQuickstartDto;

use crate::services::onboarding_service::OnboardingQueryService;
use crate::services::web_contexts::ConfigWebContext;

pub async fn get_quickstart(
    State(context): State<ConfigWebContext>,
) -> Json<OnboardingQuickstartDto> {
    Json(OnboardingQueryService::new(context).get_quickstart())
}
