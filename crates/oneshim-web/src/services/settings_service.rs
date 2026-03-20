use crate::error::ApiError;
pub(crate) use crate::services::settings_assembler::is_masked_key;
use crate::services::settings_config_mutation::apply_settings_fields_to_config;
use oneshim_api_contracts::settings::AppSettings;
use oneshim_core::config::AppConfig;

pub(crate) fn apply_settings_to_config(
    config: &mut AppConfig,
    settings: &AppSettings,
) -> Result<(), ApiError> {
    if settings.allow_external
        && config
            .web
            .integration_auth_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(ApiError::BadRequest(
            "allow_external requires web.integration_auth_token to be configured in config.json before enabling external access."
                .to_string(),
        ));
    }
    apply_settings_fields_to_config(config, settings)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::settings_assembler::config_to_settings;
    use crate::services::web_contexts::SettingsWebContext;
    use crate::AppState;
    use async_trait::async_trait;
    use oneshim_api_contracts::settings::{
        AiProviderProfileConfig as ApiAiProviderProfileConfig, AiProviderSettings,
        ExternalApiSettings, OcrValidationSettings,
        SavedAiProviderProfile as ApiSavedAiProviderProfile, SceneActionOverrideSettings,
        SceneIntelligenceSettings,
    };
    use oneshim_core::config::{
        AiAccessMode, AiProviderProfileConfig, AiProviderType, CredentialAuthMode,
        CredentialBackendKind, CredentialBinding, ExternalApiEndpoint, LlmProviderType,
        OcrProviderType, SavedAiProviderProfile, SecretRef,
    };
    use oneshim_core::config_manager::ConfigManager;
    use oneshim_core::error::CoreError;
    use oneshim_core::ports::secret_store::SecretStore;
    use oneshim_storage::sqlite::SqliteStorage;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;
    use tempfile::TempDir;
    use tokio::sync::broadcast;

    struct TestSecretStore {
        values: Mutex<HashMap<(String, String), String>>,
    }

    impl TestSecretStore {
        fn new() -> Self {
            Self {
                values: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl SecretStore for TestSecretStore {
        async fn store(&self, namespace: &str, key: &str, value: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .insert((namespace.to_string(), key.to_string()), value.to_string());
            Ok(())
        }

        async fn retrieve(&self, namespace: &str, key: &str) -> Result<Option<String>, CoreError> {
            Ok(self
                .values
                .lock()
                .unwrap()
                .get(&(namespace.to_string(), key.to_string()))
                .cloned())
        }

        async fn delete(&self, namespace: &str, key: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .remove(&(namespace.to_string(), key.to_string()));
            Ok(())
        }

        async fn delete_namespace(&self, namespace: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .retain(|(existing_namespace, _), _| existing_namespace != namespace);
            Ok(())
        }
    }

    fn test_state_without_config_manager() -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory sqlite"));
        let (event_tx, _) = broadcast::channel(8);
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

    fn test_state_with_config_manager(
        config_manager: ConfigManager,
        secret_store: Option<Arc<dyn SecretStore>>,
    ) -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory sqlite"));
        let (event_tx, _) = broadcast::channel(8);
        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: Some(config_manager),
            default_secret_backend_kind: oneshim_core::config::CredentialBackendKind::OsSecretStore,
            secret_store,
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

    fn test_context_from_state(state: &AppState) -> SettingsWebContext {
        SettingsWebContext::from_state(state)
    }

    fn anthropic_external_api_settings(api_key: &str) -> ExternalApiSettings {
        ExternalApiSettings {
            endpoint: "https://api.anthropic.com/v1/messages".to_string(),
            api_key_masked: api_key.to_string(),
            model: Some("claude-3-7-sonnet-latest".to_string()),
            provider_type: "Anthropic".to_string(),
            surface_id: Some("provider_surface.anthropic.direct_api".to_string()),
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: !api_key.is_empty(),
            can_edit_secret: true,
            secret_display_hint: None,
            projection_enabled: false,
        }
    }

    fn anthropic_api_profile_config(api_key: &str) -> ApiAiProviderProfileConfig {
        let defaults = AppSettings::default();
        ApiAiProviderProfileConfig {
            access_mode: defaults.ai_provider.access_mode,
            ocr_provider: defaults.ai_provider.ocr_provider,
            llm_provider: "Remote".to_string(),
            external_data_policy: defaults.ai_provider.external_data_policy,
            allow_unredacted_external_ocr: defaults.ai_provider.allow_unredacted_external_ocr,
            ocr_validation: OcrValidationSettings::default(),
            scene_action_override: SceneActionOverrideSettings::default(),
            scene_intelligence: SceneIntelligenceSettings::default(),
            fallback_to_local: defaults.ai_provider.fallback_to_local,
            ocr_api: None,
            llm_api: Some(anthropic_external_api_settings(api_key)),
        }
    }

    #[tokio::test]
    async fn update_settings_validates_input_without_config_manager() {
        let state = test_state_without_config_manager();
        let context = test_context_from_state(&state);
        let settings = AppSettings {
            web_port: 80,
            ..AppSettings::default()
        };

        let result = crate::services::settings_web_service::SettingsCommandService::new(context)
            .update_settings(&settings)
            .await;
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }

    #[tokio::test]
    async fn update_settings_accepts_valid_defaults_without_config_manager() {
        let state = test_state_without_config_manager();
        let context = test_context_from_state(&state);
        let settings = AppSettings::default();

        let result = crate::services::settings_web_service::SettingsCommandService::new(context)
            .update_settings(&settings)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn update_settings_accepts_provider_oauth_roundtrip_without_config_manager() {
        let state = test_state_without_config_manager();
        let context = test_context_from_state(&state);
        let mut settings = AppSettings::default();
        settings.ai_provider.access_mode = "ProviderOAuth".to_string();
        settings.ai_provider.llm_provider = "Remote".to_string();

        let result = crate::services::settings_web_service::SettingsCommandService::new(context)
            .update_settings(&settings)
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn apply_settings_to_config_rejects_allow_external_without_integration_token() {
        let mut config = AppConfig::default_config();
        let settings = AppSettings {
            allow_external: true,
            ..AppSettings::default()
        };

        let err = apply_settings_to_config(&mut config, &settings)
            .expect_err("external access should require integration token");
        match err {
            ApiError::BadRequest(message) => {
                assert!(message.contains("integration_auth_token"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_settings_persists_remote_api_key_to_secret_store_and_binding_metadata() {
        let temp_dir = TempDir::new().expect("temp dir");
        let config_path = temp_dir.path().join("config.json");
        let config_manager = ConfigManager::with_path(config_path).expect("config manager");
        let secret_store = Arc::new(TestSecretStore::new()) as Arc<dyn SecretStore>;
        let state =
            test_state_with_config_manager(config_manager.clone(), Some(secret_store.clone()));
        let context = test_context_from_state(&state);

        let mut settings = AppSettings::default();
        settings.ai_provider.llm_provider = "Remote".to_string();
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key_masked: "sk-secret-123456".to_string(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: None,
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: true,
            can_edit_secret: true,
            secret_display_hint: None,
            projection_enabled: true,
        });

        crate::services::settings_web_service::SettingsCommandService::new(context)
            .update_settings(&settings)
            .await
            .expect("settings update should succeed");

        let stored = secret_store
            .retrieve("provider/openai/llm", "api_key")
            .await
            .expect("secret lookup");
        assert_eq!(stored.as_deref(), Some("sk-secret-123456"));

        let saved = config_manager.get();
        let endpoint = saved.ai_provider.llm_api.expect("saved llm endpoint");
        let binding = endpoint.credential.expect("credential binding");
        assert_eq!(endpoint.api_key, "");
        assert_eq!(binding.backend_kind, CredentialBackendKind::OsSecretStore);
        assert_eq!(binding.auth_mode, CredentialAuthMode::ApiKey);
        assert!(binding.projection_enabled);
        let secret_ref = binding.secret_ref.expect("secret ref");
        assert_eq!(secret_ref.namespace, "provider/openai/llm");
        assert_eq!(secret_ref.key, "api_key");
    }

    #[tokio::test]
    async fn update_settings_persists_selected_saved_profile_under_profile_namespace() {
        let temp_dir = TempDir::new().expect("temp dir");
        let config_path = temp_dir.path().join("config.json");
        let config_manager = ConfigManager::with_path(config_path).expect("config manager");
        let secret_store = Arc::new(TestSecretStore::new()) as Arc<dyn SecretStore>;
        let state =
            test_state_with_config_manager(config_manager.clone(), Some(secret_store.clone()));
        let context = test_context_from_state(&state);

        let mut settings = AppSettings::default();
        settings.ai_provider.llm_provider = "Remote".to_string();
        settings.ai_provider.llm_api =
            Some(anthropic_external_api_settings("sk-ant-secret-123456"));
        settings.ai_provider.active_profile_id = Some("anthropic-prod".to_string());
        settings.ai_provider.saved_profiles = vec![ApiSavedAiProviderProfile {
            profile_id: "anthropic-prod".to_string(),
            name: "Anthropic Prod".to_string(),
            ai_provider: anthropic_api_profile_config("sk-ant-secret-123456"),
            updated_at: Some("2026-03-17T00:00:00Z".to_string()),
        }];

        crate::services::settings_web_service::SettingsCommandService::new(context)
            .update_settings(&settings)
            .await
            .expect("settings update should succeed");

        let stored = secret_store
            .retrieve("provider/anthropic/anthropic-prod", "api_key")
            .await
            .expect("secret lookup");
        assert_eq!(stored.as_deref(), Some("sk-ant-secret-123456"));

        let legacy_slot = secret_store
            .retrieve("provider/anthropic/llm", "api_key")
            .await
            .expect("legacy slot lookup");
        assert_eq!(legacy_slot, None);

        let saved = config_manager.get();
        assert_eq!(
            saved.ai_provider.active_profile_id.as_deref(),
            Some("anthropic-prod")
        );

        let active_endpoint = saved.ai_provider.llm_api.expect("active llm endpoint");
        let active_binding = active_endpoint
            .credential
            .expect("active credential binding");
        let active_secret_ref = active_binding.secret_ref.expect("active secret ref");
        assert_eq!(
            active_secret_ref.namespace,
            "provider/anthropic/anthropic-prod"
        );
        assert_eq!(active_secret_ref.key, "api_key");

        assert_eq!(saved.ai_provider.saved_profiles.len(), 1);
        let saved_profile = &saved.ai_provider.saved_profiles[0];
        assert_eq!(saved_profile.profile_id, "anthropic-prod");
        assert_eq!(saved_profile.name, "Anthropic Prod");

        let profile_endpoint = saved_profile
            .ai_provider
            .llm_api
            .as_ref()
            .expect("saved profile llm endpoint");
        let profile_binding = profile_endpoint
            .credential
            .as_ref()
            .expect("saved profile credential binding");
        let profile_secret_ref = profile_binding
            .secret_ref
            .as_ref()
            .expect("saved profile secret ref");
        assert_eq!(
            profile_secret_ref.namespace,
            "provider/anthropic/anthropic-prod"
        );
        assert_eq!(profile_secret_ref.key, "api_key");
    }

    #[test]
    fn config_to_settings_maps_plaintext_api_keys_to_current_default_backend() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.example.com/v1".to_string(),
            api_key: "sk-test-1234567890".to_string(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 45,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        });

        let settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.auth_mode, "api_key");
        assert_eq!(llm_api.backend_kind, "os_secret_store");
        assert!(llm_api.has_secret);
        assert!(llm_api.can_edit_secret);
        assert_eq!(llm_api.secret_display_hint.as_deref(), Some("sk...7890"));
        assert!(!llm_api.projection_enabled);
    }

    #[test]
    fn config_to_settings_marks_ollama_surface_as_no_auth() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "http://localhost:11434/v1/responses".to_string(),
            api_key: String::new(),
            model: Some("qwen3:8b".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::Ollama,
            surface_id: Some("provider_surface.ollama.local_http".to_string()),
            credential: None,
        });

        let settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.auth_mode, "api_key");
        assert_eq!(llm_api.backend_kind, "unavailable");
        assert!(!llm_api.has_secret);
        assert!(!llm_api.can_edit_secret);
        assert!(llm_api.api_key_masked.is_empty());
    }

    #[test]
    fn config_to_settings_marks_provider_oauth_as_managed_oauth_metadata() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderOAuth;
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        });

        let settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.auth_mode, "managed_oauth");
        assert_eq!(llm_api.backend_kind, "os_secret_store");
        assert_eq!(
            llm_api.surface_id.as_deref(),
            Some("provider_surface.openai.managed_oauth")
        );
        assert!(!llm_api.has_secret);
        assert!(!llm_api.can_edit_secret);
        assert_eq!(llm_api.secret_display_hint, None);
    }

    #[test]
    fn config_to_settings_roundtrips_saved_ai_provider_profiles() {
        let mut config = AppConfig::default_config();
        config.ai_provider.active_profile_id = Some("anthropic-prod".to_string());
        config.ai_provider.saved_profiles = vec![SavedAiProviderProfile {
            profile_id: "anthropic-prod".to_string(),
            name: "Anthropic Prod".to_string(),
            ai_provider: AiProviderProfileConfig {
                access_mode: AiAccessMode::ProviderApiKey,
                ocr_provider: OcrProviderType::Local,
                llm_provider: LlmProviderType::Remote,
                llm_api: Some(ExternalApiEndpoint {
                    endpoint: "https://api.anthropic.com/v1/messages".to_string(),
                    api_key: String::new(),
                    model: Some("claude-3-7-sonnet-latest".to_string()),
                    timeout_secs: 45,
                    provider_type: AiProviderType::Anthropic,
                    surface_id: Some("provider_surface.anthropic.direct_api".to_string()),
                    credential: Some(CredentialBinding {
                        auth_mode: CredentialAuthMode::ApiKey,
                        backend_kind: CredentialBackendKind::OsSecretStore,
                        secret_ref: Some(SecretRef {
                            namespace: "provider/anthropic/anthropic-prod".to_string(),
                            key: "api_key".to_string(),
                        }),
                        projection_enabled: false,
                    }),
                }),
                ..AiProviderProfileConfig::default()
            },
            updated_at: Some("2026-03-17T00:00:00Z".to_string()),
        }];

        let settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);

        assert_eq!(
            settings.ai_provider.active_profile_id.as_deref(),
            Some("anthropic-prod")
        );
        assert_eq!(settings.ai_provider.saved_profiles.len(), 1);

        let saved_profile = &settings.ai_provider.saved_profiles[0];
        assert_eq!(saved_profile.profile_id, "anthropic-prod");
        assert_eq!(saved_profile.name, "Anthropic Prod");
        assert_eq!(
            saved_profile.updated_at.as_deref(),
            Some("2026-03-17T00:00:00Z")
        );
        assert_eq!(saved_profile.ai_provider.access_mode, "ProviderApiKey");
        assert_eq!(saved_profile.ai_provider.llm_provider, "Remote");

        let llm_api = saved_profile
            .ai_provider
            .llm_api
            .as_ref()
            .expect("saved profile llm api");
        assert_eq!(llm_api.provider_type, "Anthropic");
        assert_eq!(
            llm_api.surface_id.as_deref(),
            Some("provider_surface.anthropic.direct_api")
        );
        assert_eq!(llm_api.timeout_secs, 45);
        assert!(llm_api.has_secret);
        assert_eq!(llm_api.backend_kind, "os_secret_store");
    }

    #[test]
    fn config_to_settings_marks_cli_subscription_as_cli_bridge_metadata() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderSubscriptionCli;
        config.ai_provider.llm_provider = LlmProviderType::Local;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: String::new(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        });

        let settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.auth_mode, "cli_bridge");
        assert_eq!(llm_api.backend_kind, "bridge_managed");
        assert_eq!(
            llm_api.surface_id.as_deref(),
            Some("provider_surface.openai.subprocess_cli")
        );
        assert!(!llm_api.can_edit_secret);
        assert_eq!(llm_api.secret_display_hint, None);
    }

    #[test]
    fn config_to_settings_keeps_backend_managed_api_key_without_fake_mask() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: Some(SecretRef {
                    namespace: "provider/openai/llm".to_string(),
                    key: "api_key".to_string(),
                }),
                projection_enabled: false,
            }),
        });

        let settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.backend_kind, "os_secret_store");
        assert!(llm_api.has_secret);
        assert_eq!(llm_api.api_key_masked, "");
        assert_eq!(llm_api.secret_display_hint, None);
    }

    #[test]
    fn config_to_settings_marks_env_bound_api_key_as_present_without_secret_ref() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::Env,
                secret_ref: None,
                projection_enabled: false,
            }),
        });

        let settings = config_to_settings(&config, CredentialBackendKind::Env);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.backend_kind, "env");
        assert!(llm_api.has_secret);
        assert!(!llm_api.can_edit_secret);
        assert_eq!(llm_api.api_key_masked, "");
        assert_eq!(llm_api.secret_display_hint, None);
    }

    #[test]
    fn apply_settings_to_config_preserves_projection_enabled_on_existing_binding() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: Some(SecretRef {
                    namespace: "provider/openai/llm".to_string(),
                    key: "api_key".to_string(),
                }),
                projection_enabled: false,
            }),
        });

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.as_mut().expect("llm settings");
        llm_api.projection_enabled = true;

        apply_settings_to_config(&mut config, &settings).expect("config update");

        let binding = config
            .ai_provider
            .llm_api
            .as_ref()
            .and_then(|endpoint| endpoint.credential.as_ref())
            .expect("binding");
        assert!(binding.projection_enabled);
    }

    #[test]
    fn apply_settings_to_config_rejects_projection_enabled_for_env_backend() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::Env);
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "openai".to_string(),
            surface_id: None,
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "env".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: true,
        });

        let err = apply_settings_to_config(&mut config, &settings).expect_err("projection guard");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn apply_settings_to_config_rejects_projection_enabled_for_managed_oauth() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderOAuth;
        config.ai_provider.llm_provider = LlmProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "openai".to_string(),
            surface_id: None,
            timeout_secs: 30,
            auth_mode: "managed_oauth".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: true,
        });

        let err = apply_settings_to_config(&mut config, &settings).expect_err("projection guard");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn apply_settings_to_config_rejects_incompatible_surface_id_for_oauth_mode() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderOAuth;
        config.ai_provider.llm_provider = LlmProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "openai".to_string(),
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            timeout_secs: 30,
            auth_mode: "managed_oauth".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let err = apply_settings_to_config(&mut config, &settings).expect_err("surface guard");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn apply_settings_to_config_rejects_projection_enabled_for_bridge_managed_backend() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;

        let settings = AppSettings {
            ai_provider: AiProviderSettings {
                llm_provider: "Remote".to_string(),
                llm_api: Some(ExternalApiSettings {
                    endpoint: "https://api.openai.com/v1".to_string(),
                    api_key_masked: String::new(),
                    model: Some("gpt-5.4".to_string()),
                    provider_type: "OpenAi".to_string(),
                    surface_id: None,
                    timeout_secs: 30,
                    auth_mode: "api_key".to_string(),
                    backend_kind: "bridge_managed".to_string(),
                    has_secret: true,
                    can_edit_secret: false,
                    secret_display_hint: None,
                    projection_enabled: true,
                }),
                ..AiProviderSettings::default()
            },
            ..AppSettings::default()
        };

        let err = apply_settings_to_config(&mut config, &settings).unwrap_err();
        match err {
            ApiError::BadRequest(message) => {
                assert!(message.contains("Projection is not supported"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn apply_settings_to_config_rewrites_binding_when_switching_to_oauth_mode() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test-123456".to_string(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: Some(SecretRef {
                    namespace: "provider/openai/llm".to_string(),
                    key: "api_key".to_string(),
                }),
                projection_enabled: true,
            }),
        });

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.access_mode = "ProviderOAuth".to_string();
        let llm_api = settings
            .ai_provider
            .llm_api
            .as_mut()
            .expect("llm api settings");
        llm_api.surface_id = Some("provider_surface.openai.managed_oauth".to_string());
        llm_api.auth_mode = "managed_oauth".to_string();
        llm_api.backend_kind = "os_secret_store".to_string();
        llm_api.projection_enabled = false;

        apply_settings_to_config(&mut config, &settings).expect("oauth mode rewrite");

        let endpoint = config.ai_provider.llm_api.as_ref().expect("llm endpoint");
        let binding = endpoint.credential.as_ref().expect("credential binding");
        assert_eq!(binding.auth_mode, CredentialAuthMode::ManagedOAuth);
        assert_eq!(binding.backend_kind, CredentialBackendKind::OsSecretStore);
        assert!(binding.secret_ref.is_none());
        assert!(!binding.projection_enabled);
        assert!(endpoint.api_key.is_empty());
    }

    #[test]
    fn apply_settings_to_config_rewrites_binding_when_switching_from_cli_bridge_to_api_key() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderSubscriptionCli;
        config.ai_provider.llm_provider = LlmProviderType::Local;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: String::new(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: Some("provider_surface.openai.subprocess_cli".to_string()),
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::CliBridge,
                backend_kind: CredentialBackendKind::BridgeManaged,
                secret_ref: None,
                projection_enabled: false,
            }),
        });

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.access_mode = "ProviderApiKey".to_string();
        settings.ai_provider.llm_provider = "Remote".to_string();
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: false,
            can_edit_secret: true,
            secret_display_hint: None,
            projection_enabled: false,
        });

        apply_settings_to_config(&mut config, &settings).expect("api key mode rewrite");

        let endpoint = config.ai_provider.llm_api.as_ref().expect("llm endpoint");
        let binding = endpoint.credential.as_ref().expect("credential binding");
        assert_eq!(binding.auth_mode, CredentialAuthMode::ApiKey);
        assert_eq!(binding.backend_kind, CredentialBackendKind::OsSecretStore);
        assert!(binding.secret_ref.is_none());
        assert_eq!(
            endpoint.surface_id.as_deref(),
            Some("provider_surface.openai.direct_api")
        );
    }

    #[test]
    fn apply_settings_to_config_allows_direct_ocr_surface_in_cli_mode() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderSubscriptionCli;
        config.ai_provider.ocr_provider = OcrProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.access_mode = "ProviderSubscriptionCli".to_string();
        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key_masked: "sk-ocr-123456".to_string(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: true,
            can_edit_secret: true,
            secret_display_hint: None,
            projection_enabled: false,
        });

        apply_settings_to_config(&mut config, &settings).expect("cli mode should keep direct OCR");

        let endpoint = config.ai_provider.ocr_api.as_ref().expect("ocr endpoint");
        assert_eq!(
            endpoint.surface_id.as_deref(),
            Some("provider_surface.openai.direct_api")
        );
        assert_eq!(endpoint.api_key, "sk-ocr-123456");
    }

    #[test]
    fn apply_settings_to_config_rejects_cli_auth_mode_for_ocr_in_api_key_mode() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderApiKey;
        config.ai_provider.ocr_provider = OcrProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.access_mode = "ProviderApiKey".to_string();
        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: String::new(),
            api_key_masked: String::new(),
            model: None,
            provider_type: "OpenAi".to_string(),
            surface_id: Some("provider_surface.openai.subprocess_cli".to_string()),
            timeout_secs: 30,
            auth_mode: "cli_bridge".to_string(),
            backend_kind: "bridge_managed".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let err = apply_settings_to_config(&mut config, &settings).expect_err("ocr cli mode guard");
        match err {
            ApiError::BadRequest(message) => {
                assert!(message.contains("ProviderApiKey"));
                assert!(message.contains("OCR"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn apply_settings_to_config_allows_subprocess_ocr_surface_in_cli_mode() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderSubscriptionCli;
        config.ai_provider.ocr_provider = OcrProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.access_mode = "ProviderSubscriptionCli".to_string();
        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: String::new(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: Some("provider_surface.openai.subprocess_cli".to_string()),
            timeout_secs: 30,
            auth_mode: "cli_bridge".to_string(),
            backend_kind: "bridge_managed".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        apply_settings_to_config(&mut config, &settings).expect("ocr subprocess surface");

        let endpoint = config.ai_provider.ocr_api.as_ref().expect("ocr endpoint");
        assert_eq!(
            endpoint.surface_id.as_deref(),
            Some("provider_surface.openai.subprocess_cli")
        );
        let binding = endpoint.credential.as_ref().expect("credential binding");
        assert_eq!(binding.auth_mode, CredentialAuthMode::CliBridge);
        assert_eq!(binding.backend_kind, CredentialBackendKind::BridgeManaged);
    }

    #[test]
    fn apply_settings_to_config_rejects_managed_ocr_surface_in_cli_mode() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderSubscriptionCli;
        config.ai_provider.ocr_provider = OcrProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.access_mode = "ProviderSubscriptionCli".to_string();
        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: String::new(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: Some("provider_surface.openai.managed_oauth".to_string()),
            timeout_secs: 30,
            auth_mode: "managed_oauth".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let err = apply_settings_to_config(&mut config, &settings).expect_err("ocr managed guard");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn apply_settings_to_config_rejects_managed_ocr_surface_in_oauth_mode() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderOAuth;
        config.ai_provider.ocr_provider = OcrProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.access_mode = "ProviderOAuth".to_string();
        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: String::new(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: Some("provider_surface.openai.managed_oauth".to_string()),
            timeout_secs: 30,
            auth_mode: "managed_oauth".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let err =
            apply_settings_to_config(&mut config, &settings).expect_err("oauth ocr managed guard");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn apply_settings_to_config_rejects_text_only_ollama_model_for_ocr() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderApiKey;
        config.ai_provider.ocr_provider = OcrProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: "http://localhost:11434/v1/chat/completions".to_string(),
            api_key_masked: String::new(),
            model: Some("qwen3:8b".to_string()),
            provider_type: "Ollama".to_string(),
            surface_id: Some("provider_surface.ollama.local_http".to_string()),
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "unavailable".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let err =
            apply_settings_to_config(&mut config, &settings).expect_err("ollama OCR model guard");
        match err {
            ApiError::BadRequest(message) => {
                assert!(message.contains("OCR-capable"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn apply_settings_to_config_rejects_google_ocr_model_override() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderApiKey;
        config.ai_provider.ocr_provider = OcrProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: "https://vision.googleapis.com/v1/images:annotate".to_string(),
            api_key_masked: "goog-key-123456".to_string(),
            model: Some("gemini-2.5-flash".to_string()),
            provider_type: "Google".to_string(),
            surface_id: Some("provider_surface.google.direct_api".to_string()),
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: true,
            can_edit_secret: true,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let err =
            apply_settings_to_config(&mut config, &settings).expect_err("google OCR model guard");
        match err {
            ApiError::BadRequest(message) => {
                assert!(message.contains("does not support configurable OCR model selection"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn apply_settings_to_config_requires_explicit_model_for_local_openai_compatible_surface() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderApiKey;
        config.ai_provider.llm_provider = LlmProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.llm_provider = "Remote".to_string();
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "http://127.0.0.1:1234/v1/chat/completions".to_string(),
            api_key_masked: String::new(),
            model: None,
            provider_type: "Generic".to_string(),
            surface_id: Some("provider_surface.generic.local_openai_compatible".to_string()),
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "unavailable".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let err = apply_settings_to_config(&mut config, &settings)
            .expect_err("surface should require explicit model selection");
        match err {
            ApiError::BadRequest(message) => {
                assert!(message.contains("requires an explicit LLM model selection"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn apply_settings_to_config_rejects_non_structured_model_for_local_openai_compatible_ocr() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderApiKey;
        config.ai_provider.ocr_provider = OcrProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: "http://127.0.0.1:1234/v1/chat/completions".to_string(),
            api_key_masked: String::new(),
            model: Some("text-embedding-3-small".to_string()),
            provider_type: "Generic".to_string(),
            surface_id: Some("provider_surface.generic.local_openai_compatible".to_string()),
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "unavailable".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let err = apply_settings_to_config(&mut config, &settings)
            .expect_err("surface should reject non-structured OCR model");
        match err {
            ApiError::BadRequest(message) => {
                assert!(
                    message.contains("structured JSON output")
                        || message.contains("OCR-capable")
                        || message.contains("not marked as OCR-capable")
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn apply_settings_to_config_clears_secret_binding_for_ollama_surface() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: Some(SecretRef {
                    namespace: "provider/openai/llm".to_string(),
                    key: "api_key".to_string(),
                }),
                projection_enabled: true,
            }),
        });

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.llm_provider = "Remote".to_string();
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "http://localhost:11434/v1/responses".to_string(),
            api_key_masked: String::new(),
            model: Some("qwen3:8b".to_string()),
            provider_type: "Ollama".to_string(),
            surface_id: Some("provider_surface.ollama.local_http".to_string()),
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "unavailable".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        apply_settings_to_config(&mut config, &settings).expect("ollama no-auth save should work");

        let endpoint = config.ai_provider.llm_api.expect("saved endpoint");
        assert_eq!(endpoint.provider_type, AiProviderType::Ollama);
        assert_eq!(
            endpoint.surface_id.as_deref(),
            Some("provider_surface.ollama.local_http")
        );
        assert!(endpoint.api_key.is_empty());
        assert!(endpoint.credential.is_none());
    }

    #[test]
    fn apply_settings_to_config_rejects_backend_change_for_existing_binding() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: Some(SecretRef {
                    namespace: "provider/openai/llm".to_string(),
                    key: "api_key".to_string(),
                }),
                projection_enabled: false,
            }),
        });

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings
            .ai_provider
            .llm_api
            .as_mut()
            .expect("llm api settings");
        llm_api.backend_kind = "file_secret_store".to_string();

        let err = apply_settings_to_config(&mut config, &settings).unwrap_err();
        match err {
            ApiError::BadRequest(message) => {
                assert!(message.contains("Changing provider credential auth mode or backend"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_settings_rejects_api_key_write_for_env_backend() {
        let temp_dir = TempDir::new().expect("temp dir");
        let config_path = temp_dir.path().join("config.json");
        let config_manager = ConfigManager::with_path(config_path).expect("config manager");
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory sqlite"));
        let (event_tx, _) = broadcast::channel(8);
        let state = AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: Some(config_manager),
            default_secret_backend_kind: CredentialBackendKind::Env,
            secret_store: Some(Arc::new(TestSecretStore::new())),
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
        };

        let mut settings = AppSettings::default();
        settings.ai_provider.llm_provider = "Remote".to_string();
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key_masked: "sk-secret-123456".to_string(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: None,
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "env".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let context = test_context_from_state(&state);
        let err = crate::services::settings_web_service::SettingsCommandService::new(context)
            .update_settings(&settings)
            .await
            .expect_err("env backend should be read-only");

        assert!(matches!(err, ApiError::BadRequest(_)));
    }
}
