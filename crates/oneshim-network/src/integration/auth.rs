use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use super::transport::{IntegrationRequestProof, IntegrationRequestProofFactory};
use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey};
use oneshim_core::error::CoreError;
use oneshim_core::models::integration::{
    IntegrationAuthContext, IntegrationAuthProfileKind, IntegrationAuthScheme,
    IntegrationAuthStatus, IntegrationAuthStatusKind, IntegrationCapabilityScope,
    IntegrationDeviceAuthorizationFlow,
};
use oneshim_core::ports::integration::IntegrationAuthPort;
use oneshim_core::ports::secret_store::{
    SecretStore, INTEGRATION_ACCESS_TOKEN_SECRET_KEY, INTEGRATION_AUTH_SECRET_NAMESPACE,
    INTEGRATION_DPOP_SIGNING_KEY_SECRET_KEY, INTEGRATION_EXPIRES_AT_SECRET_KEY,
    INTEGRATION_REFRESH_TOKEN_SECRET_KEY,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};

pub struct StaticIntegrationAuthPort {
    context: IntegrationAuthContext,
}

impl StaticIntegrationAuthPort {
    pub fn new(context: IntegrationAuthContext) -> Self {
        Self { context }
    }
}

#[async_trait]
impl IntegrationAuthPort for StaticIntegrationAuthPort {
    async fn resolve_session_auth(
        &self,
        _requested_scopes: &[IntegrationCapabilityScope],
        resource_indicator: Option<&str>,
    ) -> Result<IntegrationAuthContext, CoreError> {
        let mut context = self.context.clone();
        if context.resource_indicator.is_none() {
            context.resource_indicator = resource_indicator.map(str::to_string);
        }
        Ok(context)
    }

    async fn current_auth_status(&self) -> Result<IntegrationAuthStatus, CoreError> {
        Ok(IntegrationAuthStatus {
            profile_kind: IntegrationAuthProfileKind::EnvToken,
            status: IntegrationAuthStatusKind::Ready,
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
    ) -> Result<IntegrationDeviceAuthorizationFlow, CoreError> {
        Err(CoreError::InvalidArguments(
            "static integration auth does not support device authorization".to_string(),
        ))
    }

    async fn poll_device_authorization(
        &self,
        _flow_id: &str,
    ) -> Result<IntegrationAuthStatus, CoreError> {
        self.current_auth_status().await
    }

    async fn cancel_device_authorization(&self, _flow_id: &str) -> Result<(), CoreError> {
        Err(CoreError::InvalidArguments(
            "static integration auth does not support device authorization".to_string(),
        ))
    }

    async fn reset_auth_state(&self) -> Result<(), CoreError> {
        Err(CoreError::InvalidArguments(
            "static integration auth does not support auth reset".to_string(),
        ))
    }
}

pub struct EnvIntegrationAuthPort {
    token_env_var: String,
    scheme: IntegrationAuthScheme,
    expires_at: Option<DateTime<Utc>>,
    resource_indicator: Option<String>,
}

impl EnvIntegrationAuthPort {
    pub fn new(
        token_env_var: impl Into<String>,
        scheme: IntegrationAuthScheme,
        expires_at: Option<DateTime<Utc>>,
        resource_indicator: Option<String>,
    ) -> Self {
        Self {
            token_env_var: token_env_var.into(),
            scheme,
            expires_at,
            resource_indicator,
        }
    }

    fn read_token(&self) -> Option<String> {
        std::env::var(&self.token_env_var)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
}

#[async_trait]
impl IntegrationAuthPort for EnvIntegrationAuthPort {
    async fn resolve_session_auth(
        &self,
        _requested_scopes: &[IntegrationCapabilityScope],
        resource_indicator: Option<&str>,
    ) -> Result<IntegrationAuthContext, CoreError> {
        let access_token = self.read_token().ok_or_else(|| {
            CoreError::Auth(format!(
                "integration access token env var `{}` is not configured.",
                self.token_env_var
            ))
        })?;

        Ok(IntegrationAuthContext {
            access_token,
            scheme: self.scheme.clone(),
            expires_at: self.expires_at,
            resource_indicator: self
                .resource_indicator
                .clone()
                .or_else(|| resource_indicator.map(str::to_string)),
        })
    }

    async fn current_auth_status(&self) -> Result<IntegrationAuthStatus, CoreError> {
        let authenticated = self.read_token().is_some();
        Ok(IntegrationAuthStatus {
            profile_kind: IntegrationAuthProfileKind::EnvToken,
            status: if authenticated {
                IntegrationAuthStatusKind::Ready
            } else {
                IntegrationAuthStatusKind::Unauthenticated
            },
            interactive: false,
            authenticated,
            expires_at: self.expires_at,
            resource_indicator: self.resource_indicator.clone(),
            pending_flow: None,
            message: if authenticated {
                None
            } else {
                Some(format!(
                    "integration access token env var `{}` is not configured",
                    self.token_env_var
                ))
            },
        })
    }

    async fn start_device_authorization(
        &self,
        _requested_scopes: &[IntegrationCapabilityScope],
        _resource_indicator: Option<&str>,
    ) -> Result<IntegrationDeviceAuthorizationFlow, CoreError> {
        Err(CoreError::InvalidArguments(
            "env-token integration auth does not support device authorization".to_string(),
        ))
    }

    async fn poll_device_authorization(
        &self,
        _flow_id: &str,
    ) -> Result<IntegrationAuthStatus, CoreError> {
        self.current_auth_status().await
    }

    async fn cancel_device_authorization(&self, _flow_id: &str) -> Result<(), CoreError> {
        Err(CoreError::InvalidArguments(
            "env-token integration auth does not support device authorization".to_string(),
        ))
    }

    async fn reset_auth_state(&self) -> Result<(), CoreError> {
        Err(CoreError::InvalidArguments(
            "env-token integration auth does not support auth reset".to_string(),
        ))
    }
}

pub struct NoopIntegrationRequestProofFactory;

#[async_trait]
impl IntegrationRequestProofFactory for NoopIntegrationRequestProofFactory {
    async fn build_proof(
        &self,
        _auth: &IntegrationAuthContext,
        _method: &str,
        _url: &str,
    ) -> Result<Option<IntegrationRequestProof>, CoreError> {
        Ok(None)
    }
}

pub struct StaticIntegrationRequestProofFactory {
    proof: IntegrationRequestProof,
}

impl StaticIntegrationRequestProofFactory {
    pub fn new(header_name: impl Into<String>, header_value: impl Into<String>) -> Self {
        Self {
            proof: IntegrationRequestProof {
                header_name: header_name.into(),
                header_value: header_value.into(),
            },
        }
    }
}

#[async_trait]
impl IntegrationRequestProofFactory for StaticIntegrationRequestProofFactory {
    async fn build_proof(
        &self,
        _auth: &IntegrationAuthContext,
        _method: &str,
        _url: &str,
    ) -> Result<Option<IntegrationRequestProof>, CoreError> {
        Ok(Some(self.proof.clone()))
    }
}

#[derive(Debug, Clone)]
pub struct OidcDeviceFlowAuthConfig {
    pub client_id: String,
    pub device_authorization_url: String,
    pub token_url: String,
    pub default_scopes: Vec<String>,
    pub resource_indicator: Option<String>,
    pub scheme: IntegrationAuthScheme,
    pub request_timeout: Duration,
}

#[derive(Debug, Clone)]
struct StoredAuthMaterial {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    resource_indicator: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingDeviceAuthorization {
    flow: IntegrationDeviceAuthorizationFlow,
    device_code: String,
}

pub struct OidcDeviceFlowIntegrationAuthPort {
    config: OidcDeviceFlowAuthConfig,
    proof_factory: Arc<dyn IntegrationRequestProofFactory>,
    secret_store: Option<Arc<dyn SecretStore>>,
    client: reqwest::Client,
    auth_material: Arc<RwLock<Option<StoredAuthMaterial>>>,
    pending_flows: Arc<RwLock<HashMap<String, PendingDeviceAuthorization>>>,
    last_error: Arc<RwLock<Option<String>>>,
    refresh_lock: Arc<Mutex<()>>,
}

impl OidcDeviceFlowIntegrationAuthPort {
    pub fn new(
        config: OidcDeviceFlowAuthConfig,
        proof_factory: Arc<dyn IntegrationRequestProofFactory>,
        secret_store: Option<Arc<dyn SecretStore>>,
    ) -> Result<Self, CoreError> {
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()
            .map_err(|error| {
                CoreError::Network(format!(
                    "failed to build integration OIDC auth HTTP client: {error}"
                ))
            })?;

        Ok(Self {
            config,
            proof_factory,
            secret_store,
            client,
            auth_material: Arc::new(RwLock::new(None)),
            pending_flows: Arc::new(RwLock::new(HashMap::new())),
            last_error: Arc::new(RwLock::new(None)),
            refresh_lock: Arc::new(Mutex::new(())),
        })
    }

    async fn refresh_access_token_if_needed(
        &self,
        refresh_token: &str,
    ) -> Result<StoredAuthMaterial, CoreError> {
        let _guard = self.refresh_lock.lock().await;

        if let Some(material) = self.load_material().await? {
            let is_expired = material
                .expires_at
                .is_some_and(|expires_at| expires_at <= Utc::now());
            if !is_expired {
                return Ok(material);
            }

            if let Some(current_refresh_token) = material.refresh_token.as_deref() {
                return self.refresh_access_token(current_refresh_token).await;
            }
        }

        self.refresh_access_token(refresh_token).await
    }

    async fn find_reusable_pending_flow(
        &self,
        requested_scopes: &[IntegrationCapabilityScope],
        resource_indicator: Option<&str>,
    ) -> Option<IntegrationDeviceAuthorizationFlow> {
        let now = Utc::now();
        let expected_resource_indicator = resource_indicator.map(str::to_string);
        let mut pending_flows = self.pending_flows.write().await;
        pending_flows.retain(|_, entry| entry.flow.expires_at > now);
        pending_flows.values().find_map(|entry| {
            let same_scopes = entry.flow.requested_scopes.len() == requested_scopes.len()
                && requested_scopes
                    .iter()
                    .all(|scope| entry.flow.requested_scopes.contains(scope));
            let same_resource_indicator =
                entry.flow.resource_indicator == expected_resource_indicator;
            (same_scopes && same_resource_indicator).then(|| entry.flow.clone())
        })
    }

    async fn load_material(&self) -> Result<Option<StoredAuthMaterial>, CoreError> {
        if let Some(material) = self.auth_material.read().await.clone() {
            return Ok(Some(material));
        }

        let Some(secret_store) = self.secret_store.as_ref() else {
            return Ok(None);
        };

        let access_token = secret_store
            .retrieve(
                INTEGRATION_AUTH_SECRET_NAMESPACE,
                INTEGRATION_ACCESS_TOKEN_SECRET_KEY,
            )
            .await?;
        let Some(access_token) = access_token else {
            return Ok(None);
        };

        let refresh_token = secret_store
            .retrieve(
                INTEGRATION_AUTH_SECRET_NAMESPACE,
                INTEGRATION_REFRESH_TOKEN_SECRET_KEY,
            )
            .await?;
        let expires_at = secret_store
            .retrieve(
                INTEGRATION_AUTH_SECRET_NAMESPACE,
                INTEGRATION_EXPIRES_AT_SECRET_KEY,
            )
            .await?
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(&value).ok())
            .map(|value| value.with_timezone(&Utc));

        let material = StoredAuthMaterial {
            access_token,
            refresh_token,
            expires_at,
            resource_indicator: self.config.resource_indicator.clone(),
        };
        *self.auth_material.write().await = Some(material.clone());
        Ok(Some(material))
    }

    async fn store_material(&self, material: &StoredAuthMaterial) -> Result<(), CoreError> {
        if let Some(secret_store) = self.secret_store.as_ref() {
            secret_store
                .store(
                    INTEGRATION_AUTH_SECRET_NAMESPACE,
                    INTEGRATION_ACCESS_TOKEN_SECRET_KEY,
                    &material.access_token,
                )
                .await?;
            if let Some(refresh_token) = material.refresh_token.as_deref() {
                secret_store
                    .store(
                        INTEGRATION_AUTH_SECRET_NAMESPACE,
                        INTEGRATION_REFRESH_TOKEN_SECRET_KEY,
                        refresh_token,
                    )
                    .await?;
            } else {
                secret_store
                    .delete(
                        INTEGRATION_AUTH_SECRET_NAMESPACE,
                        INTEGRATION_REFRESH_TOKEN_SECRET_KEY,
                    )
                    .await?;
            }
            if let Some(expires_at) = material.expires_at {
                secret_store
                    .store(
                        INTEGRATION_AUTH_SECRET_NAMESPACE,
                        INTEGRATION_EXPIRES_AT_SECRET_KEY,
                        &expires_at.to_rfc3339(),
                    )
                    .await?;
            } else {
                secret_store
                    .delete(
                        INTEGRATION_AUTH_SECRET_NAMESPACE,
                        INTEGRATION_EXPIRES_AT_SECRET_KEY,
                    )
                    .await?;
            }
        }

        *self.auth_material.write().await = Some(material.clone());
        Ok(())
    }

    async fn clear_material(&self) -> Result<(), CoreError> {
        if let Some(secret_store) = self.secret_store.as_ref() {
            secret_store
                .delete(
                    INTEGRATION_AUTH_SECRET_NAMESPACE,
                    INTEGRATION_ACCESS_TOKEN_SECRET_KEY,
                )
                .await?;
            secret_store
                .delete(
                    INTEGRATION_AUTH_SECRET_NAMESPACE,
                    INTEGRATION_REFRESH_TOKEN_SECRET_KEY,
                )
                .await?;
            secret_store
                .delete(
                    INTEGRATION_AUTH_SECRET_NAMESPACE,
                    INTEGRATION_EXPIRES_AT_SECRET_KEY,
                )
                .await?;
        }
        *self.auth_material.write().await = None;
        Ok(())
    }

    fn combined_scope_string(
        &self,
        requested_scopes: &[IntegrationCapabilityScope],
    ) -> Option<String> {
        let mut scopes = self.config.default_scopes.clone();
        for scope in requested_scopes {
            let value = scope.as_str().to_string();
            if !scopes.contains(&value) {
                scopes.push(value);
            }
        }
        if scopes.is_empty() {
            None
        } else {
            Some(scopes.join(" "))
        }
    }

    async fn send_form(
        &self,
        url: &str,
        form: &[(String, String)],
        use_dpop: bool,
    ) -> Result<reqwest::Response, CoreError> {
        let mut request = self.client.post(url).form(form);
        if use_dpop {
            let proof = self
                .proof_factory
                .build_proof(
                    &IntegrationAuthContext {
                        access_token: String::new(),
                        scheme: IntegrationAuthScheme::DpopBearer,
                        expires_at: None,
                        resource_indicator: self.config.resource_indicator.clone(),
                    },
                    "POST",
                    url,
                )
                .await?;
            let proof = proof.ok_or_else(|| {
                CoreError::Auth(
                    "DPoP integration auth requires a request proof, but none was provided."
                        .to_string(),
                )
            })?;
            request = request.header(proof.header_name, proof.header_value);
        }

        request.send().await.map_err(|error| {
            if error.is_timeout() {
                CoreError::RequestTimeout {
                    timeout_ms: self.config.request_timeout.as_millis() as u64,
                }
            } else {
                CoreError::Network(format!("integration auth request failed: {error}"))
            }
        })
    }

    async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> Result<StoredAuthMaterial, CoreError> {
        let mut form = vec![
            ("grant_type".to_string(), "refresh_token".to_string()),
            ("client_id".to_string(), self.config.client_id.clone()),
            ("refresh_token".to_string(), refresh_token.to_string()),
        ];
        if let Some(resource_indicator) = self.config.resource_indicator.as_deref() {
            form.push(("resource".to_string(), resource_indicator.to_string()));
        }

        let response = self
            .send_form(
                &self.config.token_url,
                &form,
                self.config.scheme == IntegrationAuthScheme::DpopBearer,
            )
            .await?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            self.clear_material().await?;
            return Err(CoreError::Auth(format!(
                "integration refresh failed: {body}"
            )));
        }

        let payload: OidcTokenSuccessResponse = response.json().await.map_err(|error| {
            CoreError::Serialization(serde_json::Error::io(std::io::Error::other(format!(
                "failed to parse integration refresh response: {error}"
            ))))
        })?;

        let material = StoredAuthMaterial {
            access_token: payload.access_token,
            refresh_token: payload
                .refresh_token
                .or_else(|| Some(refresh_token.to_string())),
            expires_at: payload
                .expires_in
                .map(|seconds| Utc::now() + chrono::Duration::seconds(seconds as i64)),
            resource_indicator: self.config.resource_indicator.clone(),
        };
        self.store_material(&material).await?;
        Ok(material)
    }
}

#[async_trait]
impl IntegrationAuthPort for OidcDeviceFlowIntegrationAuthPort {
    async fn resolve_session_auth(
        &self,
        requested_scopes: &[IntegrationCapabilityScope],
        resource_indicator: Option<&str>,
    ) -> Result<IntegrationAuthContext, CoreError> {
        let material = self.load_material().await?;
        if let Some(material) = material {
            let is_expired = material
                .expires_at
                .is_some_and(|expires_at| expires_at <= Utc::now());
            let material = if is_expired {
                if let Some(refresh_token) = material.refresh_token.as_deref() {
                    self.refresh_access_token_if_needed(refresh_token).await?
                } else {
                    return Err(CoreError::Auth(
                        "integration device authorization has expired; re-authorize the device"
                            .to_string(),
                    ));
                }
            } else {
                material
            };

            return Ok(IntegrationAuthContext {
                access_token: material.access_token,
                scheme: self.config.scheme.clone(),
                expires_at: material.expires_at,
                resource_indicator: resource_indicator
                    .map(str::to_string)
                    .or(material.resource_indicator),
            });
        }

        let status = self.current_auth_status().await?;
        Err(CoreError::Auth(status.message.unwrap_or_else(|| {
            let scopes = self
                .combined_scope_string(requested_scopes)
                .unwrap_or_else(|| "requested scopes".to_string());
            format!("integration device authorization is required before using scopes: {scopes}")
        })))
    }

    async fn current_auth_status(&self) -> Result<IntegrationAuthStatus, CoreError> {
        if let Some(material) = self.load_material().await? {
            let is_expired = material
                .expires_at
                .is_some_and(|expires_at| expires_at <= Utc::now());
            let material = if is_expired {
                if let Some(refresh_token) = material.refresh_token.as_deref() {
                    match self.refresh_access_token_if_needed(refresh_token).await {
                        Ok(refreshed) => {
                            *self.last_error.write().await = None;
                            refreshed
                        }
                        Err(error) => {
                            let message = format!(
                                "integration auth refresh failed; re-authorize the device: {error}"
                            );
                            *self.last_error.write().await = Some(message.clone());
                            return Ok(IntegrationAuthStatus {
                                profile_kind: IntegrationAuthProfileKind::OidcDeviceFlow,
                                status: IntegrationAuthStatusKind::Error,
                                interactive: true,
                                authenticated: false,
                                expires_at: material.expires_at,
                                resource_indicator: material.resource_indicator,
                                pending_flow: None,
                                message: Some(message),
                            });
                        }
                    }
                } else {
                    material
                }
            } else {
                material
            };
            return Ok(IntegrationAuthStatus {
                profile_kind: IntegrationAuthProfileKind::OidcDeviceFlow,
                status: if material
                    .expires_at
                    .is_some_and(|expires_at| expires_at <= Utc::now())
                {
                    IntegrationAuthStatusKind::Expired
                } else {
                    IntegrationAuthStatusKind::Ready
                },
                interactive: true,
                authenticated: !material
                    .expires_at
                    .is_some_and(|expires_at| expires_at <= Utc::now()),
                expires_at: material.expires_at,
                resource_indicator: material.resource_indicator,
                pending_flow: None,
                message: if material
                    .expires_at
                    .is_some_and(|expires_at| expires_at <= Utc::now())
                {
                    Some(
                        "integration device authorization expired; re-authorize or refresh"
                            .to_string(),
                    )
                } else {
                    None
                },
            });
        }

        let pending_flow = self
            .pending_flows
            .read()
            .await
            .values()
            .find(|entry| entry.flow.expires_at > Utc::now())
            .map(|entry| entry.flow.clone());
        if let Some(flow) = pending_flow {
            return Ok(IntegrationAuthStatus {
                profile_kind: IntegrationAuthProfileKind::OidcDeviceFlow,
                status: IntegrationAuthStatusKind::AwaitingUserAuthorization,
                interactive: true,
                authenticated: false,
                expires_at: None,
                resource_indicator: flow.resource_indicator.clone(),
                pending_flow: Some(flow),
                message: Some(
                    "complete the device authorization flow to finish integration bootstrap"
                        .to_string(),
                ),
            });
        }

        Ok(IntegrationAuthStatus {
            profile_kind: IntegrationAuthProfileKind::OidcDeviceFlow,
            status: IntegrationAuthStatusKind::Unauthenticated,
            interactive: true,
            authenticated: false,
            expires_at: None,
            resource_indicator: self.config.resource_indicator.clone(),
            pending_flow: None,
            message: self.last_error.read().await.clone(),
        })
    }

    async fn start_device_authorization(
        &self,
        requested_scopes: &[IntegrationCapabilityScope],
        resource_indicator: Option<&str>,
    ) -> Result<IntegrationDeviceAuthorizationFlow, CoreError> {
        if let Some(flow) = self
            .find_reusable_pending_flow(requested_scopes, resource_indicator)
            .await
        {
            *self.last_error.write().await = None;
            return Ok(flow);
        }

        let mut form = vec![("client_id".to_string(), self.config.client_id.clone())];
        if let Some(scope) = self.combined_scope_string(requested_scopes) {
            form.push(("scope".to_string(), scope));
        }
        let resource_indicator = resource_indicator
            .map(str::to_string)
            .or_else(|| self.config.resource_indicator.clone());
        if let Some(resource_indicator) = resource_indicator.as_deref() {
            form.push(("resource".to_string(), resource_indicator.to_string()));
        }

        let response = self
            .send_form(&self.config.device_authorization_url, &form, false)
            .await?;
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            *self.last_error.write().await = Some(format!(
                "integration device authorization bootstrap failed: {body}"
            ));
            return Err(CoreError::Auth(format!(
                "integration device authorization bootstrap failed: {body}"
            )));
        }

        let payload: OidcDeviceAuthorizationResponse = response.json().await.map_err(|error| {
            CoreError::Serialization(serde_json::Error::io(std::io::Error::other(format!(
                "failed to parse integration device authorization response: {error}"
            ))))
        })?;
        let flow_id = uuid::Uuid::new_v4().to_string();
        let flow = IntegrationDeviceAuthorizationFlow {
            flow_id: flow_id.clone(),
            user_code: payload.user_code,
            verification_uri: payload.verification_uri,
            verification_uri_complete: payload.verification_uri_complete,
            expires_at: Utc::now() + chrono::Duration::seconds(payload.expires_in as i64),
            interval_secs: payload.interval.unwrap_or(5),
            requested_scopes: requested_scopes.to_vec(),
            resource_indicator,
        };
        self.pending_flows.write().await.insert(
            flow_id,
            PendingDeviceAuthorization {
                flow: flow.clone(),
                device_code: payload.device_code,
            },
        );
        *self.last_error.write().await = None;
        Ok(flow)
    }

    async fn poll_device_authorization(
        &self,
        flow_id: &str,
    ) -> Result<IntegrationAuthStatus, CoreError> {
        let pending = self
            .pending_flows
            .read()
            .await
            .get(flow_id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound {
                resource_type: "integration_device_authorization_flow".to_string(),
                id: flow_id.to_string(),
            })?;

        if pending.flow.expires_at <= Utc::now() {
            self.pending_flows.write().await.remove(flow_id);
            *self.last_error.write().await =
                Some("integration device authorization flow expired".to_string());
            return Ok(IntegrationAuthStatus {
                profile_kind: IntegrationAuthProfileKind::OidcDeviceFlow,
                status: IntegrationAuthStatusKind::Expired,
                interactive: true,
                authenticated: false,
                expires_at: None,
                resource_indicator: pending.flow.resource_indicator.clone(),
                pending_flow: None,
                message: Some("integration device authorization flow expired".to_string()),
            });
        }

        let mut form = vec![
            (
                "grant_type".to_string(),
                "urn:ietf:params:oauth:grant-type:device_code".to_string(),
            ),
            ("device_code".to_string(), pending.device_code.clone()),
            ("client_id".to_string(), self.config.client_id.clone()),
        ];
        if let Some(resource_indicator) = pending.flow.resource_indicator.as_deref() {
            form.push(("resource".to_string(), resource_indicator.to_string()));
        }

        let response = self
            .send_form(
                &self.config.token_url,
                &form,
                self.config.scheme == IntegrationAuthScheme::DpopBearer,
            )
            .await?;

        if response.status().is_success() {
            let payload: OidcTokenSuccessResponse = response.json().await.map_err(|error| {
                CoreError::Serialization(serde_json::Error::io(std::io::Error::other(format!(
                    "failed to parse integration token response: {error}"
                ))))
            })?;
            let material = StoredAuthMaterial {
                access_token: payload.access_token,
                refresh_token: payload.refresh_token,
                expires_at: payload
                    .expires_in
                    .map(|seconds| Utc::now() + chrono::Duration::seconds(seconds as i64)),
                resource_indicator: pending.flow.resource_indicator.clone(),
            };
            self.store_material(&material).await?;
            self.pending_flows.write().await.remove(flow_id);
            *self.last_error.write().await = None;
            return self.current_auth_status().await;
        }

        let error_body: OidcTokenErrorResponse =
            response.json().await.unwrap_or(OidcTokenErrorResponse {
                error: "unknown_error".to_string(),
                error_description: None,
            });
        let message = error_body
            .error_description
            .clone()
            .unwrap_or_else(|| error_body.error.clone());

        match error_body.error.as_str() {
            "authorization_pending" => {
                *self.last_error.write().await = Some(message);
                self.current_auth_status().await
            }
            "slow_down" => {
                let mut guard = self.pending_flows.write().await;
                if let Some(flow) = guard.get_mut(flow_id) {
                    flow.flow.interval_secs = flow.flow.interval_secs.saturating_add(5);
                }
                *self.last_error.write().await = Some(message);
                self.current_auth_status().await
            }
            "access_denied" | "expired_token" | "invalid_grant" => {
                self.pending_flows.write().await.remove(flow_id);
                *self.last_error.write().await = Some(message.clone());
                Ok(IntegrationAuthStatus {
                    profile_kind: IntegrationAuthProfileKind::OidcDeviceFlow,
                    status: if error_body.error == "expired_token" {
                        IntegrationAuthStatusKind::Expired
                    } else {
                        IntegrationAuthStatusKind::Error
                    },
                    interactive: true,
                    authenticated: false,
                    expires_at: None,
                    resource_indicator: pending.flow.resource_indicator.clone(),
                    pending_flow: None,
                    message: Some(message),
                })
            }
            _ => {
                *self.last_error.write().await = Some(message.clone());
                Err(CoreError::Auth(format!(
                    "integration token exchange failed: {message}"
                )))
            }
        }
    }

    async fn cancel_device_authorization(&self, flow_id: &str) -> Result<(), CoreError> {
        let removed = self.pending_flows.write().await.remove(flow_id);
        if removed.is_none() {
            return Err(CoreError::NotFound {
                resource_type: "integration_device_authorization_flow".to_string(),
                id: flow_id.to_string(),
            });
        }
        *self.last_error.write().await = None;
        Ok(())
    }

    async fn reset_auth_state(&self) -> Result<(), CoreError> {
        self.pending_flows.write().await.clear();
        self.clear_material().await?;
        *self.last_error.write().await = None;
        Ok(())
    }
}

pub struct Ed25519DpopProofFactory {
    secret_store: Option<Arc<dyn SecretStore>>,
    signing_key: Arc<Mutex<Option<SigningKey>>>,
}

impl Ed25519DpopProofFactory {
    pub fn new(secret_store: Option<Arc<dyn SecretStore>>) -> Self {
        Self {
            secret_store,
            signing_key: Arc::new(Mutex::new(None)),
        }
    }

    async fn get_signing_key(&self) -> Result<SigningKey, CoreError> {
        let mut guard = self.signing_key.lock().await;
        if let Some(key) = guard.as_ref() {
            return Ok(key.clone());
        }

        let key = if let Some(secret_store) = self.secret_store.as_ref() {
            if let Some(serialized) = secret_store
                .retrieve(
                    INTEGRATION_AUTH_SECRET_NAMESPACE,
                    INTEGRATION_DPOP_SIGNING_KEY_SECRET_KEY,
                )
                .await?
            {
                let bytes = URL_SAFE_NO_PAD
                    .decode(serialized.as_bytes())
                    .map_err(|error| {
                        CoreError::SecretStoreError(format!(
                            "failed to decode integration DPoP signing key: {error}"
                        ))
                    })?;
                let secret_bytes: [u8; 32] = bytes.try_into().map_err(|_| {
                    CoreError::SecretStoreError(
                        "integration DPoP signing key must be 32 bytes".to_string(),
                    )
                })?;
                SigningKey::from_bytes(&secret_bytes)
            } else {
                let secret_bytes: [u8; 32] = rand::random();
                secret_store
                    .store(
                        INTEGRATION_AUTH_SECRET_NAMESPACE,
                        INTEGRATION_DPOP_SIGNING_KEY_SECRET_KEY,
                        &URL_SAFE_NO_PAD.encode(secret_bytes),
                    )
                    .await?;
                SigningKey::from_bytes(&secret_bytes)
            }
        } else {
            let secret_bytes: [u8; 32] = rand::random();
            SigningKey::from_bytes(&secret_bytes)
        };

        *guard = Some(key.clone());
        Ok(key)
    }
}

#[async_trait]
impl IntegrationRequestProofFactory for Ed25519DpopProofFactory {
    async fn build_proof(
        &self,
        auth: &IntegrationAuthContext,
        method: &str,
        url: &str,
    ) -> Result<Option<IntegrationRequestProof>, CoreError> {
        let signing_key = self.get_signing_key().await?;
        let verifying_key = signing_key.verifying_key();
        let jwk = serde_json::json!({
            "kty": "OKP",
            "crv": "Ed25519",
            "x": URL_SAFE_NO_PAD.encode(verifying_key.to_bytes()),
        });
        let header = serde_json::json!({
            "typ": "dpop+jwt",
            "alg": "EdDSA",
            "jwk": jwk,
        });

        let mut claims = serde_json::Map::from_iter([
            (
                "htu".to_string(),
                serde_json::Value::String(url.to_string()),
            ),
            (
                "htm".to_string(),
                serde_json::Value::String(method.to_ascii_uppercase()),
            ),
            (
                "iat".to_string(),
                serde_json::Value::Number(serde_json::Number::from(Utc::now().timestamp())),
            ),
            (
                "jti".to_string(),
                serde_json::Value::String(uuid::Uuid::new_v4().to_string()),
            ),
        ]);

        if !auth.access_token.trim().is_empty() {
            let digest = Sha256::digest(auth.access_token.as_bytes());
            claims.insert(
                "ath".to_string(),
                serde_json::Value::String(URL_SAFE_NO_PAD.encode(digest)),
            );
        }

        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).map_err(|error| {
            CoreError::Serialization(serde_json::Error::io(std::io::Error::other(format!(
                "failed to serialize DPoP JWT header: {error}"
            ))))
        })?);
        let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).map_err(|error| {
            CoreError::Serialization(serde_json::Error::io(std::io::Error::other(format!(
                "failed to serialize DPoP JWT payload: {error}"
            ))))
        })?);
        let signing_input = format!("{header_b64}.{payload_b64}");
        let signature = signing_key.sign(signing_input.as_bytes()).to_bytes();
        let proof = format!(
            "{signing_input}.{}",
            URL_SAFE_NO_PAD.encode(signature.as_slice())
        );

        Ok(Some(IntegrationRequestProof {
            header_name: "dpop".to_string(),
            header_value: proof,
        }))
    }
}

#[derive(Debug, Deserialize)]
struct OidcDeviceAuthorizationResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    expires_in: u64,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OidcTokenSuccessResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OidcTokenErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Matcher;
    use oneshim_core::ports::secret_store::SecretStore;
    use std::collections::HashMap;

    #[derive(Default)]
    struct InMemorySecretStore {
        values: std::sync::Mutex<HashMap<String, String>>,
    }

    #[async_trait]
    impl SecretStore for InMemorySecretStore {
        async fn store(&self, namespace: &str, key: &str, value: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .insert(format!("{namespace}:{key}"), value.to_string());
            Ok(())
        }

        async fn retrieve(&self, namespace: &str, key: &str) -> Result<Option<String>, CoreError> {
            Ok(self
                .values
                .lock()
                .unwrap()
                .get(&format!("{namespace}:{key}"))
                .cloned())
        }

        async fn delete(&self, namespace: &str, key: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .remove(&format!("{namespace}:{key}"));
            Ok(())
        }

        async fn delete_namespace(&self, namespace: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .retain(|entry, _| !entry.starts_with(&format!("{namespace}:")));
            Ok(())
        }
    }

    #[tokio::test]
    async fn static_auth_port_preserves_auth_context() {
        let port = StaticIntegrationAuthPort::new(IntegrationAuthContext {
            access_token: "token-1".to_string(),
            scheme: IntegrationAuthScheme::BearerToken,
            expires_at: None,
            resource_indicator: None,
        });

        let context = port
            .resolve_session_auth(
                &[IntegrationCapabilityScope::InsightWrite],
                Some("https://integration.example.com"),
            )
            .await
            .unwrap();

        assert_eq!(context.access_token, "token-1");
        assert_eq!(context.scheme, IntegrationAuthScheme::BearerToken);
        assert_eq!(
            context.resource_indicator.as_deref(),
            Some("https://integration.example.com")
        );
        assert_eq!(
            port.current_auth_status().await.unwrap().status,
            IntegrationAuthStatusKind::Ready
        );
    }

    #[tokio::test]
    async fn env_auth_port_reads_token_from_env() {
        let env_name = "ONESHIM_TEST_INTEGRATION_TOKEN";
        unsafe {
            std::env::set_var(env_name, "token-from-env");
        }

        let port = EnvIntegrationAuthPort::new(
            env_name,
            IntegrationAuthScheme::DpopBearer,
            None,
            Some("https://integration.example.com".to_string()),
        );

        let context = port
            .resolve_session_auth(&[IntegrationCapabilityScope::SessionManage], None)
            .await
            .unwrap();

        assert_eq!(context.access_token, "token-from-env");
        assert_eq!(context.scheme, IntegrationAuthScheme::DpopBearer);
        assert!(port.current_auth_status().await.unwrap().authenticated);

        unsafe {
            std::env::remove_var(env_name);
        }
    }

    #[tokio::test]
    async fn noop_proof_factory_returns_none() {
        let factory = NoopIntegrationRequestProofFactory;
        let proof = factory
            .build_proof(
                &IntegrationAuthContext {
                    access_token: "token".to_string(),
                    scheme: IntegrationAuthScheme::BearerToken,
                    expires_at: None,
                    resource_indicator: None,
                },
                "POST",
                "https://integration.example.com/bootstrap",
            )
            .await
            .unwrap();

        assert!(proof.is_none());
    }

    #[tokio::test]
    async fn static_proof_factory_returns_configured_header() {
        let factory = StaticIntegrationRequestProofFactory::new("dpop", "proof-value");
        let proof = factory
            .build_proof(
                &IntegrationAuthContext {
                    access_token: "token".to_string(),
                    scheme: IntegrationAuthScheme::DpopBearer,
                    expires_at: None,
                    resource_indicator: None,
                },
                "POST",
                "https://integration.example.com/bootstrap",
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(proof.header_name, "dpop");
        assert_eq!(proof.header_value, "proof-value");
    }

    #[tokio::test]
    async fn ed25519_dpop_factory_builds_real_jwt_proof() {
        let factory = Ed25519DpopProofFactory::new(None);
        let proof = factory
            .build_proof(
                &IntegrationAuthContext {
                    access_token: "access-token".to_string(),
                    scheme: IntegrationAuthScheme::DpopBearer,
                    expires_at: None,
                    resource_indicator: None,
                },
                "POST",
                "https://integration.example.com/bootstrap",
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(proof.header_name, "dpop");
        let parts: Vec<&str> = proof.header_value.split('.').collect();
        assert_eq!(parts.len(), 3);

        let header: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0].as_bytes()).unwrap()).unwrap();
        let payload: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1].as_bytes()).unwrap()).unwrap();
        assert_eq!(header["typ"], "dpop+jwt");
        assert_eq!(header["alg"], "EdDSA");
        assert_eq!(payload["htu"], "https://integration.example.com/bootstrap");
        assert_eq!(payload["htm"], "POST");
        assert!(payload.get("ath").is_some());
    }

    #[tokio::test]
    async fn oidc_device_flow_starts_and_polls_into_ready_auth() {
        let mut server = mockito::Server::new_async().await;
        let device_endpoint = format!("{}/oauth/device/code", server.url());
        let token_endpoint = format!("{}/oauth/token", server.url());

        let _device_mock = server
            .mock("POST", "/oauth/device/code")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("client_id".into(), "desktop-client".into()),
                Matcher::UrlEncoded("scope".into(), "openid insight:write".into()),
                Matcher::UrlEncoded("resource".into(), "https://integration.example.com".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "device_code": "device-code-1",
                    "user_code": "ABCD-EFGH",
                    "verification_uri": "https://id.example.com/activate",
                    "verification_uri_complete": "https://id.example.com/activate?user_code=ABCD-EFGH",
                    "expires_in": 900,
                    "interval": 5
                })
                .to_string(),
            )
            .create_async()
            .await;

        let _token_mock = server
            .mock("POST", "/oauth/token")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded(
                    "grant_type".into(),
                    "urn:ietf:params:oauth:grant-type:device_code".into(),
                ),
                Matcher::UrlEncoded("client_id".into(), "desktop-client".into()),
                Matcher::UrlEncoded("device_code".into(), "device-code-1".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "access_token": "access-token-1",
                    "refresh_token": "refresh-token-1",
                    "token_type": "Bearer",
                    "expires_in": 3600
                })
                .to_string(),
            )
            .create_async()
            .await;

        let secret_store = Arc::new(InMemorySecretStore::default()) as Arc<dyn SecretStore>;
        let port = OidcDeviceFlowIntegrationAuthPort::new(
            OidcDeviceFlowAuthConfig {
                client_id: "desktop-client".to_string(),
                device_authorization_url: device_endpoint,
                token_url: token_endpoint,
                default_scopes: vec!["openid".to_string()],
                resource_indicator: Some("https://integration.example.com".to_string()),
                scheme: IntegrationAuthScheme::BearerToken,
                request_timeout: Duration::from_secs(5),
            },
            Arc::new(NoopIntegrationRequestProofFactory),
            Some(secret_store.clone()),
        )
        .unwrap();

        let flow = port
            .start_device_authorization(
                &[IntegrationCapabilityScope::InsightWrite],
                Some("https://integration.example.com"),
            )
            .await
            .unwrap();
        assert_eq!(flow.user_code, "ABCD-EFGH");

        let status = port.poll_device_authorization(&flow.flow_id).await.unwrap();
        assert_eq!(status.status, IntegrationAuthStatusKind::Ready);
        assert!(status.authenticated);

        let auth = port
            .resolve_session_auth(&[IntegrationCapabilityScope::InsightWrite], None)
            .await
            .unwrap();
        assert_eq!(auth.access_token, "access-token-1");
        assert_eq!(
            secret_store
                .retrieve(
                    INTEGRATION_AUTH_SECRET_NAMESPACE,
                    INTEGRATION_ACCESS_TOKEN_SECRET_KEY
                )
                .await
                .unwrap()
                .as_deref(),
            Some("access-token-1")
        );
    }

    #[tokio::test]
    async fn start_device_authorization_reuses_matching_pending_flow() {
        let mut server = mockito::Server::new_async().await;
        let device_endpoint = format!("{}/oauth/device/code", server.url());
        let token_endpoint = format!("{}/oauth/token", server.url());

        let device_mock = server
            .mock("POST", "/oauth/device/code")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("client_id".into(), "desktop-client".into()),
                Matcher::UrlEncoded("scope".into(), "openid prompt:read".into()),
                Matcher::UrlEncoded("resource".into(), "https://integration.example.com".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "device_code": "device-code-2",
                    "user_code": "IJKL-MNOP",
                    "verification_uri": "https://id.example.com/activate",
                    "verification_uri_complete": "https://id.example.com/activate?user_code=IJKL-MNOP",
                    "expires_in": 900,
                    "interval": 5
                })
                .to_string(),
            )
            .expect(1)
            .create_async()
            .await;

        let port = OidcDeviceFlowIntegrationAuthPort::new(
            OidcDeviceFlowAuthConfig {
                client_id: "desktop-client".to_string(),
                device_authorization_url: device_endpoint,
                token_url: token_endpoint,
                default_scopes: vec!["openid".to_string()],
                resource_indicator: Some("https://integration.example.com".to_string()),
                scheme: IntegrationAuthScheme::BearerToken,
                request_timeout: Duration::from_secs(5),
            },
            Arc::new(NoopIntegrationRequestProofFactory),
            None,
        )
        .unwrap();

        let first = port
            .start_device_authorization(
                &[IntegrationCapabilityScope::PromptRead],
                Some("https://integration.example.com"),
            )
            .await
            .unwrap();
        let second = port
            .start_device_authorization(
                &[IntegrationCapabilityScope::PromptRead],
                Some("https://integration.example.com"),
            )
            .await
            .unwrap();

        assert_eq!(first.flow_id, second.flow_id);
        assert_eq!(first.user_code, second.user_code);
        device_mock.assert_async().await;
    }

    #[tokio::test]
    async fn current_auth_status_refreshes_expired_material_when_refresh_token_exists() {
        let mut server = mockito::Server::new_async().await;
        let token_endpoint = format!("{}/oauth/token", server.url());

        let refresh_mock = server
            .mock("POST", "/oauth/token")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("grant_type".into(), "refresh_token".into()),
                Matcher::UrlEncoded("client_id".into(), "desktop-client".into()),
                Matcher::UrlEncoded("refresh_token".into(), "refresh-token-2".into()),
                Matcher::UrlEncoded("resource".into(), "https://integration.example.com".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "access_token": "access-token-2",
                    "refresh_token": "refresh-token-2b",
                    "token_type": "Bearer",
                    "expires_in": 3600
                })
                .to_string(),
            )
            .expect(1)
            .create_async()
            .await;

        let secret_store = Arc::new(InMemorySecretStore::default()) as Arc<dyn SecretStore>;
        secret_store
            .store(
                INTEGRATION_AUTH_SECRET_NAMESPACE,
                INTEGRATION_ACCESS_TOKEN_SECRET_KEY,
                "expired-access-token",
            )
            .await
            .unwrap();
        secret_store
            .store(
                INTEGRATION_AUTH_SECRET_NAMESPACE,
                INTEGRATION_REFRESH_TOKEN_SECRET_KEY,
                "refresh-token-2",
            )
            .await
            .unwrap();
        secret_store
            .store(
                INTEGRATION_AUTH_SECRET_NAMESPACE,
                INTEGRATION_EXPIRES_AT_SECRET_KEY,
                &(Utc::now() - chrono::Duration::minutes(5)).to_rfc3339(),
            )
            .await
            .unwrap();

        let port = OidcDeviceFlowIntegrationAuthPort::new(
            OidcDeviceFlowAuthConfig {
                client_id: "desktop-client".to_string(),
                device_authorization_url: format!("{}/oauth/device/code", server.url()),
                token_url: token_endpoint,
                default_scopes: vec!["openid".to_string()],
                resource_indicator: Some("https://integration.example.com".to_string()),
                scheme: IntegrationAuthScheme::BearerToken,
                request_timeout: Duration::from_secs(5),
            },
            Arc::new(NoopIntegrationRequestProofFactory),
            Some(secret_store.clone()),
        )
        .unwrap();

        let status = port.current_auth_status().await.unwrap();
        assert_eq!(status.status, IntegrationAuthStatusKind::Ready);
        assert!(status.authenticated);
        assert_eq!(
            secret_store
                .retrieve(
                    INTEGRATION_AUTH_SECRET_NAMESPACE,
                    INTEGRATION_ACCESS_TOKEN_SECRET_KEY
                )
                .await
                .unwrap()
                .as_deref(),
            Some("access-token-2")
        );
        assert_eq!(
            secret_store
                .retrieve(
                    INTEGRATION_AUTH_SECRET_NAMESPACE,
                    INTEGRATION_REFRESH_TOKEN_SECRET_KEY
                )
                .await
                .unwrap()
                .as_deref(),
            Some("refresh-token-2b")
        );
        refresh_mock.assert_async().await;
    }

    #[tokio::test]
    async fn cancel_device_authorization_requires_existing_flow() {
        let port = OidcDeviceFlowIntegrationAuthPort::new(
            OidcDeviceFlowAuthConfig {
                client_id: "desktop-client".to_string(),
                device_authorization_url: "https://id.example.com/oauth/device/code".to_string(),
                token_url: "https://id.example.com/oauth/token".to_string(),
                default_scopes: vec!["openid".to_string()],
                resource_indicator: Some("https://integration.example.com".to_string()),
                scheme: IntegrationAuthScheme::BearerToken,
                request_timeout: Duration::from_secs(5),
            },
            Arc::new(NoopIntegrationRequestProofFactory),
            None,
        )
        .unwrap();

        let error = port
            .cancel_device_authorization("missing-flow")
            .await
            .unwrap_err();
        assert!(matches!(error, CoreError::NotFound { .. }));
    }

    #[tokio::test]
    async fn reset_auth_state_clears_pending_flow_and_stored_material() {
        let mut server = mockito::Server::new_async().await;
        let device_endpoint = format!("{}/oauth/device/code", server.url());
        let token_endpoint = format!("{}/oauth/token", server.url());

        let _device_mock = server
            .mock("POST", "/oauth/device/code")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("client_id".into(), "desktop-client".into()),
                Matcher::UrlEncoded("scope".into(), "openid prompt:read".into()),
                Matcher::UrlEncoded("resource".into(), "https://integration.example.com".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "device_code": "device-code-3",
                    "user_code": "QRST-UVWX",
                    "verification_uri": "https://id.example.com/activate",
                    "verification_uri_complete": "https://id.example.com/activate?user_code=QRST-UVWX",
                    "expires_in": 900,
                    "interval": 5
                })
                .to_string(),
            )
            .expect(1)
            .create_async()
            .await;

        let secret_store = Arc::new(InMemorySecretStore::default()) as Arc<dyn SecretStore>;
        secret_store
            .store(
                INTEGRATION_AUTH_SECRET_NAMESPACE,
                INTEGRATION_ACCESS_TOKEN_SECRET_KEY,
                "stale-access-token",
            )
            .await
            .unwrap();
        secret_store
            .store(
                INTEGRATION_AUTH_SECRET_NAMESPACE,
                INTEGRATION_REFRESH_TOKEN_SECRET_KEY,
                "stale-refresh-token",
            )
            .await
            .unwrap();

        let port = OidcDeviceFlowIntegrationAuthPort::new(
            OidcDeviceFlowAuthConfig {
                client_id: "desktop-client".to_string(),
                device_authorization_url: device_endpoint,
                token_url: token_endpoint,
                default_scopes: vec!["openid".to_string()],
                resource_indicator: Some("https://integration.example.com".to_string()),
                scheme: IntegrationAuthScheme::BearerToken,
                request_timeout: Duration::from_secs(5),
            },
            Arc::new(NoopIntegrationRequestProofFactory),
            Some(secret_store.clone()),
        )
        .unwrap();

        let flow = port
            .start_device_authorization(
                &[IntegrationCapabilityScope::PromptRead],
                Some("https://integration.example.com"),
            )
            .await
            .unwrap();

        assert!(!flow.flow_id.is_empty());

        port.reset_auth_state().await.unwrap();

        let status_after_reset = port.current_auth_status().await.unwrap();
        assert_eq!(
            status_after_reset.status,
            IntegrationAuthStatusKind::Unauthenticated
        );
        assert!(status_after_reset.pending_flow.is_none());
        assert_eq!(
            secret_store
                .retrieve(
                    INTEGRATION_AUTH_SECRET_NAMESPACE,
                    INTEGRATION_ACCESS_TOKEN_SECRET_KEY
                )
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            secret_store
                .retrieve(
                    INTEGRATION_AUTH_SECRET_NAMESPACE,
                    INTEGRATION_REFRESH_TOKEN_SECRET_KEY
                )
                .await
                .unwrap(),
            None
        );
    }
}
