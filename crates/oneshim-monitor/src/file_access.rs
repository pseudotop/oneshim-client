//! 파일 접근 모니터링.
//!
//! 화이트리스트 기반 폴더 모니터링. 변경 이벤트 감지 후 메타데이터만 수집.
//! notify crate와 통합하여 사용 (실제 watcher 인스턴스는 oneshim-app에서 생성).

use chrono::{DateTime, Utc};
use oneshim_core::config::FileAccessConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// 파일 이벤트 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileEventType {
    /// 파일 생성
    Created,
    /// 파일 수정
    Modified,
    /// 파일 삭제
    Deleted,
    /// 파일 이름 변경
    Renamed,
}

/// 파일 접근 이벤트 (메타데이터만, 내용 없음)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccessEvent {
    /// 이벤트 시각
    pub timestamp: DateTime<Utc>,
    /// 파일 상대 경로 (모니터 폴더 기준)
    pub relative_path: PathBuf,
    /// 이벤트 유형
    pub event_type: FileEventType,
    /// 파일 확장자
    pub extension: Option<String>,
}

/// 파일 접근 이벤트 필터 — 설정 기반 필터링 + 레이트 리밋
pub struct FileAccessFilter {
    /// 설정
    config: FileAccessConfig,
    /// 현재 분의 이벤트 수 (레이트 리밋용)
    events_this_minute: Arc<AtomicU32>,
}

impl FileAccessFilter {
    /// 새 필터 생성
    pub fn new(config: FileAccessConfig) -> Self {
        Self {
            config,
            events_this_minute: Arc::new(AtomicU32::new(0)),
        }
    }

    /// 이벤트가 수집 대상인지 확인
    pub fn should_collect(&self, path: &Path) -> bool {
        if !self.config.enabled {
            return false;
        }

        // 레이트 리밋 확인
        let count = self.events_this_minute.load(Ordering::Relaxed);
        if count >= self.config.max_events_per_minute {
            return false;
        }

        // 확장자 제외 확인
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_with_dot = format!(".{ext}");
            if self.config.excluded_extensions.contains(&ext_with_dot) {
                return false;
            }
        }

        // 화이트리스트 폴더 확인
        if !self.config.monitored_folders.is_empty() {
            let in_whitelist = self
                .config
                .monitored_folders
                .iter()
                .any(|folder| path.starts_with(folder));
            if !in_whitelist {
                return false;
            }
        }

        true
    }

    /// 이벤트 카운터 증가
    pub fn record_event(&self) {
        self.events_this_minute.fetch_add(1, Ordering::Relaxed);
    }

    /// 분당 카운터 리셋 (1분 간격으로 호출)
    pub fn reset_minute_counter(&self) {
        self.events_this_minute.store(0, Ordering::Relaxed);
    }

    /// 경로에서 상대 경로 추출 (모니터 폴더 기준)
    pub fn to_relative_path(&self, absolute_path: &Path) -> PathBuf {
        for folder in &self.config.monitored_folders {
            if let Ok(rel) = absolute_path.strip_prefix(folder) {
                return rel.to_path_buf();
            }
        }
        // 매칭 폴더 없으면 파일명만
        absolute_path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| absolute_path.to_path_buf())
    }

    /// 파일 접근 이벤트 생성
    pub fn create_event(
        &self,
        absolute_path: &Path,
        event_type: FileEventType,
    ) -> Option<FileAccessEvent> {
        if !self.should_collect(absolute_path) {
            return None;
        }

        self.record_event();

        Some(FileAccessEvent {
            timestamp: Utc::now(),
            relative_path: self.to_relative_path(absolute_path),
            event_type,
            extension: absolute_path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> FileAccessConfig {
        FileAccessConfig {
            enabled: true,
            monitored_folders: vec![PathBuf::from("/home/user/projects")],
            excluded_extensions: vec![".tmp".to_string(), ".log".to_string()],
            max_events_per_minute: 10,
        }
    }

    #[test]
    fn filter_disabled() {
        let mut config = test_config();
        config.enabled = false;
        let filter = FileAccessFilter::new(config);
        assert!(!filter.should_collect(&PathBuf::from("/home/user/projects/file.rs")));
    }

    #[test]
    fn filter_excluded_extension() {
        let filter = FileAccessFilter::new(test_config());
        assert!(!filter.should_collect(&PathBuf::from("/home/user/projects/debug.tmp")));
        assert!(!filter.should_collect(&PathBuf::from("/home/user/projects/app.log")));
    }

    #[test]
    fn filter_outside_whitelist() {
        let filter = FileAccessFilter::new(test_config());
        assert!(!filter.should_collect(&PathBuf::from("/home/user/downloads/file.rs")));
    }

    #[test]
    fn filter_allows_valid_path() {
        let filter = FileAccessFilter::new(test_config());
        assert!(filter.should_collect(&PathBuf::from("/home/user/projects/src/main.rs")));
    }

    #[test]
    fn rate_limit() {
        let mut config = test_config();
        config.max_events_per_minute = 2;
        let filter = FileAccessFilter::new(config);

        let path = PathBuf::from("/home/user/projects/file.rs");
        assert!(filter.should_collect(&path));
        filter.record_event();
        assert!(filter.should_collect(&path));
        filter.record_event();
        // 리밋 초과
        assert!(!filter.should_collect(&path));

        // 리셋 후 다시 허용
        filter.reset_minute_counter();
        assert!(filter.should_collect(&path));
    }

    #[test]
    fn relative_path_extraction() {
        let filter = FileAccessFilter::new(test_config());
        let rel = filter.to_relative_path(&PathBuf::from("/home/user/projects/src/main.rs"));
        assert_eq!(rel, PathBuf::from("src/main.rs"));
    }

    #[test]
    fn create_event_success() {
        let filter = FileAccessFilter::new(test_config());
        let event = filter.create_event(
            &PathBuf::from("/home/user/projects/src/lib.rs"),
            FileEventType::Modified,
        );
        assert!(event.is_some());
        let evt = event.unwrap();
        assert_eq!(evt.event_type, FileEventType::Modified);
        assert_eq!(evt.extension, Some("rs".to_string()));
    }
}
