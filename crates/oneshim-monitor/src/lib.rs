//! # oneshim-monitor

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
