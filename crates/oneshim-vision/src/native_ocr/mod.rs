//! Platform-native OCR provider using OS-level text recognition APIs.
//!
//! On macOS, this uses Vision.framework's `VNRecognizeTextRequest` via raw
//! objc2 FFI. On other platforms, `create_native_ocr()` returns `None`.

use oneshim_core::ports::ocr_provider::OcrProvider;
use std::sync::Arc;

#[cfg(target_os = "macos")]
mod macos;

/// Create platform-native OCR provider.
///
/// Returns `Some(Arc<dyn OcrProvider>)` on macOS (Vision.framework),
/// `None` on all other platforms.
pub fn create_native_ocr() -> Option<Arc<dyn OcrProvider>> {
    #[cfg(target_os = "macos")]
    {
        Some(Arc::new(macos::MacOsNativeOcr))
    }

    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}
