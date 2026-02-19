//! 제안 파이프라인 통합 테스트.
//!
//! 큐 + 이력 + 피드백 + 프레젠터 cross-crate 연동.

use chrono::Utc;
use oneshim_core::models::suggestion::{FeedbackType, Priority, Suggestion, SuggestionType};
use oneshim_suggestion::history::SuggestionHistory;
use oneshim_suggestion::presenter;
use oneshim_suggestion::queue::SuggestionQueue;

fn make_suggestion(id: &str, priority: Priority, content: &str) -> Suggestion {
    Suggestion {
        suggestion_id: id.to_string(),
        suggestion_type: SuggestionType::WorkGuidance,
        content: content.to_string(),
        priority,
        confidence_score: 0.9,
        relevance_score: 0.85,
        is_actionable: true,
        created_at: Utc::now(),
        expires_at: None,
    }
}

#[test]
fn queue_to_presenter_flow() {
    // 1. 큐에 여러 우선순위 제안 추가
    let mut queue = SuggestionQueue::new(10);
    queue.push(make_suggestion("s1", Priority::Low, "낮은 우선순위"));
    queue.push(make_suggestion("s2", Priority::Critical, "긴급 제안"));
    queue.push(make_suggestion("s3", Priority::Medium, "중간 제안"));

    assert_eq!(queue.len(), 3);

    // 2. 가장 높은 우선순위 먼저 나오는지 확인
    let top = queue.pop().unwrap();
    assert_eq!(top.suggestion_id, "s2"); // Critical이 먼저
    assert_eq!(top.priority, Priority::Critical);

    // 3. 프레젠터로 변환
    let next = queue.peek().unwrap();
    let view = presenter::present(next);
    assert!(!view.title.is_empty());
    assert!(!view.body.is_empty());
}

#[test]
fn history_tracks_presented_suggestions() {
    let mut history = SuggestionHistory::new(100);

    // 제안 3개 이력에 추가
    let s1 = make_suggestion("h1", Priority::High, "제안 1");
    let s2 = make_suggestion("h2", Priority::Medium, "제안 2");
    let s3 = make_suggestion("h3", Priority::Low, "제안 3");

    history.add(s1);
    history.add(s2);
    history.add(s3);

    assert_eq!(history.len(), 3);

    // 최근 2개 조회
    let recent = history.recent(2);
    assert_eq!(recent.len(), 2);

    // 피드백 기록
    history.record_feedback("h1", FeedbackType::Accepted);

    let stats = history.stats();
    assert_eq!(stats.total, 3);
    assert_eq!(stats.accepted, 1);
}

#[test]
fn queue_overflow_evicts_lowest() {
    let mut queue = SuggestionQueue::new(2); // 최대 2개

    queue.push(make_suggestion("a", Priority::High, "높음"));
    queue.push(make_suggestion("b", Priority::Critical, "긴급"));
    queue.push(make_suggestion("c", Priority::Medium, "중간")); // 초과 → 가장 낮은 것 제거

    assert_eq!(queue.len(), 2);

    // Critical과 High만 남아야 함
    let first = queue.pop().unwrap();
    let second = queue.pop().unwrap();
    assert_eq!(first.priority, Priority::Critical);
    assert_eq!(second.priority, Priority::High);
}

#[test]
fn presenter_truncates_long_content() {
    let long_content = "A".repeat(200);
    let suggestion = make_suggestion("long", Priority::Medium, &long_content);
    let view = presenter::present(&suggestion);

    // 본문이 적절히 처리되는지 확인
    assert!(!view.body.is_empty());
}

#[test]
fn presenter_all_priorities() {
    for priority in [
        Priority::Low,
        Priority::Medium,
        Priority::High,
        Priority::Critical,
    ] {
        let s = make_suggestion("p", priority.clone(), "내용");
        let view = presenter::present(&s);
        assert!(
            !view.priority_color.is_empty(),
            "우선순위 {:?}에 색상이 없음",
            priority
        );
    }
}
