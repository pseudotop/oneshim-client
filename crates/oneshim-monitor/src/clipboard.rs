use chrono::{DateTime, Utc};
use oneshim_core::config::PiiFilterLevel;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClipboardContentType {
    Text,
    Image,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEvent {
    pub timestamp: DateTime<Utc>,
    pub content_type: ClipboardContentType,
    pub char_count: usize,
    pub preview: Option<String>,
}

pub struct ClipboardMonitor {
    last_content_hash: u64,
    pii_filter_level: PiiFilterLevel,
}

impl ClipboardMonitor {
    pub fn new(pii_level: PiiFilterLevel) -> Self {
        Self {
            last_content_hash: 0,
            pii_filter_level: pii_level,
        }
    }

    pub fn check_text_change(&mut self, text: &str) -> Option<ClipboardEvent> {
        let hash = hash_string(text);
        if hash == self.last_content_hash {
            return None;
        }
        self.last_content_hash = hash;

        let preview = if self.pii_filter_level != PiiFilterLevel::Off {
            Some(truncate(text, 50))
        } else {
            None
        };

        Some(ClipboardEvent {
            timestamp: Utc::now(),
            content_type: ClipboardContentType::Text,
            char_count: text.len(),
            preview,
        })
    }

    pub fn set_pii_filter_level(&mut self, level: PiiFilterLevel) {
        self.pii_filter_level = level;
    }
}

fn hash_string(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut result: String = s.chars().take(max_len).collect();
        result.push_str("...");
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_text_change() {
        let mut monitor = ClipboardMonitor::new(PiiFilterLevel::Standard);
        let event = monitor.check_text_change("hello world");
        assert!(event.is_some());
        let evt = event.unwrap();
        assert_eq!(evt.content_type, ClipboardContentType::Text);
        assert_eq!(evt.char_count, 11);
    }

    #[test]
    fn no_change_on_same_text() {
        let mut monitor = ClipboardMonitor::new(PiiFilterLevel::Standard);
        monitor.check_text_change("hello");
        let event = monitor.check_text_change("hello");
        assert!(event.is_none());
    }

    #[test]
    fn preview_included_with_filter() {
        let mut monitor = ClipboardMonitor::new(PiiFilterLevel::Standard);
        let event = monitor.check_text_change("short").unwrap();
        assert!(event.preview.is_some());
    }

    #[test]
    fn no_preview_when_off() {
        let mut monitor = ClipboardMonitor::new(PiiFilterLevel::Off);
        let event = monitor.check_text_change("something").unwrap();
        assert!(event.preview.is_none());
    }
}
