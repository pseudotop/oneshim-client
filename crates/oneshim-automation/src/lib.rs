//! # oneshim-automation
//!
//! 자동화 제어 크레이트.
//! 서버 정책 기반 명령 실행, 감사 로깅, Policy-Based Sudo Access를 담당한다.
//! 모든 자동화 명령은 서버 정책 토큰 검증 후 실행되며, 감사 로그가 기록된다.

pub mod audit;
pub mod controller;
pub mod input_driver;
pub mod intent_resolver;
pub mod local_llm;
pub mod policy;
pub mod presets;
pub mod resolver;
pub mod sandbox;
