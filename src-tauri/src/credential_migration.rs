use std::sync::Arc;

use oneshim_core::config::{
    CredentialAuthMode, CredentialBackendKind, CredentialBinding, ExternalApiEndpoint, SecretRef,
};
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::error::CoreError;
use oneshim_core::ports::secret_store::{provider_api_key_secret_ref, SecretStore};

pub async fn migrate_legacy_provider_api_keys(
    config_manager: &ConfigManager,
    secret_store: Arc<dyn SecretStore>,
    backend_kind: CredentialBackendKind,
) -> Result<bool, CoreError> {
    let mut config = config_manager.get();
    let mut changed = false;

    if let Some(endpoint) = config.ai_provider.ocr_api.as_mut() {
        changed |=
            migrate_endpoint_if_needed(endpoint, "ocr", secret_store.clone(), backend_kind).await?;
    }

    if let Some(endpoint) = config.ai_provider.llm_api.as_mut() {
        changed |= migrate_endpoint_if_needed(endpoint, "llm", secret_store, backend_kind).await?;
    }

    if changed {
        config_manager.update(config)?;
    }

    Ok(changed)
}

async fn migrate_endpoint_if_needed(
    endpoint: &mut ExternalApiEndpoint,
    profile_id: &str,
    secret_store: Arc<dyn SecretStore>,
    backend_kind: CredentialBackendKind,
) -> Result<bool, CoreError> {
    if !matches!(
        backend_kind,
        CredentialBackendKind::OsSecretStore | CredentialBackendKind::FileSecretStore
    ) {
        return Err(CoreError::Config(format!(
            "legacy provider API key migration requires a writable secret backend, got {:?}",
            backend_kind
        )));
    }

    let api_key = endpoint.api_key.trim();
    if api_key.is_empty() {
        return Ok(false);
    }

    let (namespace, key) = provider_api_key_secret_ref(provider_type_id(endpoint), profile_id)?;
    secret_store.store(&namespace, key, api_key).await?;

    endpoint.credential = Some(CredentialBinding {
        auth_mode: CredentialAuthMode::ApiKey,
        backend_kind,
        secret_ref: Some(SecretRef {
            namespace,
            key: key.to_string(),
        }),
        projection_enabled: false,
    });
    endpoint.api_key.clear();

    Ok(true)
}

fn provider_type_id(endpoint: &ExternalApiEndpoint) -> &'static str {
    match endpoint.provider_type {
        oneshim_core::config::AiProviderType::OpenAi => "openai",
        oneshim_core::config::AiProviderType::Anthropic => "anthropic",
        oneshim_core::config::AiProviderType::Google => "google",
        oneshim_core::config::AiProviderType::Generic => "generic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oneshim_core::config::{AiProviderConfig, AiProviderType, AppConfig};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tempfile::TempDir;

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

    #[tokio::test]
    async fn migrate_legacy_provider_api_keys_moves_plaintext_to_secret_store() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let config_manager = ConfigManager::with_path(config_path).unwrap();

        let mut config = AppConfig::default_config();
        config.ai_provider = AiProviderConfig {
            llm_api: Some(ExternalApiEndpoint {
                endpoint: "https://api.openai.com/v1".to_string(),
                api_key: "sk-legacy".to_string(),
                model: Some("gpt-4.1-mini".to_string()),
                timeout_secs: 30,
                provider_type: AiProviderType::OpenAi,
                surface_id: None,
                credential: None,
            }),
            ..AiProviderConfig::default()
        };
        config_manager.update(config).unwrap();

        let secret_store = Arc::new(TestSecretStore::new()) as Arc<dyn SecretStore>;
        let changed = migrate_legacy_provider_api_keys(
            &config_manager,
            secret_store.clone(),
            CredentialBackendKind::OsSecretStore,
        )
        .await;
        let changed = changed.unwrap();

        assert!(changed);
        let migrated = config_manager.get();
        let endpoint = migrated.ai_provider.llm_api.expect("llm endpoint");
        assert_eq!(endpoint.api_key, "");
        let binding = endpoint.credential.expect("binding");
        assert_eq!(binding.backend_kind, CredentialBackendKind::OsSecretStore);
        let secret_ref = binding.secret_ref.expect("secret ref");
        assert_eq!(secret_ref.namespace, "provider/openai/llm");
        assert_eq!(
            secret_store
                .retrieve(&secret_ref.namespace, &secret_ref.key)
                .await
                .unwrap()
                .as_deref(),
            Some("sk-legacy")
        );
    }

    #[tokio::test]
    async fn migrate_legacy_provider_api_keys_is_noop_when_no_plaintext_exists() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let config_manager = ConfigManager::with_path(config_path).unwrap();
        let secret_store = Arc::new(TestSecretStore::new()) as Arc<dyn SecretStore>;

        let changed = migrate_legacy_provider_api_keys(
            &config_manager,
            secret_store,
            CredentialBackendKind::OsSecretStore,
        )
        .await
        .unwrap();

        assert!(!changed);
    }

    #[tokio::test]
    async fn migrate_legacy_provider_api_keys_preserves_selected_backend_kind() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let config_manager = ConfigManager::with_path(config_path).unwrap();

        let mut config = AppConfig::default_config();
        config.ai_provider = AiProviderConfig {
            llm_api: Some(ExternalApiEndpoint {
                endpoint: "https://api.openai.com/v1".to_string(),
                api_key: "sk-file-backend".to_string(),
                model: Some("gpt-4.1-mini".to_string()),
                timeout_secs: 30,
                provider_type: AiProviderType::OpenAi,
                surface_id: None,
                credential: None,
            }),
            ..AiProviderConfig::default()
        };
        config_manager.update(config).unwrap();

        let secret_store = Arc::new(TestSecretStore::new()) as Arc<dyn SecretStore>;
        let changed = migrate_legacy_provider_api_keys(
            &config_manager,
            secret_store,
            CredentialBackendKind::FileSecretStore,
        )
        .await
        .unwrap();

        assert!(changed);
        let migrated = config_manager.get();
        let binding = migrated
            .ai_provider
            .llm_api
            .and_then(|endpoint| endpoint.credential)
            .expect("binding");
        assert_eq!(binding.backend_kind, CredentialBackendKind::FileSecretStore);
    }
}
