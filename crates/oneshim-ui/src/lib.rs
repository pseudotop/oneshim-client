//! # oneshim-ui

pub mod app;
pub mod autostart;
pub mod i18n;
pub mod notifier;
pub mod theme;
pub mod tray;
pub mod views;

#[cfg(target_os = "macos")]
pub mod native_macos;

#[cfg(target_os = "windows")]
pub mod native_windows;

pub use app::{Message, OneshimApp, Screen, UpdateStatusSnapshot, UpdateUserAction};
pub use i18n::{Locale, Strings};
