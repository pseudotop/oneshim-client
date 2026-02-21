//! # oneshim-ui
//!
//! 순수 Rust UI 크레이트.
//! iced 프레임워크 기반 메인 창, 시스템 트레이(tray-icon),
//! 데스크톱 알림(notify-rust)을 제공한다.
//! 제안 팝업, 컨텍스트 패널, 타임라인 뷰 등을 포함.

pub mod app;
pub mod autostart;
pub mod i18n;
pub mod notifier;
pub mod theme;
pub mod tray;
pub mod views;

// 플랫폼별 네이티브 API (창 숨기기/표시)
// Docker Desktop 스타일: X 버튼 → 트레이로 숨김, 트레이 → 다시 표시

#[cfg(target_os = "macos")]
pub mod native_macos;

#[cfg(target_os = "windows")]
pub mod native_windows;

// Linux: X11/Wayland 분열로 네이티브 API 대신 iced minimize 사용

// 메인 앱 재내보내기
pub use app::{Message, OneshimApp, Screen, UpdateUserAction};
pub use i18n::{Locale, Strings};
