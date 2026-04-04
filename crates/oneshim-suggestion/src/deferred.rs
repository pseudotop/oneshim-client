use chrono::{DateTime, Duration, Utc};
use oneshim_core::models::suggestion::Suggestion;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct DeferredEntry {
    pub suggestion: Suggestion,
    pub deferred_at: DateTime<Utc>,
    pub resurface_at: DateTime<Utc>,
}

pub struct DeferredManager {
    items: VecDeque<DeferredEntry>,
    max_size: usize,
}

impl DeferredManager {
    pub fn new(max_size: usize) -> Self {
        Self {
            items: VecDeque::new(),
            max_size,
        }
    }

    pub fn defer(&mut self, suggestion: Suggestion, duration: Duration) -> bool {
        let now = Utc::now();
        if self.items.len() >= self.max_size {
            self.items.pop_front(); // FIFO eviction
        }
        self.items.push_back(DeferredEntry {
            suggestion,
            deferred_at: now,
            resurface_at: now + duration,
        });
        true
    }

    pub fn collect_resurfaced(&mut self) -> Vec<Suggestion> {
        let now = Utc::now();
        let mut resurfaced = Vec::new();
        self.items.retain(|entry| {
            if entry.resurface_at <= now {
                resurfaced.push(entry.suggestion.clone());
                false
            } else {
                true
            }
        });
        resurfaced
    }

    pub fn pending_count(&self) -> usize {
        self.items.len()
    }

    pub fn list_deferred(&self) -> Vec<&DeferredEntry> {
        self.items.iter().collect()
    }

    pub fn cancel(&mut self, suggestion_id: &str) -> Option<Suggestion> {
        let pos = self
            .items
            .iter()
            .position(|e| e.suggestion.suggestion_id == suggestion_id)?;
        self.items.remove(pos).map(|e| e.suggestion)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::models::suggestion::{Priority, SuggestionSource, SuggestionType};

    fn make_suggestion(id: &str) -> Suggestion {
        Suggestion {
            suggestion_id: id.to_string(),
            suggestion_type: SuggestionType::ProductivityTip,
            content: format!("tip {id}"),
            priority: Priority::Medium,
            confidence_score: 0.8,
            relevance_score: 0.7,
            source: SuggestionSource::LlmServer,
            is_actionable: true,
            reasoning: None,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    #[test]
    fn defer_and_collect_after_duration() {
        let mut mgr = DeferredManager::new(50);
        let s = make_suggestion("s1");
        assert!(mgr.defer(s, Duration::zero()));
        assert_eq!(mgr.pending_count(), 1);
        let resurfaced = mgr.collect_resurfaced();
        assert_eq!(resurfaced.len(), 1);
        assert_eq!(resurfaced[0].suggestion_id, "s1");
        assert_eq!(mgr.pending_count(), 0);
    }

    #[test]
    fn collect_skips_future_items() {
        let mut mgr = DeferredManager::new(50);
        let s = make_suggestion("s1");
        assert!(mgr.defer(s, Duration::hours(2)));
        let resurfaced = mgr.collect_resurfaced();
        assert!(resurfaced.is_empty());
        assert_eq!(mgr.pending_count(), 1);
    }

    #[test]
    fn max_size_eviction() {
        let mut mgr = DeferredManager::new(2);
        assert!(mgr.defer(make_suggestion("s1"), Duration::hours(1)));
        assert!(mgr.defer(make_suggestion("s2"), Duration::hours(1)));
        // Third item evicts oldest (s1)
        assert!(mgr.defer(make_suggestion("s3"), Duration::hours(1)));
        assert_eq!(mgr.pending_count(), 2);
        let ids: Vec<_> = mgr
            .list_deferred()
            .iter()
            .map(|e| e.suggestion.suggestion_id.as_str())
            .collect();
        assert!(!ids.contains(&"s1"));
    }

    #[test]
    fn cancel_removes_and_returns() {
        let mut mgr = DeferredManager::new(50);
        mgr.defer(make_suggestion("s1"), Duration::hours(1));
        mgr.defer(make_suggestion("s2"), Duration::hours(1));
        let cancelled = mgr.cancel("s1");
        assert!(cancelled.is_some());
        assert_eq!(cancelled.unwrap().suggestion_id, "s1");
        assert_eq!(mgr.pending_count(), 1);
    }

    #[test]
    fn cancel_nonexistent_returns_none() {
        let mut mgr = DeferredManager::new(50);
        assert!(mgr.cancel("nope").is_none());
    }
}
