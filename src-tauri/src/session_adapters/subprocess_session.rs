use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_stream::try_stream;
use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use tempfile::tempdir;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::timeout;
use uuid::Uuid;

use oneshim_api_contracts::provider_specs::{default_surface_model, provider_surface_spec};
use oneshim_core::config::AiSessionConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::ai_session::{
    ChatMessage, ChatRole, ConversationSessionInfo, OutboundMessage, SessionConfig, SessionMessage,
    SessionState, SessionTransport, ToolDefinition,
};
use oneshim_core::ports::conversation_session::{ConversationSession, ResponseStream};

use crate::session_adapters::prompt_payload::{render_conversation_prompt, render_message_payload};
use crate::subprocess_provider::{
    append_model_flag, append_oneshot_flags, classify_subprocess_error, DetectedSubprocessCli,
};

pub struct GenericSubprocessSession {
    session_id: String,
    surface: DetectedSubprocessCli,
    provider_name: String,
    model: String,
    system_prompt: Option<String>,
    default_tools: Option<Vec<ToolDefinition>>,
    history: Arc<RwLock<Vec<ChatMessage>>>,
    state: Mutex<SessionState>,
    turn_count: AtomicU32,
    created_at: chrono::DateTime<chrono::Utc>,
    last_active: Mutex<Instant>,
    timeout: Duration,
    max_history_turns: u32,
}

impl GenericSubprocessSession {
    pub fn new(
        surface: DetectedSubprocessCli,
        config: &SessionConfig,
        session_config: Arc<AiSessionConfig>,
        default_tools: Option<Vec<ToolDefinition>>,
    ) -> Self {
        let model = config
            .model
            .clone()
            .or_else(|| {
                default_surface_model(
                    &surface.surface_id,
                    oneshim_api_contracts::provider_specs::SurfaceCapabilityKind::Llm,
                )
                .ok()
                .flatten()
            })
            .unwrap_or_else(|| "gpt-5.4".to_string());

        let provider_name = provider_surface_spec(&surface.surface_id)
            .map(|spec| spec.vendor_id.clone())
            .unwrap_or_else(|_| "subprocess".to_string());

        Self {
            session_id: Uuid::new_v4().to_string(),
            surface,
            provider_name,
            model,
            system_prompt: config.system_prompt.clone(),
            default_tools,
            history: Arc::new(RwLock::new(Vec::new())),
            state: Mutex::new(SessionState::Active),
            turn_count: AtomicU32::new(0),
            created_at: Utc::now(),
            last_active: Mutex::new(Instant::now()),
            timeout: Duration::from_secs(session_config.session_timeout_secs),
            max_history_turns: session_config.max_history_turns,
        }
    }

    async fn invoke_surface(&self, prompt: &str) -> Result<String, CoreError> {
        if self
            .surface
            .surface_id
            .eq("provider_surface.openai.subprocess_cli")
        {
            self.run_codex(prompt).await
        } else if self
            .surface
            .surface_id
            .eq("provider_surface.google.subprocess_cli")
        {
            self.run_gemini(prompt).await
        } else {
            Err(CoreError::Internal(format!(
                "subprocess conversation sessions are not implemented for surface '{}'",
                self.surface.surface_id
            )))
        }
    }

    async fn run_codex(&self, prompt: &str) -> Result<String, CoreError> {
        let temp_dir = tempdir().map_err(|err| {
            CoreError::Internal(format!("Failed to create Codex session tempdir: {err}"))
        })?;

        let mut child = Command::new(&self.surface.executable_path);
        child
            .arg("exec")
            .arg("-C")
            .arg(temp_dir.path())
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        append_oneshot_flags(&mut child, &self.surface.surface_id);
        append_model_flag(&mut child, &self.surface.surface_id, &self.model);

        let mut child = child.spawn().map_err(|err| {
            CoreError::Internal(format!("Failed to spawn Codex session subprocess: {err}"))
        })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            CoreError::Internal("Failed to open stdin for Codex session subprocess".to_string())
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

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    async fn run_gemini(&self, prompt: &str) -> Result<String, CoreError> {
        let temp_dir = tempdir().map_err(|err| {
            CoreError::Internal(format!("Failed to create Gemini session tempdir: {err}"))
        })?;

        let mut command = Command::new(&self.surface.executable_path);
        command
            .arg("-p")
            .arg(prompt)
            .current_dir(temp_dir.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        append_oneshot_flags(&mut command, &self.surface.surface_id);
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

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[async_trait]
impl ConversationSession for GenericSubprocessSession {
    async fn send_message(&self, message: &SessionMessage) -> Result<ResponseStream, CoreError> {
        let rendered_user_message = render_message_payload(message, self.default_tools.as_deref());

        {
            let mut history = self.history.write().await;
            history.push(ChatMessage {
                role: ChatRole::User,
                content: rendered_user_message,
                content_blocks: None,
            });
        }

        let prompt = {
            let history = self.history.read().await;
            render_conversation_prompt(self.system_prompt.as_deref(), &history)
        };

        self.turn_count.fetch_add(1, Ordering::Relaxed);
        *self.last_active.lock() = Instant::now();

        let output = self.invoke_surface(&prompt).await.inspect_err(|_| {
            *self.state.lock() = SessionState::Failed;
        })?;
        let history = self.history.clone();
        let max_history_turns = self.max_history_turns;
        let provider_name = self.provider_name.clone();

        let stream: ResponseStream = Box::pin(try_stream! {
            if output.is_empty() {
                Err(CoreError::Internal(format!(
                    "{} CLI returned an empty session response",
                    provider_name
                )))?;
            }

            {
                let mut history = history.write().await;
                history.push(ChatMessage {
                    role: ChatRole::Assistant,
                    content: output.clone(),
                    content_blocks: None,
                });
                truncate_history(&mut history, max_history_turns);
            }

            yield OutboundMessage::Result {
                content: output,
                done: true,
                usage: None,
            };
        });

        Ok(stream)
    }

    fn info(&self) -> ConversationSessionInfo {
        let elapsed = self.last_active.lock().elapsed();
        let last_active_utc = Utc::now() - chrono::Duration::from_std(elapsed).unwrap_or_default();
        ConversationSessionInfo {
            session_id: self.session_id.clone(),
            provider_name: self.provider_name.clone(),
            model: self.model.clone(),
            state: *self.state.lock(),
            transport: SessionTransport::Subprocess,
            created_at: self.created_at,
            last_active: last_active_utc,
            turn_count: self.turn_count.load(Ordering::Relaxed),
        }
    }

    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn provider_name(&self) -> &str {
        &self.provider_name
    }
}

fn truncate_history(history: &mut Vec<ChatMessage>, max_turns: u32) {
    let max = max_turns as usize;
    if max == 0 || history.len() <= max {
        return;
    }

    let drain_count = history.len() - max;
    history.drain(0..drain_count);
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::models::ai_session::{MessageContext, MessageRole};
    use std::path::PathBuf;

    #[test]
    fn truncate_history_keeps_latest_turns_without_system_header() {
        let mut history = vec![
            ChatMessage {
                role: ChatRole::User,
                content: "one".to_string(),
                content_blocks: None,
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "two".to_string(),
                content_blocks: None,
            },
            ChatMessage {
                role: ChatRole::User,
                content: "three".to_string(),
                content_blocks: None,
            },
        ];

        truncate_history(&mut history, 2);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "two");
        assert_eq!(history[1].content, "three");
    }

    #[tokio::test]
    async fn session_prompt_includes_message_metadata() {
        let session = GenericSubprocessSession {
            session_id: "test".to_string(),
            surface: DetectedSubprocessCli {
                surface_id: "provider_surface.google.subprocess_cli".to_string(),
                executable_path: PathBuf::from("/usr/bin/false"),
            },
            provider_name: "google".to_string(),
            model: "gemini-2.5-pro".to_string(),
            system_prompt: Some("Be concise.".to_string()),
            default_tools: Some(vec![ToolDefinition {
                name: "search".to_string(),
                description: "Search".to_string(),
                endpoint: "http://localhost/api/search".to_string(),
                method: "GET".to_string(),
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": { "q": { "type": "string" } },
                    "required": ["q"],
                    "additionalProperties": false
                })),
            }]),
            history: Arc::new(RwLock::new(Vec::new())),
            state: Mutex::new(SessionState::Active),
            turn_count: AtomicU32::new(0),
            created_at: Utc::now(),
            last_active: Mutex::new(Instant::now()),
            timeout: Duration::from_secs(30),
            max_history_turns: 8,
        };

        let message = SessionMessage {
            role: MessageRole::User,
            content: "Summarize this".to_string(),
            attachments: vec![],
            tools: None,
            context: Some(MessageContext {
                regime: Some("focus".to_string()),
                active_app: Some("ONESHIM".to_string()),
            }),
            response_format: Some(serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "summary",
                    "schema": { "type": "object" }
                }
            })),
        };

        {
            let mut history = session.history.write().await;
            history.push(ChatMessage {
                role: ChatRole::User,
                content: render_message_payload(&message, session.default_tools.as_deref()),
                content_blocks: None,
            });
        }

        let prompt = {
            let history = session.history.read().await;
            render_conversation_prompt(session.system_prompt.as_deref(), &history)
        };

        assert!(prompt.contains("Available tools JSON"));
        assert!(prompt.contains("Required response format JSON"));
        assert!(prompt.contains("Additional context JSON"));
    }
}
