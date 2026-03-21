//! DEPRECATED: Use `accessibility::WindowsUiaAccessibility` (Phase 2 UIAutomation)
//! instead. This empty stub is retained for the `ElementFinder` trait. Will be
//! removed when the `ChainedElementFinder` is updated.
//!
//! Windows UI Automation adapter — `ElementFinder` stub implementation.
//!
//! A full implementation would use `IUIAutomation::ElementFromPoint` via the
//! `windows` crate or the `uiautomation` crate.  This stub compiles on all
//! platforms (gated behind `#[cfg(target_os = "windows")]`) and returns empty
//! results, allowing the [`ChainedElementFinder`] to fall through to OCR.
//!
//! Phase 3 TODO: Wire real `windows::Win32::UI::Accessibility` FFI calls to
//! extract `ControlType`, `Name`, and `BoundingRectangle` from the automation
//! tree.

#[cfg(target_os = "windows")]
mod inner {
    use async_trait::async_trait;
    use tracing::debug;

    use oneshim_core::error::CoreError;
    use oneshim_core::models::intent::{ElementBounds, UiElement};
    use oneshim_core::ports::element_finder::ElementFinder;

    /// Windows UI Automation element finder (stub).
    ///
    /// Returns empty results until Phase 3 adds real UIAutomation FFI.
    /// The [`ChainedElementFinder`] will fall through to the OCR backend.
    #[deprecated(
        since = "0.4.0",
        note = "Use accessibility::WindowsUiaAccessibility (Phase 2 UIAutomation) instead"
    )]
    pub struct WindowsAccessibilityFinder;

    impl WindowsAccessibilityFinder {
        pub fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl ElementFinder for WindowsAccessibilityFinder {
        async fn find_element(
            &self,
            _text: Option<&str>,
            _role: Option<&str>,
            _region: Option<&ElementBounds>,
        ) -> Result<Vec<UiElement>, CoreError> {
            // TODO(Phase 3): Implement via IUIAutomation::ElementFromPoint
            //   1. CoCreateInstance(CLSID_CUIAutomation)
            //   2. automation.ElementFromPoint(POINT { x, y })
            //   3. Extract ControlType, Name, BoundingRectangle, IsEnabled
            //   4. Map ControlType to role string ("Button", "Edit", "MenuItem", etc.)
            debug!("WindowsAccessibilityFinder: stub — returning empty (Phase 3 TODO)");
            Ok(vec![])
        }

        fn name(&self) -> &str {
            "windows-accessibility"
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn stub_returns_empty() {
            let finder = WindowsAccessibilityFinder::new();
            let result = finder.find_element(None, None, None).await.unwrap();
            assert!(result.is_empty());
        }

        #[tokio::test]
        async fn stub_returns_empty_with_query() {
            let finder = WindowsAccessibilityFinder::new();
            let result = finder
                .find_element(Some("Save"), Some("Button"), None)
                .await
                .unwrap();
            assert!(result.is_empty());
        }

        #[tokio::test]
        async fn name_is_correct() {
            let finder = WindowsAccessibilityFinder::new();
            assert_eq!(finder.name(), "windows-accessibility");
        }
    }
}

#[cfg(target_os = "windows")]
pub use inner::WindowsAccessibilityFinder;
