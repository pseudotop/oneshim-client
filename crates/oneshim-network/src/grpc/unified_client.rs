//!

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use oneshim_core::error::CoreError;
use oneshim_core::models::event::EventBatch;
use oneshim_core::models::suggestion::SuggestionFeedback as RestSuggestionFeedback;
use oneshim_core::ports::api_client::ApiClient; // ApiClient trait for HttpApiClient methods
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::auth_client::GrpcAuthClient;
use super::config::GrpcConfig;
use super::context_client::GrpcContextClient;
use super::session_client::GrpcSessionClient;
use crate::auth::TokenManager;
use crate::http_client::HttpApiClient;

pub use crate::proto::user_context::{
    ContextBatchUploadRequest, ContextBatchUploadResponse, FeedbackType, ListSuggestionsResponse,
    Suggestion, SuggestionType,
};
pub use tonic::Streaming;

#[derive(Debug, Clone)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionResponse {
    pub session_id: String,
    pub user_id: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

///
pub struct UnifiedClient {
    config: GrpcConfig,

    grpc_auth: RwLock<Option<GrpcAuthClient>>,
    grpc_session: RwLock<Option<GrpcSessionClient>>,
    grpc_context: RwLock<Option<GrpcContextClient>>,

    token_manager: Arc<TokenManager>,
    http_client: HttpApiClient,
}

impl UnifiedClient {
    pub fn new(config: GrpcConfig, token_manager: Arc<TokenManager>) -> Result<Self, CoreError> {
        info!(
            use_grpc_auth = config.use_grpc_auth,
            use_grpc_context = config.use_grpc_context,
            "UnifiedClient initialize"
        );

        let http_client = HttpApiClient::new(
            &config.rest_endpoint,
            token_manager.clone(),
            Duration::from_secs(config.request_timeout_secs),
        )?;

        Ok(Self {
            config,
            grpc_auth: RwLock::new(None),
            grpc_session: RwLock::new(None),
            grpc_context: RwLock::new(None),
            token_manager,
            http_client,
        })
    }

    async fn ensure_grpc_auth(&self) -> Result<(), CoreError> {
        if self.grpc_auth.read().await.is_some() {
            return Ok(());
        }

        let client = GrpcAuthClient::connect(self.config.clone()).await?;
        *self.grpc_auth.write().await = Some(client);
        Ok(())
    }

    async fn ensure_grpc_session(&self) -> Result<(), CoreError> {
        if self.grpc_session.read().await.is_some() {
            return Ok(());
        }

        let client = GrpcSessionClient::connect(self.config.clone()).await?;
        *self.grpc_session.write().await = Some(client);
        Ok(())
    }

    #[allow(dead_code)]
    async fn ensure_grpc_context(&self) -> Result<(), CoreError> {
        if self.grpc_context.read().await.is_some() {
            return Ok(());
        }

        let client = GrpcContextClient::connect(self.config.clone()).await?;
        *self.grpc_context.write().await = Some(client);
        Ok(())
    }

    async fn with_grpc_context_client<R, F>(&self, op: &str, f: F) -> Result<R, CoreError>
    where
        F: for<'a> FnOnce(
            &'a mut GrpcContextClient,
        )
            -> Pin<Box<dyn Future<Output = Result<R, CoreError>> + Send + 'a>>,
    {
        self.ensure_grpc_context().await?;
        let mut guard = self.grpc_context.write().await;
        let client = guard.as_mut().ok_or_else(|| {
            CoreError::Network(format!("gRPC context client initialize failure ({op})"))
        })?;
        f(client).await
    }

    ///
    pub async fn login(
        &self,
        identifier: &str,
        password: &str,
        organization_id: &str,
    ) -> Result<AuthResponse, CoreError> {
        if self.config.should_use_grpc_for_auth() {
            debug!("gRPC login attempt");
            self.login_grpc(identifier, password, organization_id).await
        } else {
            debug!("REST login attempt");
            self.login_rest(identifier, password, organization_id).await
        }
    }

    async fn login_grpc(
        &self,
        identifier: &str,
        password: &str,
        organization_id: &str,
    ) -> Result<AuthResponse, CoreError> {
        self.ensure_grpc_auth().await?;

        let mut guard = self.grpc_auth.write().await;
        let client = guard.as_mut().ok_or_else(|| {
            CoreError::Network("Failed to initialize gRPC auth client".to_string())
        })?;

        let device_info = HashMap::new();
        let response = client
            .login(identifier, password, organization_id, device_info)
            .await?;

        Ok(AuthResponse {
            access_token: response.access_token,
            refresh_token: response.refresh_token,
            expires_in: response.expires_in as i64,
            user_id: if response.user_id.is_empty() {
                None
            } else {
                Some(response.user_id)
            },
        })
    }

    async fn login_rest(
        &self,
        identifier: &str,
        password: &str,
        organization_id: &str,
    ) -> Result<AuthResponse, CoreError> {
        self.token_manager
            .login_with_org(identifier, password, organization_id)
            .await?;

        let access_token = self.token_manager.get_token().await?;

        Ok(AuthResponse {
            access_token,
            refresh_token: String::new(), // refresh token is not exposed in REST mode
            expires_in: 3600,
            user_id: None,
        })
    }

    pub async fn refresh_token(&self) -> Result<AuthResponse, CoreError> {
        self.refresh_token_rest().await
    }

    async fn refresh_token_rest(&self) -> Result<AuthResponse, CoreError> {
        self.token_manager.refresh().await?;

        let access_token = self.token_manager.get_token().await?;

        Ok(AuthResponse {
            access_token,
            refresh_token: String::new(),
            expires_in: 3600,
            user_id: None,
        })
    }

    pub async fn create_session(
        &self,
        client_id: &str,
        device_info: HashMap<String, String>,
    ) -> Result<SessionResponse, CoreError> {
        if self.config.should_use_grpc_for_context() {
            self.create_session_grpc(client_id, device_info).await
        } else {
            Ok(SessionResponse {
                session_id: String::new(),
                user_id: String::new(),
                access_token: None,
                refresh_token: None,
            })
        }
    }

    async fn create_session_grpc(
        &self,
        client_id: &str,
        device_info: HashMap<String, String>,
    ) -> Result<SessionResponse, CoreError> {
        self.ensure_grpc_session().await?;

        let mut guard = self.grpc_session.write().await;
        let client = guard.as_mut().ok_or_else(|| {
            CoreError::Network("gRPC session client initialize failure".to_string())
        })?;

        let response = client.create_session(client_id, device_info).await?;

        let session = response
            .session
            .ok_or_else(|| CoreError::Network("Session response is empty".to_string()))?;

        Ok(SessionResponse {
            session_id: session.session_id,
            user_id: session.user_id,
            access_token: if response.access_token.is_empty() {
                None
            } else {
                Some(response.access_token)
            },
            refresh_token: if response.refresh_token.is_empty() {
                None
            } else {
                Some(response.refresh_token)
            },
        })
    }

    pub async fn heartbeat(&self, session_id: &str, client_id: &str) -> Result<bool, CoreError> {
        if self.config.should_use_grpc_for_context() {
            self.heartbeat_grpc(session_id, client_id).await
        } else {
            Ok(true)
        }
    }

    async fn heartbeat_grpc(&self, session_id: &str, client_id: &str) -> Result<bool, CoreError> {
        self.ensure_grpc_session().await?;

        let mut guard = self.grpc_session.write().await;
        let client = guard.as_mut().ok_or_else(|| {
            CoreError::Network("gRPC session client initialize failure".to_string())
        })?;

        let response = client
            .heartbeat(session_id, client_id, HashMap::new())
            .await?;

        Ok(response.success)
    }

    ///
    ///
    /// # Arguments
    ///
    /// # Returns
    ///
    /// # Example
    /// ```ignore
    /// let mut stream = client.subscribe_suggestions("session-123", "client-456").await?;
    /// while let Some(suggestion) = stream.message().await? {
    /// }
    /// ```
    pub async fn subscribe_suggestions(
        &self,
        session_id: &str,
        client_id: &str,
    ) -> Result<Streaming<Suggestion>, CoreError> {
        if !self.config.should_use_grpc_for_context() {
            return Err(CoreError::Network(
                "Suggestion streaming is available only in gRPC mode. Set use_grpc_context=true."
                    .to_string(),
            ));
        }

        debug!(
            "gRPC suggestion stream subscribe started: session_id={}, client_id={}",
            session_id, client_id
        );
        self.ensure_grpc_context().await?;

        let mut guard = self.grpc_context.write().await;
        let client = guard.as_mut().ok_or_else(|| {
            CoreError::Network("gRPC context client initialize failure".to_string())
        })?;

        let stream = client.subscribe_suggestions(session_id, client_id).await?;
        info!("gRPC suggestion stream subscribe success");

        Ok(stream)
    }

    ///
    ///
    ///
    /// # Arguments
    ///
    /// # Returns
    ///
    /// # Example
    /// ```ignore
    /// let request = ContextBatchUploadRequest {
    ///     client_id: "client-123".to_string(),
    ///     session_id: "session-456".to_string(),
    ///     events: vec![...],
    ///     frames: vec![...],
    ///     ..Default::default()
    /// };
    /// let response = client.upload_batch(request).await?;
    /// ```
    pub async fn upload_batch(
        &self,
        request: ContextBatchUploadRequest,
    ) -> Result<ContextBatchUploadResponse, CoreError> {
        if self.config.should_use_grpc_for_context() {
            debug!(
                "gRPC batch upload started: session_id={}, events={}, frames={}",
                request.session_id,
                request.events.len(),
                request.frames.len()
            );
            let response = self
                .with_grpc_context_client("upload_batch", |client| {
                    Box::pin(async move { client.upload_batch(request).await })
                })
                .await?;
            info!(
                "gRPC batch upload completed: processed_events={}, processed_frames={}, status={}",
                response.processed_events, response.processed_frames, response.status
            );

            Ok(response)
        } else {
            if !request.frames.is_empty() {
                warn!(
                    "REST mode does not support frame upload. Ignoring {} frame(s).",
                    request.frames.len()
                );
            }

            debug!(
                "REST batch upload started: session_id={}, events={}",
                request.session_id,
                request.events.len()
            );

            let batch = EventBatch {
                session_id: request.session_id.clone(),
                events: vec![], // REST path only sends event batches
                created_at: chrono::Utc::now(),
            };

            self.http_client.upload_batch(&batch).await?;
            info!("REST batch upload completed");

            Ok(ContextBatchUploadResponse {
                status: "success".to_string(),
                processed_events: 0, // REST endpoint does not return this count
                processed_frames: 0,
                sync_sequence: request.sync_sequence,
                next_sync_time: None,
                server_instructions: HashMap::new(),
                errors: vec![],
            })
        }
    }

    ///
    ///
    /// # Arguments
    ///
    /// # Example
    /// ```ignore
    /// client.send_feedback(
    ///     "suggestion-123",
    ///     FeedbackType::Accepted,
    /// ).await?;
    /// ```
    pub async fn send_feedback(
        &self,
        suggestion_id: &str,
        feedback_type: FeedbackType,
        comment: Option<&str>,
    ) -> Result<(), CoreError> {
        if self.config.should_use_grpc_for_context() {
            debug!(
                "gRPC feedback sent: suggestion_id={}, feedback_type={:?}",
                suggestion_id, feedback_type
            );
            let suggestion_id_owned = suggestion_id.to_string();
            let comment_owned = comment.map(String::from);
            self.with_grpc_context_client("send_feedback", |client| {
                let suggestion_id = suggestion_id_owned;
                let comment = comment_owned;
                Box::pin(async move {
                    client
                        .send_feedback(&suggestion_id, feedback_type, comment.as_deref())
                        .await
                })
            })
            .await?;
            info!(
                "gRPC feedback sent completed: suggestion_id={}",
                suggestion_id
            );

            Ok(())
        } else {
            debug!(
                "REST feedback sent: suggestion_id={}, feedback_type={:?}",
                suggestion_id, feedback_type
            );

            let rest_feedback_type = match feedback_type {
                FeedbackType::Accepted => oneshim_core::models::suggestion::FeedbackType::Accepted,
                FeedbackType::Rejected => oneshim_core::models::suggestion::FeedbackType::Rejected,
                FeedbackType::Deferred => oneshim_core::models::suggestion::FeedbackType::Deferred,
                _ => oneshim_core::models::suggestion::FeedbackType::Rejected, // unknown -> rejected
            };

            let feedback = RestSuggestionFeedback {
                suggestion_id: suggestion_id.to_string(),
                feedback_type: rest_feedback_type,
                comment: comment.map(String::from),
                timestamp: chrono::Utc::now(),
            };

            self.http_client.send_feedback(&feedback).await?;
            info!(
                "REST feedback sent completed: suggestion_id={}",
                suggestion_id
            );

            Ok(())
        }
    }

    ///
    ///
    ///
    /// # Arguments
    ///
    /// # Example
    /// ```ignore
    /// let response = client.list_suggestions(vec![], 20).await?;
    /// for suggestion in response.suggestions {
    /// }
    ///
    /// let response = client.list_suggestions(
    ///     vec![SuggestionType::WorkGuidance, SuggestionType::ProductivityTip],
    ///     10
    /// ).await?;
    /// ```
    pub async fn list_suggestions(
        &self,
        types: Vec<SuggestionType>,
        limit: i32,
    ) -> Result<ListSuggestionsResponse, CoreError> {
        if self.config.should_use_grpc_for_context() {
            debug!(
                "gRPC suggestion list query: types={:?}, limit={}",
                types, limit
            );
            let response = self
                .with_grpc_context_client("list_suggestions", |client| {
                    Box::pin(async move { client.list_suggestions(types, limit).await })
                })
                .await?;
            info!(
                "gRPC suggestion list query completed: count={}",
                response.suggestions.len()
            );

            Ok(response)
        } else {
            warn!(
                "Suggestion list queries are limited in REST mode. \
                 Set use_grpc_context=true for full functionality."
            );

            Ok(ListSuggestionsResponse {
                suggestions: vec![],
                total_count: 0,
            })
        }
    }

    pub fn config(&self) -> &GrpcConfig {
        &self.config
    }

    pub fn is_using_grpc(&self) -> bool {
        self.config.use_grpc_auth || self.config.use_grpc_context
    }

    pub fn token_manager(&self) -> &Arc<TokenManager> {
        &self.token_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_response() {
        let response = AuthResponse {
            access_token: "token".to_string(),
            refresh_token: "refresh".to_string(),
            expires_in: 3600,
            user_id: Some("user-123".to_string()),
        };
        assert_eq!(response.access_token, "token");
        assert_eq!(response.user_id, Some("user-123".to_string()));
    }

    #[test]
    fn test_session_response() {
        let response = SessionResponse {
            session_id: "session-123".to_string(),
            user_id: "user-456".to_string(),
            access_token: None,
            refresh_token: None,
        };
        assert_eq!(response.session_id, "session-123");
    }
}
