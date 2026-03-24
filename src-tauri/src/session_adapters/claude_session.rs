//! Claude subprocess session -- serial `-p` calls with `--session-id`/`--continue`.

use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use uuid::Uuid;

use oneshim_core::config::AiSessionConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::ai_session::{
    ConversationSessionInfo, OutboundMessage, SessionConfig, SessionMessage, SessionState,
    SessionTransport,
};
use oneshim_core::ports::conversation_session::{ConversationSession, ResponseStream};

use crate::session_adapters::claude_normalizer::normalize_claude_stream_event;
use crate::subprocess_provider::DetectedSubprocessCli;

#[allow(dead_code)]
pub struct ClaudeSubprocessSession {
    session_id: String,
    cli_session_id: String,
    surface: DetectedSubprocessCli,
    model: String,
    system_prompt: Option<String>,
    turn_count: AtomicU32,
    created_at: chrono::DateTime<chrono::Utc>,
    last_active: Mutex<Instant>,
    config: Arc<AiSessionConfig>,
}

#[allow(dead_code)]
impl ClaudeSubprocessSession {
    pub fn new(
        surface: DetectedSubprocessCli,
        config: &SessionConfig,
        session_config: Arc<AiSessionConfig>,
    ) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            cli_session_id: Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            surface,
            model: config.model.clone().unwrap_or_else(|| "sonnet".to_string()),
            system_prompt: config.system_prompt.clone(),
            turn_count: AtomicU32::new(0),
            last_active: Mutex::new(Instant::now()),
            config: session_config,
        }
    }

    fn build_command(&self, prompt: &str) -> Command {
        let mut cmd = Command::new(&self.surface.executable_path);
        cmd.arg("-p");
        cmd.arg("--output-format").arg("stream-json");
        cmd.arg("--bare");
        cmd.arg("--permission-mode").arg("dontAsk");
        cmd.arg("--model").arg(&self.model);
        cmd.arg("--session-id").arg(&self.cli_session_id);

        let turn = self.turn_count.load(Ordering::Relaxed);
        if turn > 0 {
            cmd.arg("--continue");
        }

        if turn == 0 {
            if let Some(ref sp) = self.system_prompt {
                cmd.arg("--system-prompt").arg(sp);
            }
        }

        cmd.arg(prompt);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);
        cmd
    }
}

#[async_trait]
impl ConversationSession for ClaudeSubprocessSession {
    async fn send_message(&self, message: &SessionMessage) -> Result<ResponseStream, CoreError> {
        let mut cmd = self.build_command(&message.content);

        let mut child = cmd.spawn().map_err(|err| {
            CoreError::Internal(format!("Failed to spawn Claude session subprocess: {err}"))
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            CoreError::Internal("Failed to capture Claude session stdout".to_string())
        })?;

        self.turn_count.fetch_add(1, Ordering::Relaxed);
        *self.last_active.lock() = Instant::now();

        let timeout_secs = self.config.session_timeout_secs;
        let reader = tokio::io::BufReader::new(stdout);

        let stream = async_stream::try_stream! {
            let mut lines = reader.lines();
            let deadline = tokio::time::Instant::now()
                + tokio::time::Duration::from_secs(timeout_secs);

            loop {
                let line_result = tokio::time::timeout_at(deadline, lines.next_line()).await;
                match line_result {
                    Ok(Ok(Some(line))) => {
                        if let Some(msg) = normalize_claude_stream_event(&line) {
                            yield msg;
                        }
                    }
                    Ok(Ok(None)) => break, // EOF
                    Ok(Err(err)) => {
                        yield OutboundMessage::Error {
                            code: "io_error".to_string(),
                            message: err.to_string(),
                            retryable: false,
                        };
                        break;
                    }
                    Err(_) => {
                        yield OutboundMessage::Error {
                            code: "timeout".to_string(),
                            message: format!("Session response timeout ({timeout_secs}s)"),
                            retryable: true,
                        };
                        break;
                    }
                }
            }

            // Wait for process exit
            let _ = child.wait().await;
        };

        Ok(Box::pin(stream))
    }

    fn info(&self) -> ConversationSessionInfo {
        ConversationSessionInfo {
            session_id: self.session_id.clone(),
            provider_name: "claude".to_string(),
            model: self.model.clone(),
            state: SessionState::Active,
            transport: SessionTransport::Subprocess,
            created_at: self.created_at,
            last_active: Utc::now(),
            turn_count: self.turn_count.load(Ordering::Relaxed),
        }
    }

    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn provider_name(&self) -> &str {
        "claude"
    }
}
