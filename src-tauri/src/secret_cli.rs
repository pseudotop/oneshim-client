//! CLI secret projection helpers.
//!
//! Explicit local-user surface for exporting provider credentials into
//! process-scoped environment variables for CLI compatibility.

use std::path::Path;
use std::sync::Arc;

use oneshim_core::config::{
    CredentialAuthMode, CredentialBackendKind, CredentialBinding, ExternalApiEndpoint,
};
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::ports::credential_source::CredentialSource;
use oneshim_core::ports::secret_projection::{
    ProjectionPurpose, SecretProjectionPort, SecretProjectionRequest, SecretProjectionResult,
};
use oneshim_core::ports::secret_store::SecretStore;
use oneshim_storage::env_secret_store::EnvSecretStore;
use oneshim_storage::file_secret_store::FileSecretStore;
use oneshim_storage::keychain::{KeychainOps, KeychainSecretStore};
use oneshim_storage::process_env_projection::{
    provider_api_key_cli_template, ProcessEnvSecretProjection,
};

const CONFIG_FILE_NAME: &str = "config.json";
const FILE_SECRET_STORE_NAME: &str = "oneshim-secrets.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretSurface {
    Llm,
    Ocr,
}

impl SecretSurface {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "llm" => Some(Self::Llm),
            "ocr" => Some(Self::Ocr),
            _ => None,
        }
    }

    fn profile_id(self) -> &'static str {
        match self {
            Self::Llm => "llm",
            Self::Ocr => "ocr",
        }
    }
}

pub fn run(args: &[String], config_dir: &Path) -> i32 {
    match args.first().map(String::as_str) {
        Some("env") => cmd_env(&args[1..], config_dir),
        Some("exec") => cmd_exec(&args[1..], config_dir),
        _ => {
            eprintln!("Usage: oneshim secret <env|exec> ...");
            eprintln!();
            eprintln!("Commands:");
            eprintln!(
                "  env <llm|ocr>       Emit shell export lines for the configured provider API key"
            );
            eprintln!(
                "  exec <llm|ocr> -- <command...>    Run a child command with projected provider credentials"
            );
            1
        }
    }
}

fn cmd_env(args: &[String], config_dir: &Path) -> i32 {
    let Some(surface) = args.first().and_then(|value| SecretSurface::parse(value)) else {
        eprintln!("Usage: oneshim secret env <llm|ocr>");
        return 1;
    };

    let config_manager = match ConfigManager::with_path(config_dir.join(CONFIG_FILE_NAME)) {
        Ok(manager) => manager,
        Err(err) => {
            eprintln!("Error: failed to load config: {err}");
            return 1;
        }
    };
    let config = config_manager.get();
    let Some(endpoint) = endpoint_for_surface(&config.ai_provider, surface) else {
        eprintln!(
            "Error: no {} provider endpoint is configured. Save a remote provider in Settings first.",
            surface.profile_id()
        );
        return 1;
    };

    let auth_mode = endpoint
        .credential
        .as_ref()
        .map(|binding| binding.auth_mode)
        .unwrap_or(CredentialAuthMode::ApiKey);
    if auth_mode != CredentialAuthMode::ApiKey {
        eprintln!(
            "Error: {} is not configured for API-key projection. Current auth mode: {:?}",
            surface.profile_id(),
            auth_mode
        );
        return 1;
    }

    let env_vars = match resolve_env_projection(endpoint, config_dir) {
        Ok(env_vars) => env_vars,
        Err(err) => {
            eprintln!("Error: {err}");
            return 1;
        }
    };

    for (name, value) in env_vars {
        println!("export {name}={}", shell_quote(&value));
    }

    0
}

fn cmd_exec(args: &[String], config_dir: &Path) -> i32 {
    let Ok((surface, command)) = parse_exec_args(args) else {
        eprintln!("Usage: oneshim secret exec <llm|ocr> -- <command...>");
        return 1;
    };

    let config_manager = match ConfigManager::with_path(config_dir.join(CONFIG_FILE_NAME)) {
        Ok(manager) => manager,
        Err(err) => {
            eprintln!("Error: failed to load config: {err}");
            return 1;
        }
    };
    let config = config_manager.get();
    let Some(endpoint) = endpoint_for_surface(&config.ai_provider, surface) else {
        eprintln!(
            "Error: no {} provider endpoint is configured. Save a remote provider in Settings first.",
            surface.profile_id()
        );
        return 1;
    };

    let auth_mode = endpoint
        .credential
        .as_ref()
        .map(|binding| binding.auth_mode)
        .unwrap_or(CredentialAuthMode::ApiKey);
    if auth_mode != CredentialAuthMode::ApiKey {
        eprintln!(
            "Error: {} is not configured for API-key projection. Current auth mode: {:?}",
            surface.profile_id(),
            auth_mode
        );
        return 1;
    }

    let env_vars = match resolve_env_projection(endpoint, config_dir) {
        Ok(env_vars) => env_vars,
        Err(err) => {
            eprintln!("Error: {err}");
            return 1;
        }
    };

    let mut child = std::process::Command::new(&command[0]);
    child.args(&command[1..]);
    child.envs(env_vars);

    match child.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            eprintln!("Error: failed to launch '{}': {err}", command[0]);
            1
        }
    }
}

fn endpoint_for_surface(
    config: &oneshim_core::config::AiProviderConfig,
    surface: SecretSurface,
) -> Option<&ExternalApiEndpoint> {
    match surface {
        SecretSurface::Llm => config.llm_api.as_ref(),
        SecretSurface::Ocr => config.ocr_api.as_ref(),
    }
}

fn resolve_env_projection(
    endpoint: &ExternalApiEndpoint,
    config_dir: &Path,
) -> Result<Vec<(String, String)>, String> {
    let template =
        provider_api_key_cli_template(endpoint.provider_type).map_err(|err| err.to_string())?;
    let secret_store = create_secret_store_for_endpoint(endpoint.credential.as_ref(), config_dir)
        .map_err(|err| err.to_string())?;
    let runtime = build_runtime()?;

    if let (Some(secret_store), Some(secret_ref)) = (
        secret_store.clone(),
        endpoint
            .credential
            .as_ref()
            .and_then(|binding| binding.secret_ref.clone()),
    ) {
        let projection =
            ProcessEnvSecretProjection::with_default_provider_api_key_cli_templates(secret_store);
        let request = SecretProjectionRequest {
            namespace: secret_ref.namespace,
            key: secret_ref.key,
            target: oneshim_core::ports::secret_projection::ProjectionTarget::ProcessEnv,
            purpose: ProjectionPurpose::ProviderCliExecution,
            consumer_id: template.consumer_id.clone(),
        };

        if let Ok(SecretProjectionResult::EnvVars(envs)) =
            runtime.block_on(projection.project(request))
        {
            return Ok(envs);
        }
    }

    let source = CredentialSource::from_api_key_endpoint(endpoint, secret_store)
        .map_err(|err| err.to_string())?;

    let resolved = runtime
        .block_on(source.resolve_bearer_token())
        .map_err(|err| err.to_string())?;

    Ok(template
        .env_names
        .into_iter()
        .map(|name| (name, resolved.clone()))
        .collect())
}

fn build_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("failed to build CLI runtime: {err}"))
}

fn create_secret_store_for_endpoint(
    binding: Option<&CredentialBinding>,
    config_dir: &Path,
) -> Result<Option<Arc<dyn SecretStore>>, oneshim_core::error::CoreError> {
    let Some(binding) = binding else {
        return Ok(create_desktop_secret_store(config_dir));
    };

    match binding.backend_kind {
        CredentialBackendKind::OsSecretStore => Ok(create_desktop_secret_store(config_dir)),
        CredentialBackendKind::FileSecretStore => Ok(Some(Arc::new(FileSecretStore::new(
            config_dir.join(FILE_SECRET_STORE_NAME),
        )?) as Arc<dyn SecretStore>)),
        CredentialBackendKind::Env => Ok(Some(
            Arc::new(EnvSecretStore::from_current_process()) as Arc<dyn SecretStore>
        )),
        CredentialBackendKind::BridgeManaged
        | CredentialBackendKind::LegacyConfig
        | CredentialBackendKind::Unavailable => Ok(None),
    }
}

fn create_desktop_secret_store(config_dir: &Path) -> Option<Arc<dyn SecretStore>> {
    let registry_path = config_dir.join("oneshim-keychain-registry.json");
    KeychainOps::new(registry_path)
        .ok()
        .map(|ops| Arc::new(KeychainSecretStore::new(Arc::new(ops))) as Arc<dyn SecretStore>)
}

fn shell_quote(value: &str) -> String {
    let escaped = value.replace('\'', r#"'\"'\"'"#);
    format!("'{escaped}'")
}

fn parse_exec_args(args: &[String]) -> Result<(SecretSurface, &[String]), ()> {
    let Some(surface) = args.first().and_then(|value| SecretSurface::parse(value)) else {
        return Err(());
    };

    if args.get(1).map(String::as_str) != Some("--") {
        return Err(());
    }

    let command = &args[2..];
    if command.is_empty() {
        return Err(());
    }

    Ok((surface, command))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::config::{AiProviderConfig, AiProviderType};
    use tempfile::TempDir;

    #[test]
    fn surface_parser_accepts_llm_and_ocr() {
        assert_eq!(SecretSurface::parse("llm"), Some(SecretSurface::Llm));
        assert_eq!(SecretSurface::parse("ocr"), Some(SecretSurface::Ocr));
        assert_eq!(SecretSurface::parse("other"), None);
    }

    #[test]
    fn endpoint_for_surface_returns_expected_endpoint() {
        let endpoint = ExternalApiEndpoint {
            endpoint: "https://api.example.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: Some("gpt-4.1-mini".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            credential: None,
        };
        let config = AiProviderConfig {
            llm_api: Some(endpoint.clone()),
            ocr_api: Some(endpoint),
            ..AiProviderConfig::default()
        };

        assert!(endpoint_for_surface(&config, SecretSurface::Llm).is_some());
        assert!(endpoint_for_surface(&config, SecretSurface::Ocr).is_some());
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("sk-test"), "'sk-test'");
        assert_eq!(shell_quote("a'b"), "'a'\\\"'\\\"'b'");
    }

    #[test]
    fn parse_exec_args_requires_surface_separator_and_command() {
        let args = vec!["llm".to_string(), "--".to_string(), "codex".to_string()];
        let (surface, command) = parse_exec_args(&args).unwrap();
        assert_eq!(surface, SecretSurface::Llm);
        assert_eq!(command, &["codex".to_string()]);

        assert!(parse_exec_args(&["llm".to_string()]).is_err());
        assert!(parse_exec_args(&["llm".to_string(), "codex".to_string()]).is_err());
        assert!(
            parse_exec_args(&["other".to_string(), "--".to_string(), "codex".to_string()]).is_err()
        );
    }

    #[test]
    fn config_manager_uses_explicit_config_dir() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ConfigManager::with_path(temp_dir.path().join(CONFIG_FILE_NAME)).unwrap();

        assert_eq!(
            manager.config_path(),
            &temp_dir.path().join(CONFIG_FILE_NAME)
        );
    }

    #[test]
    fn file_secret_store_path_is_config_relative() {
        let temp_dir = TempDir::new().unwrap();
        let binding = CredentialBinding {
            auth_mode: CredentialAuthMode::ApiKey,
            backend_kind: CredentialBackendKind::FileSecretStore,
            secret_ref: None,
            projection_enabled: false,
        };

        let store = create_secret_store_for_endpoint(Some(&binding), temp_dir.path()).unwrap();
        assert!(store.is_some());
    }
}
