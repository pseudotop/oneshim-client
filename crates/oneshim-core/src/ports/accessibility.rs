//! Port trait for OS accessibility API integration.
//!
//! Separate from `ElementFinder` which is used for click-target lookup in
//! the automation pipeline. `AccessibilityExtractor` passively extracts the
//! currently focused element on each scheduler tick for context enrichment.

use async_trait::async_trait;

use crate::config::PiiFilterLevel;
use crate::error::CoreError;
use crate::models::focused_element::FocusedElementInfo;

/// Extract focused UI element information from the OS accessibility API.
///
/// Implementations MUST:
/// - Return `Ok(None)` when no element is focused or permission is denied
/// - Never panic on OS permission revocation at runtime
/// - Apply PII-level gating according to the provided level
/// - Use `Zeroizing<String>` for raw text before PII filtering (in adapter)
#[async_trait]
pub trait AccessibilityExtractor: Send + Sync {
    /// Extract the currently focused UI element, filtered by PII level.
    ///
    /// `has_full_text_consent` gates the `Off` PII level. When `pii_level` is
    /// `Off` but consent is missing, implementations MUST silently fall back
    /// to `Standard`.
    async fn extract_focused_element(
        &self,
        pii_level: PiiFilterLevel,
        has_full_text_consent: bool,
    ) -> Result<Option<FocusedElementInfo>, CoreError>;

    /// Check if OS-level accessibility permission is currently granted.
    fn has_permission(&self) -> bool;

    /// Human-readable name for logging/diagnostics.
    fn name(&self) -> &str;
}
