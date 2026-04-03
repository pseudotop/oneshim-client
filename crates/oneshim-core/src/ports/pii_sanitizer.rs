//! PII (Personally Identifiable Information) sanitization port.
//! Implemented by `VisionPiiSanitizer` in `oneshim-vision`.

use crate::config::PiiFilterLevel;

/// Sanitizes text by replacing PII patterns with redaction markers.
pub trait PiiSanitizer: Send + Sync {
    /// Replace PII in `text` according to the given filter level.
    /// Returns sanitized text with markers like `[EMAIL]`, `[PHONE]`, `[USER]`.
    fn sanitize_text(&self, text: &str, level: PiiFilterLevel) -> String;
}
