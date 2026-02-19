//! 포트 인터페이스 (trait).
//!
//! Hexagonal Architecture의 포트 레이어.
//! 각 어댑터 crate가 이 trait들을 구현하며,
//! `oneshim-app`에서 `Arc<dyn T>`로 와이어링한다.
//!
//! 모든 async trait은 `async_trait` 매크로를 사용하여
//! object safety를 보장한다 (ADR-001 §2 참조).

pub mod api_client;
pub mod compressor;
pub mod element_finder;
pub mod input_driver;
pub mod llm_provider;
pub mod monitor;
pub mod notifier;
pub mod ocr_provider;
pub mod sandbox;
pub mod storage;
pub mod vision;
