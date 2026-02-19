//! 제안 우선순위 큐.
//!
//! BTreeSet 기반 우선순위 큐. Critical > High > Medium > Low 순으로 정렬.

use oneshim_core::models::suggestion::Suggestion;
use std::cmp::Ordering;
use std::collections::BTreeSet;

/// 우선순위 비교를 위한 래퍼
#[derive(Debug, Clone)]
struct PrioritizedSuggestion {
    suggestion: Suggestion,
}

impl PartialEq for PrioritizedSuggestion {
    fn eq(&self, other: &Self) -> bool {
        self.suggestion.suggestion_id == other.suggestion.suggestion_id
    }
}

impl Eq for PrioritizedSuggestion {}

impl PartialOrd for PrioritizedSuggestion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrioritizedSuggestion {
    fn cmp(&self, other: &Self) -> Ordering {
        // 높은 우선순위가 먼저 (역순)
        other
            .suggestion
            .priority
            .cmp(&self.suggestion.priority)
            .then_with(|| {
                // 같은 우선순위면 최신 것이 먼저
                other.suggestion.created_at.cmp(&self.suggestion.created_at)
            })
            .then_with(|| {
                self.suggestion
                    .suggestion_id
                    .cmp(&other.suggestion.suggestion_id)
            })
    }
}

/// 제안 우선순위 큐 (최대 용량 제한)
pub struct SuggestionQueue {
    items: BTreeSet<PrioritizedSuggestion>,
    max_size: usize,
}

impl SuggestionQueue {
    /// 새 큐 생성 (최대 크기 지정)
    pub fn new(max_size: usize) -> Self {
        Self {
            items: BTreeSet::new(),
            max_size,
        }
    }

    /// 제안 추가 (큐가 가득 찬 경우 가장 낮은 우선순위 항목 제거)
    pub fn push(&mut self, suggestion: Suggestion) -> bool {
        let item = PrioritizedSuggestion { suggestion };

        if self.items.len() >= self.max_size {
            // 새 항목이 현재 최하위보다 높은 우선순위인 경우에만 교체
            if let Some(last) = self.items.iter().next_back() {
                if item < *last {
                    // 새 항목이 더 높은 우선순위
                    let last_clone = last.clone();
                    self.items.remove(&last_clone);
                } else {
                    return false; // 큐가 가득 차고 낮은 우선순위
                }
            }
        }

        self.items.insert(item)
    }

    /// 가장 높은 우선순위 제안 꺼내기
    pub fn pop(&mut self) -> Option<Suggestion> {
        let first = self.items.iter().next()?.clone();
        self.items.remove(&first);
        Some(first.suggestion)
    }

    /// 가장 높은 우선순위 제안 조회 (제거 안함)
    pub fn peek(&self) -> Option<&Suggestion> {
        self.items.iter().next().map(|p| &p.suggestion)
    }

    /// 현재 큐 크기
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 큐가 비어있는지
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 모든 제안을 우선순위 순으로 반환
    pub fn iter(&self) -> impl Iterator<Item = &Suggestion> {
        self.items.iter().map(|p| &p.suggestion)
    }

    /// 큐 비우기
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// 만료된 제안 제거
    pub fn remove_expired(&mut self) -> usize {
        let now = chrono::Utc::now();
        let before = self.items.len();
        self.items.retain(|p| {
            p.suggestion
                .expires_at
                .map_or(true, |expires| expires > now)
        });
        before - self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use oneshim_core::models::suggestion::{Priority, SuggestionType};

    fn make_suggestion(id: &str, priority: Priority) -> Suggestion {
        Suggestion {
            suggestion_id: id.to_string(),
            suggestion_type: SuggestionType::WorkGuidance,
            content: format!("제안 {id}"),
            priority,
            confidence_score: 0.9,
            relevance_score: 0.8,
            is_actionable: true,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    #[test]
    fn priority_ordering() {
        let mut queue = SuggestionQueue::new(50);
        queue.push(make_suggestion("low", Priority::Low));
        queue.push(make_suggestion("critical", Priority::Critical));
        queue.push(make_suggestion("medium", Priority::Medium));
        queue.push(make_suggestion("high", Priority::High));

        assert_eq!(queue.pop().unwrap().suggestion_id, "critical");
        assert_eq!(queue.pop().unwrap().suggestion_id, "high");
        assert_eq!(queue.pop().unwrap().suggestion_id, "medium");
        assert_eq!(queue.pop().unwrap().suggestion_id, "low");
    }

    #[test]
    fn max_size_enforcement() {
        let mut queue = SuggestionQueue::new(2);
        queue.push(make_suggestion("1", Priority::Low));
        queue.push(make_suggestion("2", Priority::Medium));
        assert_eq!(queue.len(), 2);

        // 높은 우선순위가 Low를 교체
        queue.push(make_suggestion("3", Priority::High));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.peek().unwrap().suggestion_id, "3");
    }

    #[test]
    fn empty_queue() {
        let mut queue = SuggestionQueue::new(50);
        assert!(queue.is_empty());
        assert!(queue.pop().is_none());
        assert!(queue.peek().is_none());
    }

    #[test]
    fn clear_queue() {
        let mut queue = SuggestionQueue::new(50);
        queue.push(make_suggestion("1", Priority::High));
        queue.push(make_suggestion("2", Priority::Medium));
        assert_eq!(queue.len(), 2);
        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn remove_expired() {
        let mut queue = SuggestionQueue::new(50);

        let mut expired = make_suggestion("expired", Priority::High);
        expired.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
        queue.push(expired);

        let valid = make_suggestion("valid", Priority::Medium);
        queue.push(valid);

        let removed = queue.remove_expired();
        assert_eq!(removed, 1);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.peek().unwrap().suggestion_id, "valid");
    }
}
