//!

use oneshim_core::models::suggestion::Suggestion;
use std::cmp::Ordering;
use std::collections::BTreeSet;

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
        other
            .suggestion
            .priority
            .cmp(&self.suggestion.priority)
            .then_with(|| other.suggestion.created_at.cmp(&self.suggestion.created_at))
            .then_with(|| {
                self.suggestion
                    .suggestion_id
                    .cmp(&other.suggestion.suggestion_id)
            })
    }
}

pub struct SuggestionQueue {
    items: BTreeSet<PrioritizedSuggestion>,
    max_size: usize,
}

impl SuggestionQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            items: BTreeSet::new(),
            max_size,
        }
    }

    pub fn push(&mut self, suggestion: Suggestion) -> bool {
        let item = PrioritizedSuggestion { suggestion };

        if self.items.len() >= self.max_size {
            if let Some(last) = self.items.iter().next_back() {
                if item < *last {
                    let last_clone = last.clone();
                    self.items.remove(&last_clone);
                } else {
                    return false; // queue full and lower priority
                }
            }
        }

        self.items.insert(item)
    }

    pub fn pop(&mut self) -> Option<Suggestion> {
        let first = self.items.iter().next()?.clone();
        self.items.remove(&first);
        Some(first.suggestion)
    }

    pub fn peek(&self) -> Option<&Suggestion> {
        self.items.iter().next().map(|p| &p.suggestion)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Suggestion> {
        self.items.iter().map(|p| &p.suggestion)
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

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
            content: format!("suggestion {id}"),
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
