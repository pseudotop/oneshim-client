//! 제안 모델.
//!
//! 서버에서 SSE로 수신하는 제안 데이터와 사용자 피드백 구조체.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// AI 제안 (서버 → 클라이언트 SSE로 수신)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// 제안 고유 ID
    pub suggestion_id: String,
    /// 제안 유형
    pub suggestion_type: SuggestionType,
    /// 제안 내용 텍스트
    pub content: String,
    /// 우선순위
    pub priority: Priority,
    /// AI 신뢰도 점수 (0.0 ~ 1.0)
    pub confidence_score: f64,
    /// 현재 컨텍스트와의 관련도 (0.0 ~ 1.0)
    pub relevance_score: f64,
    /// 사용자가 즉시 실행 가능한 제안인지
    pub is_actionable: bool,
    /// 제안 생성 시각
    pub created_at: DateTime<Utc>,
    /// 제안 만료 시각 (None이면 만료 없음)
    pub expires_at: Option<DateTime<Utc>>,
}

/// 제안 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SuggestionType {
    /// 업무 가이던스
    WorkGuidance,
    /// 이메일 초안
    EmailDraft,
    /// 생산성 팁
    ProductivityTip,
    /// 워크플로우 최적화
    WorkflowOptimization,
    /// 컨텍스트 기반 제안
    ContextBased,
}

/// 제안 우선순위
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

/// 사용자 피드백 (수락/거절)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionFeedback {
    /// 대상 제안 ID
    pub suggestion_id: String,
    /// 피드백 유형
    pub feedback_type: FeedbackType,
    /// 피드백 시각
    pub timestamp: DateTime<Utc>,
    /// 추가 코멘트 (선택)
    pub comment: Option<String>,
}

/// 피드백 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FeedbackType {
    /// 제안 수락
    Accepted,
    /// 제안 거절
    Rejected,
    /// 나중에 보기
    Deferred,
}
