//! gRPC 컨텍스트 클라이언트
//!
//! 서버의 UserContextService와 통신합니다.
//! 배치 업로드, 제안 스트림, 피드백 등을 처리합니다.

use oneshim_core::error::CoreError;
use tonic::transport::Channel;
use tracing::{debug, error, info};

use super::{map_grpc_status_error, GrpcConfig};
use crate::proto::user_context::{
    user_context_service_client::UserContextServiceClient, ContextBatchUploadRequest,
    ContextBatchUploadResponse, FeedbackType, HeartbeatRequest, HeartbeatResponse,
    ListSuggestionsRequest, ListSuggestionsResponse, SubscribeRequest, Suggestion,
    SuggestionFeedback, SuggestionType,
};

/// gRPC 컨텍스트 클라이언트
pub struct GrpcContextClient {
    client: UserContextServiceClient<Channel>,
    config: GrpcConfig,
}

impl GrpcContextClient {
    /// 새 gRPC 컨텍스트 클라이언트 생성 (포트 fallback 지원)
    pub async fn connect(config: GrpcConfig) -> Result<Self, CoreError> {
        let endpoints = config.all_endpoints();
        let mut last_error = None;

        for endpoint_url in &endpoints {
            info!(endpoint = %endpoint_url, "gRPC 컨텍스트 클라이언트 연결 시도");

            match config.connect_channel(endpoint_url).await {
                Ok(channel) => {
                    let client = UserContextServiceClient::new(channel);
                    info!(endpoint = %endpoint_url, "gRPC 컨텍스트 클라이언트 연결 완료");
                    return Ok(Self { client, config });
                }
                Err(e) => {
                    debug!(endpoint = %endpoint_url, error = %e, "gRPC 연결 실패, 다음 포트 시도");
                    last_error = Some(e);
                }
            }
        }

        // 모든 포트 시도 실패
        error!(endpoints = ?endpoints, "모든 gRPC 엔드포인트 연결 실패");
        Err(last_error.unwrap_or_else(|| CoreError::Network("gRPC 엔드포인트 없음".to_string())))
    }

    /// 배치 업로드
    ///
    /// 이벤트와 프레임을 서버로 일괄 전송합니다.
    pub async fn upload_batch(
        &mut self,
        request: ContextBatchUploadRequest,
    ) -> Result<ContextBatchUploadResponse, CoreError> {
        debug!("gRPC 배치 업로드 요청");

        let response = self
            .client
            .upload_batch(tonic::Request::new(request))
            .await
            .map_err(|status| {
                error!(error = %status, "gRPC 배치 업로드 실패");
                map_grpc_status_error("grpc batch upload failed", status)
            })?;

        Ok(response.into_inner())
    }

    /// 제안 스트림 구독
    ///
    /// 서버에서 실시간으로 제안을 수신합니다. (SSE 대체)
    pub async fn subscribe_suggestions(
        &mut self,
        session_id: &str,
        client_id: &str,
    ) -> Result<tonic::Streaming<Suggestion>, CoreError> {
        debug!("gRPC 제안 스트림 구독 요청");

        let request = tonic::Request::new(SubscribeRequest {
            session_id: session_id.to_string(),
            client_id: client_id.to_string(),
            subscription_types: vec![],
        });

        let response = self
            .client
            .subscribe_suggestions(request)
            .await
            .map_err(|status| {
                error!(error = %status, "gRPC 제안 스트림 구독 실패");
                map_grpc_status_error("grpc suggestion stream subscription failed", status)
            })?;

        Ok(response.into_inner())
    }

    /// 제안 피드백 전송
    pub async fn send_feedback(
        &mut self,
        suggestion_id: &str,
        feedback_type: FeedbackType,
        comment: Option<&str>,
    ) -> Result<(), CoreError> {
        debug!(suggestion_id = %suggestion_id, "gRPC 피드백 전송");

        let request = tonic::Request::new(SuggestionFeedback {
            suggestion_id: suggestion_id.to_string(),
            feedback_type: feedback_type as i32,
            timestamp: None,
            comment: comment.map(String::from),
            reason: None,
        });

        self.client.send_feedback(request).await.map_err(|status| {
            error!(error = %status, "gRPC 피드백 전송 실패");
            map_grpc_status_error("grpc feedback submission failed", status)
        })?;

        Ok(())
    }

    /// 하트비트 전송
    pub async fn heartbeat(
        &mut self,
        session_id: &str,
        client_id: &str,
    ) -> Result<HeartbeatResponse, CoreError> {
        debug!(session_id = %session_id, "gRPC 하트비트 전송");

        let request = tonic::Request::new(HeartbeatRequest {
            session_id: session_id.to_string(),
            client_id: client_id.to_string(),
            timestamp: None,
            client_state: std::collections::HashMap::new(),
        });

        let response = self.client.heartbeat(request).await.map_err(|status| {
            error!(error = %status, "gRPC 하트비트 실패");
            map_grpc_status_error("grpc heartbeat failed", status)
        })?;

        Ok(response.into_inner())
    }

    /// 제안 목록 조회
    ///
    /// 서버에서 제안 목록을 가져옵니다.
    pub async fn list_suggestions(
        &mut self,
        types: Vec<SuggestionType>,
        limit: i32,
    ) -> Result<ListSuggestionsResponse, CoreError> {
        debug!(limit = %limit, "gRPC 제안 목록 조회");

        let request = tonic::Request::new(ListSuggestionsRequest {
            types: types.into_iter().map(|t| t as i32).collect(),
            min_priority: 0, // 모든 우선순위
            limit,
            active_only: true,
        });

        let response = self
            .client
            .list_suggestions(request)
            .await
            .map_err(|status| {
                error!(error = %status, "gRPC 제안 목록 조회 실패");
                map_grpc_status_error("grpc suggestion list failed", status)
            })?;

        Ok(response.into_inner())
    }

    /// 설정 조회
    pub fn config(&self) -> &GrpcConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_upload_request() {
        let request = ContextBatchUploadRequest {
            client_id: "client-123".to_string(),
            session_id: "session-456".to_string(),
            upload_trigger: 0, // UNSPECIFIED
            upload_timestamp: None,
            events: vec![],
            frames: vec![],
            client_stats: std::collections::HashMap::new(),
            last_sync_timestamp: None,
            sync_sequence: 1,
        };
        assert_eq!(request.client_id, "client-123");
    }
}
