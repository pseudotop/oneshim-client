//! 프레임 타임라인 인덱스.
//!
//! SQLite 연동 프레임 인덱스 + 검색.

use oneshim_core::models::frame::FrameMetadata;
use tracing::debug;

/// 타임라인 항목
#[derive(Debug, Clone)]
pub struct TimelineEntry {
    /// 프레임 ID
    pub id: u64,
    /// 메타데이터
    pub metadata: FrameMetadata,
    /// 이미지 존재 여부
    pub has_image: bool,
}

/// 타임라인 필터
#[derive(Debug, Clone, Default)]
pub struct TimelineFilter {
    /// 앱 이름 필터
    pub app_name: Option<String>,
    /// 최소 중요도
    pub min_importance: Option<f32>,
    /// 텍스트 검색 (창 제목)
    pub text_search: Option<String>,
    /// 최대 결과 수
    pub limit: usize,
}

impl TimelineFilter {
    /// 새 필터 생성
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            ..Default::default()
        }
    }

    /// 앱 이름 필터 설정
    pub fn with_app(mut self, app_name: &str) -> Self {
        self.app_name = Some(app_name.to_string());
        self
    }

    /// 최소 중요도 필터 설정
    pub fn with_min_importance(mut self, importance: f32) -> Self {
        self.min_importance = Some(importance);
        self
    }

    /// 텍스트 검색 설정
    pub fn with_text(mut self, text: &str) -> Self {
        self.text_search = Some(text.to_string());
        self
    }

    /// 메타데이터가 필터 조건에 맞는지 확인
    pub fn matches(&self, meta: &FrameMetadata) -> bool {
        if let Some(app) = &self.app_name {
            if !meta.app_name.contains(app) {
                return false;
            }
        }
        if let Some(min) = self.min_importance {
            if meta.importance < min {
                return false;
            }
        }
        if let Some(text) = &self.text_search {
            let lower = text.to_lowercase();
            if !meta.window_title.to_lowercase().contains(&lower)
                && !meta.app_name.to_lowercase().contains(&lower)
            {
                return false;
            }
        }
        true
    }
}

/// 인메모리 타임라인 (SQLite 연동 없는 경량 구현)
pub struct Timeline {
    entries: Vec<TimelineEntry>,
    max_entries: usize,
    next_id: u64,
}

impl Timeline {
    /// 새 타임라인 생성
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
            next_id: 1,
        }
    }

    /// 프레임 추가
    pub fn add_frame(&mut self, metadata: FrameMetadata, has_image: bool) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        self.entries.push(TimelineEntry {
            id,
            metadata,
            has_image,
        });

        // 최대 크기 초과 시 오래된 항목 제거
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }

        debug!("타임라인 프레임 추가: ID {id}");
        id
    }

    /// 필터 적용 조회
    pub fn query(&self, filter: &TimelineFilter) -> Vec<&TimelineEntry> {
        self.entries
            .iter()
            .rev()
            .filter(|e| filter.matches(&e.metadata))
            .take(filter.limit)
            .collect()
    }

    /// 전체 항목 수
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 비어있는지
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_metadata(app: &str, title: &str, importance: f32) -> FrameMetadata {
        FrameMetadata {
            timestamp: Utc::now(),
            trigger_type: "WindowChange".to_string(),
            app_name: app.to_string(),
            window_title: title.to_string(),
            resolution: (1920, 1080),
            importance,
        }
    }

    #[test]
    fn add_and_query() {
        let mut timeline = Timeline::new(100);
        timeline.add_frame(make_metadata("Code", "main.rs", 0.8), true);
        timeline.add_frame(make_metadata("Firefox", "Google", 0.3), false);

        let filter = TimelineFilter::new(10);
        assert_eq!(timeline.query(&filter).len(), 2);
    }

    #[test]
    fn filter_by_app() {
        let mut timeline = Timeline::new(100);
        timeline.add_frame(make_metadata("Code", "main.rs", 0.8), true);
        timeline.add_frame(make_metadata("Firefox", "Google", 0.3), false);

        let filter = TimelineFilter::new(10).with_app("Code");
        assert_eq!(timeline.query(&filter).len(), 1);
    }

    #[test]
    fn filter_by_importance() {
        let mut timeline = Timeline::new(100);
        timeline.add_frame(make_metadata("Code", "main.rs", 0.8), true);
        timeline.add_frame(make_metadata("Firefox", "Google", 0.3), false);

        let filter = TimelineFilter::new(10).with_min_importance(0.5);
        assert_eq!(timeline.query(&filter).len(), 1);
    }

    #[test]
    fn max_entries_eviction() {
        let mut timeline = Timeline::new(2);
        timeline.add_frame(make_metadata("A", "a", 0.5), false);
        timeline.add_frame(make_metadata("B", "b", 0.5), false);
        timeline.add_frame(make_metadata("C", "c", 0.5), false);

        assert_eq!(timeline.len(), 2);
    }
}
