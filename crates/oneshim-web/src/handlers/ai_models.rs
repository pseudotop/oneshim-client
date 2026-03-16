use axum::{extract::State, Json};
use oneshim_api_contracts::ai_providers::{ProviderModelsRequest, ProviderModelsResponse};

use crate::error::ApiError;
use crate::services::ai_model_catalog_service;
use crate::AppState;

pub async fn discover_provider_models(
    State(state): State<AppState>,
    Json(request): Json<ProviderModelsRequest>,
) -> Result<Json<ProviderModelsResponse>, ApiError> {
    let response = ai_model_catalog_service::fetch_provider_models(&request, &state).await?;
    Ok(Json(response))
}

pub async fn discover_provider_models_for_integration(
    State(state): State<AppState>,
    Json(request): Json<ProviderModelsRequest>,
) -> Result<Json<ProviderModelsResponse>, ApiError> {
    if request.use_saved_secret {
        return Err(ApiError::BadRequest(
            "Integration model discovery requires caller-supplied credentials and does not permit use_saved_secret."
                .to_string(),
        ));
    }

    let response = ai_model_catalog_service::fetch_provider_models(&request, &state).await?;
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Json;
    use oneshim_storage::sqlite::SqliteStorage;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    fn test_state() -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
        let (event_tx, _) = broadcast::channel(16);
        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: None,
            default_secret_backend_kind: oneshim_core::config::CredentialBackendKind::Unavailable,
            secret_store: None,
            secret_stores: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            integration_runtime_status: None,
            integration_session: None,
            update_control: None,
        }
    }

    #[tokio::test]
    async fn integration_model_discovery_rejects_saved_secret() {
        let request = ProviderModelsRequest {
            provider_type: "openai".to_string(),
            api_key: String::new(),
            endpoint: None,
            surface: Some("llm_api".to_string()),
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            use_saved_secret: true,
        };

        let err = discover_provider_models_for_integration(State(test_state()), Json(request))
            .await
            .expect_err("integration discovery should reject saved secrets");

        match err {
            ApiError::BadRequest(message) => {
                assert!(message.contains("caller-supplied credentials"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
