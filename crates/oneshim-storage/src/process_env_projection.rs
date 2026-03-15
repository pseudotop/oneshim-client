//! Secret projection adapter for process-scoped environment variable injection.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::ports::secret_projection::{
    ProjectionTarget, ProjectionTemplate, SecretProjectionPort, SecretProjectionRequest,
    SecretProjectionResult,
};
use oneshim_core::ports::secret_store::SecretStore;

#[derive(Clone)]
pub struct ProcessEnvSecretProjection {
    secret_store: Arc<dyn SecretStore>,
    templates: Arc<HashMap<String, ProjectionTemplate>>,
}

impl ProcessEnvSecretProjection {
    pub fn new(
        secret_store: Arc<dyn SecretStore>,
        templates: impl IntoIterator<Item = ProjectionTemplate>,
    ) -> Self {
        let templates = templates
            .into_iter()
            .map(|template| (template.consumer_id.clone(), template))
            .collect();

        Self {
            secret_store,
            templates: Arc::new(templates),
        }
    }
}

#[async_trait]
impl SecretProjectionPort for ProcessEnvSecretProjection {
    async fn project(
        &self,
        request: SecretProjectionRequest,
    ) -> Result<SecretProjectionResult, CoreError> {
        if request.target != ProjectionTarget::ProcessEnv {
            return Err(CoreError::InvalidArguments(
                "process-env projection adapter only supports ProjectionTarget::ProcessEnv"
                    .to_string(),
            ));
        }

        let template = self.templates.get(&request.consumer_id).ok_or_else(|| {
            CoreError::Config(format!(
                "no process-env projection template registered for consumer '{}'",
                request.consumer_id
            ))
        })?;

        let secret = self
            .secret_store
            .retrieve(&request.namespace, &request.key)
            .await?
            .ok_or_else(|| {
                CoreError::Auth(format!(
                    "secret not found for projection request {}:{}",
                    request.namespace, request.key
                ))
            })?;

        let env_vars = template
            .env_names
            .iter()
            .cloned()
            .map(|name| (name, secret.clone()))
            .collect();

        Ok(SecretProjectionResult::EnvVars(env_vars))
    }

    async fn revoke_projection(&self, _consumer_id: &str) -> Result<(), CoreError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::ports::secret_projection::{ProjectionPurpose, SecretProjectionPort};
    use oneshim_core::ports::secret_store::SecretStore;
    use std::sync::Mutex;

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
    async fn project_process_env_maps_secret_to_template_env_names() {
        let secret_store = Arc::new(TestSecretStore::new());
        secret_store
            .store("provider/openai/llm", "api_key", "sk-projected")
            .await
            .unwrap();

        let adapter = ProcessEnvSecretProjection::new(
            secret_store,
            vec![ProjectionTemplate::process_env(
                "provider/openai/api-key-cli",
                vec![
                    "OPENAI_API_KEY".to_string(),
                    "ONESHIM_OPENAI_API_KEY".to_string(),
                ],
            )],
        );

        let result = adapter
            .project(SecretProjectionRequest {
                namespace: "provider/openai/llm".to_string(),
                key: "api_key".to_string(),
                target: ProjectionTarget::ProcessEnv,
                purpose: ProjectionPurpose::ProviderCliExecution,
                consumer_id: "provider/openai/api-key-cli".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(
            result,
            SecretProjectionResult::EnvVars(vec![
                ("OPENAI_API_KEY".to_string(), "sk-projected".to_string()),
                (
                    "ONESHIM_OPENAI_API_KEY".to_string(),
                    "sk-projected".to_string()
                ),
            ])
        );
    }

    #[tokio::test]
    async fn project_process_env_errors_when_template_missing() {
        let secret_store = Arc::new(TestSecretStore::new());
        let adapter =
            ProcessEnvSecretProjection::new(secret_store, Vec::<ProjectionTemplate>::new());

        let err = adapter
            .project(SecretProjectionRequest {
                namespace: "provider/openai/llm".to_string(),
                key: "api_key".to_string(),
                target: ProjectionTarget::ProcessEnv,
                purpose: ProjectionPurpose::ProviderCliExecution,
                consumer_id: "provider/openai/api-key-cli".to_string(),
            })
            .await
            .unwrap_err();

        assert!(matches!(err, CoreError::Config(_)));
    }
}
