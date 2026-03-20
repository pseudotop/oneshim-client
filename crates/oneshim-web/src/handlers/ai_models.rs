use axum::{extract::State, Json};
use oneshim_api_contracts::ai_providers::{ProviderModelsRequest, ProviderModelsResponse};

use crate::error::ApiError;
use crate::services::ai_model_catalog_web_service::AiModelCatalogQueryService;
use crate::services::web_contexts::AiModelCatalogWebContext;

pub async fn discover_provider_models(
    State(context): State<AiModelCatalogWebContext>,
    Json(request): Json<ProviderModelsRequest>,
) -> Result<Json<ProviderModelsResponse>, ApiError> {
    let response = AiModelCatalogQueryService::new(context)
        .discover_provider_models(&request)
        .await?;
    Ok(Json(response))
}

pub async fn discover_provider_models_for_integration(
    State(context): State<AiModelCatalogWebContext>,
    Json(request): Json<ProviderModelsRequest>,
) -> Result<Json<ProviderModelsResponse>, ApiError> {
    let response = AiModelCatalogQueryService::new(context)
        .discover_provider_models_for_integration(&request)
        .await?;
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
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
            integration_auth: None,
            integration_session: None,
            integration_outbox: None,
            integration_inbox: None,
            integration_inbox_store: None,
            integration_audit: None,
            integration_runtime_telemetry: None,
            update_control: None,
            vector_store: None,
            embedding_provider: None,
            text_search: None,
            override_store: None,
            recluster_requested: None,
            pomodoro: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    fn test_context() -> AiModelCatalogWebContext {
        AiModelCatalogWebContext::from_state(&test_state())
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

        let err = discover_provider_models_for_integration(State(test_context()), Json(request))
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
