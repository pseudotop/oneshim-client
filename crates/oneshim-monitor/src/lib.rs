//! # oneshim-monitor
//!
//! 시스템 모니터링 어댑터.
//! CPU/메모리/디스크 사용량, 활성 창/프로세스 정보, 유휴 감지를 수집한다.
//! 플랫폼별(macOS, Windows, Linux) 네이티브 API를 통해 구현.

pub mod activity;
pub mod clipboard;
pub mod file_access;
pub mod idle;
pub mod input_activity;
pub mod input_detail;
pub mod process;
pub mod system;
pub mod window_layout;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;
