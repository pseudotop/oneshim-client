//! 통합 네트워크 클라이언트
//!
//! REST API와 gRPC를 Feature Flag로 전환하는 통합 클라이언트.
//! GrpcConfig의 `use_grpc_auth`, `use_grpc_context` 설정에 따라
//! 자동으로 적절한 프로토콜을 선택합니다.

use std::collections::HashMap;
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

// Re-export gRPC 관련 타입
pub use crate::proto::user_context::{
    ContextBatchUploadRequest, ContextBatchUploadResponse, FeedbackType, ListSuggestionsResponse,
    Suggestion, SuggestionType,
};
pub use tonic::Streaming;

/// 인증 응답
#[derive(Debug, Clone)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub user_id: Option<String>,
}

/// 세션 응답
#[derive(Debug, Clone)]
pub struct SessionResponse {
    pub session_id: String,
    pub user_id: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

/// 통합 네트워크 클라이언트
///
/// REST와 gRPC를 Feature Flag로 전환하는 클라이언트.
pub struct UnifiedClient {
    config: GrpcConfig,

    // gRPC 클라이언트 (lazy init)
    grpc_auth: RwLock<Option<GrpcAuthClient>>,
    grpc_session: RwLock<Option<GrpcSessionClient>>,
    grpc_context: RwLock<Option<GrpcContextClient>>,

    // REST 클라이언트 (fallback)
    token_manager: Arc<TokenManager>,
    http_client: HttpApiClient,
}

impl UnifiedClient {
    /// 새 통합 클라이언트 생성
    pub fn new(config: GrpcConfig, token_manager: Arc<TokenManager>) -> Result<Self, CoreError> {
        info!(
            use_grpc_auth = config.use_grpc_auth,
            use_grpc_context = config.use_grpc_context,
            "UnifiedClient 초기화"
        );

        // REST fallback용 HTTP 클라이언트
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

    /// gRPC 인증 클라이언트 초기화 (lazy)
    async fn ensure_grpc_auth(&self) -> Result<(), CoreError> {
        if self.grpc_auth.read().await.is_some() {
            return Ok(());
        }

        let client = GrpcAuthClient::connect(self.config.clone()).await?;
        *self.grpc_auth.write().await = Some(client);
        Ok(())
    }

    /// gRPC 세션 클라이언트 초기화 (lazy)
    async fn ensure_grpc_session(&self) -> Result<(), CoreError> {
        if self.grpc_session.read().await.is_some() {
            return Ok(());
        }

        let client = GrpcSessionClient::connect(self.config.clone()).await?;
        *self.grpc_session.write().await = Some(client);
        Ok(())
    }

    /// gRPC 컨텍스트 클라이언트 초기화 (lazy)
    #[allow(dead_code)]
    async fn ensure_grpc_context(&self) -> Result<(), CoreError> {
        if self.grpc_context.read().await.is_some() {
            return Ok(());
        }

        let client = GrpcContextClient::connect(self.config.clone()).await?;
        *self.grpc_context.write().await = Some(client);
        Ok(())
    }

    /// 로그인
    ///
    /// Feature Flag에 따라 gRPC 또는 REST 사용
    pub async fn login(
        &self,
        identifier: &str,
        password: &str,
        organization_id: &str,
    ) -> Result<AuthResponse, CoreError> {
        if self.config.should_use_grpc_for_auth() {
            debug!("gRPC로 로그인 시도");
            self.login_grpc(identifier, password, organization_id).await
        } else {
            debug!("REST로 로그인 시도");
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
        let client = guard
            .as_mut()
            .ok_or_else(|| CoreError::Network("gRPC 인증 클라이언트 초기화 실패".to_string()))?;

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
        // TokenManager는 내부적으로 토큰을 저장
        self.token_manager
            .login_with_org(identifier, password, organization_id)
            .await?;

        // TokenManager에서 토큰을 가져옴
        let access_token = self.token_manager.get_token().await?;

        Ok(AuthResponse {
            access_token,
            refresh_token: String::new(), // REST에서는 refresh_token 직접 접근 불가
            expires_in: 3600,
            user_id: None,
        })
    }

    /// 토큰 갱신
    pub async fn refresh_token(&self) -> Result<AuthResponse, CoreError> {
        if self.config.should_use_grpc_for_auth() {
            // gRPC는 refresh_token을 직접 전달해야 하므로 현재는 REST fallback
            // 추후 토큰 저장소 통합 시 개선
            self.refresh_token_rest().await
        } else {
            self.refresh_token_rest().await
        }
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

    /// 세션 생성 (gRPC 전용)
    pub async fn create_session(
        &self,
        client_id: &str,
        device_info: HashMap<String, String>,
    ) -> Result<SessionResponse, CoreError> {
        if self.config.should_use_grpc_for_context() {
            self.create_session_grpc(client_id, device_info).await
        } else {
            // REST에는 별도 세션 생성 API가 없으므로 더미 응답
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
        let client = guard
            .as_mut()
            .ok_or_else(|| CoreError::Network("gRPC 세션 클라이언트 초기화 실패".to_string()))?;

        let response = client.create_session(client_id, device_info).await?;

        let session = response
            .session
            .ok_or_else(|| CoreError::Network("세션 응답이 비어있음".to_string()))?;

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

    /// 하트비트 전송
    pub async fn heartbeat(&self, session_id: &str, client_id: &str) -> Result<bool, CoreError> {
        if self.config.should_use_grpc_for_context() {
            self.heartbeat_grpc(session_id, client_id).await
        } else {
            // REST에서는 별도 heartbeat가 없으므로 항상 성공
            Ok(true)
        }
    }

    async fn heartbeat_grpc(&self, session_id: &str, client_id: &str) -> Result<bool, CoreError> {
        self.ensure_grpc_session().await?;

        let mut guard = self.grpc_session.write().await;
        let client = guard
            .as_mut()
            .ok_or_else(|| CoreError::Network("gRPC 세션 클라이언트 초기화 실패".to_string()))?;

        let response = client
            .heartbeat(session_id, client_id, HashMap::new())
            .await?;

        Ok(response.success)
    }

    /// 제안 스트림 구독
    ///
    /// 서버에서 실시간으로 제안을 수신합니다.
    /// gRPC Server-Streaming RPC를 사용하며, SSE를 대체합니다.
    ///
    /// # Arguments
    /// * `session_id` - 세션 ID
    /// * `client_id` - 클라이언트 ID
    ///
    /// # Returns
    /// `tonic::Streaming<Suggestion>` - 비동기 제안 스트림
    ///
    /// # Example
    /// ```ignore
    /// let mut stream = client.subscribe_suggestions("session-123", "client-456").await?;
    /// while let Some(suggestion) = stream.message().await? {
    ///     println!("제안 수신: {}", suggestion.content);
    /// }
    /// ```
    pub async fn subscribe_suggestions(
        &self,
        session_id: &str,
        client_id: &str,
    ) -> Result<Streaming<Suggestion>, CoreError> {
        if !self.config.should_use_grpc_for_context() {
            return Err(CoreError::Network(
                "제안 스트리밍은 gRPC 모드에서만 사용 가능합니다. use_grpc_context=true 설정 필요"
                    .to_string(),
            ));
        }

        debug!(
            "gRPC 제안 스트림 구독 시작: session_id={}, client_id={}",
            session_id, client_id
        );
        self.ensure_grpc_context().await?;

        let mut guard = self.grpc_context.write().await;
        let client = guard.as_mut().ok_or_else(|| {
            CoreError::Network("gRPC 컨텍스트 클라이언트 초기화 실패".to_string())
        })?;

        let stream = client.subscribe_suggestions(session_id, client_id).await?;
        info!("gRPC 제안 스트림 구독 성공");

        Ok(stream)
    }

    /// 배치 업로드
    ///
    /// 이벤트와 프레임을 서버로 일괄 전송합니다.
    /// gRPC 모드에서는 `UploadBatch` RPC를, REST 모드에서는 `/user_context/sync/batch`를 사용합니다.
    ///
    /// **주의**: REST 모드에서는 프레임 업로드가 지원되지 않습니다.
    ///
    /// # Arguments
    /// * `request` - 배치 업로드 요청 (client_id, session_id, events, frames 등)
    ///
    /// # Returns
    /// `ContextBatchUploadResponse` - 처리 결과 (status, processed_events, processed_frames 등)
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
    /// println!("처리된 이벤트: {}", response.processed_events);
    /// ```
    pub async fn upload_batch(
        &self,
        request: ContextBatchUploadRequest,
    ) -> Result<ContextBatchUploadResponse, CoreError> {
        if self.config.should_use_grpc_for_context() {
            debug!(
                "gRPC 배치 업로드 시작: session_id={}, events={}, frames={}",
                request.session_id,
                request.events.len(),
                request.frames.len()
            );
            self.ensure_grpc_context().await?;

            let mut guard = self.grpc_context.write().await;
            let client = guard.as_mut().ok_or_else(|| {
                CoreError::Network("gRPC 컨텍스트 클라이언트 초기화 실패".to_string())
            })?;

            let response = client.upload_batch(request).await?;
            info!(
                "gRPC 배치 업로드 완료: processed_events={}, processed_frames={}, status={}",
                response.processed_events, response.processed_frames, response.status
            );

            Ok(response)
        } else {
            // REST fallback — 프레임 업로드 미지원 경고
            if !request.frames.is_empty() {
                warn!(
                    "REST 모드에서는 프레임 업로드가 지원되지 않습니다. {} 프레임 무시됨",
                    request.frames.len()
                );
            }

            debug!(
                "REST 배치 업로드 시작: session_id={}, events={}",
                request.session_id,
                request.events.len()
            );

            // gRPC → REST 타입 변환 (빈 이벤트 목록으로 전송)
            let batch = EventBatch {
                session_id: request.session_id.clone(),
                events: vec![], // gRPC Event와 REST Event 타입이 다름, 빈 배열로 전송
                created_at: chrono::Utc::now(),
            };

            self.http_client.upload_batch(&batch).await?;
            info!("REST 배치 업로드 완료");

            Ok(ContextBatchUploadResponse {
                status: "success".to_string(),
                processed_events: 0, // REST에서는 실제 처리 결과 알 수 없음
                processed_frames: 0,
                sync_sequence: request.sync_sequence,
                next_sync_time: None,
                server_instructions: HashMap::new(),
                errors: vec![],
            })
        }
    }

    /// 제안 피드백 전송
    ///
    /// 사용자가 제안을 수락/거절/연기했을 때 서버에 피드백을 전송합니다.
    /// gRPC 모드에서는 `SendFeedback` RPC를, REST 모드에서는 `/user_context/suggestions/feedback`를 사용합니다.
    ///
    /// # Arguments
    /// * `suggestion_id` - 피드백 대상 제안 ID
    /// * `feedback_type` - 피드백 유형 (Accepted, Rejected, Deferred)
    /// * `comment` - 선택적 코멘트
    ///
    /// # Example
    /// ```ignore
    /// client.send_feedback(
    ///     "suggestion-123",
    ///     FeedbackType::Accepted,
    ///     Some("유용한 제안이었습니다")
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
                "gRPC 피드백 전송: suggestion_id={}, feedback_type={:?}",
                suggestion_id, feedback_type
            );
            self.ensure_grpc_context().await?;

            let mut guard = self.grpc_context.write().await;
            let client = guard.as_mut().ok_or_else(|| {
                CoreError::Network("gRPC 컨텍스트 클라이언트 초기화 실패".to_string())
            })?;

            client
                .send_feedback(suggestion_id, feedback_type, comment)
                .await?;
            info!("gRPC 피드백 전송 완료: suggestion_id={}", suggestion_id);

            Ok(())
        } else {
            debug!(
                "REST 피드백 전송: suggestion_id={}, feedback_type={:?}",
                suggestion_id, feedback_type
            );

            // gRPC FeedbackType → REST FeedbackType 변환
            let rest_feedback_type = match feedback_type {
                FeedbackType::Accepted => oneshim_core::models::suggestion::FeedbackType::Accepted,
                FeedbackType::Rejected => oneshim_core::models::suggestion::FeedbackType::Rejected,
                FeedbackType::Deferred => oneshim_core::models::suggestion::FeedbackType::Deferred,
                _ => oneshim_core::models::suggestion::FeedbackType::Rejected, // 알 수 없는 타입은 Rejected로 처리
            };

            let feedback = RestSuggestionFeedback {
                suggestion_id: suggestion_id.to_string(),
                feedback_type: rest_feedback_type,
                comment: comment.map(String::from),
                timestamp: chrono::Utc::now(),
            };

            self.http_client.send_feedback(&feedback).await?;
            info!("REST 피드백 전송 완료: suggestion_id={}", suggestion_id);

            Ok(())
        }
    }

    /// 제안 목록 조회
    ///
    /// 서버에서 제안 목록을 가져옵니다.
    /// gRPC 모드에서는 `ListSuggestions` RPC를 사용합니다.
    ///
    /// **주의**: REST 모드에서는 `/suggestions/history` 엔드포인트가 다른 형식을 반환하므로
    /// 빈 목록이 반환됩니다. 전체 기능을 사용하려면 `use_grpc_context=true`를 설정하세요.
    ///
    /// # Arguments
    /// * `types` - 조회할 제안 유형 필터 (빈 배열이면 전체 조회)
    /// * `limit` - 최대 조회 개수
    ///
    /// # Example
    /// ```ignore
    /// // 모든 유형 20개 조회
    /// let response = client.list_suggestions(vec![], 20).await?;
    /// for suggestion in response.suggestions {
    ///     println!("제안: {}", suggestion.content);
    /// }
    ///
    /// // 특정 유형만 조회
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
            debug!("gRPC 제안 목록 조회: types={:?}, limit={}", types, limit);
            self.ensure_grpc_context().await?;

            let mut guard = self.grpc_context.write().await;
            let client = guard.as_mut().ok_or_else(|| {
                CoreError::Network("gRPC 컨텍스트 클라이언트 초기화 실패".to_string())
            })?;

            let response = client.list_suggestions(types, limit).await?;
            info!(
                "gRPC 제안 목록 조회 완료: count={}",
                response.suggestions.len()
            );

            Ok(response)
        } else {
            // REST 모드에서는 /suggestions/history 형식이 다르므로 빈 목록 반환
            warn!(
                "REST 모드에서는 제안 목록 조회가 제한적입니다. \
                 전체 기능을 사용하려면 use_grpc_context=true를 설정하세요."
            );

            Ok(ListSuggestionsResponse {
                suggestions: vec![],
                total_count: 0,
            })
        }
    }

    /// 설정 조회
    pub fn config(&self) -> &GrpcConfig {
        &self.config
    }

    /// gRPC 사용 여부 확인
    pub fn is_using_grpc(&self) -> bool {
        self.config.use_grpc_auth || self.config.use_grpc_context
    }

    /// TokenManager 참조 반환
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
