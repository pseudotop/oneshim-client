use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use oneshim_api_contracts::integration::{
    IntegrationBootstrapRequest, IntegrationBootstrapResponse, IntegrationSessionDisconnectPayload,
    IntegrationSessionHeartbeatPayload,
};
use oneshim_core::error::CoreError;
use oneshim_core::models::integration::{
    IntegrationAckCursor, IntegrationAuthContext, IntegrationAuthScheme,
    IntegrationCapabilityScope, IntegrationTransportKind, ProactivePrompt,
    QueuedIntegrationEgressMessage,
};
use oneshim_core::ports::integration::IntegrationAuthPort;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use tokio::sync::RwLock;

use super::transport::{
    IntegrationEgressTransportClient, IntegrationEgressTransportResponse,
    IntegrationInboxTransportClient, IntegrationInboxTransportResponse,
    IntegrationRequestProofFactory, IntegrationTransportClient, IntegrationTransportConnectRequest,
    IntegrationTransportConnectResponse,
};
use super::{
    outbound_message_to_cloudevent, prompt_from_cloudevent, IntegrationOutboundCloudEventBatch,
    IntegrationOutboundCloudEventBatchItem, WebSocketIntegrationSessionChannel,
};

#[derive(Debug, Clone)]
pub struct HttpsIntegrationTransportConfig {
    pub bootstrap_url: String,
    pub request_timeout: Duration,
}

impl HttpsIntegrationTransportConfig {
    pub fn new(bootstrap_url: impl Into<String>, request_timeout: Duration) -> Self {
        Self {
            bootstrap_url: bootstrap_url.into(),
            request_timeout,
        }
    }
}

#[derive(Clone)]
struct SessionBinding {
    heartbeat_url: Option<String>,
    disconnect_url: Option<String>,
    send_events_url: Option<String>,
    receive_prompts_url: Option<String>,
    auth: IntegrationAuthContext,
    live_session_channel: Option<Arc<WebSocketIntegrationSessionChannel>>,
}

#[derive(Clone, Default)]
pub struct HttpsIntegrationSessionBindings {
    sessions: Arc<RwLock<HashMap<String, SessionBinding>>>,
}

impl HttpsIntegrationSessionBindings {
    async fn insert(&self, session_id: String, binding: SessionBinding) {
        self.sessions.write().await.insert(session_id, binding);
    }

    async fn get(&self, session_id: &str) -> Option<SessionBinding> {
        self.sessions.read().await.get(session_id).cloned()
    }

    async fn remove(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
    }
}

#[derive(Clone)]
struct HttpsIntegrationHttpShared {
    client: reqwest::Client,
    proof_factory: Arc<dyn IntegrationRequestProofFactory>,
    request_timeout: Duration,
}

pub struct HttpsIntegrationTransportClient {
    config: HttpsIntegrationTransportConfig,
    shared: HttpsIntegrationHttpShared,
    auth_port: Arc<dyn IntegrationAuthPort>,
    session_bindings: HttpsIntegrationSessionBindings,
}

pub struct HttpsIntegrationEgressTransportClient {
    shared: HttpsIntegrationHttpShared,
    session_bindings: HttpsIntegrationSessionBindings,
}

pub struct HttpsIntegrationInboxTransportClient {
    shared: HttpsIntegrationHttpShared,
    session_bindings: HttpsIntegrationSessionBindings,
}

impl HttpsIntegrationTransportClient {
    pub fn new(
        config: HttpsIntegrationTransportConfig,
        auth_port: Arc<dyn IntegrationAuthPort>,
        proof_factory: Arc<dyn IntegrationRequestProofFactory>,
    ) -> Result<Self, CoreError> {
        let request_timeout = config.request_timeout;
        let client = reqwest::Client::builder()
            .timeout(request_timeout)
            .build()
            .map_err(|error| {
                CoreError::Network(format!(
                    "Failed to build integration transport HTTP client: {error}"
                ))
            })?;

        Ok(Self {
            config,
            shared: HttpsIntegrationHttpShared {
                client,
                proof_factory,
                request_timeout,
            },
            auth_port,
            session_bindings: HttpsIntegrationSessionBindings::default(),
        })
    }

    pub fn egress_transport(&self) -> HttpsIntegrationEgressTransportClient {
        HttpsIntegrationEgressTransportClient {
            shared: self.shared.clone(),
            session_bindings: self.session_bindings.clone(),
        }
    }

    pub fn inbox_transport(&self) -> HttpsIntegrationInboxTransportClient {
        HttpsIntegrationInboxTransportClient {
            shared: self.shared.clone(),
            session_bindings: self.session_bindings.clone(),
        }
    }
}

impl HttpsIntegrationHttpShared {
    async fn build_headers(
        &self,
        auth: &IntegrationAuthContext,
        method: &str,
        url: &str,
    ) -> Result<HeaderMap, CoreError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let auth_value = match auth.scheme {
            IntegrationAuthScheme::BearerToken => format!("Bearer {}", auth.access_token),
            IntegrationAuthScheme::DpopBearer => format!("DPoP {}", auth.access_token),
        };
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value).map_err(|error| CoreError::Validation {
                field: "integration.authorization".to_string(),
                message: format!("invalid authorization header value: {error}"),
            })?,
        );

        let maybe_proof = self.proof_factory.build_proof(auth, method, url).await?;
        if auth.scheme == IntegrationAuthScheme::DpopBearer {
            let proof = maybe_proof.ok_or_else(|| {
                CoreError::Auth(
                    "DPoP auth scheme requires a request proof, but none was provided.".to_string(),
                )
            })?;
            let name = HeaderName::from_bytes(proof.header_name.as_bytes()).map_err(|error| {
                CoreError::Validation {
                    field: "integration.request_proof.header_name".to_string(),
                    message: format!("invalid proof header name: {error}"),
                }
            })?;
            let value = HeaderValue::from_str(&proof.header_value).map_err(|error| {
                CoreError::Validation {
                    field: "integration.request_proof.header_value".to_string(),
                    message: format!("invalid proof header value: {error}"),
                }
            })?;
            headers.insert(name, value);
        } else if let Some(proof) = maybe_proof {
            let name = HeaderName::from_bytes(proof.header_name.as_bytes()).map_err(|error| {
                CoreError::Validation {
                    field: "integration.request_proof.header_name".to_string(),
                    message: format!("invalid proof header name: {error}"),
                }
            })?;
            let value = HeaderValue::from_str(&proof.header_value).map_err(|error| {
                CoreError::Validation {
                    field: "integration.request_proof.header_value".to_string(),
                    message: format!("invalid proof header value: {error}"),
                }
            })?;
            headers.insert(name, value);
        }

        Ok(headers)
    }

    async fn send_with_auth(
        &self,
        method: reqwest::Method,
        url: &str,
        auth: &IntegrationAuthContext,
        body: Option<&impl serde::Serialize>,
    ) -> Result<reqwest::Response, CoreError> {
        let headers = self.build_headers(auth, method.as_str(), url).await?;
        let mut request = self.client.request(method, url).headers(headers);
        if let Some(body) = body {
            request = request.json(body);
        }
        request.send().await.map_err(|error| {
            if error.is_timeout() {
                CoreError::RequestTimeout {
                    timeout_ms: self.request_timeout.as_millis() as u64,
                }
            } else {
                CoreError::Network(format!("integration transport request failed: {error}"))
            }
        })
    }

    async fn check_response(
        &self,
        response: reqwest::Response,
        context: &str,
    ) -> Result<reqwest::Response, CoreError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let body = response
            .text()
            .await
            .unwrap_or_else(|_| String::from("<unreadable response body>"));

        match status.as_u16() {
            401 | 403 => Err(CoreError::Auth(format!("{context}: {body}"))),
            429 => Err(CoreError::RateLimit {
                retry_after_secs: 60,
            }),
            503 => Err(CoreError::ServiceUnavailable(body)),
            _ => Err(CoreError::Network(format!(
                "{context}: HTTP {status} {body}"
            ))),
        }
    }

    fn validate_selected_transport(
        request: &IntegrationTransportConnectRequest,
        response: &IntegrationBootstrapResponse,
        transport_kind: &IntegrationTransportKind,
    ) -> Result<(), CoreError> {
        let client_supports = request.preferred_transports.contains(transport_kind);
        let server_advertises = response.supported_transports.is_empty()
            || response.supported_transports.contains(transport_kind);
        if client_supports && server_advertises {
            return Ok(());
        }

        Err(CoreError::Validation {
            field: "integration.bootstrap.selected_transport".to_string(),
            message: format!(
                "server selected unsupported transport: {:?}",
                transport_kind
            ),
        })
    }

    fn validate_selected_auth_scheme(
        request: &IntegrationTransportConnectRequest,
        response: &IntegrationBootstrapResponse,
        auth_scheme: &IntegrationAuthScheme,
    ) -> Result<(), CoreError> {
        let client_supports = request.supported_auth_schemes.contains(auth_scheme);
        let server_advertises = response.supported_auth_schemes.is_empty()
            || response.supported_auth_schemes.contains(auth_scheme);
        if client_supports && server_advertises {
            return Ok(());
        }

        Err(CoreError::Validation {
            field: "integration.bootstrap.selected_auth_scheme".to_string(),
            message: format!("server selected unsupported auth scheme: {:?}", auth_scheme),
        })
    }

    fn parse_granted_scopes(
        request: &IntegrationTransportConnectRequest,
        response: &IntegrationBootstrapResponse,
    ) -> Result<Vec<IntegrationCapabilityScope>, CoreError> {
        let mut granted = Vec::with_capacity(response.granted_scopes.len());
        for raw_scope in &response.granted_scopes {
            let scope = IntegrationCapabilityScope::parse(raw_scope).ok_or_else(|| {
                CoreError::Validation {
                    field: "integration.bootstrap.granted_scopes".to_string(),
                    message: format!("unknown granted scope: {raw_scope}"),
                }
            })?;
            if !request.requested_scopes.contains(&scope) {
                return Err(CoreError::Validation {
                    field: "integration.bootstrap.granted_scopes".to_string(),
                    message: format!("server granted an unexpected scope: {raw_scope}"),
                });
            }
            granted.push(scope);
        }
        Ok(granted)
    }
}

#[async_trait]
impl IntegrationTransportClient for HttpsIntegrationTransportClient {
    async fn connect(
        &self,
        request: IntegrationTransportConnectRequest,
    ) -> Result<IntegrationTransportConnectResponse, CoreError> {
        let resource_indicator = request.resource_indicator.clone();
        let auth = self
            .auth_port
            .resolve_session_auth(&request.requested_scopes, resource_indicator.as_deref())
            .await?;

        let bootstrap_request = IntegrationBootstrapRequest {
            client_version: request.client_version.clone(),
            device_id: Some(request.device_id.clone()),
            device_label: request.device_label.clone(),
            nonce: format!("nonce-{}", uuid::Uuid::new_v4()),
            requested_scopes: request
                .requested_scopes
                .iter()
                .map(IntegrationCapabilityScope::as_str)
                .map(str::to_string)
                .collect(),
            preferred_transports: request.preferred_transports.clone(),
            supported_auth_schemes: request.supported_auth_schemes.clone(),
            resource_indicator: resource_indicator.or_else(|| auth.resource_indicator.clone()),
        };

        let response = self
            .shared
            .send_with_auth(
                reqwest::Method::POST,
                &self.config.bootstrap_url,
                &auth,
                Some(&bootstrap_request),
            )
            .await?;
        let response = self
            .shared
            .check_response(response, "integration bootstrap request failed")
            .await?;
        let payload: IntegrationBootstrapResponse = response.json().await.map_err(|error| {
            CoreError::Serialization(serde_json::Error::io(std::io::Error::other(format!(
                "failed to parse integration bootstrap response: {error}"
            ))))
        })?;

        let session = payload
            .session
            .clone()
            .ok_or_else(|| CoreError::Validation {
                field: "integration.bootstrap.session".to_string(),
                message: "bootstrap response did not include a session binding.".to_string(),
            })?;

        let transport_kind = payload
            .selected_transport
            .clone()
            .or_else(|| request.preferred_transports.first().cloned())
            .unwrap_or_default();
        HttpsIntegrationHttpShared::validate_selected_transport(
            &request,
            &payload,
            &transport_kind,
        )?;

        let auth_scheme = payload
            .selected_auth_scheme
            .clone()
            .unwrap_or_else(|| auth.scheme.clone());
        HttpsIntegrationHttpShared::validate_selected_auth_scheme(
            &request,
            &payload,
            &auth_scheme,
        )?;

        let granted_scopes = HttpsIntegrationHttpShared::parse_granted_scopes(&request, &payload)?;
        let connected_at = Utc::now();
        let live_control_channel = if transport_kind == IntegrationTransportKind::WebSocket {
            let channel_url = session
                .channel_url
                .clone()
                .ok_or_else(|| CoreError::Validation {
                    field: "integration.bootstrap.session.channel_url".to_string(),
                    message: "websocket session transport requires a channel URL.".to_string(),
                })?;
            let headers = self
                .shared
                .build_headers(&auth, reqwest::Method::GET.as_str(), &channel_url)
                .await?;
            Some(Arc::new(
                WebSocketIntegrationSessionChannel::connect(&channel_url, headers).await?,
            ))
        } else {
            None
        };

        self.session_bindings
            .insert(
                session.session_id.clone(),
                SessionBinding {
                    heartbeat_url: session.heartbeat_url.clone(),
                    disconnect_url: session.disconnect_url.clone(),
                    send_events_url: session.send_events_url.clone(),
                    receive_prompts_url: session.receive_prompts_url.clone(),
                    auth,
                    live_session_channel: live_control_channel,
                },
            )
            .await;

        Ok(IntegrationTransportConnectResponse {
            session_id: session.session_id,
            connected_at,
            granted_scopes,
            transport_kind,
            auth_scheme,
        })
    }

    async fn heartbeat(&self, session_id: &str) -> Result<chrono::DateTime<Utc>, CoreError> {
        let binding =
            self.session_bindings
                .get(session_id)
                .await
                .ok_or_else(|| CoreError::NotFound {
                    resource_type: "integration_session".to_string(),
                    id: session_id.to_string(),
                })?;

        if let Some(channel) = binding.live_session_channel.clone() {
            let heartbeat = IntegrationSessionHeartbeatPayload {
                session_id: session_id.to_string(),
                occurred_at: Utc::now(),
                cursor_snapshot: Vec::new(),
            };
            channel.send_json(&heartbeat).await?;
            return Ok(heartbeat.occurred_at);
        }

        let url = binding.heartbeat_url.ok_or_else(|| CoreError::Validation {
            field: "integration.session.heartbeat_url".to_string(),
            message: "active integration session does not have a heartbeat URL.".to_string(),
        })?;

        let response = self
            .shared
            .send_with_auth(
                reqwest::Method::POST,
                &url,
                &binding.auth,
                Option::<&()>::None,
            )
            .await?;
        self.shared
            .check_response(response, "integration heartbeat request failed")
            .await?;
        Ok(Utc::now())
    }

    async fn disconnect(&self, session_id: &str) -> Result<(), CoreError> {
        let binding =
            self.session_bindings
                .get(session_id)
                .await
                .ok_or_else(|| CoreError::NotFound {
                    resource_type: "integration_session".to_string(),
                    id: session_id.to_string(),
                })?;

        if let Some(channel) = binding.live_session_channel.clone() {
            channel
                .send_json(&IntegrationSessionDisconnectPayload {
                    session_id: session_id.to_string(),
                    occurred_at: Utc::now(),
                    reason: None,
                })
                .await?;
            channel.close().await?;
            self.session_bindings.remove(session_id).await;
            return Ok(());
        }

        let url = binding
            .disconnect_url
            .ok_or_else(|| CoreError::Validation {
                field: "integration.session.disconnect_url".to_string(),
                message: "active integration session does not have a disconnect URL.".to_string(),
            })?;

        let response = self
            .shared
            .send_with_auth(
                reqwest::Method::DELETE,
                &url,
                &binding.auth,
                Option::<&()>::None,
            )
            .await?;
        self.shared
            .check_response(response, "integration disconnect request failed")
            .await?;
        self.session_bindings.remove(session_id).await;
        Ok(())
    }
}

#[derive(Debug, serde::Serialize)]
struct PromptPullRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    after_stream_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    after_cursor: Option<String>,
    #[serde(default)]
    limit: usize,
}

#[derive(Debug, serde::Deserialize)]
struct PromptPullResponse {
    #[serde(default)]
    events: Vec<super::IntegrationCloudEvent<ProactivePrompt>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ack_cursor: Option<IntegrationAckCursor>,
}

#[async_trait]
impl IntegrationEgressTransportClient for HttpsIntegrationEgressTransportClient {
    async fn send_messages(
        &self,
        session_id: &str,
        items: Vec<QueuedIntegrationEgressMessage>,
    ) -> Result<IntegrationEgressTransportResponse, CoreError> {
        let binding =
            self.session_bindings
                .get(session_id)
                .await
                .ok_or_else(|| CoreError::NotFound {
                    resource_type: "integration_session".to_string(),
                    id: session_id.to_string(),
                })?;
        if let Some(channel) = binding.live_session_channel.clone() {
            for item in &items {
                channel
                    .send_json(&outbound_message_to_cloudevent(
                        &item.envelope,
                        &item.payload,
                        Some(&item.queue_id),
                    )?)
                    .await?;
            }
            let expected_queue_ids = items
                .iter()
                .map(|item| item.queue_id.clone())
                .collect::<Vec<_>>();
            return channel
                .wait_for_outbound_ack(&expected_queue_ids, self.shared.request_timeout)
                .await;
        }

        let url = binding
            .send_events_url
            .ok_or_else(|| CoreError::Validation {
                field: "integration.session.send_events_url".to_string(),
                message: "active integration session does not have an outbound event URL."
                    .to_string(),
            })?;

        let mut batch_items = Vec::with_capacity(items.len());
        for item in &items {
            batch_items.push(IntegrationOutboundCloudEventBatchItem {
                queue_id: item.queue_id.clone(),
                event: outbound_message_to_cloudevent(
                    &item.envelope,
                    &item.payload,
                    Some(&item.queue_id),
                )?,
            });
        }
        let batch = IntegrationOutboundCloudEventBatch { items: batch_items };

        let response = self
            .shared
            .send_with_auth(reqwest::Method::POST, &url, &binding.auth, Some(&batch))
            .await?;
        let response = self
            .shared
            .check_response(response, "integration outbound event request failed")
            .await?;

        #[derive(serde::Deserialize)]
        struct OutboundEventResponseBody {
            #[serde(default)]
            accepted_ids: Vec<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            ack_cursor: Option<IntegrationAckCursor>,
        }

        let payload: OutboundEventResponseBody = response.json().await.map_err(|error| {
            CoreError::Serialization(serde_json::Error::io(std::io::Error::other(format!(
                "failed to parse integration outbound event response: {error}"
            ))))
        })?;

        Ok(IntegrationEgressTransportResponse {
            acknowledged_queue_ids: payload.accepted_ids,
            ack_cursor: payload.ack_cursor,
        })
    }
}

#[async_trait]
impl IntegrationInboxTransportClient for HttpsIntegrationInboxTransportClient {
    async fn receive_prompts(
        &self,
        session_id: &str,
        after_cursor: Option<IntegrationAckCursor>,
        limit: usize,
    ) -> Result<IntegrationInboxTransportResponse, CoreError> {
        let binding =
            self.session_bindings
                .get(session_id)
                .await
                .ok_or_else(|| CoreError::NotFound {
                    resource_type: "integration_session".to_string(),
                    id: session_id.to_string(),
                })?;
        if let Some(channel) = binding.live_session_channel.clone() {
            return Ok(IntegrationInboxTransportResponse {
                prompts: channel.drain_prompts(limit).await,
                ack_cursor: None,
            });
        }

        let url = binding
            .receive_prompts_url
            .ok_or_else(|| CoreError::Validation {
                field: "integration.session.receive_prompts_url".to_string(),
                message: "active integration session does not have a prompt receive URL."
                    .to_string(),
            })?;

        let request = PromptPullRequest {
            after_stream_id: after_cursor.as_ref().map(|cursor| cursor.stream_id.clone()),
            after_cursor: after_cursor.map(|cursor| cursor.cursor),
            limit,
        };

        let response = self
            .shared
            .send_with_auth(reqwest::Method::POST, &url, &binding.auth, Some(&request))
            .await?;
        let response = self
            .shared
            .check_response(response, "integration prompt pull request failed")
            .await?;
        let payload: PromptPullResponse = response.json().await.map_err(|error| {
            CoreError::Serialization(serde_json::Error::io(std::io::Error::other(format!(
                "failed to parse integration prompt pull response: {error}"
            ))))
        })?;

        let mut prompts = Vec::with_capacity(payload.events.len());
        for event in payload.events {
            prompts.push(prompt_from_cloudevent(event)?);
        }

        Ok(IntegrationInboxTransportResponse {
            prompts,
            ack_cursor: payload.ack_cursor,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc as StdArc, Mutex as StdMutex};

    use super::*;
    use crate::integration::IntegrationRequestProof;

    use async_trait::async_trait;
    use futures::{SinkExt, StreamExt};
    use mockito::Matcher;
    use oneshim_core::models::integration::{
        InsightPacket, InsightSourceWindow, IntegrationEnvelope, IntegrationMessageType,
        IntegrationOrigin, IntegrationOutboundPayload, IntegrationPrivacyClassification,
        ProactivePromptCategory, ProactivePromptPriority, QueuedIntegrationEgressMessage,
    };
    use tokio::net::TcpListener;
    use tokio::sync::{mpsc, Mutex};
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
    use tokio_tungstenite::tungstenite::Message;

    struct StaticAuthPort {
        context: IntegrationAuthContext,
    }

    #[async_trait]
    impl IntegrationAuthPort for StaticAuthPort {
        async fn resolve_session_auth(
            &self,
            _requested_scopes: &[IntegrationCapabilityScope],
            _resource_indicator: Option<&str>,
        ) -> Result<IntegrationAuthContext, CoreError> {
            Ok(self.context.clone())
        }

        async fn current_auth_status(
            &self,
        ) -> Result<oneshim_core::models::integration::IntegrationAuthStatus, CoreError> {
            Ok(oneshim_core::models::integration::IntegrationAuthStatus {
                profile_kind:
                    oneshim_core::models::integration::IntegrationAuthProfileKind::EnvToken,
                status: oneshim_core::models::integration::IntegrationAuthStatusKind::Ready,
                interactive: false,
                authenticated: true,
                expires_at: self.context.expires_at,
                resource_indicator: self.context.resource_indicator.clone(),
                pending_flow: None,
                message: None,
            })
        }

        async fn start_device_authorization(
            &self,
            _requested_scopes: &[IntegrationCapabilityScope],
            _resource_indicator: Option<&str>,
        ) -> Result<oneshim_core::models::integration::IntegrationDeviceAuthorizationFlow, CoreError>
        {
            Err(CoreError::InvalidArguments(
                "static auth port does not support device authorization".to_string(),
            ))
        }

        async fn poll_device_authorization(
            &self,
            _flow_id: &str,
        ) -> Result<oneshim_core::models::integration::IntegrationAuthStatus, CoreError> {
            self.current_auth_status().await
        }

        async fn cancel_device_authorization(&self, _flow_id: &str) -> Result<(), CoreError> {
            Ok(())
        }
    }

    struct RecordingProofFactory {
        returned: Option<IntegrationRequestProof>,
        calls: Arc<Mutex<Vec<(String, String)>>>,
    }

    #[async_trait]
    impl IntegrationRequestProofFactory for RecordingProofFactory {
        async fn build_proof(
            &self,
            _auth: &IntegrationAuthContext,
            method: &str,
            url: &str,
        ) -> Result<Option<IntegrationRequestProof>, CoreError> {
            self.calls
                .lock()
                .await
                .push((method.to_string(), url.to_string()));
            Ok(self.returned.clone())
        }
    }

    fn connect_request(server_url: &str) -> IntegrationTransportConnectRequest {
        IntegrationTransportConnectRequest {
            device_id: "device-001".to_string(),
            client_version: "0.3.8".to_string(),
            device_label: Some("macbook".to_string()),
            requested_scopes: vec![
                IntegrationCapabilityScope::InsightWrite,
                IntegrationCapabilityScope::SessionManage,
            ],
            preferred_transports: vec![
                IntegrationTransportKind::WebSocket,
                IntegrationTransportKind::HttpsLongPoll,
            ],
            supported_auth_schemes: vec![
                IntegrationAuthScheme::DpopBearer,
                IntegrationAuthScheme::BearerToken,
            ],
            resource_indicator: Some(server_url.to_string()),
        }
    }

    async fn start_session_ws_server(
        auto_ack_insights: bool,
    ) -> (
        String,
        StdArc<StdMutex<Vec<String>>>,
        StdArc<StdMutex<Vec<(String, String)>>>,
        mpsc::UnboundedSender<String>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let messages = StdArc::new(StdMutex::new(Vec::new()));
        let headers = StdArc::new(StdMutex::new(Vec::new()));
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<String>();
        let messages_task = messages.clone();
        let headers_task = headers.clone();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let websocket =
                accept_hdr_async(stream, move |request: &Request, response: Response| {
                    if let Some(value) = request.headers().get("authorization") {
                        headers_task.lock().unwrap().push((
                            "authorization".to_string(),
                            value.to_str().unwrap_or_default().to_string(),
                        ));
                    }
                    if let Some(value) = request.headers().get("dpop") {
                        headers_task.lock().unwrap().push((
                            "dpop".to_string(),
                            value.to_str().unwrap_or_default().to_string(),
                        ));
                    }
                    Ok(response)
                })
                .await
                .unwrap();

            let (mut writer, mut reader) = websocket.split();
            loop {
                tokio::select! {
                    maybe_outbound = outbound_rx.recv() => {
                        let Some(outbound) = maybe_outbound else {
                            break;
                        };
                        writer.send(Message::Text(outbound.into())).await.unwrap();
                    }
                    maybe_message = reader.next() => {
                        let Some(message) = maybe_message else {
                            break;
                        };
                        match message.unwrap() {
                            Message::Text(text) => {
                                let text = text.to_string();
                                messages_task.lock().unwrap().push(text.clone());
                                if auto_ack_insights {
                                    if let Ok(event) = serde_json::from_str::<crate::integration::cloudevents::IntegrationCloudEvent<InsightPacket>>(&text) {
                                        if let Some(queue_id) = event.oneshimqueueid {
                                            let ack = serde_json::json!({
                                                "session_id": event.oneshimsessionid.unwrap_or_else(|| "session-test".to_string()),
                                                "acknowledged_ids": [queue_id],
                                            });
                                            writer.send(Message::Text(ack.to_string().into())).await.unwrap();
                                        }
                                    }
                                }
                            }
                            Message::Close(_) => break,
                            _ => {}
                        }
                    }
                }
            }
        });

        (
            format!("ws://{address}/integration/session-control"),
            messages,
            headers,
            outbound_tx,
        )
    }

    #[tokio::test]
    async fn connect_bootstraps_with_bearer_auth() {
        let mut server = mockito::Server::new_async().await;
        let heartbeat_url = format!(
            "{}/integration/sessions/session-001/heartbeat",
            server.url()
        );
        let disconnect_url = format!("{}/integration/sessions/session-001", server.url());

        let bootstrap = server
            .mock("POST", "/integration/bootstrap")
            .match_header("authorization", "Bearer access-token")
            .match_body(Matcher::PartialJson(serde_json::json!({
                "client_version": "0.3.8",
                "device_id": "device-001"
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "schema_version": "integration.bootstrap.v1",
                    "supported_scopes": ["insight:write", "session:manage"],
                    "granted_scopes": ["insight:write", "session:manage"],
                    "supported_transports": ["https_long_poll"],
                    "selected_transport": "https_long_poll",
                    "supported_auth_schemes": ["bearer_token"],
                    "selected_auth_scheme": "bearer_token",
                    "resource_indicator": server.url(),
                    "session_required": true,
                    "session": {
                        "session_id": "session-001",
                        "heartbeat_url": heartbeat_url,
                        "disconnect_url": disconnect_url
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = HttpsIntegrationTransportClient::new(
            HttpsIntegrationTransportConfig::new(
                format!("{}/integration/bootstrap", server.url()),
                Duration::from_secs(5),
            ),
            Arc::new(StaticAuthPort {
                context: IntegrationAuthContext {
                    access_token: "access-token".to_string(),
                    scheme: IntegrationAuthScheme::BearerToken,
                    expires_at: None,
                    resource_indicator: None,
                },
            }),
            Arc::new(RecordingProofFactory {
                returned: None,
                calls: Arc::new(Mutex::new(Vec::new())),
            }),
        )
        .unwrap();

        let response = client
            .connect(connect_request(&server.url()))
            .await
            .unwrap();
        bootstrap.assert_async().await;
        assert_eq!(response.session_id, "session-001");
        assert_eq!(
            response.transport_kind,
            IntegrationTransportKind::HttpsLongPoll
        );
        assert_eq!(response.auth_scheme, IntegrationAuthScheme::BearerToken);
    }

    #[tokio::test]
    async fn connect_heartbeat_and_disconnect_use_dpop_websocket_control_channel() {
        let mut server = mockito::Server::new_async().await;
        let bootstrap_url = format!("{}/integration/bootstrap", server.url());
        let proof_calls = Arc::new(Mutex::new(Vec::new()));
        let (channel_url, live_messages, live_headers, _outbound_tx) =
            start_session_ws_server(false).await;

        let bootstrap = server
            .mock("POST", "/integration/bootstrap")
            .match_header("authorization", "DPoP access-token")
            .match_header("dpop", "proof-token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "schema_version": "integration.bootstrap.v1",
                    "supported_scopes": ["insight:write", "session:manage"],
                    "granted_scopes": ["insight:write", "session:manage"],
                    "supported_transports": ["web_socket"],
                    "selected_transport": "web_socket",
                    "supported_auth_schemes": ["dpop_bearer"],
                    "selected_auth_scheme": "dpop_bearer",
                    "resource_indicator": server.url(),
                    "session_required": true,
                    "session": {
                        "session_id": "session-002",
                        "channel_url": channel_url
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = HttpsIntegrationTransportClient::new(
            HttpsIntegrationTransportConfig::new(bootstrap_url.clone(), Duration::from_secs(5)),
            Arc::new(StaticAuthPort {
                context: IntegrationAuthContext {
                    access_token: "access-token".to_string(),
                    scheme: IntegrationAuthScheme::DpopBearer,
                    expires_at: None,
                    resource_indicator: Some(server.url()),
                },
            }),
            Arc::new(RecordingProofFactory {
                returned: Some(IntegrationRequestProof {
                    header_name: "dpop".to_string(),
                    header_value: "proof-token".to_string(),
                }),
                calls: proof_calls.clone(),
            }),
        )
        .unwrap();

        let response = client
            .connect(connect_request(&server.url()))
            .await
            .unwrap();
        client.heartbeat(&response.session_id).await.unwrap();
        client.disconnect(&response.session_id).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        bootstrap.assert_async().await;

        let calls = proof_calls.lock().await;
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "POST");
        assert_eq!(calls[1].0, "GET");

        let live_headers = live_headers.lock().unwrap().clone();
        assert!(live_headers
            .iter()
            .any(|(name, value)| { name == "authorization" && value == "DPoP access-token" }));
        assert!(live_headers
            .iter()
            .any(|(name, value)| name == "dpop" && value == "proof-token"));

        let live_messages = live_messages.lock().unwrap().clone();
        assert_eq!(live_messages.len(), 2);
        let heartbeat: IntegrationSessionHeartbeatPayload =
            serde_json::from_str(&live_messages[0]).unwrap();
        assert_eq!(heartbeat.session_id, "session-002");
        let disconnect: IntegrationSessionDisconnectPayload =
            serde_json::from_str(&live_messages[1]).unwrap();
        assert_eq!(disconnect.session_id, "session-002");
    }

    #[tokio::test]
    async fn connect_rejects_unexpected_granted_scope() {
        let mut server = mockito::Server::new_async().await;
        let bootstrap = server
            .mock("POST", "/integration/bootstrap")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "schema_version": "integration.bootstrap.v1",
                    "supported_scopes": ["insight:write", "policy:read"],
                    "granted_scopes": ["policy:read"],
                    "supported_transports": ["https_long_poll"],
                    "selected_transport": "https_long_poll",
                    "supported_auth_schemes": ["bearer_token"],
                    "selected_auth_scheme": "bearer_token",
                    "session_required": true,
                    "session": {
                        "session_id": "session-003"
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = HttpsIntegrationTransportClient::new(
            HttpsIntegrationTransportConfig::new(
                format!("{}/integration/bootstrap", server.url()),
                Duration::from_secs(5),
            ),
            Arc::new(StaticAuthPort {
                context: IntegrationAuthContext {
                    access_token: "access-token".to_string(),
                    scheme: IntegrationAuthScheme::BearerToken,
                    expires_at: None,
                    resource_indicator: None,
                },
            }),
            Arc::new(RecordingProofFactory {
                returned: None,
                calls: Arc::new(Mutex::new(Vec::new())),
            }),
        )
        .unwrap();

        let err = client
            .connect(connect_request(&server.url()))
            .await
            .expect_err("unexpected scope should fail");
        bootstrap.assert_async().await;

        match err {
            CoreError::Validation { field, .. } => {
                assert_eq!(field, "integration.bootstrap.granted_scopes");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[tokio::test]
    async fn egress_transport_posts_outbound_cloudevents() {
        let mut server = mockito::Server::new_async().await;
        let events_url = format!("{}/integration/sessions/session-010/events", server.url());

        let bootstrap = server
            .mock("POST", "/integration/bootstrap")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "schema_version": "integration.bootstrap.v1",
                    "supported_scopes": ["insight:write", "session:manage"],
                    "granted_scopes": ["insight:write", "session:manage"],
                    "supported_transports": ["https_long_poll"],
                    "selected_transport": "https_long_poll",
                    "supported_auth_schemes": ["bearer_token"],
                    "selected_auth_scheme": "bearer_token",
                    "session_required": true,
                    "session": {
                        "session_id": "session-010",
                        "send_events_url": events_url
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let egress = server
            .mock("POST", "/integration/sessions/session-010/events")
            .match_header("authorization", "Bearer access-token")
            .match_body(Matcher::PartialJson(serde_json::json!({
                "items": [{
                    "queue_id": "queue-010",
                    "event": {
                        "type": "io.oneshim.integration.insight.v1",
                        "oneshimscope": "insight:write",
                        "data": {
                            "packet_id": "packet-010"
                        }
                    }
                }]
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "accepted_ids": ["queue-010"]
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = HttpsIntegrationTransportClient::new(
            HttpsIntegrationTransportConfig::new(
                format!("{}/integration/bootstrap", server.url()),
                Duration::from_secs(5),
            ),
            Arc::new(StaticAuthPort {
                context: IntegrationAuthContext {
                    access_token: "access-token".to_string(),
                    scheme: IntegrationAuthScheme::BearerToken,
                    expires_at: None,
                    resource_indicator: None,
                },
            }),
            Arc::new(RecordingProofFactory {
                returned: None,
                calls: Arc::new(Mutex::new(Vec::new())),
            }),
        )
        .unwrap();

        let session = client
            .connect(connect_request(&server.url()))
            .await
            .unwrap();
        let response = client
            .egress_transport()
            .send_messages(
                &session.session_id,
                vec![QueuedIntegrationEgressMessage {
                    queue_id: "queue-010".to_string(),
                    envelope: IntegrationEnvelope {
                        envelope_id: "env-010".to_string(),
                        schema_version: "integration.envelope.v1".to_string(),
                        message_type: IntegrationMessageType::InsightPacket,
                        timestamp: Utc::now(),
                        nonce: "nonce-010".to_string(),
                        origin: IntegrationOrigin {
                            device_id: "device-001".to_string(),
                            workspace_id: None,
                            session_id: Some("session-010".to_string()),
                            source: "desktop-client".to_string(),
                        },
                        capability_scope: IntegrationCapabilityScope::InsightWrite,
                    },
                    payload: IntegrationOutboundPayload::Insight(InsightPacket {
                        packet_id: "packet-010".to_string(),
                        summary: "summary".to_string(),
                        derived_tags: vec!["focus".to_string()],
                        source_window: InsightSourceWindow {
                            started_at: Utc::now(),
                            ended_at: Utc::now(),
                        },
                        privacy_classification: IntegrationPrivacyClassification::DerivedSummary,
                        audit_reference_id: None,
                    }),
                    queued_at: Utc::now(),
                }],
            )
            .await
            .unwrap();

        bootstrap.assert_async().await;
        egress.assert_async().await;
        assert_eq!(
            response.acknowledged_queue_ids,
            vec!["queue-010".to_string()]
        );
    }

    #[tokio::test]
    async fn egress_transport_uses_websocket_channel_with_queue_id_extension() {
        let mut server = mockito::Server::new_async().await;
        let (channel_url, live_messages, _live_headers, _outbound_tx) =
            start_session_ws_server(true).await;

        let bootstrap = server
            .mock("POST", "/integration/bootstrap")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "schema_version": "integration.bootstrap.v1",
                    "supported_scopes": ["insight:write", "session:manage"],
                    "granted_scopes": ["insight:write", "session:manage"],
                    "supported_transports": ["web_socket"],
                    "selected_transport": "web_socket",
                    "supported_auth_schemes": ["bearer_token"],
                    "selected_auth_scheme": "bearer_token",
                    "session_required": true,
                    "session": {
                        "session_id": "session-010-ws",
                        "channel_url": channel_url
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = HttpsIntegrationTransportClient::new(
            HttpsIntegrationTransportConfig::new(
                format!("{}/integration/bootstrap", server.url()),
                Duration::from_secs(5),
            ),
            Arc::new(StaticAuthPort {
                context: IntegrationAuthContext {
                    access_token: "access-token".to_string(),
                    scheme: IntegrationAuthScheme::BearerToken,
                    expires_at: None,
                    resource_indicator: None,
                },
            }),
            Arc::new(RecordingProofFactory {
                returned: None,
                calls: Arc::new(Mutex::new(Vec::new())),
            }),
        )
        .unwrap();

        let session = client
            .connect(connect_request(&server.url()))
            .await
            .unwrap();
        let response = client
            .egress_transport()
            .send_messages(
                &session.session_id,
                vec![QueuedIntegrationEgressMessage {
                    queue_id: "queue-010-ws".to_string(),
                    envelope: IntegrationEnvelope {
                        envelope_id: "env-010-ws".to_string(),
                        schema_version: "integration.envelope.v1".to_string(),
                        message_type: IntegrationMessageType::InsightPacket,
                        timestamp: Utc::now(),
                        nonce: "nonce-010-ws".to_string(),
                        origin: IntegrationOrigin {
                            device_id: "device-001".to_string(),
                            workspace_id: None,
                            session_id: Some("session-010-ws".to_string()),
                            source: "desktop-client".to_string(),
                        },
                        capability_scope: IntegrationCapabilityScope::InsightWrite,
                    },
                    payload: IntegrationOutboundPayload::Insight(InsightPacket {
                        packet_id: "packet-010-ws".to_string(),
                        summary: "summary".to_string(),
                        derived_tags: vec!["focus".to_string()],
                        source_window: InsightSourceWindow {
                            started_at: Utc::now(),
                            ended_at: Utc::now(),
                        },
                        privacy_classification: IntegrationPrivacyClassification::DerivedSummary,
                        audit_reference_id: None,
                    }),
                    queued_at: Utc::now(),
                }],
            )
            .await
            .unwrap();

        bootstrap.assert_async().await;
        assert_eq!(
            response.acknowledged_queue_ids,
            vec!["queue-010-ws".to_string()]
        );

        let live_messages = live_messages.lock().unwrap().clone();
        let event: crate::integration::cloudevents::IntegrationCloudEvent<InsightPacket> =
            serde_json::from_str(&live_messages[0]).unwrap();
        assert_eq!(event.oneshimqueueid.as_deref(), Some("queue-010-ws"));
    }

    #[tokio::test]
    async fn egress_transport_posts_prompt_receipt_cloudevents() {
        let mut server = mockito::Server::new_async().await;
        let events_url = format!("{}/integration/sessions/session-011/events", server.url());

        let bootstrap = server
            .mock("POST", "/integration/bootstrap")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "schema_version": "integration.bootstrap.v1",
                    "supported_scopes": ["prompt:ack", "session:manage"],
                    "granted_scopes": ["prompt:ack", "session:manage"],
                    "supported_transports": ["https_long_poll"],
                    "selected_transport": "https_long_poll",
                    "supported_auth_schemes": ["bearer_token"],
                    "selected_auth_scheme": "bearer_token",
                    "session_required": true,
                    "session": {
                        "session_id": "session-011",
                        "send_events_url": events_url
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let egress = server
            .mock("POST", "/integration/sessions/session-011/events")
            .match_header("authorization", "Bearer access-token")
            .match_body(Matcher::PartialJson(serde_json::json!({
                "items": [{
                    "queue_id": "queue-011",
                    "event": {
                        "type": "io.oneshim.integration.prompt_receipt.v1",
                        "oneshimscope": "prompt:ack",
                        "data": {
                            "prompt_id": "prompt-011",
                            "action": "acknowledged"
                        }
                    }
                }]
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "accepted_ids": ["queue-011"]
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = HttpsIntegrationTransportClient::new(
            HttpsIntegrationTransportConfig::new(
                format!("{}/integration/bootstrap", server.url()),
                Duration::from_secs(5),
            ),
            Arc::new(StaticAuthPort {
                context: IntegrationAuthContext {
                    access_token: "access-token".to_string(),
                    scheme: IntegrationAuthScheme::BearerToken,
                    expires_at: None,
                    resource_indicator: None,
                },
            }),
            Arc::new(RecordingProofFactory {
                returned: None,
                calls: Arc::new(Mutex::new(Vec::new())),
            }),
        )
        .unwrap();

        let session = client
            .connect(IntegrationTransportConnectRequest {
                device_id: "device-001".to_string(),
                client_version: "0.3.8".to_string(),
                device_label: Some("macbook".to_string()),
                requested_scopes: vec![
                    IntegrationCapabilityScope::PromptAck,
                    IntegrationCapabilityScope::SessionManage,
                ],
                preferred_transports: vec![IntegrationTransportKind::HttpsLongPoll],
                supported_auth_schemes: vec![IntegrationAuthScheme::BearerToken],
                resource_indicator: Some(server.url()),
            })
            .await
            .unwrap();

        let response = client
            .egress_transport()
            .send_messages(
                &session.session_id,
                vec![QueuedIntegrationEgressMessage {
                    queue_id: "queue-011".to_string(),
                    envelope: IntegrationEnvelope {
                        envelope_id: "env-011".to_string(),
                        schema_version: "integration.prompt_receipt.v1".to_string(),
                        message_type: IntegrationMessageType::PromptReceipt,
                        timestamp: Utc::now(),
                        nonce: "nonce-011".to_string(),
                        origin: IntegrationOrigin {
                            device_id: "device-001".to_string(),
                            workspace_id: None,
                            session_id: Some("session-011".to_string()),
                            source: "desktop-client".to_string(),
                        },
                        capability_scope: IntegrationCapabilityScope::PromptAck,
                    },
                    payload: IntegrationOutboundPayload::PromptReceipt(
                        oneshim_core::models::integration::IntegrationPromptReceipt {
                            receipt_id: "receipt-011".to_string(),
                            prompt_id: "prompt-011".to_string(),
                            action:
                                oneshim_core::models::integration::IntegrationPromptReceiptAction::Acknowledged,
                            occurred_at: Utc::now(),
                            reason: None,
                        },
                    ),
                    queued_at: Utc::now(),
                }],
            )
            .await
            .unwrap();

        bootstrap.assert_async().await;
        egress.assert_async().await;
        assert_eq!(
            response.acknowledged_queue_ids,
            vec!["queue-011".to_string()]
        );
    }

    #[tokio::test]
    async fn inbox_transport_parses_prompt_cloudevents() {
        let mut server = mockito::Server::new_async().await;
        let prompts_url = format!("{}/integration/sessions/session-011/prompts", server.url());

        let bootstrap = server
            .mock("POST", "/integration/bootstrap")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "schema_version": "integration.bootstrap.v1",
                    "supported_scopes": ["prompt:read", "session:manage"],
                    "granted_scopes": ["prompt:read", "session:manage"],
                    "supported_transports": ["https_long_poll"],
                    "selected_transport": "https_long_poll",
                    "supported_auth_schemes": ["bearer_token"],
                    "selected_auth_scheme": "bearer_token",
                    "session_required": true,
                    "session": {
                        "session_id": "session-011",
                        "receive_prompts_url": prompts_url
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let pull = server
            .mock("POST", "/integration/sessions/session-011/prompts")
            .match_header("authorization", "Bearer access-token")
            .match_body(Matcher::PartialJson(serde_json::json!({
                "after_stream_id": "prompt",
                "after_cursor": "cursor-0",
                "limit": 10
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "events": [{
                        "specversion": "1.0",
                        "id": "prompt-env-1",
                        "source": "oneshim://devices/device-001",
                        "type": "io.oneshim.integration.prompt.v1",
                        "subject": "prompt-011",
                        "time": Utc::now(),
                        "datacontenttype": "application/json",
                        "data": {
                            "prompt_id": "prompt-011",
                            "category": "task",
                            "title": "title",
                            "body": "body",
                            "priority": "medium",
                            "actions": [],
                            "provenance": {
                                "source_system": "integration"
                            }
                        },
                        "oneshimscope": "prompt:read",
                        "oneshimnonce": "nonce-011",
                        "oneshimsessionid": "session-011",
                        "oneshimpromptcategory": "task"
                    }],
                    "ack_cursor": {
                        "stream_id": "prompt",
                        "cursor": "cursor-1",
                        "acknowledged_at": Utc::now()
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = HttpsIntegrationTransportClient::new(
            HttpsIntegrationTransportConfig::new(
                format!("{}/integration/bootstrap", server.url()),
                Duration::from_secs(5),
            ),
            Arc::new(StaticAuthPort {
                context: IntegrationAuthContext {
                    access_token: "access-token".to_string(),
                    scheme: IntegrationAuthScheme::BearerToken,
                    expires_at: None,
                    resource_indicator: None,
                },
            }),
            Arc::new(RecordingProofFactory {
                returned: None,
                calls: Arc::new(Mutex::new(Vec::new())),
            }),
        )
        .unwrap();

        let session = client
            .connect(IntegrationTransportConnectRequest {
                device_id: "device-001".to_string(),
                client_version: "0.3.8".to_string(),
                device_label: Some("macbook".to_string()),
                requested_scopes: vec![
                    IntegrationCapabilityScope::PromptRead,
                    IntegrationCapabilityScope::SessionManage,
                ],
                preferred_transports: vec![IntegrationTransportKind::HttpsLongPoll],
                supported_auth_schemes: vec![IntegrationAuthScheme::BearerToken],
                resource_indicator: Some(server.url()),
            })
            .await
            .unwrap();

        let response = client
            .inbox_transport()
            .receive_prompts(
                &session.session_id,
                Some(IntegrationAckCursor {
                    stream_id: "prompt".to_string(),
                    cursor: "cursor-0".to_string(),
                    acknowledged_at: Utc::now(),
                }),
                10,
            )
            .await
            .unwrap();

        bootstrap.assert_async().await;
        pull.assert_async().await;
        assert_eq!(response.prompts.len(), 1);
        assert_eq!(response.prompts[0].prompt_id, "prompt-011");
        assert_eq!(
            response
                .ack_cursor
                .as_ref()
                .map(|cursor| cursor.cursor.as_str()),
            Some("cursor-1")
        );
    }

    #[tokio::test]
    async fn inbox_transport_drains_websocket_prompt_events() {
        let mut server = mockito::Server::new_async().await;
        let (channel_url, _live_messages, _live_headers, outbound_tx) =
            start_session_ws_server(false).await;

        let bootstrap = server
            .mock("POST", "/integration/bootstrap")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "schema_version": "integration.bootstrap.v1",
                    "supported_scopes": ["prompt:read", "session:manage"],
                    "granted_scopes": ["prompt:read", "session:manage"],
                    "supported_transports": ["web_socket"],
                    "selected_transport": "web_socket",
                    "supported_auth_schemes": ["bearer_token"],
                    "selected_auth_scheme": "bearer_token",
                    "session_required": true,
                    "session": {
                        "session_id": "session-011-ws",
                        "channel_url": channel_url
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = HttpsIntegrationTransportClient::new(
            HttpsIntegrationTransportConfig::new(
                format!("{}/integration/bootstrap", server.url()),
                Duration::from_secs(5),
            ),
            Arc::new(StaticAuthPort {
                context: IntegrationAuthContext {
                    access_token: "access-token".to_string(),
                    scheme: IntegrationAuthScheme::BearerToken,
                    expires_at: None,
                    resource_indicator: None,
                },
            }),
            Arc::new(RecordingProofFactory {
                returned: None,
                calls: Arc::new(Mutex::new(Vec::new())),
            }),
        )
        .unwrap();

        let session = client
            .connect(IntegrationTransportConnectRequest {
                device_id: "device-001".to_string(),
                client_version: "0.3.8".to_string(),
                device_label: Some("macbook".to_string()),
                requested_scopes: vec![
                    IntegrationCapabilityScope::PromptRead,
                    IntegrationCapabilityScope::SessionManage,
                ],
                preferred_transports: vec![IntegrationTransportKind::WebSocket],
                supported_auth_schemes: vec![IntegrationAuthScheme::BearerToken],
                resource_indicator: Some(server.url()),
            })
            .await
            .unwrap();

        outbound_tx
            .send(
                serde_json::json!({
                    "events": [{
                        "specversion": "1.0",
                        "id": "prompt-env-ws-1",
                        "source": "oneshim://devices/device-001",
                        "type": "io.oneshim.integration.prompt.v1",
                        "subject": "prompt-011-ws",
                        "time": Utc::now(),
                        "datacontenttype": "application/json",
                        "data": {
                            "prompt_id": "prompt-011-ws",
                            "category": "task",
                            "title": "title",
                            "body": "body",
                            "priority": "medium",
                            "actions": [],
                            "provenance": {
                                "source_system": "integration"
                            }
                        },
                        "oneshimscope": "prompt:read",
                        "oneshimnonce": "nonce-011-ws",
                        "oneshimsessionid": "session-011-ws",
                        "oneshimpromptcategory": "task"
                    }, {
                        "specversion": "1.0",
                        "id": "prompt-env-ws-2",
                        "source": "oneshim://devices/device-001",
                        "type": "io.oneshim.integration.prompt.v1",
                        "subject": "prompt-012-ws",
                        "time": Utc::now(),
                        "datacontenttype": "application/json",
                        "data": {
                            "prompt_id": "prompt-012-ws",
                            "category": "reminder",
                            "title": "title-2",
                            "body": "body-2",
                            "priority": "low",
                            "actions": [],
                            "provenance": {
                                "source_system": "integration"
                            }
                        }
                    ,
                        "oneshimscope": "prompt:read",
                        "oneshimnonce": "nonce-012-ws",
                        "oneshimsessionid": "session-011-ws",
                        "oneshimpromptcategory": "reminder"
                    }]
                })
                .to_string(),
            )
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        let response = client
            .inbox_transport()
            .receive_prompts(&session.session_id, None, 10)
            .await
            .unwrap();

        bootstrap.assert_async().await;
        assert_eq!(response.prompts.len(), 2);
        assert_eq!(response.prompts[0].prompt_id, "prompt-011-ws");
        assert_eq!(response.prompts[0].category, ProactivePromptCategory::Task);
        assert_eq!(
            response.prompts[0].priority,
            ProactivePromptPriority::Medium
        );
        assert_eq!(response.prompts[1].prompt_id, "prompt-012-ws");
        assert!(response.ack_cursor.is_none());
    }
}
