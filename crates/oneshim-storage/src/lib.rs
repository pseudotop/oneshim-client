//! # oneshim-storage
//!
//! 로컬 저장소 어댑터.
//! SQLite 기반 이벤트 로그 저장, 스키마 마이그레이션,
//! 보존 정책(30일, 500MB)을 관리한다.
//!
//! ## 모듈
//! - `sqlite`: 이벤트 저장소 (StorageService 구현)
//! - `frame_storage`: 프레임 이미지 파일 저장소
//! - `migration`: 스키마 마이그레이션

pub mod frame_storage;
pub mod migration;
pub mod sqlite;
