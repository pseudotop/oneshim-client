use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use oneshim_api_contracts::integration::{
    IntegrationBootstrapRequest, IntegrationBootstrapResponse,
};
use oneshim_core::error::CoreError;
use oneshim_core::models::integration::{
    IntegrationAuthContext, IntegrationAuthScheme, IntegrationCapabilityScope,
    IntegrationTransportKind,
};
use oneshim_core::ports::integration::IntegrationAuthPort;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use tokio::sync::RwLock;

use super::transport::{
    IntegrationRequestProofFactory, IntegrationTransportClient, IntegrationTransportConnectRequest,
    IntegrationTransportConnectResponse,
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

#[derive(Debug, Clone)]
struct SessionBinding {
    heartbeat_url: Option<String>,
    disconnect_url: Option<String>,
    auth: IntegrationAuthContext,
}

pub struct HttpsIntegrationTransportClient {
    config: HttpsIntegrationTransportConfig,
    client: reqwest::Client,
    auth_port: Arc<dyn IntegrationAuthPort>,
    proof_factory: Arc<dyn IntegrationRequestProofFactory>,
    sessions: Arc<RwLock<HashMap<String, SessionBinding>>>,
}

impl HttpsIntegrationTransportClient {
    pub fn new(
        config: HttpsIntegrationTransportConfig,
        auth_port: Arc<dyn IntegrationAuthPort>,
        proof_factory: Arc<dyn IntegrationRequestProofFactory>,
    ) -> Result<Self, CoreError> {
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()
            .map_err(|error| {
                CoreError::Network(format!(
                    "Failed to build integration transport HTTP client: {error}"
                ))
            })?;

        Ok(Self {
            config,
            client,
            auth_port,
            proof_factory,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        })
    }

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
                    timeout_ms: self.config.request_timeout.as_millis() as u64,
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
            .send_with_auth(
                reqwest::Method::POST,
                &self.config.bootstrap_url,
                &auth,
                Some(&bootstrap_request),
            )
            .await?;
        let response = self
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
        Self::validate_selected_transport(&request, &payload, &transport_kind)?;

        let auth_scheme = payload
            .selected_auth_scheme
            .clone()
            .unwrap_or_else(|| auth.scheme.clone());
        Self::validate_selected_auth_scheme(&request, &payload, &auth_scheme)?;

        let granted_scopes = Self::parse_granted_scopes(&request, &payload)?;
        let connected_at = Utc::now();

        self.sessions.write().await.insert(
            session.session_id.clone(),
            SessionBinding {
                heartbeat_url: session.heartbeat_url.clone(),
                disconnect_url: session.disconnect_url.clone(),
                auth,
            },
        );

        Ok(IntegrationTransportConnectResponse {
            session_id: session.session_id,
            connected_at,
            granted_scopes,
            transport_kind,
            auth_scheme,
        })
    }

    async fn heartbeat(&self, session_id: &str) -> Result<chrono::DateTime<Utc>, CoreError> {
        let binding = self
            .sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound {
                resource_type: "integration_session".to_string(),
                id: session_id.to_string(),
            })?;

        let url = binding.heartbeat_url.ok_or_else(|| CoreError::Validation {
            field: "integration.session.heartbeat_url".to_string(),
            message: "active integration session does not have a heartbeat URL.".to_string(),
        })?;

        let response = self
            .send_with_auth(
                reqwest::Method::POST,
                &url,
                &binding.auth,
                Option::<&()>::None,
            )
            .await?;
        self.check_response(response, "integration heartbeat request failed")
            .await?;
        Ok(Utc::now())
    }

    async fn disconnect(&self, session_id: &str) -> Result<(), CoreError> {
        let binding = self
            .sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound {
                resource_type: "integration_session".to_string(),
                id: session_id.to_string(),
            })?;

        let url = binding
            .disconnect_url
            .ok_or_else(|| CoreError::Validation {
                field: "integration.session.disconnect_url".to_string(),
                message: "active integration session does not have a disconnect URL.".to_string(),
            })?;

        let response = self
            .send_with_auth(
                reqwest::Method::DELETE,
                &url,
                &binding.auth,
                Option::<&()>::None,
            )
            .await?;
        self.check_response(response, "integration disconnect request failed")
            .await?;
        self.sessions.write().await.remove(session_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::IntegrationRequestProof;

    use async_trait::async_trait;
    use mockito::Matcher;
    use tokio::sync::Mutex;

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
            preferred_transports: vec![IntegrationTransportKind::WebSocket],
            supported_auth_schemes: vec![
                IntegrationAuthScheme::DpopBearer,
                IntegrationAuthScheme::BearerToken,
            ],
            resource_indicator: Some(server_url.to_string()),
        }
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
                    "supported_transports": ["web_socket"],
                    "selected_transport": "web_socket",
                    "supported_auth_schemes": ["bearer_token"],
                    "selected_auth_scheme": "bearer_token",
                    "resource_indicator": server.url(),
                    "session_required": true,
                    "session": {
                        "session_id": "session-001",
                        "channel_url": format!("wss://integration.example.com/sessions/{}", "session-001"),
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
        assert_eq!(response.transport_kind, IntegrationTransportKind::WebSocket);
        assert_eq!(response.auth_scheme, IntegrationAuthScheme::BearerToken);
    }

    #[tokio::test]
    async fn connect_heartbeat_and_disconnect_use_dpop_auth() {
        let mut server = mockito::Server::new_async().await;
        let bootstrap_url = format!("{}/integration/bootstrap", server.url());
        let heartbeat_url = format!(
            "{}/integration/sessions/session-002/heartbeat",
            server.url()
        );
        let disconnect_url = format!("{}/integration/sessions/session-002", server.url());
        let proof_calls = Arc::new(Mutex::new(Vec::new()));

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
                        "channel_url": format!("wss://integration.example.com/sessions/{}", "session-002"),
                        "heartbeat_url": heartbeat_url,
                        "disconnect_url": disconnect_url
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let heartbeat = server
            .mock("POST", "/integration/sessions/session-002/heartbeat")
            .match_header("authorization", "DPoP access-token")
            .match_header("dpop", "proof-token")
            .with_status(204)
            .create_async()
            .await;

        let disconnect = server
            .mock("DELETE", "/integration/sessions/session-002")
            .match_header("authorization", "DPoP access-token")
            .match_header("dpop", "proof-token")
            .with_status(204)
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

        bootstrap.assert_async().await;
        heartbeat.assert_async().await;
        disconnect.assert_async().await;

        let calls = proof_calls.lock().await;
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].0, "POST");
        assert_eq!(calls[1].0, "POST");
        assert_eq!(calls[2].0, "DELETE");
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
                    "supported_transports": ["web_socket"],
                    "selected_transport": "web_socket",
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
}
