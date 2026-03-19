//! OS accessibility API integration for focused element extraction.
//!
//! Platform-dispatched module:
//! - macOS: Native AXUIElement FFI (`macos.rs`)
//! - Windows: UIAutomation stub (`windows.rs`)
//! - Linux: Stub returning None (AT-SPI2 deferred)
//!
//! The `create_extractor()` factory function returns the appropriate
//! platform implementation behind `Arc<dyn AccessibilityExtractor>`.

#[cfg(target_os = "macos")]
pub(crate) mod ffi_macos;
