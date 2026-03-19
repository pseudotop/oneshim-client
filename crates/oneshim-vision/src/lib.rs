//! # oneshim-vision

#[cfg(target_os = "macos")]
pub mod accessibility_macos;
#[cfg(target_os = "windows")]
pub mod accessibility_windows;
pub mod capture;
pub mod delta;
pub mod element_finder;
pub mod encoder;
pub mod gui_detector;
/// Backward-compatible re-export for Phase 1 callers.
pub use gui_detector as input_correlator;
pub mod local_ocr_provider;
#[cfg(feature = "ocr")]
pub mod ocr;
pub mod privacy;
pub mod privacy_gateway;
pub mod processor;
pub mod ring_buffer;
pub mod thumbnail;
pub mod timeline;
pub mod trigger;
