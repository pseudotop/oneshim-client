//! LocalLlmSession — Ollama `/api/chat` conversation adapter with NDJSON streaming.
//!
//! Self-managed conversation history targeting local LLM servers.
//! Streams responses line-by-line (NDJSON), mapping Ollama token usage
//! fields (`eval_count` / `prompt_eval_count`) to `TokenUsage`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_stream::try_stream;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use oneshim_core::config::AiSessionConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::ai_session::{
    truncate_chat_history, ChatMessage, ChatRole, ConversationSessionInfo, OutboundMessage,
    SessionMessage, SessionState, SessionTransport, TokenUsage,
};
use oneshim_core::ports::conversation_session::{ConversationSession, ResponseStream};

// ── Ollama NDJSON response shapes ────────────────────────────────

/// Single NDJSON line from Ollama `/api/chat` with `stream: true`.
#[derive(Debug, Deserialize)]
struct OllamaChatChunk {
    #[serde(default)]
    message: Option<OllamaChunkMessage>,
    #[serde(default)]
    done: bool,
    /// Token count for the generated response (present on final chunk).
    #[serde(default)]
    eval_count: Option<u64>,
    /// Token count for the prompt (present on final chunk).
    #[serde(default)]
    prompt_eval_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OllamaChunkMessage {
    #[serde(default)]
    content: String,
}

// ── LocalLlmSession ─────────────────────────────────────────────

pub struct LocalLlmSession {
    session_id: String,
    model: String,
    base_url: String,
    history: Arc<RwLock<Vec<ChatMessage>>>,
    /// Retained for session introspection; content is pre-seeded into `history`.
    #[allow(dead_code)]
    system_prompt: Option<String>,
    turn_count: AtomicU32,
    created_at: DateTime<Utc>,
    last_active: parking_lot::Mutex<Instant>,
    http_client: reqwest::Client,
    config: Arc<AiSessionConfig>,
}

impl LocalLlmSession {
    /// Create a new session targeting an Ollama-compatible server.
    pub fn new(
        session_id: String,
        model: String,
        base_url: String,
        system_prompt: Option<String>,
        config: Arc<AiSessionConfig>,
    ) -> Self {
        let mut initial_history = Vec::new();
        if let Some(ref prompt) = system_prompt {
            initial_history.push(ChatMessage {
                role: ChatRole::System,
                content: prompt.clone(),
            });
        }

        Self {
            session_id,
            model,
            base_url: base_url.trim_end_matches('/').to_string(),
            history: Arc::new(RwLock::new(initial_history)),
            system_prompt,
            turn_count: AtomicU32::new(0),
            created_at: Utc::now(),
            last_active: parking_lot::Mutex::new(Instant::now()),
            http_client: reqwest::Client::new(),
            config,
        }
    }
}

/// Parse a single NDJSON line into an `OllamaChatChunk`.
fn parse_ndjson_line(line: &str) -> Result<OllamaChatChunk, CoreError> {
    serde_json::from_str(line).map_err(|e| {
        CoreError::Internal(format!("failed to parse Ollama NDJSON chunk: {e}: {line}"))
    })
}

#[async_trait]
impl ConversationSession for LocalLlmSession {
    async fn send_message(&self, message: &SessionMessage) -> Result<ResponseStream, CoreError> {
        // Convert SessionMessage to ChatMessage and append to history.
        let user_msg = ChatMessage {
            role: ChatRole::User,
            content: message.content.clone(),
        };

        {
            let mut history = self.history.write().await;
            history.push(user_msg);
        }

        // Build request body with full history.
        let messages: Vec<serde_json::Value> = {
            let history = self.history.read().await;
            history
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "role": m.role,
                        "content": m.content,
                    })
                })
                .collect()
        };

        let url = format!("{}/api/chat", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
        });

        debug!(
            session_id = %self.session_id,
            model = %self.model,
            url = %url,
            history_len = messages.len(),
            "sending Ollama chat request"
        );

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::Network(format!("Ollama request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            return Err(CoreError::Network(format!(
                "Ollama API error {status}: {body_text}"
            )));
        }

        // Stream NDJSON lines from the response body.
        let mut byte_stream = response.bytes_stream();
        let history = self.history.clone();
        let turn_count = &self.turn_count;
        let max_history = self.config.max_history_turns;

        // Pre-increment turn count.
        turn_count.fetch_add(1, Ordering::Relaxed);
        *self.last_active.lock() = Instant::now();

        // We need to move owned values into the stream closure.
        let session_id = self.session_id.clone();

        let stream: ResponseStream = Box::pin(try_stream! {
            let mut accumulated = String::new();
            let mut line_buffer = String::new();

            while let Some(chunk_result) = byte_stream.next().await {
                let bytes = chunk_result
                    .map_err(|e| CoreError::Network(format!("stream read error: {e}")))?;
                let text = String::from_utf8_lossy(&bytes);
                line_buffer.push_str(&text);

                // Process complete lines (NDJSON = one JSON object per line).
                while let Some(newline_pos) = line_buffer.find('\n') {
                    let line = line_buffer[..newline_pos].trim().to_string();
                    line_buffer = line_buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    let chunk = parse_ndjson_line(&line)?;

                    if chunk.done {
                        // Final chunk — emit Result with token usage.
                        let usage = match (chunk.eval_count, chunk.prompt_eval_count) {
                            (Some(output), Some(input)) => Some(TokenUsage {
                                input_tokens: input,
                                output_tokens: output,
                            }),
                            _ => None,
                        };

                        // Append accumulated assistant message to history.
                        if !accumulated.is_empty() {
                            let mut hist: tokio::sync::RwLockWriteGuard<'_, Vec<ChatMessage>> = history.write().await;
                            hist.push(ChatMessage {
                                role: ChatRole::Assistant,
                                content: accumulated.clone(),
                            });
                            truncate_chat_history(&mut hist, max_history);
                        }

                        debug!(
                            session_id = %session_id,
                            accumulated_len = accumulated.len(),
                            ?usage,
                            "Ollama stream completed"
                        );

                        yield OutboundMessage::Result {
                            content: accumulated.clone(),
                            done: true,
                            usage,
                        };
                    } else if let Some(ref msg) = chunk.message {
                        // Streaming content chunk.
                        if !msg.content.is_empty() {
                            accumulated.push_str(&msg.content);
                            yield OutboundMessage::Text {
                                content: msg.content.clone(),
                                done: false,
                            };
                        }
                    }
                }
            }

            // Handle any remaining data in the buffer (no trailing newline).
            let remaining = line_buffer.trim().to_string();
            if !remaining.is_empty() {
                match parse_ndjson_line(&remaining) {
                    Ok(chunk) => {
                        if chunk.done {
                            let usage = match (chunk.eval_count, chunk.prompt_eval_count) {
                                (Some(output), Some(input)) => Some(TokenUsage {
                                    input_tokens: input,
                                    output_tokens: output,
                                }),
                                _ => None,
                            };

                            if !accumulated.is_empty() {
                                let mut hist: tokio::sync::RwLockWriteGuard<'_, Vec<ChatMessage>> = history.write().await;
                                hist.push(ChatMessage {
                                    role: ChatRole::Assistant,
                                    content: accumulated.clone(),
                                });
                                truncate_chat_history(&mut hist, max_history);
                            }

                            yield OutboundMessage::Result {
                                content: accumulated.clone(),
                                done: true,
                                usage,
                            };
                        } else if let Some(ref msg) = chunk.message {
                            if !msg.content.is_empty() {
                                accumulated.push_str(&msg.content);
                                yield OutboundMessage::Text {
                                    content: msg.content.clone(),
                                    done: false,
                                };
                            }
                        }
                    }
                    Err(e) => {
                        warn!("failed to parse trailing NDJSON: {e}");
                    }
                }
            }
        });

        Ok(stream)
    }

    fn info(&self) -> ConversationSessionInfo {
        let elapsed = self.last_active.lock().elapsed();
        let last_active_utc = Utc::now() - chrono::Duration::from_std(elapsed).unwrap_or_default();
        ConversationSessionInfo {
            session_id: self.session_id.clone(),
            provider_name: "ollama".to_string(),
            model: self.model.clone(),
            state: SessionState::Active, // TODO(Phase 3): State tracked by SessionManager, not by adapter
            transport: SessionTransport::LocalLlm,
            created_at: self.created_at,
            last_active: last_active_utc,
            turn_count: self.turn_count.load(Ordering::Relaxed),
        }
    }

    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── NDJSON parsing ──────────────────────────────────────────

    #[test]
    fn parse_ndjson_content_chunk() {
        let line =
            r#"{"model":"llama3","message":{"role":"assistant","content":"Hello"},"done":false}"#;
        let chunk = parse_ndjson_line(line).unwrap();
        assert!(!chunk.done);
        assert_eq!(chunk.message.as_ref().unwrap().content, "Hello");
        assert!(chunk.eval_count.is_none());
    }

    #[test]
    fn parse_ndjson_final_chunk_with_token_usage() {
        let line = r#"{"model":"llama3","message":{"role":"assistant","content":""},"done":true,"eval_count":50,"prompt_eval_count":20}"#;
        let chunk = parse_ndjson_line(line).unwrap();
        assert!(chunk.done);
        assert_eq!(chunk.eval_count, Some(50));
        assert_eq!(chunk.prompt_eval_count, Some(20));
    }

    #[test]
    fn parse_ndjson_final_chunk_without_usage() {
        let line = r#"{"done":true}"#;
        let chunk = parse_ndjson_line(line).unwrap();
        assert!(chunk.done);
        assert!(chunk.eval_count.is_none());
        assert!(chunk.prompt_eval_count.is_none());
    }

    #[test]
    fn parse_ndjson_invalid_json_returns_error() {
        let line = "not json at all";
        assert!(parse_ndjson_line(line).is_err());
    }

    // ── History truncation ──────────────────────────────────────

    #[test]
    fn truncate_preserves_system_prompt() {
        let mut history = vec![
            ChatMessage {
                role: ChatRole::System,
                content: "You are helpful.".to_string(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: "msg 1".to_string(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "resp 1".to_string(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: "msg 2".to_string(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "resp 2".to_string(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: "msg 3".to_string(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "resp 3".to_string(),
            },
        ];

        // Keep max 4 messages: system + last 3
        truncate_chat_history(&mut history, 4);

        assert_eq!(history.len(), 4);
        // First message is always the system prompt.
        assert_eq!(history[0].role, ChatRole::System);
        assert_eq!(history[0].content, "You are helpful.");
        // Last 3 messages are the most recent.
        assert_eq!(history[1].content, "resp 2");
        assert_eq!(history[2].content, "msg 3");
        assert_eq!(history[3].content, "resp 3");
    }

    #[test]
    fn truncate_no_op_when_under_limit() {
        let mut history = vec![
            ChatMessage {
                role: ChatRole::System,
                content: "system".to_string(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: "hello".to_string(),
            },
        ];

        truncate_chat_history(&mut history, 10);
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn truncate_exact_boundary() {
        let mut history = vec![
            ChatMessage {
                role: ChatRole::System,
                content: "system".to_string(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: "a".to_string(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "b".to_string(),
            },
        ];

        truncate_chat_history(&mut history, 3);
        assert_eq!(history.len(), 3);
    }

    // ── Session construction and info ───────────────────────────

    #[test]
    fn session_info_returns_correct_metadata() {
        let config = Arc::new(AiSessionConfig::default());
        let session = LocalLlmSession::new(
            "test-session-1".to_string(),
            "llama3".to_string(),
            "http://localhost:11434".to_string(),
            Some("You are helpful.".to_string()),
            config,
        );

        let info = session.info();
        assert_eq!(info.session_id, "test-session-1");
        assert_eq!(info.provider_name, "ollama");
        assert_eq!(info.model, "llama3");
        assert_eq!(info.state, SessionState::Active);
        assert_eq!(info.transport, SessionTransport::LocalLlm);
        assert_eq!(info.turn_count, 0);
    }

    #[test]
    fn session_id_and_provider_name() {
        let config = Arc::new(AiSessionConfig::default());
        let session = LocalLlmSession::new(
            "sid-42".to_string(),
            "qwen3:8b".to_string(),
            "http://localhost:11434".to_string(),
            None,
            config,
        );

        assert_eq!(session.session_id(), "sid-42");
        assert_eq!(session.provider_name(), "ollama");
    }

    #[test]
    fn session_initializes_with_system_prompt_in_history() {
        let config = Arc::new(AiSessionConfig::default());
        let session = LocalLlmSession::new(
            "s1".to_string(),
            "llama3".to_string(),
            "http://localhost:11434".to_string(),
            Some("Be concise.".to_string()),
            config,
        );

        let history = session.history.blocking_read();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, ChatRole::System);
        assert_eq!(history[0].content, "Be concise.");
    }

    #[test]
    fn session_initializes_empty_history_without_system_prompt() {
        let config = Arc::new(AiSessionConfig::default());
        let session = LocalLlmSession::new(
            "s2".to_string(),
            "llama3".to_string(),
            "http://localhost:11434".to_string(),
            None,
            config,
        );

        let history = session.history.blocking_read();
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn base_url_trailing_slash_stripped() {
        let config = Arc::new(AiSessionConfig::default());
        let session = LocalLlmSession::new(
            "s3".to_string(),
            "llama3".to_string(),
            "http://localhost:11434/".to_string(),
            None,
            config,
        );

        assert_eq!(session.base_url, "http://localhost:11434");
    }

    // ── NDJSON to OutboundMessage normalization ─────────────────

    #[test]
    fn ndjson_chunk_to_outbound_text() {
        let line =
            r#"{"model":"llama3","message":{"role":"assistant","content":"world"},"done":false}"#;
        let chunk = parse_ndjson_line(line).unwrap();

        assert!(!chunk.done);
        let content = &chunk.message.as_ref().unwrap().content;
        assert_eq!(content, "world");
    }

    #[test]
    fn ndjson_final_to_outbound_result_with_usage() {
        let line = r#"{"model":"llama3","message":{"role":"assistant","content":""},"done":true,"eval_count":123,"prompt_eval_count":45}"#;
        let chunk = parse_ndjson_line(line).unwrap();

        assert!(chunk.done);
        let usage = TokenUsage {
            input_tokens: chunk.prompt_eval_count.unwrap(),
            output_tokens: chunk.eval_count.unwrap(),
        };
        assert_eq!(usage.input_tokens, 45);
        assert_eq!(usage.output_tokens, 123);
    }
}
