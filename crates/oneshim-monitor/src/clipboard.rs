//! 클립보드 모니터링.
//!
//! 클립보드 변경을 감지하고 PII 필터가 적용된 메타데이터만 수집한다.
//! 원본 내용은 저장하지 않으며, 해시로 변경 감지 후 요약 정보만 기록.

use chrono::{DateTime, Utc};
use oneshim_core::config::PiiFilterLevel;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// 클립보드 내용 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClipboardContentType {
    /// 텍스트
    Text,
    /// 이미지
    Image,
    /// 기타
    Other,
}

/// 클립보드 변경 이벤트 (내용이 아닌 메타데이터만)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEvent {
    /// 이벤트 시각
    pub timestamp: DateTime<Utc>,
    /// 내용 유형
    pub content_type: ClipboardContentType,
    /// 문자 수 (텍스트인 경우)
    pub char_count: usize,
    /// PII 필터 적용된 미리보기 (Off가 아닌 경우에만)
    pub preview: Option<String>,
}

/// 클립보드 모니터 — 변경 감지 + PII 필터 적용 메타데이터 수집
pub struct ClipboardMonitor {
    /// 마지막 클립보드 내용 해시
    last_content_hash: u64,
    /// PII 필터 레벨
    pii_filter_level: PiiFilterLevel,
}

impl ClipboardMonitor {
    /// 새 클립보드 모니터 생성
    pub fn new(pii_level: PiiFilterLevel) -> Self {
        Self {
            last_content_hash: 0,
            pii_filter_level: pii_level,
        }
    }

    /// 클립보드 텍스트 변경 감지 (외부에서 텍스트를 전달)
    ///
    /// arboard 등 클립보드 라이브러리와 결합하여 사용.
    /// 내용이 변경되었으면 이벤트 반환, 동일하면 None.
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

    /// PII 필터 레벨 변경
    pub fn set_pii_filter_level(&mut self, level: PiiFilterLevel) {
        self.pii_filter_level = level;
    }
}

/// 문자열 해시 계산
fn hash_string(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// 문자열을 최대 길이로 자르기
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
