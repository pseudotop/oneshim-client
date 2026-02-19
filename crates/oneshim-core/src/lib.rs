//! # oneshim-core
//!
//! ONESHIM 도메인 모델, 포트(trait) 정의, 에러 타입.
//! 모든 크레이트가 공유하는 핵심 타입과 인터페이스를 제공한다.
//!
//! ## 구조
//!
//! - [`models`] — 도메인 데이터 구조체 (serde Serialize/Deserialize)
//! - [`ports`] — Hexagonal Architecture 포트 인터페이스 (async_trait)
//! - [`error`] — 핵심 에러 타입 (thiserror)
//! - [`config`] — 애플리케이션 설정 구조체
//! - [`config_manager`] — 설정 파일 관리 (로드/저장)

pub mod config;
pub mod config_manager;
pub mod consent;
pub mod error;
pub mod models;
pub mod ports;

#[cfg(test)]
mod tests {
    use crate::models::suggestion::{Priority, Suggestion, SuggestionType};

    #[test]
    fn suggestion_serde_roundtrip() {
        let suggestion = Suggestion {
            suggestion_id: "sug_001".to_string(),
            suggestion_type: SuggestionType::WorkGuidance,
            content: "커밋하세요".to_string(),
            priority: Priority::High,
            confidence_score: 0.95,
            relevance_score: 0.88,
            is_actionable: true,
            created_at: chrono::Utc::now(),
            expires_at: None,
        };

        let json = serde_json::to_string(&suggestion).unwrap();
        let deserialized: Suggestion = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.suggestion_id, "sug_001");
        assert_eq!(deserialized.suggestion_type, SuggestionType::WorkGuidance);
        assert!(deserialized.confidence_score > 0.9);
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
    }

    #[test]
    fn config_defaults() {
        let config = crate::config::AppConfig::default_config();
        assert_eq!(config.monitor.poll_interval_ms, 1_000);
        assert_eq!(config.monitor.sync_interval_ms, 10_000);
        assert_eq!(config.storage.retention_days, 30);
        assert_eq!(config.storage.max_storage_mb, 500);
        assert_eq!(config.vision.thumbnail_width, 480);
        assert!(!config.vision.ocr_enabled);
    }
}
