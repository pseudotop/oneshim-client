//! CLI secret projection helpers.
//!
//! Explicit local-user surface for exporting provider credentials into
//! process-scoped environment variables for CLI compatibility.

use std::path::Path;

use oneshim_core::config::{CredentialAuthMode, CredentialBackendKind, ExternalApiEndpoint};
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::ports::credential_source::CredentialSource;
use oneshim_core::ports::secret_projection::{
    ProjectionPurpose, SecretProjectionPort, SecretProjectionRequest, SecretProjectionResult,
};
use oneshim_storage::process_env_projection::{
    provider_api_key_cli_template, ProcessEnvSecretProjection,
};

#[cfg(feature = "server")]
use crate::credential_migration::migrate_legacy_provider_api_keys;
#[cfg(feature = "server")]
use crate::provider_secret_backend::resolve_provider_secret_backend;
use crate::provider_secret_backend::{create_os_secret_store, create_secret_store_for_binding};

const CONFIG_FILE_NAME: &str = "config.json";

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
        Some("migrate") => cmd_migrate(config_dir),
        Some("env") => cmd_env(&args[1..], config_dir),
        Some("exec") => cmd_exec(&args[1..], config_dir),
        _ => {
            eprintln!("Usage: oneshim secret <migrate|env|exec> ...");
            eprintln!();
            eprintln!("Commands:");
            eprintln!(
                "  migrate             Move legacy plaintext provider API keys into the selected secret backend"
            );
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

#[cfg(feature = "server")]
fn cmd_migrate(config_dir: &Path) -> i32 {
    let config_manager = match ConfigManager::with_path(config_dir.join(CONFIG_FILE_NAME)) {
        Ok(manager) => manager,
        Err(err) => {
            eprintln!("Error: failed to load config: {err}");
            return 1;
        }
    };

    let desktop_secret_store = create_os_secret_store(config_dir);
    let resolution = match resolve_provider_secret_backend(config_dir, desktop_secret_store) {
        Ok(resolution) => resolution,
        Err(err) => {
            eprintln!("Error: failed to resolve provider secret backend: {err}");
            return 1;
        }
    };

    if let Err(message) = ensure_migration_backend_writable(resolution.backend_kind) {
        eprintln!("Error: {message}");
        return 1;
    }

    let Some(secret_store) = resolution.secret_store else {
        eprintln!("Error: selected writable provider backend is unavailable.");
        return 1;
    };

    let runtime = match build_runtime() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("Error: {err}");
            return 1;
        }
    };

    match runtime.block_on(migrate_legacy_provider_api_keys(
        &config_manager,
        secret_store,
        resolution.backend_kind,
    )) {
        Ok(true) => {
            println!(
                "migrated legacy provider API keys to {:?}",
                resolution.backend_kind
            );
            0
        }
        Ok(false) => {
            println!("no legacy provider API keys found");
            0
        }
        Err(err) => {
            eprintln!("Error: failed to migrate legacy provider API keys: {err}");
            1
        }
    }
}

#[cfg(not(feature = "server"))]
fn cmd_migrate(_config_dir: &Path) -> i32 {
    eprintln!("Error: secret migration requires the server feature.");
    1
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

    if let Err(message) = ensure_projection_allowed(endpoint, surface) {
        eprintln!("Error: {message}");
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

    if let Err(message) = ensure_projection_allowed(endpoint, surface) {
        eprintln!("Error: {message}");
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
    let desktop_secret_store = create_os_secret_store(config_dir);
    let secret_store = create_secret_store_for_binding(
        endpoint.credential.as_ref(),
        config_dir,
        desktop_secret_store,
    )
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

fn ensure_projection_allowed(
    endpoint: &ExternalApiEndpoint,
    surface: SecretSurface,
) -> Result<(), String> {
    let Some(binding) = endpoint.credential.as_ref() else {
        return Ok(());
    };

    if binding.auth_mode != CredentialAuthMode::ApiKey {
        return Err(format!(
            "{} is not configured for API-key projection. Current auth mode: {:?}",
            surface.profile_id(),
            binding.auth_mode
        ));
    }

    if matches!(
        binding.backend_kind,
        CredentialBackendKind::OsSecretStore
            | CredentialBackendKind::FileSecretStore
            | CredentialBackendKind::BridgeManaged
    ) && !binding.projection_enabled
    {
        return Err(format!(
            "{} secret projection is disabled. Enable CLI projection in Settings first.",
            surface.profile_id()
        ));
    }

    Ok(())
}

#[cfg(feature = "server")]
fn ensure_migration_backend_writable(backend_kind: CredentialBackendKind) -> Result<(), String> {
    if matches!(
        backend_kind,
        CredentialBackendKind::OsSecretStore | CredentialBackendKind::FileSecretStore
    ) {
        return Ok(());
    }

    Err(format!(
        "legacy credential migration requires a writable backend. Current backend: {:?}",
        backend_kind
    ))
}

fn build_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("failed to build CLI runtime: {err}"))
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
    use oneshim_core::config::{
        AiProviderConfig, AiProviderType, CredentialAuthMode, CredentialBackendKind,
        CredentialBinding,
    };
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

        let store = create_secret_store_for_binding(Some(&binding), temp_dir.path(), None).unwrap();
        assert!(store.is_some());
    }

    #[test]
    fn ensure_projection_allowed_rejects_disabled_backend_managed_projection() {
        let endpoint = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-4.1-mini".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: None,
                projection_enabled: false,
            }),
        };

        let error = ensure_projection_allowed(&endpoint, SecretSurface::Llm).unwrap_err();
        assert!(error.contains("Enable CLI projection"));
    }

    #[test]
    fn ensure_projection_allowed_accepts_enabled_backend_managed_projection() {
        let endpoint = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-4.1-mini".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::FileSecretStore,
                secret_ref: None,
                projection_enabled: true,
            }),
        };

        assert!(ensure_projection_allowed(&endpoint, SecretSurface::Llm).is_ok());
    }

    #[test]
    fn migration_backend_guard_rejects_non_writable_backend() {
        let error = ensure_migration_backend_writable(CredentialBackendKind::Env).unwrap_err();
        assert!(error.contains("requires a writable backend"));
        assert!(ensure_migration_backend_writable(CredentialBackendKind::OsSecretStore).is_ok());
    }

    #[test]
    fn ensure_projection_allowed_keeps_legacy_plaintext_compatibility() {
        let endpoint = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: Some("gpt-4.1-mini".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::LegacyConfig,
                secret_ref: None,
                projection_enabled: false,
            }),
        };

        assert!(ensure_projection_allowed(&endpoint, SecretSurface::Llm).is_ok());
    }
}
