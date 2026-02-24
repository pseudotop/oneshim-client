//! # oneshim-vision
//!

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
