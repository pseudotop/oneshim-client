use oneshim_core::models::suggestion::{FeedbackType, Suggestion};
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub suggestion: Suggestion,
    pub feedback: Option<FeedbackType>,
}

pub struct SuggestionHistory {
    entries: VecDeque<HistoryEntry>,
    max_size: usize,
}

impl SuggestionHistory {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_size,
        }
    }

    pub fn add(&mut self, suggestion: Suggestion) {
        if self.entries.len() >= self.max_size {
            self.entries.pop_front();
        }
        self.entries.push_back(HistoryEntry {
            suggestion,
            feedback: None,
        });
    }

    pub fn record_feedback(&mut self, suggestion_id: &str, feedback: FeedbackType) -> bool {
        for entry in self.entries.iter_mut().rev() {
            if entry.suggestion.suggestion_id == suggestion_id {
                entry.feedback = Some(feedback);
                return true;
            }
        }
        false
    }

    pub fn recent(&self, limit: usize) -> Vec<&HistoryEntry> {
        self.entries.iter().rev().take(limit).collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn stats(&self) -> HistoryStats {
        let mut accepted = 0u32;
        let mut rejected = 0u32;
        let mut deferred = 0u32;
        let mut pending = 0u32;

        for entry in &self.entries {
            match &entry.feedback {
                Some(FeedbackType::Accepted) => accepted += 1,
                Some(FeedbackType::Rejected) => rejected += 1,
                Some(FeedbackType::Deferred) => deferred += 1,
                None => pending += 1,
            }
        }

        HistoryStats {
            total: self.entries.len() as u32,
            accepted,
            rejected,
            deferred,
            pending,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HistoryStats {
    pub total: u32,
    pub accepted: u32,
    pub rejected: u32,
    pub deferred: u32,
    pub pending: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use oneshim_core::models::suggestion::{Priority, SuggestionType};

    fn make_suggestion(id: &str) -> Suggestion {
        Suggestion {
            suggestion_id: id.to_string(),
            suggestion_type: SuggestionType::WorkGuidance,
            content: format!("suggestion {id}"),
            priority: Priority::Medium,
            confidence_score: 0.9,
            relevance_score: 0.8,
            is_actionable: true,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    #[test]
    fn add_and_recent() {
        let mut history = SuggestionHistory::new(100);
        history.add(make_suggestion("1"));
        history.add(make_suggestion("2"));
        history.add(make_suggestion("3"));

        let recent = history.recent(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].suggestion.suggestion_id, "3");
        assert_eq!(recent[1].suggestion.suggestion_id, "2");
    }

    #[test]
    fn max_size_eviction() {
        let mut history = SuggestionHistory::new(2);
        history.add(make_suggestion("1"));
        history.add(make_suggestion("2"));
        history.add(make_suggestion("3"));

        assert_eq!(history.len(), 2);
        let recent = history.recent(10);
        assert_eq!(recent[0].suggestion.suggestion_id, "3");
        assert_eq!(recent[1].suggestion.suggestion_id, "2");
    }

    #[test]
    fn record_feedback() {
        let mut history = SuggestionHistory::new(100);
        history.add(make_suggestion("1"));
        history.add(make_suggestion("2"));

        assert!(history.record_feedback("1", FeedbackType::Accepted));
        assert!(history.record_feedback("2", FeedbackType::Rejected));
        assert!(!history.record_feedback("999", FeedbackType::Deferred));

        let stats = history.stats();
        assert_eq!(stats.accepted, 1);
        assert_eq!(stats.rejected, 1);
        assert_eq!(stats.pending, 0);
    }

    #[test]
    fn stats() {
        let mut history = SuggestionHistory::new(100);
        history.add(make_suggestion("1"));
        history.add(make_suggestion("2"));
        history.add(make_suggestion("3"));

        history.record_feedback("1", FeedbackType::Accepted);

        let stats = history.stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.accepted, 1);
        assert_eq!(stats.pending, 2);
    }
}
