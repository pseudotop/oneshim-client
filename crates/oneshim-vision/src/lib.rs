// Cast safety: pixel coordinates, image dimensions, confidence scores — precision loss acceptable.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

//! # oneshim-vision

pub mod accessibility;
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
pub mod work_classifier;

#[cfg(feature = "native-vision")]
pub mod native_ocr;

#[cfg(feature = "native-vision")]
pub mod native_detect;
