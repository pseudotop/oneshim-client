//! # oneshim-vision
//!
//! Edge 이미지 처리 크레이트.
//! 스크린 캡처, 델타 인코딩, 썸네일 생성, WebP 인코딩, OCR 등
//! 클라이언트 사이드 이미지 전처리 파이프라인을 담당한다.

pub mod capture;
pub mod delta;
pub mod element_finder;
pub mod encoder;
pub mod local_ocr_provider;
#[cfg(feature = "ocr")]
pub mod ocr;
pub mod privacy;
pub mod privacy_gateway;
pub mod processor;
pub mod thumbnail;
pub mod timeline;
pub mod trigger;
