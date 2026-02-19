//! 제안 프레젠터.
//!
//! Suggestion → UI 표시용 데이터 변환.

use oneshim_core::models::suggestion::{Priority, Suggestion, SuggestionType};

/// UI 표시용 제안 데이터
#[derive(Debug, Clone)]
pub struct SuggestionView {
    /// 제안 ID
    pub id: String,
    /// 제목 (유형 기반)
    pub title: String,
    /// 본문 (제안 내용)
    pub body: String,
    /// 우선순위 라벨
    pub priority_label: String,
    /// 우선순위 색상 (#RRGGBB)
    pub priority_color: String,
    /// 유형 아이콘 이모지
    pub type_icon: String,
    /// 신뢰도 텍스트
    pub confidence_text: String,
    /// 실행 가능 여부
    pub is_actionable: bool,
    /// 시간 텍스트 (상대)
    pub time_text: String,
}

/// Suggestion → SuggestionView 변환
pub fn present(suggestion: &Suggestion) -> SuggestionView {
    SuggestionView {
        id: suggestion.suggestion_id.clone(),
        title: type_to_title(&suggestion.suggestion_type),
        body: suggestion.content.clone(),
        priority_label: priority_to_label(&suggestion.priority),
        priority_color: priority_to_color(&suggestion.priority),
        type_icon: type_to_icon(&suggestion.suggestion_type),
        confidence_text: format!("{:.0}%", suggestion.confidence_score * 100.0),
        is_actionable: suggestion.is_actionable,
        time_text: format_relative_time(suggestion.created_at),
    }
}

/// 여러 제안 일괄 변환
pub fn present_all(suggestions: &[Suggestion]) -> Vec<SuggestionView> {
    suggestions.iter().map(present).collect()
}

fn type_to_title(st: &SuggestionType) -> String {
    match st {
        SuggestionType::WorkGuidance => "업무 가이던스".to_string(),
        SuggestionType::EmailDraft => "이메일 초안".to_string(),
        SuggestionType::ProductivityTip => "생산성 팁".to_string(),
        SuggestionType::WorkflowOptimization => "워크플로우 최적화".to_string(),
        SuggestionType::ContextBased => "컨텍스트 기반 제안".to_string(),
    }
}

fn type_to_icon(st: &SuggestionType) -> String {
    match st {
        SuggestionType::WorkGuidance => "compass".to_string(),
        SuggestionType::EmailDraft => "mail".to_string(),
        SuggestionType::ProductivityTip => "zap".to_string(),
        SuggestionType::WorkflowOptimization => "git-branch".to_string(),
        SuggestionType::ContextBased => "brain".to_string(),
    }
}

fn priority_to_label(p: &Priority) -> String {
    match p {
        Priority::Critical => "긴급".to_string(),
        Priority::High => "높음".to_string(),
        Priority::Medium => "보통".to_string(),
        Priority::Low => "낮음".to_string(),
    }
}

fn priority_to_color(p: &Priority) -> String {
    match p {
        Priority::Critical => "#EF4444".to_string(), // red-500
        Priority::High => "#F97316".to_string(),     // orange-500
        Priority::Medium => "#3B82F6".to_string(),   // blue-500
        Priority::Low => "#6B7280".to_string(),      // gray-500
    }
}

fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now - dt;

    if diff.num_seconds() < 60 {
        "방금 전".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}분 전", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}시간 전", diff.num_hours())
    } else {
        format!("{}일 전", diff.num_days())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_suggestion() -> Suggestion {
        Suggestion {
            suggestion_id: "sug_001".to_string(),
            suggestion_type: SuggestionType::WorkGuidance,
            content: "테스트 제안".to_string(),
            priority: Priority::High,
            confidence_score: 0.95,
            relevance_score: 0.88,
            is_actionable: true,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    #[test]
    fn present_suggestion() {
        let view = present(&make_suggestion());
        assert_eq!(view.id, "sug_001");
        assert_eq!(view.title, "업무 가이던스");
        assert_eq!(view.priority_label, "높음");
        assert_eq!(view.priority_color, "#F97316");
        assert_eq!(view.confidence_text, "95%");
        assert!(view.is_actionable);
    }

    #[test]
    fn present_all_suggestions() {
        let suggestions = vec![make_suggestion(), make_suggestion()];
        let views = present_all(&suggestions);
        assert_eq!(views.len(), 2);
    }

    #[test]
    fn priority_colors() {
        assert_eq!(priority_to_color(&Priority::Critical), "#EF4444");
        assert_eq!(priority_to_color(&Priority::High), "#F97316");
        assert_eq!(priority_to_color(&Priority::Medium), "#3B82F6");
        assert_eq!(priority_to_color(&Priority::Low), "#6B7280");
    }

    #[test]
    fn type_titles() {
        assert_eq!(type_to_title(&SuggestionType::EmailDraft), "이메일 초안");
        assert_eq!(type_to_title(&SuggestionType::ProductivityTip), "생산성 팁");
    }
}
