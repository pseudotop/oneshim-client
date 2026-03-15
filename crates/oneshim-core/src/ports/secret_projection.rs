use std::path::PathBuf;

use async_trait::async_trait;

use crate::error::CoreError;

/// Projection target for compatibility output generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionTarget {
    ProcessEnv,
    TempFile,
    PersistentBridgeFile,
}

/// Why a projection is being created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionPurpose {
    ProviderCliExecution,
    ToolBridge,
    UserExportedCompatibilityArtifact,
}

/// Provider/tool-specific projection template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionTemplate {
    pub consumer_id: String,
    pub target: ProjectionTarget,
    pub env_names: Vec<String>,
    pub file_name_hint: Option<String>,
}

impl ProjectionTemplate {
    pub fn process_env(consumer_id: impl Into<String>, env_names: Vec<String>) -> Self {
        Self {
            consumer_id: consumer_id.into(),
            target: ProjectionTarget::ProcessEnv,
            env_names,
            file_name_hint: None,
        }
    }
}

/// Request to project a canonical secret into a compatibility target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretProjectionRequest {
    pub namespace: String,
    pub key: String,
    pub target: ProjectionTarget,
    pub purpose: ProjectionPurpose,
    pub consumer_id: String,
}

/// Result of projecting a secret for compatibility use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretProjectionResult {
    EnvVars(Vec<(String, String)>),
    TempFile {
        path: PathBuf,
        cleanup_required: bool,
    },
    PersistentFile {
        path: PathBuf,
    },
}

/// Compatibility projection port.
#[async_trait]
pub trait SecretProjectionPort: Send + Sync {
    async fn project(
        &self,
        request: SecretProjectionRequest,
    ) -> Result<SecretProjectionResult, CoreError>;

    async fn revoke_projection(&self, consumer_id: &str) -> Result<(), CoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_env_template_sets_target_and_names() {
        let template = ProjectionTemplate::process_env(
            "provider/openai/api-key-cli",
            vec!["OPENAI_API_KEY".to_string()],
        );

        assert_eq!(template.consumer_id, "provider/openai/api-key-cli");
        assert_eq!(template.target, ProjectionTarget::ProcessEnv);
        assert_eq!(template.env_names, vec!["OPENAI_API_KEY".to_string()]);
        assert!(template.file_name_hint.is_none());
    }
}
