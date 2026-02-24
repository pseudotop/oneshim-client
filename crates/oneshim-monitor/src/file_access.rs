//!

use chrono::{DateTime, Utc};
use oneshim_core::config::FileAccessConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileEventType {
    Created,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccessEvent {
    pub timestamp: DateTime<Utc>,
    pub relative_path: PathBuf,
    pub event_type: FileEventType,
    pub extension: Option<String>,
}

pub struct FileAccessFilter {
    config: FileAccessConfig,
    events_this_minute: Arc<AtomicU32>,
}

impl FileAccessFilter {
    pub fn new(config: FileAccessConfig) -> Self {
        Self {
            config,
            events_this_minute: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn should_collect(&self, path: &Path) -> bool {
        if !self.config.enabled {
            return false;
        }

        let count = self.events_this_minute.load(Ordering::Relaxed);
        if count >= self.config.max_events_per_minute {
            return false;
        }

        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_with_dot = format!(".{ext}");
            if self.config.excluded_extensions.contains(&ext_with_dot) {
                return false;
            }
        }

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

    pub fn record_event(&self) {
        self.events_this_minute.fetch_add(1, Ordering::Relaxed);
    }

    pub fn reset_minute_counter(&self) {
        self.events_this_minute.store(0, Ordering::Relaxed);
    }

    pub fn to_relative_path(&self, absolute_path: &Path) -> PathBuf {
        for folder in &self.config.monitored_folders {
            if let Ok(rel) = absolute_path.strip_prefix(folder) {
                return rel.to_path_buf();
            }
        }
        absolute_path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| absolute_path.to_path_buf())
    }

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
        assert!(!filter.should_collect(&path));

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
