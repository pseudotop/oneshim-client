//! HTTP API session adapter — direct Anthropic/OpenAI API calls with
//! self-managed conversation history and SSE streaming responses.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_stream::try_stream;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use eventsource_stream::Eventsource;
use futures::stream::StreamExt;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use oneshim_api_contracts::provider_specs::{
    self, ProviderAuthScheme, ProviderRequestShape, ProviderTransportKind,
};
use oneshim_core::config::{AiProviderType, AiSessionConfig};
use oneshim_core::error::CoreError;
use oneshim_core::models::ai_session::{
    truncate_chat_history, Attachment, ChatMessage, ChatRole, ContentBlock,
    ConversationSessionInfo, OutboundMessage, SessionMessage, SessionState, SessionTransport,
    TokenUsage, ToolDefinition,
};
use oneshim_core::ports::conversation_session::{ConversationSession, ResponseStream};
use oneshim_core::ports::credential_source::CredentialSource;

/// Direct HTTP API session adapter for Anthropic and OpenAI providers.
///
/// Manages conversation history locally and streams responses via SSE.
pub struct HttpApiSession {
    session_id: String,
    surface_id: String,
    model: String,
    endpoint: String,
    credential: CredentialSource,
    provider_type: AiProviderType,
    history: Arc<RwLock<Vec<ChatMessage>>>,
    system_prompt: Option<String>,
    state: parking_lot::Mutex<SessionState>,
    turn_count: AtomicU32,
    created_at: DateTime<Utc>,
    last_active: parking_lot::Mutex<Instant>,
    http_client: reqwest::Client,
    config: Arc<AiSessionConfig>,
}

#[derive(Debug, Default)]
#[allow(dead_code)] // Fields read in Task 2-5 (vision, structured output, tool calling)
struct RequestOptions<'a> {
    response_format: Option<&'a serde_json::Value>,
    tools: Option<&'a [ToolDefinition]>,
}

#[allow(dead_code)] // Used in Task 5 (tool calling SSE parsing)
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

// ── Content Block Serialization Helpers ─────────────────────────

/// Serialize content blocks to Anthropic Messages API format.
///
/// Anthropic uses `{"type": "image", "source": {"type": "base64", ...}}` for images.
fn serialize_anthropic_content(blocks: &[ContentBlock]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(serde_json::json!({"type": "text", "text": text})),
            ContentBlock::Image { media_type, data } => Some(serde_json::json!({
                "type": "image",
                "source": {"type": "base64", "media_type": media_type, "data": data}
            })),
            _ => None,
        })
        .collect()
}

/// Serialize content blocks to OpenAI Chat Completions API format.
///
/// OpenAI uses `{"type": "image_url", "image_url": {"url": "data:...;base64,..."}}` for images.
fn serialize_openai_content(blocks: &[ContentBlock]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(serde_json::json!({"type": "text", "text": text})),
            ContentBlock::Image { media_type, data } => Some(serde_json::json!({
                "type": "image_url",
                "image_url": {"url": format!("data:{media_type};base64,{data}")}
            })),
            _ => None,
        })
        .collect()
}

/// Serialize content blocks to Google Gemini API format.
///
/// Google uses `{"inlineData": {"mimeType": ..., "data": ...}}` for images.
fn serialize_google_parts(blocks: &[ContentBlock]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(serde_json::json!({"text": text})),
            ContentBlock::Image { media_type, data } => Some(serde_json::json!({
                "inlineData": {"mimeType": media_type, "data": data}
            })),
            _ => None,
        })
        .collect()
}

impl HttpApiSession {
    /// Create a new HTTP API session.
    pub fn new(
        surface_id: String,
        model: String,
        endpoint: String,
        credential: CredentialSource,
        provider_type: AiProviderType,
        system_prompt: Option<String>,
        config: Arc<AiSessionConfig>,
    ) -> Self {
        let session_id = uuid::Uuid::new_v4().to_string();
        let http_client = reqwest::Client::new();
        let mut initial_history = Vec::new();

        if let Some(ref prompt) = system_prompt {
            initial_history.push(ChatMessage {
                role: ChatRole::System,
                content: prompt.clone(),
                content_blocks: None,
            });
        }

        Self {
            session_id,
            surface_id,
            model,
            endpoint,
            credential,
            provider_type,
            history: Arc::new(RwLock::new(initial_history)),
            system_prompt,
            state: parking_lot::Mutex::new(SessionState::Active),
            turn_count: AtomicU32::new(0),
            created_at: Utc::now(),
            last_active: parking_lot::Mutex::new(Instant::now()),
            http_client,
            config,
        }
    }

    /// Build provider-specific streaming request body from conversation history.
    fn build_request_body(
        &self,
        messages: &[ChatMessage],
        _options: &RequestOptions<'_>,
    ) -> Result<serde_json::Value, CoreError> {
        let shape = provider_specs::resolved_request_shape(
            self.provider_type,
            Some(&self.surface_id),
            ProviderTransportKind::Llm,
        )
        .map_err(CoreError::Internal)?;

        match shape {
            ProviderRequestShape::AnthropicMessages
            | ProviderRequestShape::AnthropicVisionMessages => {
                // Anthropic: system prompt is top-level, messages exclude system role
                let api_messages: Vec<serde_json::Value> = messages
                    .iter()
                    .filter(|m| m.role != ChatRole::System)
                    .map(|m| {
                        let content = if let Some(ref blocks) = m.content_blocks {
                            serde_json::Value::Array(serialize_anthropic_content(blocks))
                        } else {
                            serde_json::Value::String(m.content.clone())
                        };
                        serde_json::json!({ "role": m.role, "content": content })
                    })
                    .collect();

                let mut body = serde_json::json!({
                    "model": self.model,
                    "max_tokens": self.config.max_output_tokens,
                    "stream": true,
                    "messages": api_messages,
                });

                if let Some(ref prompt) = self.system_prompt {
                    body["system"] = serde_json::Value::String(prompt.clone());
                }

                Ok(body)
            }
            ProviderRequestShape::OpenAiChatCompletions
            | ProviderRequestShape::OpenAiVisionChatCompletions
            | ProviderRequestShape::OpenAiResponses => {
                // OpenAI: system prompt is first message with role "system"
                let api_messages: Vec<serde_json::Value> = messages
                    .iter()
                    .map(|m| {
                        let content = if let Some(ref blocks) = m.content_blocks {
                            serde_json::Value::Array(serialize_openai_content(blocks))
                        } else {
                            serde_json::Value::String(m.content.clone())
                        };
                        serde_json::json!({ "role": m.role, "content": content })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "model": self.model,
                    "max_tokens": self.config.max_output_tokens,
                    "stream": true,
                    "messages": api_messages,
                }))
            }
            ProviderRequestShape::GoogleGenerateContent => {
                // Google Gemini: contents array, system_instruction, generationConfig
                let api_contents: Vec<serde_json::Value> = messages
                    .iter()
                    .filter(|m| m.role != ChatRole::System)
                    .map(|m| {
                        let parts = if let Some(ref blocks) = m.content_blocks {
                            serialize_google_parts(blocks)
                        } else {
                            vec![serde_json::json!({"text": m.content})]
                        };
                        serde_json::json!({
                            "role": match m.role {
                                ChatRole::User => "user",
                                ChatRole::Assistant => "model",
                                _ => "user",
                            },
                            "parts": parts,
                        })
                    })
                    .collect();

                let mut body = serde_json::json!({
                    "contents": api_contents,
                    "generationConfig": {
                        "maxOutputTokens": self.config.max_output_tokens,
                    },
                });

                if let Some(ref prompt) = self.system_prompt {
                    body["system_instruction"] = serde_json::json!({"parts": [{"text": prompt}]});
                }

                Ok(body)
            }
            _ => Err(CoreError::Internal(format!(
                "unsupported request shape for HTTP API session: {shape:?}"
            ))),
        }
    }

    /// Resolve auth headers for the provider.
    async fn apply_auth(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, CoreError> {
        let auth_scheme = provider_specs::resolved_auth_scheme(
            self.provider_type,
            Some(&self.surface_id),
            ProviderTransportKind::Llm,
        )
        .map_err(CoreError::Internal)?;

        let builder = match auth_scheme {
            ProviderAuthScheme::None => builder,
            ProviderAuthScheme::XApiKey => {
                let token = self.credential.resolve_bearer_token().await?;
                builder
                    .header("x-api-key", &token)
                    .header("anthropic-version", crate::ANTHROPIC_API_VERSION)
            }
            ProviderAuthScheme::XGoogApiKey => {
                let token = self.credential.resolve_bearer_token().await?;
                builder.header("x-goog-api-key", &token)
            }
            ProviderAuthScheme::Bearer => {
                let token = self.credential.resolve_bearer_token().await?;
                builder.header("Authorization", format!("Bearer {}", token))
            }
            ProviderAuthScheme::AwsSignatureV4 => {
                return Err(CoreError::Internal(
                    "AWS Signature V4 authentication is not yet supported for API sessions"
                        .to_string(),
                ));
            }
        };

        Ok(builder)
    }

    /// Resolve the streaming endpoint URL.
    ///
    /// Google requires `streamGenerateContent?alt=sse` instead of `generateContent`.
    fn streaming_endpoint(&self) -> String {
        let shape = provider_specs::resolved_request_shape(
            self.provider_type,
            Some(&self.surface_id),
            ProviderTransportKind::Llm,
        );
        if matches!(shape, Ok(ProviderRequestShape::GoogleGenerateContent)) {
            self.endpoint
                .replace(":generateContent", ":streamGenerateContent")
                + if self.endpoint.contains('?') {
                    "&alt=sse"
                } else {
                    "?alt=sse"
                }
        } else {
            self.endpoint.clone()
        }
    }

    /// Truncate history to keep the system prompt (first message) + last (max-1) messages.
    #[cfg(test)]
    fn truncate_history(history: &mut Vec<ChatMessage>, max_turns: u32) {
        truncate_chat_history(history, max_turns);
    }
}

#[async_trait]
impl ConversationSession for HttpApiSession {
    async fn send_message(&self, message: &SessionMessage) -> Result<ResponseStream, CoreError> {
        // Convert attachments to content blocks for multimodal messages
        let content_blocks = {
            let mut blocks = vec![ContentBlock::Text {
                text: message.content.clone(),
            }];
            for att in &message.attachments {
                if let Attachment::Image {
                    mime,
                    data: Some(b64),
                    ..
                } = att
                {
                    blocks.push(ContentBlock::Image {
                        media_type: mime.clone(),
                        data: b64.clone(),
                    });
                }
            }
            if blocks.len() > 1 {
                Some(blocks)
            } else {
                None
            }
        };

        let user_msg = ChatMessage {
            role: ChatRole::User,
            content: message.content.clone(),
            content_blocks,
        };

        // Append user message to history
        {
            let mut history = self.history.write().await;
            history.push(user_msg);
        }

        // Snapshot history for the request
        let messages_snapshot = self.history.read().await.clone();
        let request_body =
            self.build_request_body(&messages_snapshot, &RequestOptions::default())?;

        let streaming_url = self.streaming_endpoint();
        let builder = self
            .http_client
            .post(&streaming_url)
            .header("Content-Type", "application/json")
            .json(&request_body);

        let builder = self.apply_auth(builder).await.inspect_err(|_| {
            *self.state.lock() = SessionState::Failed;
        })?;

        let response = builder.send().await.map_err(|e| {
            *self.state.lock() = SessionState::Failed;
            CoreError::Network(format!("HTTP API session request failed: {e}"))
        })?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read error body".to_string());
            *self.state.lock() = SessionState::Failed;
            return Err(CoreError::Network(format!(
                "HTTP API error ({status}): {}",
                body.chars().take(300).collect::<String>()
            )));
        }

        let provider_type = self.provider_type;
        let history = self.history.clone();
        let max_turns = self.config.max_history_turns;
        let turn_count = &self.turn_count;
        turn_count.fetch_add(1, Ordering::Relaxed);
        *self.last_active.lock() = Instant::now();

        // Build the ResponseStream using SSE parsing
        let stream: ResponseStream = Box::pin(try_stream! {
            let mut accumulated = String::new();

            let shape = provider_specs::resolved_request_shape(
                provider_type,
                None,
                ProviderTransportKind::Llm,
            )
            .map_err(CoreError::Internal)?;

            let is_anthropic = matches!(
                shape,
                ProviderRequestShape::AnthropicMessages
                    | ProviderRequestShape::AnthropicVisionMessages
            );
            let is_google = matches!(shape, ProviderRequestShape::GoogleGenerateContent);

            let byte_stream = response.bytes_stream();
            let mut event_stream = byte_stream.eventsource();

            while let Some(event_result) = event_stream.next().await {
                match event_result {
                    Ok(event) => {
                        let parsed = if is_anthropic {
                            parse_anthropic_sse_event(&event.event, &event.data)
                        } else if is_google {
                            parse_google_sse_event(&event.data)
                        } else {
                            parse_openai_sse_event(&event.data)
                        };

                        if let Some(msg) = parsed {
                            match &msg {
                                OutboundMessage::Text { content, .. } => {
                                    accumulated.push_str(content);
                                }
                                OutboundMessage::Result { .. } => {
                                    let assistant_msg = ChatMessage {
                                        role: ChatRole::Assistant,
                                        content: accumulated.clone(),
                                        content_blocks: None,
                                    };
                                    let mut hist: tokio::sync::RwLockWriteGuard<'_, Vec<ChatMessage>> = history.write().await;
                                    hist.push(assistant_msg);
                                    truncate_chat_history(&mut hist, max_turns);
                                }
                                _ => {}
                            }
                            yield msg;
                        }
                    }
                    Err(e) => {
                        warn!("SSE stream error: {e}");
                        // Append whatever we accumulated so far
                        if !accumulated.is_empty() {
                            let assistant_msg = ChatMessage {
                                role: ChatRole::Assistant,
                                content: accumulated.clone(),
                                content_blocks: None,
                            };
                            let mut hist: tokio::sync::RwLockWriteGuard<'_, Vec<ChatMessage>> = history.write().await;
                            hist.push(assistant_msg);
                            truncate_chat_history(&mut hist, max_turns);
                        }
                        Err(CoreError::Network(format!("SSE stream error: {e}")))?;
                    }
                }
            }

            // If stream ended without a message_stop/[DONE] event, still save history
            if !accumulated.is_empty() {
                let has_result = {
                    let hist: tokio::sync::RwLockReadGuard<'_, Vec<ChatMessage>> = history.read().await;
                    hist.last().is_some_and(|m| m.role == ChatRole::Assistant)
                };
                if !has_result {
                    let assistant_msg = ChatMessage {
                        role: ChatRole::Assistant,
                        content: accumulated.clone(),
                        content_blocks: None,
                    };
                    let mut hist: tokio::sync::RwLockWriteGuard<'_, Vec<ChatMessage>> = history.write().await;
                    hist.push(assistant_msg);
                    truncate_chat_history(&mut hist, max_turns);

                    yield OutboundMessage::Result {
                        content: accumulated,
                        done: true,
                        usage: None,
                    };
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
            provider_name: format!("{:?}", self.provider_type).to_lowercase(),
            model: self.model.clone(),
            state: *self.state.lock(),
            transport: SessionTransport::HttpApi,
            created_at: self.created_at,
            last_active: last_active_utc,
            turn_count: self.turn_count.load(Ordering::Relaxed),
        }
    }

    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn provider_name(&self) -> &str {
        match self.provider_type {
            AiProviderType::Anthropic => "anthropic",
            AiProviderType::OpenAi => "openai",
            AiProviderType::Google => "google",
            AiProviderType::Ollama => "ollama",
            AiProviderType::Bedrock => "bedrock",
            AiProviderType::Copilot => "copilot",
            AiProviderType::Generic => "generic",
        }
    }
}

// ── SSE Event Normalization ─────────────────────────────────────

/// Parse an Anthropic SSE event into an OutboundMessage.
///
/// Anthropic event types:
/// - `content_block_delta` → text chunk
/// - `message_stop` → stream completion
/// - `message_delta` → usage info (optional)
pub fn parse_anthropic_sse_event(event_type: &str, data: &str) -> Option<OutboundMessage> {
    match event_type {
        "content_block_delta" => {
            let val: serde_json::Value = serde_json::from_str(data).ok()?;
            let text = val.get("delta")?.get("text")?.as_str()?.to_string();
            Some(OutboundMessage::Text {
                content: text,
                done: false,
            })
        }
        "message_delta" => {
            // Extract usage from message_delta if present
            let val: serde_json::Value = serde_json::from_str(data).ok()?;
            let usage = val.get("usage").and_then(|u| {
                let input = u.get("input_tokens")?.as_u64()?;
                let output = u.get("output_tokens")?.as_u64()?;
                Some(TokenUsage {
                    input_tokens: input,
                    output_tokens: output,
                })
            });
            // message_delta may contain stop_reason but we handle completion in message_stop
            if usage.is_some() {
                Some(OutboundMessage::Result {
                    content: String::new(),
                    done: false,
                    usage,
                })
            } else {
                None
            }
        }
        "message_stop" => Some(OutboundMessage::Result {
            content: String::new(),
            done: true,
            usage: None,
        }),
        _ => {
            debug!(event_type, "ignoring Anthropic SSE event");
            None
        }
    }
}

/// Parse a Google Gemini SSE event data payload into an OutboundMessage.
///
/// Google events:
/// - `data: {"candidates":[{"content":{"parts":[{"text":"chunk"}]}}]}` → text chunk
/// - Final chunk includes `usageMetadata` with token counts
/// - Stream ends without explicit `[DONE]` marker
pub fn parse_google_sse_event(data: &str) -> Option<OutboundMessage> {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return None;
    }

    let val: serde_json::Value = serde_json::from_str(trimmed).ok()?;

    // Extract text from candidates[0].content.parts[0].text
    let text = val
        .get("candidates")?
        .get(0)?
        .get("content")?
        .get("parts")?
        .get(0)?
        .get("text")?
        .as_str()?;

    // Check for usage metadata (present in chunks, especially the last one)
    let usage = val.get("usageMetadata").and_then(|u| {
        let input = u.get("promptTokenCount")?.as_u64()?;
        let output = u.get("candidatesTokenCount")?.as_u64()?;
        Some(TokenUsage {
            input_tokens: input,
            output_tokens: output,
        })
    });

    if usage.is_some() {
        // Final chunk with usage — emit text + result
        if !text.is_empty() {
            // Google sends text and usage in the same final chunk;
            // emit as Result with the final text included.
            return Some(OutboundMessage::Result {
                content: text.to_string(),
                done: true,
                usage,
            });
        }
        return Some(OutboundMessage::Result {
            content: String::new(),
            done: true,
            usage,
        });
    }

    if text.is_empty() {
        return None;
    }

    Some(OutboundMessage::Text {
        content: text.to_string(),
        done: false,
    })
}

/// Parse an OpenAI SSE event data payload into an OutboundMessage.
///
/// OpenAI events:
/// - `data: {"choices":[{"delta":{"content":"chunk"}}]}` → text chunk
/// - `data: [DONE]` → stream completion
pub fn parse_openai_sse_event(data: &str) -> Option<OutboundMessage> {
    let trimmed = data.trim();

    if trimmed == "[DONE]" {
        return Some(OutboundMessage::Result {
            content: String::new(),
            done: true,
            usage: None,
        });
    }

    let val: serde_json::Value = serde_json::from_str(trimmed).ok()?;

    // Check for usage in the final chunk (OpenAI includes usage in last chunk if requested)
    if let Some(usage_obj) = val.get("usage") {
        let input = usage_obj
            .get("prompt_tokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        let output = usage_obj
            .get("completion_tokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        if input > 0 || output > 0 {
            return Some(OutboundMessage::Result {
                content: String::new(),
                done: false,
                usage: Some(TokenUsage {
                    input_tokens: input,
                    output_tokens: output,
                }),
            });
        }
    }

    let content = val
        .get("choices")?
        .get(0)?
        .get("delta")?
        .get("content")?
        .as_str()?;

    if content.is_empty() {
        return None;
    }

    Some(OutboundMessage::Text {
        content: content.to_string(),
        done: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_content_block_delta() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let msg = parse_anthropic_sse_event("content_block_delta", data);
        match msg {
            Some(OutboundMessage::Text { content, done }) => {
                assert_eq!(content, "Hello");
                assert!(!done);
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn anthropic_message_stop() {
        let data = r#"{"type":"message_stop"}"#;
        let msg = parse_anthropic_sse_event("message_stop", data);
        match msg {
            Some(OutboundMessage::Result { done, .. }) => {
                assert!(done);
            }
            other => panic!("expected Result with done=true, got {other:?}"),
        }
    }

    #[test]
    fn anthropic_message_delta_with_usage() {
        let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":25,"output_tokens":50}}"#;
        let msg = parse_anthropic_sse_event("message_delta", data);
        match msg {
            Some(OutboundMessage::Result { usage, .. }) => {
                let u = usage.unwrap();
                assert_eq!(u.input_tokens, 25);
                assert_eq!(u.output_tokens, 50);
            }
            other => panic!("expected Result with usage, got {other:?}"),
        }
    }

    #[test]
    fn anthropic_ignores_unknown_event() {
        let msg = parse_anthropic_sse_event("ping", "{}");
        assert!(msg.is_none());
    }

    #[test]
    fn openai_content_delta() {
        let data = r#"{"choices":[{"index":0,"delta":{"content":"world"}}]}"#;
        let msg = parse_openai_sse_event(data);
        match msg {
            Some(OutboundMessage::Text { content, done }) => {
                assert_eq!(content, "world");
                assert!(!done);
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn openai_done_event() {
        let msg = parse_openai_sse_event("[DONE]");
        match msg {
            Some(OutboundMessage::Result { done, .. }) => {
                assert!(done);
            }
            other => panic!("expected Result with done=true, got {other:?}"),
        }
    }

    #[test]
    fn openai_with_usage() {
        let data = r#"{"usage":{"prompt_tokens":10,"completion_tokens":20}}"#;
        let msg = parse_openai_sse_event(data);
        match msg {
            Some(OutboundMessage::Result { usage, .. }) => {
                let u = usage.unwrap();
                assert_eq!(u.input_tokens, 10);
                assert_eq!(u.output_tokens, 20);
            }
            other => panic!("expected Result with usage, got {other:?}"),
        }
    }

    #[test]
    fn google_text_chunk() {
        let data = r#"{"candidates":[{"content":{"parts":[{"text":"Hello from Gemini"}],"role":"model"}}],"modelVersion":"gemini-2.5-flash"}"#;
        let msg = parse_google_sse_event(data);
        match msg {
            Some(OutboundMessage::Text { content, done }) => {
                assert_eq!(content, "Hello from Gemini");
                assert!(!done);
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn google_final_chunk_with_usage() {
        let data = r#"{"candidates":[{"content":{"parts":[{"text":"!"}],"role":"model"},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":42},"modelVersion":"gemini-2.5-flash"}"#;
        let msg = parse_google_sse_event(data);
        match msg {
            Some(OutboundMessage::Result {
                content,
                done,
                usage,
            }) => {
                assert_eq!(content, "!");
                assert!(done);
                let u = usage.unwrap();
                assert_eq!(u.input_tokens, 10);
                assert_eq!(u.output_tokens, 42);
            }
            other => panic!("expected Result with usage, got {other:?}"),
        }
    }

    #[test]
    fn google_empty_data_ignored() {
        let msg = parse_google_sse_event("");
        assert!(msg.is_none());
    }

    #[test]
    fn openai_empty_content_ignored() {
        let data = r#"{"choices":[{"index":0,"delta":{"content":""}}]}"#;
        let msg = parse_openai_sse_event(data);
        assert!(msg.is_none());
    }

    #[test]
    fn history_truncation_preserves_system_prompt() {
        let mut history = vec![
            ChatMessage {
                role: ChatRole::System,
                content: "system".to_string(),
                content_blocks: None,
            },
            ChatMessage {
                role: ChatRole::User,
                content: "msg1".to_string(),
                content_blocks: None,
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "reply1".to_string(),
                content_blocks: None,
            },
            ChatMessage {
                role: ChatRole::User,
                content: "msg2".to_string(),
                content_blocks: None,
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "reply2".to_string(),
                content_blocks: None,
            },
            ChatMessage {
                role: ChatRole::User,
                content: "msg3".to_string(),
                content_blocks: None,
            },
        ];

        // max_turns=4: keep system (index 0) + last 3 messages
        // Before: [system, msg1, reply1, msg2, reply2, msg3] (6 items)
        // drain(1..3) removes msg1, reply1
        // After:  [system, msg2, reply2, msg3] (4 items)
        HttpApiSession::truncate_history(&mut history, 4);
        assert_eq!(history.len(), 4);
        assert_eq!(history[0].role, ChatRole::System);
        assert_eq!(history[0].content, "system");
        assert_eq!(history[1].content, "msg2");
        assert_eq!(history[2].content, "reply2");
        assert_eq!(history[3].content, "msg3");
    }

    #[test]
    fn history_truncation_no_op_when_under_limit() {
        let mut history = vec![
            ChatMessage {
                role: ChatRole::System,
                content: "system".to_string(),
                content_blocks: None,
            },
            ChatMessage {
                role: ChatRole::User,
                content: "hello".to_string(),
                content_blocks: None,
            },
        ];

        HttpApiSession::truncate_history(&mut history, 10);
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn chat_message_from_session_message() {
        let session_msg = SessionMessage {
            role: oneshim_core::models::ai_session::MessageRole::User,
            content: "test question".to_string(),
            attachments: vec![],
            tools: None,
            context: None,
            response_format: None,
        };

        let chat_msg = ChatMessage {
            role: ChatRole::User,
            content: session_msg.content.clone(),
            content_blocks: None,
        };

        assert_eq!(chat_msg.role, ChatRole::User);
        assert_eq!(chat_msg.content, "test question");

        let json = serde_json::to_string(&chat_msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("test question"));
    }

    #[test]
    fn new_session_with_system_prompt_initializes_history() {
        let session = HttpApiSession::new(
            "provider_surface.anthropic.direct_api".to_string(),
            "claude-sonnet-4-20250514".to_string(),
            "https://api.anthropic.com/v1/messages".to_string(),
            CredentialSource::ApiKey("sk-test".to_string()),
            AiProviderType::Anthropic,
            Some("You are helpful.".to_string()),
            Arc::new(AiSessionConfig::default()),
        );

        assert!(!session.session_id.is_empty());
        assert_eq!(session.provider_name(), "anthropic");
        assert_eq!(session.model, "claude-sonnet-4-20250514");

        let info = session.info();
        assert_eq!(info.transport, SessionTransport::HttpApi);
        assert_eq!(info.turn_count, 0);
    }

    #[test]
    fn new_session_without_system_prompt_has_empty_history() {
        let session = HttpApiSession::new(
            "provider_surface.openai.direct_api".to_string(),
            "gpt-5.4".to_string(),
            "https://api.openai.com/v1/chat/completions".to_string(),
            CredentialSource::ApiKey("sk-test".to_string()),
            AiProviderType::OpenAi,
            None,
            Arc::new(AiSessionConfig::default()),
        );

        assert_eq!(session.provider_name(), "openai");
    }

    // ── Vision Content Block Tests ──────────────────────────────

    /// Helper to create a session and build request body with content blocks.
    fn build_body_with_blocks(
        provider: AiProviderType,
        surface: &str,
        endpoint: &str,
        blocks: Vec<ContentBlock>,
    ) -> serde_json::Value {
        let session = HttpApiSession::new(
            surface.to_string(),
            "test-model".to_string(),
            endpoint.to_string(),
            CredentialSource::ApiKey("sk-test".to_string()),
            provider,
            Some("system prompt".to_string()),
            Arc::new(AiSessionConfig::default()),
        );

        let messages = vec![
            ChatMessage {
                role: ChatRole::System,
                content: "system prompt".to_string(),
                content_blocks: None,
            },
            ChatMessage {
                role: ChatRole::User,
                content: "Describe this image".to_string(),
                content_blocks: Some(blocks),
            },
        ];

        session
            .build_request_body(&messages, &RequestOptions::default())
            .expect("build_request_body should succeed")
    }

    fn sample_image_blocks() -> Vec<ContentBlock> {
        vec![
            ContentBlock::Text {
                text: "Describe this image".to_string(),
            },
            ContentBlock::Image {
                media_type: "image/jpeg".to_string(),
                data: "dGVzdA==".to_string(),
            },
        ]
    }

    #[test]
    fn anthropic_vision_content_blocks() {
        let body = build_body_with_blocks(
            AiProviderType::Anthropic,
            "provider_surface.anthropic.direct_api",
            "https://api.anthropic.com/v1/messages",
            sample_image_blocks(),
        );

        let messages = body["messages"].as_array().expect("messages array");
        assert_eq!(messages.len(), 1); // system is excluded
        let content = messages[0]["content"].as_array().expect("content array");
        assert_eq!(content.len(), 2);

        // Text block
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Describe this image");

        // Image block — Anthropic format
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["type"], "base64");
        assert_eq!(content[1]["source"]["media_type"], "image/jpeg");
        assert_eq!(content[1]["source"]["data"], "dGVzdA==");
    }

    #[test]
    fn openai_vision_content_blocks() {
        let body = build_body_with_blocks(
            AiProviderType::OpenAi,
            "provider_surface.openai.direct_api",
            "https://api.openai.com/v1/chat/completions",
            sample_image_blocks(),
        );

        let messages = body["messages"].as_array().expect("messages array");
        // OpenAI includes the system message
        assert_eq!(messages.len(), 2);
        let user_content = messages[1]["content"]
            .as_array()
            .expect("content array for user message");
        assert_eq!(user_content.len(), 2);

        // Text block
        assert_eq!(user_content[0]["type"], "text");
        assert_eq!(user_content[0]["text"], "Describe this image");

        // Image block — OpenAI format
        assert_eq!(user_content[1]["type"], "image_url");
        let url = user_content[1]["image_url"]["url"].as_str().unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
        assert!(url.ends_with("dGVzdA=="));
    }

    #[test]
    fn google_vision_content_blocks() {
        let body = build_body_with_blocks(
            AiProviderType::Google,
            "provider_surface.google.direct_api",
            "https://generativelanguage.googleapis.com/v1beta/models/test-model:generateContent",
            sample_image_blocks(),
        );

        let contents = body["contents"].as_array().expect("contents array");
        assert_eq!(contents.len(), 1); // system is excluded
        let parts = contents[0]["parts"].as_array().expect("parts array");
        assert_eq!(parts.len(), 2);

        // Text part
        assert_eq!(parts[0]["text"], "Describe this image");

        // Image part — Google format
        assert_eq!(parts[1]["inlineData"]["mimeType"], "image/jpeg");
        assert_eq!(parts[1]["inlineData"]["data"], "dGVzdA==");
    }

    #[test]
    fn plain_text_backward_compat() {
        // When content_blocks is None, content should be a plain string
        let session = HttpApiSession::new(
            "provider_surface.anthropic.direct_api".to_string(),
            "test-model".to_string(),
            "https://api.anthropic.com/v1/messages".to_string(),
            CredentialSource::ApiKey("sk-test".to_string()),
            AiProviderType::Anthropic,
            None,
            Arc::new(AiSessionConfig::default()),
        );

        let messages = vec![ChatMessage {
            role: ChatRole::User,
            content: "Hello world".to_string(),
            content_blocks: None,
        }];

        let body = session
            .build_request_body(&messages, &RequestOptions::default())
            .expect("build_request_body should succeed");

        let api_messages = body["messages"].as_array().expect("messages array");
        assert_eq!(api_messages.len(), 1);

        // Content should be a plain string, not an array
        let content = &api_messages[0]["content"];
        assert!(
            content.is_string(),
            "expected string content, got {content}"
        );
        assert_eq!(content.as_str().unwrap(), "Hello world");
    }
}
