//! 제안 피드백 전송.
//!
//! 수락/거절/나중에 → 서버 HTTP POST.

use chrono::Utc;
use oneshim_core::error::CoreError;
use oneshim_core::models::suggestion::{FeedbackType, SuggestionFeedback};
use oneshim_core::ports::api_client::ApiClient;
use std::sync::Arc;
use tracing::{debug, warn};

/// 피드백 전송기 — 사용자 피드백을 서버에 전송
pub struct FeedbackSender {
    api_client: Arc<dyn ApiClient>,
}

impl FeedbackSender {
    /// 새 피드백 전송기 생성
    pub fn new(api_client: Arc<dyn ApiClient>) -> Self {
        Self { api_client }
    }

    /// 제안 수락
    pub async fn accept(
        &self,
        suggestion_id: &str,
        comment: Option<String>,
    ) -> Result<(), CoreError> {
        self.send_feedback(suggestion_id, FeedbackType::Accepted, comment)
            .await
    }

    /// 제안 거절
    pub async fn reject(
        &self,
        suggestion_id: &str,
        comment: Option<String>,
    ) -> Result<(), CoreError> {
        self.send_feedback(suggestion_id, FeedbackType::Rejected, comment)
            .await
    }

    /// 나중에 보기
    pub async fn defer(
        &self,
        suggestion_id: &str,
        comment: Option<String>,
    ) -> Result<(), CoreError> {
        self.send_feedback(suggestion_id, FeedbackType::Deferred, comment)
            .await
    }

    /// 피드백 전송 공통 로직
    async fn send_feedback(
        &self,
        suggestion_id: &str,
        feedback_type: FeedbackType,
        comment: Option<String>,
    ) -> Result<(), CoreError> {
        let feedback = SuggestionFeedback {
            suggestion_id: suggestion_id.to_string(),
            feedback_type: feedback_type.clone(),
            timestamp: Utc::now(),
            comment,
        };

        debug!("피드백 전송: {suggestion_id} → {feedback_type:?}");

        match self.api_client.send_feedback(&feedback).await {
            Ok(()) => {
                debug!("피드백 전송 성공");
                Ok(())
            }
            Err(e) => {
                warn!("피드백 전송 실패: {e}");
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::models::event::EventBatch;
    use oneshim_core::models::frame::ContextUpload;

    struct MockApiClient;

    #[async_trait::async_trait]
    impl ApiClient for MockApiClient {
        async fn create_session(
            &self,
            client_id: &str,
        ) -> Result<oneshim_core::ports::api_client::SessionCreateResponse, CoreError> {
            Ok(oneshim_core::ports::api_client::SessionCreateResponse {
                session_id: format!("sess_{client_id}"),
                user_id: "user_1".to_string(),
                client_id: client_id.to_string(),
                capabilities: vec![],
            })
        }
        async fn end_session(&self, _: &str) -> Result<(), CoreError> {
            Ok(())
        }
        async fn upload_batch(&self, _: &EventBatch) -> Result<(), CoreError> {
            Ok(())
        }
        async fn upload_context(&self, _: &ContextUpload) -> Result<(), CoreError> {
            Ok(())
        }
        async fn send_feedback(&self, feedback: &SuggestionFeedback) -> Result<(), CoreError> {
            assert!(!feedback.suggestion_id.is_empty());
            Ok(())
        }
        async fn send_heartbeat(&self, _: &str) -> Result<(), CoreError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn accept_feedback() {
        let sender = FeedbackSender::new(Arc::new(MockApiClient));
        sender.accept("sug_001", None).await.unwrap();
    }

    #[tokio::test]
    async fn reject_feedback_with_comment() {
        let sender = FeedbackSender::new(Arc::new(MockApiClient));
        sender
            .reject("sug_002", Some("관련 없음".to_string()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn defer_feedback() {
        let sender = FeedbackSender::new(Arc::new(MockApiClient));
        sender.defer("sug_003", None).await.unwrap();
    }
}
