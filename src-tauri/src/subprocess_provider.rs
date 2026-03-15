use async_trait::async_trait;
use oneshim_api_contracts::provider_specs::{
    list_subprocess_surface_specs, provider_surface_spec as catalog_surface_spec,
    subprocess_auth_probe_mode, subprocess_invocation_mode, subprocess_runtime_supported,
    subprocess_transport as catalog_subprocess_transport, SubprocessAuthProbeMode,
    SubprocessInvocationMode, SurfaceCapabilityKind,
};
use oneshim_core::config::{AiProviderConfig, AiProviderType};
use oneshim_core::error::CoreError;
use oneshim_core::ports::llm_provider::{
    InterpretedAction, LlmProvider, ScreenContext, SkillContext,
};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tempfile::tempdir;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

const DEFAULT_SUBPROCESS_TIMEOUT_SECS: u64 = 60;
const CLI_AUTH_PROBE_TIMEOUT_SECS: u64 = 2;
const ACTION_SCHEMA_JSON: &str = r#"{
  "type": "object",
  "properties": {
    "target_text": { "type": ["string", "null"] },
    "target_role": { "type": ["string", "null"] },
    "action_type": { "type": "string" },
    "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
  },
  "required": ["target_text", "target_role", "action_type", "confidence"],
  "additionalProperties": false
}"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedSubprocessCli {
    pub surface_id: String,
    pub executable_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubprocessCliAuthStatus {
    Authenticated,
    Unauthenticated,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedSubprocessCli {
    pub detected: DetectedSubprocessCli,
    pub auth_status: SubprocessCliAuthStatus,
    pub auth_detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SubprocessLlmProvider {
    surface: DetectedSubprocessCli,
    provider_name: String,
    model: String,
    timeout: Duration,
}

impl SubprocessLlmProvider {
    pub fn new(surface: DetectedSubprocessCli, config: &AiProviderConfig) -> Self {
        let model = config
            .llm_api
            .as_ref()
            .and_then(|endpoint| endpoint.model.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                default_llm_model_for_surface(&surface.surface_id)
                    .ok()
                    .flatten()
            })
            .unwrap_or_else(|| "gpt-5.4".to_string());
        let timeout_secs = config
            .llm_api
            .as_ref()
            .map(|endpoint| endpoint.timeout_secs)
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_SUBPROCESS_TIMEOUT_SECS);

        Self {
            provider_name: provider_name_for_surface_id(&surface.surface_id)
                .unwrap_or_else(|_| "subprocess-provider-cli".to_string()),
            surface,
            model,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    async fn invoke(
        &self,
        screen_context: &ScreenContext,
        intent_hint: &str,
        skill_ctx: &SkillContext,
    ) -> Result<InterpretedAction, CoreError> {
        let prompt = build_intent_prompt(screen_context, intent_hint, skill_ctx)?;
        let raw = match invocation_mode_for_surface(&self.surface.surface_id)? {
            SubprocessInvocationMode::CodexExecJson => self.run_codex(&prompt).await?,
            SubprocessInvocationMode::ClaudePrintJson => self.run_claude(&prompt).await?,
            SubprocessInvocationMode::GeminiCliPrompt => self.run_gemini(&prompt).await?,
        };

        parse_interpreted_action_output(&raw)
    }

    async fn run_codex(&self, prompt: &str) -> Result<String, CoreError> {
        let temp_dir = tempdir().map_err(|err| {
            CoreError::Internal(format!("Failed to create Codex subprocess tempdir: {err}"))
        })?;
        let schema_path = temp_dir.path().join("action.schema.json");
        let output_path = temp_dir.path().join("codex-output.json");
        std::fs::write(&schema_path, ACTION_SCHEMA_JSON).map_err(|err| {
            CoreError::Internal(format!("Failed to write Codex output schema: {err}"))
        })?;

        let mut child = Command::new(&self.surface.executable_path);
        child
            .arg("exec")
            .arg("--sandbox")
            .arg("read-only")
            .arg("--skip-git-repo-check")
            .arg("--color")
            .arg("never")
            .arg("-C")
            .arg(temp_dir.path())
            .arg("--output-schema")
            .arg(&schema_path)
            .arg("--output-last-message")
            .arg(&output_path)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        append_model_flag(&mut child, &self.surface.surface_id, &self.model);

        let mut child = child.spawn().map_err(|err| {
            CoreError::Internal(format!("Failed to spawn Codex CLI subprocess: {err}"))
        })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            CoreError::Internal("Failed to open stdin for Codex CLI subprocess".to_string())
        })?;
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(CoreError::Io)?;
        drop(stdin);

        let output = timeout(self.timeout, child.wait_with_output())
            .await
            .map_err(|_| CoreError::RequestTimeout {
                timeout_ms: self.timeout.as_millis() as u64,
            })?
            .map_err(CoreError::Io)?;

        if !output.status.success() {
            return Err(classify_subprocess_error(
                &self.surface.surface_id,
                &String::from_utf8_lossy(&output.stderr),
            ));
        }

        if let Ok(rendered) = std::fs::read_to_string(&output_path) {
            if !rendered.trim().is_empty() {
                return Ok(rendered);
            }
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn run_claude(&self, prompt: &str) -> Result<String, CoreError> {
        let temp_dir = tempdir().map_err(|err| {
            CoreError::Internal(format!("Failed to create Claude subprocess tempdir: {err}"))
        })?;

        let mut command = Command::new(&self.surface.executable_path);
        command
            .arg("-p")
            .arg("--permission-mode")
            .arg("dontAsk")
            .arg("--tools")
            .arg("")
            .arg("--no-session-persistence")
            .arg("--output-format")
            .arg("text")
            .arg("--json-schema")
            .arg(ACTION_SCHEMA_JSON)
            .arg(prompt)
            .current_dir(temp_dir.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        append_model_flag(&mut command, &self.surface.surface_id, &self.model);

        let output = timeout(self.timeout, command.output())
            .await
            .map_err(|_| CoreError::RequestTimeout {
                timeout_ms: self.timeout.as_millis() as u64,
            })?
            .map_err(CoreError::Io)?;

        if !output.status.success() {
            return Err(classify_subprocess_error(
                &self.surface.surface_id,
                &String::from_utf8_lossy(&output.stderr),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn run_gemini(&self, prompt: &str) -> Result<String, CoreError> {
        let temp_dir = tempdir().map_err(|err| {
            CoreError::Internal(format!("Failed to create Gemini subprocess tempdir: {err}"))
        })?;

        let mut command = Command::new(&self.surface.executable_path);
        command
            .arg("-p")
            .arg(prompt)
            .current_dir(temp_dir.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        append_model_flag(&mut command, &self.surface.surface_id, &self.model);

        let output = timeout(self.timeout, command.output())
            .await
            .map_err(|_| CoreError::RequestTimeout {
                timeout_ms: self.timeout.as_millis() as u64,
            })?
            .map_err(CoreError::Io)?;

        if !output.status.success() {
            return Err(classify_subprocess_error(
                &self.surface.surface_id,
                &String::from_utf8_lossy(&output.stderr),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[async_trait]
impl LlmProvider for SubprocessLlmProvider {
    async fn interpret_intent(
        &self,
        screen_context: &ScreenContext,
        intent_hint: &str,
    ) -> Result<InterpretedAction, CoreError> {
        self.invoke(screen_context, intent_hint, &SkillContext::default())
            .await
    }

    async fn interpret_intent_with_skills(
        &self,
        screen_context: &ScreenContext,
        intent_hint: &str,
        skill_ctx: &SkillContext,
    ) -> Result<InterpretedAction, CoreError> {
        self.invoke(screen_context, intent_hint, skill_ctx).await
    }

    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn is_external(&self) -> bool {
        true
    }
}

fn default_llm_model_for_surface(surface_id: &str) -> Result<Option<String>, String> {
    oneshim_api_contracts::provider_specs::default_surface_model(
        surface_id,
        SurfaceCapabilityKind::Llm,
    )
}

fn provider_name_for_surface_id(surface_id: &str) -> Result<String, String> {
    Ok(format!("subprocess-{}", cli_id_for_surface_id(surface_id)?))
}

pub(crate) fn cli_id_for_surface_id(surface_id: &str) -> Result<String, String> {
    Ok(catalog_subprocess_transport(surface_id)?.tool_id.clone())
}

fn invocation_mode_for_surface(surface_id: &str) -> Result<SubprocessInvocationMode, CoreError> {
    subprocess_invocation_mode(surface_id).map_err(CoreError::Internal)
}

pub(crate) fn runtime_supported_for_surface(surface_id: &str) -> bool {
    subprocess_runtime_supported(surface_id).unwrap_or(false)
}

fn auth_probe_mode_for_surface(surface_id: &str) -> Result<SubprocessAuthProbeMode, String> {
    subprocess_auth_probe_mode(surface_id)
}

fn auth_probe_command_for_surface(surface_id: &str) -> Result<Vec<String>, String> {
    Ok(catalog_subprocess_transport(surface_id)?
        .auth_probe_command
        .clone())
}

fn append_model_flag(command: &mut Command, surface_id: &str, model: &str) {
    if let Ok(transport) = catalog_subprocess_transport(surface_id) {
        if let Some(flag) = transport
            .model_flag
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            command.arg(flag).arg(model);
        }
    }
}

pub fn detect_known_cli_surfaces() -> Vec<DetectedSubprocessCli> {
    list_subprocess_surface_specs()
        .unwrap_or_default()
        .into_iter()
        .filter(|surface| surface.supports.llm)
        .filter_map(|surface| {
            let transport = catalog_subprocess_transport(&surface.surface_id).ok()?;
            transport
                .executable_candidates
                .iter()
                .find_map(|candidate| find_executable(candidate))
                .map(|executable_path| DetectedSubprocessCli {
                    surface_id: surface.surface_id.clone(),
                    executable_path,
                })
        })
        .collect()
}

pub fn probe_known_cli_surfaces() -> Vec<ProbedSubprocessCli> {
    detect_known_cli_surfaces()
        .into_iter()
        .map(probe_cli_surface)
        .collect()
}

pub fn select_cli_surface_for_config(
    config: &AiProviderConfig,
    detected: &[ProbedSubprocessCli],
) -> Option<DetectedSubprocessCli> {
    if let Some(surface_id) = preferred_cli_surface_for_config(config) {
        return detected
            .iter()
            .find(|surface| {
                surface
                    .detected
                    .surface_id
                    .eq_ignore_ascii_case(&surface_id)
                    && runtime_supported_for_surface(&surface.detected.surface_id)
                    && surface.auth_status == SubprocessCliAuthStatus::Authenticated
            })
            .map(|surface| surface.detected.clone());
    }

    detected
        .iter()
        .find(|surface| {
            runtime_supported_for_surface(&surface.detected.surface_id)
                && surface.auth_status == SubprocessCliAuthStatus::Authenticated
        })
        .map(|surface| surface.detected.clone())
}

pub fn preferred_cli_surface_for_config(config: &AiProviderConfig) -> Option<String> {
    config
        .llm_api
        .as_ref()
        .and_then(|endpoint| {
            endpoint
                .surface_id
                .as_deref()
                .and_then(surface_for_provider_surface_id)
                .filter(|surface_id| {
                    endpoint.provider_type == AiProviderType::Generic
                        || surface_for_provider_type(endpoint.provider_type).as_deref()
                            == Some(surface_id.as_str())
                })
        })
        .or_else(|| {
            config
                .llm_api
                .as_ref()
                .map(|endpoint| endpoint.provider_type)
                .filter(|provider_type| *provider_type != AiProviderType::Generic)
                .and_then(surface_for_provider_type)
        })
}

pub fn probe_for_surface_id<'a>(
    probed: &'a [ProbedSubprocessCli],
    surface_id: &str,
) -> Option<&'a ProbedSubprocessCli> {
    probed
        .iter()
        .find(|surface| surface.detected.surface_id.eq_ignore_ascii_case(surface_id))
}

fn surface_for_provider_type(provider_type: AiProviderType) -> Option<String> {
    let provider_label = match provider_type {
        AiProviderType::Anthropic => "Anthropic",
        AiProviderType::OpenAi => "OpenAi",
        AiProviderType::Google => "Google",
        AiProviderType::Generic => return None,
    };

    list_subprocess_surface_specs()
        .ok()?
        .into_iter()
        .filter(|surface| surface.provider_type.eq_ignore_ascii_case(provider_label))
        .max_by_key(|surface| surface.preferred_for_product_auth)
        .map(|surface| surface.surface_id.clone())
}

fn surface_for_provider_surface_id(raw: &str) -> Option<String> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return None;
    }
    catalog_surface_spec(normalized).ok().and_then(|surface| {
        catalog_subprocess_transport(&surface.surface_id)
            .ok()
            .map(|_| surface.surface_id.clone())
    })
}

fn probe_cli_surface(detected: DetectedSubprocessCli) -> ProbedSubprocessCli {
    let (auth_status, auth_detail) = match auth_probe_mode_for_surface(&detected.surface_id) {
        Ok(SubprocessAuthProbeMode::CodexLoginStatusText) => {
            let probe_args =
                auth_probe_command_for_surface(&detected.surface_id).unwrap_or_default();
            probe_codex_auth_status(&detected.executable_path, &probe_args)
        }
        Ok(SubprocessAuthProbeMode::ClaudeAuthStatusJson) => {
            let probe_args =
                auth_probe_command_for_surface(&detected.surface_id).unwrap_or_default();
            probe_claude_auth_status(&detected.executable_path, &probe_args)
        }
        Ok(SubprocessAuthProbeMode::None) => (
            SubprocessCliAuthStatus::Unknown,
            Some("auth_status_probe_not_implemented".to_string()),
        ),
        Err(error) => (
            SubprocessCliAuthStatus::Unknown,
            Some(format!("probe_spec_error:{error}")),
        ),
    };

    ProbedSubprocessCli {
        detected,
        auth_status,
        auth_detail,
    }
}

fn probe_codex_auth_status(
    executable_path: &Path,
    args: &[String],
) -> (SubprocessCliAuthStatus, Option<String>) {
    let output = match run_probe_command_with_timeout(
        executable_path,
        args,
        Duration::from_secs(CLI_AUTH_PROBE_TIMEOUT_SECS),
    ) {
        Ok(output) => output,
        Err(detail) => return (SubprocessCliAuthStatus::Unknown, Some(detail)),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    parse_codex_auth_status(&combined)
}

fn parse_codex_auth_status(raw: &str) -> (SubprocessCliAuthStatus, Option<String>) {
    let normalized = raw.trim();
    let lowered = normalized.to_ascii_lowercase();
    if lowered.contains("not logged in") || lowered.contains("login required") {
        return (
            SubprocessCliAuthStatus::Unauthenticated,
            Some("cli_auth_required".to_string()),
        );
    }

    if lowered.starts_with("logged in") || lowered.contains("logged in using") {
        return (
            SubprocessCliAuthStatus::Authenticated,
            Some("cli_authenticated".to_string()),
        );
    }

    (
        SubprocessCliAuthStatus::Unknown,
        Some(format!(
            "unexpected_status_output:{}",
            truncate_for_error(normalized)
        )),
    )
}

fn probe_claude_auth_status(
    executable_path: &Path,
    args: &[String],
) -> (SubprocessCliAuthStatus, Option<String>) {
    let output = match run_probe_command_with_timeout(
        executable_path,
        args,
        Duration::from_secs(CLI_AUTH_PROBE_TIMEOUT_SECS),
    ) {
        Ok(output) => output,
        Err(detail) => return (SubprocessCliAuthStatus::Unknown, Some(detail)),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_claude_auth_status(&stdout)
}

fn parse_claude_auth_status(raw: &str) -> (SubprocessCliAuthStatus, Option<String>) {
    let normalized = raw.trim();
    let value = match serde_json::from_str::<serde_json::Value>(normalized) {
        Ok(value) => value,
        Err(err) => {
            return (
                SubprocessCliAuthStatus::Unknown,
                Some(format!("invalid_status_json:{err}")),
            );
        }
    };

    match value.get("loggedIn").and_then(|value| value.as_bool()) {
        Some(true) => (
            SubprocessCliAuthStatus::Authenticated,
            Some("cli_authenticated".to_string()),
        ),
        Some(false) => (
            SubprocessCliAuthStatus::Unauthenticated,
            Some("cli_auth_required".to_string()),
        ),
        None => (
            SubprocessCliAuthStatus::Unknown,
            Some("missing_loggedIn_field".to_string()),
        ),
    }
}

fn run_probe_command_with_timeout(
    executable_path: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let mut child = StdCommand::new(executable_path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("probe_failed:{err}"))?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|err| format!("probe_failed:{err}"));
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("probe_timeout:{}ms", timeout.as_millis()));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("probe_failed:{err}"));
            }
        }
    }
}

fn build_intent_prompt(
    screen_context: &ScreenContext,
    intent_hint: &str,
    skill_ctx: &SkillContext,
) -> Result<String, CoreError> {
    let screen_context_json = serde_json::to_string_pretty(screen_context)?;
    let available_skills = if skill_ctx.available_skills.is_empty() {
        "[]".to_string()
    } else {
        serde_json::to_string(
            &skill_ctx
                .available_skills
                .iter()
                .map(|skill| skill.name.clone())
                .collect::<Vec<_>>(),
        )?
    };
    let active_skill = skill_ctx
        .active_skill_body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("(none)");

    Ok(format!(
        "You are ONESHIM's subprocess-backed UI intent planner.\n\
Return only compact JSON matching this schema:\n{schema}\n\n\
Rules:\n\
- action_type must be one of: click, type, hotkey, wait, activate.\n\
- confidence must be a number between 0.0 and 1.0.\n\
- target_text should be the visible text to target when known, otherwise null.\n\
- target_role should be a concise accessibility-style role when known, otherwise null.\n\
- Do not include markdown, commentary, or code fences.\n\n\
Available skill names: {available_skills}\n\
Active skill body:\n{active_skill}\n\n\
Intent hint:\n{intent_hint}\n\n\
Screen context JSON:\n{screen_context_json}",
        schema = ACTION_SCHEMA_JSON,
        available_skills = available_skills,
        active_skill = active_skill,
        intent_hint = intent_hint.trim(),
        screen_context_json = screen_context_json
    ))
}

fn parse_interpreted_action_output(raw: &str) -> Result<InterpretedAction, CoreError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(CoreError::Internal(
            "Subprocess CLI returned an empty response.".to_string(),
        ));
    }

    if let Ok(action) = serde_json::from_str::<InterpretedAction>(normalized) {
        return Ok(clamp_confidence(action));
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(normalized) {
        if let Some(action) = parse_interpreted_action_value(&value) {
            return Ok(clamp_confidence(action));
        }
    }

    if let Some(fragment) = extract_json_object_fragment(normalized) {
        if let Ok(action) = serde_json::from_str::<InterpretedAction>(&fragment) {
            return Ok(clamp_confidence(action));
        }
    }

    Err(CoreError::Internal(format!(
        "Subprocess CLI returned non-JSON intent output: {}",
        truncate_for_error(normalized)
    )))
}

fn parse_interpreted_action_value(value: &serde_json::Value) -> Option<InterpretedAction> {
    if let Ok(action) = serde_json::from_value::<InterpretedAction>(value.clone()) {
        return Some(action);
    }

    match value {
        serde_json::Value::Object(map) => {
            for key in ["result", "response", "content", "message"] {
                let nested = map.get(key)?;
                if let Some(action) = parse_interpreted_action_value(nested) {
                    return Some(action);
                }
            }
            None
        }
        serde_json::Value::String(text) => serde_json::from_str::<InterpretedAction>(text).ok(),
        serde_json::Value::Array(items) => items.iter().find_map(parse_interpreted_action_value),
        _ => None,
    }
}

fn clamp_confidence(mut action: InterpretedAction) -> InterpretedAction {
    action.confidence = action.confidence.clamp(0.0, 1.0);
    action
}

fn extract_json_object_fragment(raw: &str) -> Option<String> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(raw[start..=end].to_string())
}

fn truncate_for_error(value: &str) -> String {
    const MAX_LEN: usize = 240;
    if value.chars().count() <= MAX_LEN {
        return value.to_string();
    }
    let truncated: String = value.chars().take(MAX_LEN).collect();
    format!("{truncated}...")
}

fn classify_subprocess_error(surface_id: &str, stderr: &str) -> CoreError {
    let normalized = stderr.trim();
    let lowered = normalized.to_ascii_lowercase();
    let cli_id = cli_id_for_surface_id(surface_id).unwrap_or_else(|_| surface_id.to_string());
    if lowered.contains("login")
        || lowered.contains("auth")
        || lowered.contains("sign in")
        || lowered.contains("not authenticated")
    {
        return CoreError::Auth(format!(
            "{} CLI authentication is required: {}",
            cli_id,
            truncate_for_error(normalized)
        ));
    }

    CoreError::Internal(format!(
        "{} CLI invocation failed: {}",
        cli_id,
        truncate_for_error(normalized)
    ))
}

fn find_executable(name: &str) -> Option<PathBuf> {
    if name.contains(std::path::MAIN_SEPARATOR) {
        let path = PathBuf::from(name);
        return is_executable(&path).then_some(path);
    }

    let path_var = env::var_os("PATH")?;
    #[cfg(windows)]
    let exts: Vec<String> = env::var_os("PATHEXT")
        .map(|value| {
            env::split_paths(&PathBuf::from(value))
                .map(|path| path.to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_else(|| {
            vec![
                ".COM".to_string(),
                ".EXE".to_string(),
                ".BAT".to_string(),
                ".CMD".to_string(),
            ]
        });

    for dir in env::split_paths(&path_var) {
        let base = dir.join(name);
        if is_executable(&base) {
            return Some(base);
        }
        #[cfg(windows)]
        {
            for ext in &exts {
                let candidate = dir.join(format!("{name}{ext}"));
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            return metadata.permissions().mode() & 0o111 != 0;
        }
        false
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn endpoint(
        provider_type: AiProviderType,
        model: Option<&str>,
    ) -> oneshim_core::config::ExternalApiEndpoint {
        oneshim_core::config::ExternalApiEndpoint {
            endpoint: "https://example.invalid".to_string(),
            api_key: String::new(),
            model: model.map(|value| value.to_string()),
            timeout_secs: 30,
            provider_type,
            surface_id: None,
            credential: None,
        }
    }

    fn probed(surface_id: &str, auth_status: SubprocessCliAuthStatus) -> ProbedSubprocessCli {
        ProbedSubprocessCli {
            detected: DetectedSubprocessCli {
                surface_id: surface_id.to_string(),
                executable_path: PathBuf::from(format!(
                    "/tmp/{}",
                    cli_id_for_surface_id(surface_id).unwrap_or_else(|_| surface_id.to_string())
                )),
            },
            auth_status,
            auth_detail: None,
        }
    }

    #[test]
    fn selects_provider_matching_surface_when_available() {
        let config = AiProviderConfig {
            llm_api: Some(endpoint(AiProviderType::Anthropic, None)),
            ..AiProviderConfig::default()
        };
        let surfaces = vec![
            probed(
                "provider_surface.openai.subprocess_cli",
                SubprocessCliAuthStatus::Authenticated,
            ),
            probed(
                "provider_surface.anthropic.subprocess_cli",
                SubprocessCliAuthStatus::Authenticated,
            ),
        ];

        let resolved = select_cli_surface_for_config(&config, &surfaces).unwrap();
        assert_eq!(
            resolved.surface_id,
            "provider_surface.anthropic.subprocess_cli"
        );
    }

    #[test]
    fn falls_back_to_first_runtime_supported_surface() {
        let config = AiProviderConfig::default();
        let surfaces = vec![
            probed(
                "provider_surface.google.subprocess_cli",
                SubprocessCliAuthStatus::Authenticated,
            ),
            probed(
                "provider_surface.openai.subprocess_cli",
                SubprocessCliAuthStatus::Authenticated,
            ),
        ];

        let resolved = select_cli_surface_for_config(&config, &surfaces).unwrap();
        assert_eq!(
            resolved.surface_id,
            "provider_surface.google.subprocess_cli"
        );
    }

    #[test]
    fn does_not_switch_to_a_different_vendor_when_matching_surface_requires_auth() {
        let config = AiProviderConfig {
            llm_api: Some(endpoint(AiProviderType::OpenAi, None)),
            ..AiProviderConfig::default()
        };
        let surfaces = vec![
            probed(
                "provider_surface.openai.subprocess_cli",
                SubprocessCliAuthStatus::Unauthenticated,
            ),
            probed(
                "provider_surface.anthropic.subprocess_cli",
                SubprocessCliAuthStatus::Authenticated,
            ),
        ];

        let resolved = select_cli_surface_for_config(&config, &surfaces);
        assert!(resolved.is_none());
    }

    #[test]
    fn prefers_explicit_surface_id_when_provider_type_is_generic() {
        let mut llm_endpoint = endpoint(AiProviderType::Generic, None);
        llm_endpoint.surface_id = Some("provider_surface.anthropic.subprocess_cli".to_string());
        let config = AiProviderConfig {
            llm_api: Some(llm_endpoint),
            ..AiProviderConfig::default()
        };
        let surfaces = vec![
            probed(
                "provider_surface.openai.subprocess_cli",
                SubprocessCliAuthStatus::Authenticated,
            ),
            probed(
                "provider_surface.anthropic.subprocess_cli",
                SubprocessCliAuthStatus::Authenticated,
            ),
        ];

        let resolved = select_cli_surface_for_config(&config, &surfaces).unwrap();
        assert_eq!(
            resolved.surface_id,
            "provider_surface.anthropic.subprocess_cli"
        );
    }

    #[test]
    fn ignores_explicit_surface_id_when_it_conflicts_with_provider_type() {
        let mut llm_endpoint = endpoint(AiProviderType::OpenAi, None);
        llm_endpoint.surface_id = Some("provider_surface.anthropic.subprocess_cli".to_string());
        let config = AiProviderConfig {
            llm_api: Some(llm_endpoint),
            ..AiProviderConfig::default()
        };
        let surfaces = vec![
            probed(
                "provider_surface.openai.subprocess_cli",
                SubprocessCliAuthStatus::Authenticated,
            ),
            probed(
                "provider_surface.anthropic.subprocess_cli",
                SubprocessCliAuthStatus::Authenticated,
            ),
        ];

        let resolved = select_cli_surface_for_config(&config, &surfaces).unwrap();
        assert_eq!(
            resolved.surface_id,
            "provider_surface.openai.subprocess_cli"
        );
    }

    #[test]
    fn parses_codex_logged_in_status() {
        let (status, detail) = parse_codex_auth_status("Logged in using ChatGPT");
        assert_eq!(status, SubprocessCliAuthStatus::Authenticated);
        assert_eq!(detail.as_deref(), Some("cli_authenticated"));
    }

    #[test]
    fn parses_claude_logged_out_status_json() {
        let (status, detail) =
            parse_claude_auth_status(r#"{"loggedIn":false,"authMethod":"none"}"#);
        assert_eq!(status, SubprocessCliAuthStatus::Unauthenticated);
        assert_eq!(detail.as_deref(), Some("cli_auth_required"));
    }

    #[test]
    fn parses_direct_json_output() {
        let raw = r#"{"target_text":"Save","target_role":"button","action_type":"click","confidence":1.4}"#;
        let action = parse_interpreted_action_output(raw).unwrap();
        assert_eq!(action.target_text.as_deref(), Some("Save"));
        assert_eq!(action.target_role.as_deref(), Some("button"));
        assert_eq!(action.action_type, "click");
        assert_eq!(action.confidence, 1.0);
    }

    #[test]
    fn parses_nested_json_payload() {
        let raw = json!({
            "result": {
                "target_text": "Search",
                "target_role": "input",
                "action_type": "type",
                "confidence": 0.82
            }
        })
        .to_string();
        let action = parse_interpreted_action_output(&raw).unwrap();
        assert_eq!(action.target_text.as_deref(), Some("Search"));
        assert_eq!(action.action_type, "type");
    }

    #[test]
    fn builds_prompt_with_screen_context_and_skills() {
        let prompt = build_intent_prompt(
            &ScreenContext {
                visible_texts: vec!["Save".to_string()],
                active_app: "Editor".to_string(),
                active_window_title: "main.rs".to_string(),
                layout_description: Some("toolbar".to_string()),
            },
            "click save",
            &SkillContext::default(),
        )
        .unwrap();

        assert!(prompt.contains("click save"));
        assert!(prompt.contains("\"active_app\": \"Editor\""));
        assert!(prompt.contains("\"action_type\""));
    }
}
