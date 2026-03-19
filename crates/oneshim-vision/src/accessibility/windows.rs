//! Windows UIAutomation accessibility extractor -- structurally complete stub.
//!
//! The API calls are documented but not wired because full Windows testing
//! is not available on the current dev platform. The stub compiles on all
//! platforms (gated behind `#[cfg(target_os = "windows")]`) and returns None,
//! causing the scheduler to skip accessibility data for the tick.
//!
//! TODO: Implement via IUIAutomation COM API:
//!   1. CoCreateInstance(CLSID_CUIAutomation) -> IUIAutomation
//!   2. IUIAutomation::GetFocusedElement() -> IUIAutomationElement
//!   3. get_CurrentControlType() -> role mapping
//!   4. get_CurrentName() -> label
//!   5. get_CurrentBoundingRectangle() -> ElementRect
//!   6. ITextRangeProvider::GetText() -> value (with Zeroizing<String>)

#[cfg(target_os = "windows")]
mod inner {
    use async_trait::async_trait;
    use tracing::debug;

    use oneshim_core::config::PiiFilterLevel;
    use oneshim_core::error::CoreError;
    use oneshim_core::models::focused_element::FocusedElementInfo;
    use oneshim_core::ports::accessibility::AccessibilityExtractor;

    pub struct WindowsUiaAccessibility;

    impl Default for WindowsUiaAccessibility {
        fn default() -> Self {
            Self
        }
    }

    impl WindowsUiaAccessibility {
        pub fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl AccessibilityExtractor for WindowsUiaAccessibility {
        async fn extract_focused_element(
            &self,
            _pii_level: PiiFilterLevel,
            _has_full_text_consent: bool,
        ) -> Result<Option<FocusedElementInfo>, CoreError> {
            debug!("WindowsUiaAccessibility: stub -- returning None (Phase 2 TODO)");
            Ok(None)
        }

        fn has_permission(&self) -> bool {
            // Windows UIAutomation does not require special permissions
            true
        }

        fn name(&self) -> &str {
            "windows-uia-accessibility"
        }
    }
}

#[cfg(target_os = "windows")]
pub use inner::WindowsUiaAccessibility;

#[cfg(test)]
#[cfg(target_os = "windows")]
mod tests {
    use super::inner::*;
    use oneshim_core::config::PiiFilterLevel;
    use oneshim_core::ports::accessibility::AccessibilityExtractor;

    #[tokio::test]
    async fn stub_returns_none() {
        let extractor = WindowsUiaAccessibility::new();
        let result = extractor
            .extract_focused_element(PiiFilterLevel::Standard, false)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn has_permission_true() {
        let extractor = WindowsUiaAccessibility::new();
        assert!(extractor.has_permission());
    }

    #[test]
    fn name_is_correct() {
        let extractor = WindowsUiaAccessibility::new();
        assert_eq!(extractor.name(), "windows-uia-accessibility");
    }
}
