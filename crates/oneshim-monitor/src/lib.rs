// Cast safety: system metrics, CPU percentages, process counters — precision loss acceptable.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

//! # oneshim-monitor

pub mod error;
pub use error::MonitorError;

pub mod activity;
pub mod clipboard;
pub mod file_access;
pub mod idle;
pub mod input_activity;
pub mod input_detail;
pub mod key_hook;
pub mod keyboard_pattern;
pub mod process;
pub mod system;
pub mod system_info;
pub mod window_layout;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;
