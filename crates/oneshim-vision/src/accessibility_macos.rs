//! DEPRECATED: Use `accessibility::MacOsNativeAccessibility` (Phase 2 native AX FFI)
//! instead. This osascript-based stub is retained for the `ElementFinder` trait
//! (automation click-target lookup) but should not be used for focused-element
//! extraction. Will be removed when the `ChainedElementFinder` is updated to
//! use the new native implementation.
//!
//! macOS Accessibility API adapter — `ElementFinder` implementation.
//!
//! Uses `osascript` to query AXUIElement attributes for the element at a given
//! screen position.  A full Core Foundation FFI approach
//! (`AXUIElementCopyElementAtPosition`) is a Phase 3 TODO; the current stub
//! relies on the same osascript pattern used in `oneshim-monitor/src/macos.rs`
//! and degrades gracefully when Accessibility permission is not granted.

#[cfg(target_os = "macos")]
mod inner {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use tokio::process::Command;
    use tokio::time::timeout;
    use tracing::{debug, warn};

    use oneshim_core::error::CoreError;
    use oneshim_core::models::intent::{ElementBounds, FinderSource, UiElement};
    use oneshim_core::ports::element_finder::ElementFinder;

    /// Consecutive failure counter — circuit breaker mirrors
    /// `oneshim-monitor/src/macos.rs`.
    static CONSECUTIVE_FAILURES: AtomicU32 = AtomicU32::new(0);
    const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;
    const CIRCUIT_BREAKER_RETRY_INTERVAL: u32 = 60;
    const SUBPROCESS_TIMEOUT_SECS: u64 = 3;

    /// macOS accessibility element finder using `osascript` (AppleScript).
    ///
    /// Queries `System Events` for the focused UI element and returns it as a
    /// [`UiElement`].  When the app lacks Accessibility permission the finder
    /// returns an empty result set (rather than an error) so that the
    /// [`ChainedElementFinder`] can fall through to the OCR backend.
    #[deprecated(
        since = "0.4.0",
        note = "Use accessibility::MacOsNativeAccessibility (Phase 2 native AX FFI) instead"
    )]
    pub struct MacOsAccessibilityFinder;

    #[allow(deprecated)]
    impl Default for MacOsAccessibilityFinder {
        fn default() -> Self {
            Self
        }
    }

    #[allow(deprecated)]
    impl MacOsAccessibilityFinder {
        pub fn new() -> Self {
            Self
        }

        /// Build the AppleScript that introspects the frontmost UI element.
        ///
        /// Returns `role|title|x|y|w|h|enabled|focused` pipe-separated fields.
        fn build_script(text_query: Option<&str>, role_query: Option<&str>) -> String {
            // When a text or role query is supplied we search children of the
            // front window; otherwise we report the focused element.
            let filter = match (text_query, role_query) {
                (Some(txt), Some(role)) => format!(
                    r#"set matches to {{}}
                    repeat with elem in (every UI element of frontWin)
                        try
                            if (description of elem contains "{txt}") and (role of elem is "{role}") then
                                set end of matches to elem
                            end if
                        end try
                    end repeat"#
                ),
                (Some(txt), None) => format!(
                    r#"set matches to {{}}
                    repeat with elem in (every UI element of frontWin)
                        try
                            if (description of elem contains "{txt}") or (name of elem contains "{txt}") then
                                set end of matches to elem
                            end if
                        end try
                    end repeat"#
                ),
                (None, Some(role)) => format!(
                    r#"set matches to {{}}
                    repeat with elem in (every UI element of frontWin)
                        try
                            if role of elem is "{role}" then
                                set end of matches to elem
                            end if
                        end try
                    end repeat"#
                ),
                (None, None) => {
                    // Return the focused UI element of the front window.
                    return r#"tell application "System Events"
    set frontApp to first application process whose frontmost is true
    set frontWin to front window of frontApp
    set focusElem to focused UI element of frontWin
    set elemRole to role of focusElem
    set elemTitle to ""
    try
        set elemTitle to description of focusElem
    end try
    set elemPos to position of focusElem
    set elemSize to size of focusElem
    return elemRole & "|" & elemTitle & "|" & (item 1 of elemPos as integer) & "|" & (item 2 of elemPos as integer) & "|" & (item 1 of elemSize as integer) & "|" & (item 2 of elemSize as integer) & "|true|true"
end tell"#
                        .to_string();
                }
            };

            format!(
                r#"tell application "System Events"
    set frontApp to first application process whose frontmost is true
    set frontWin to front window of frontApp
    {filter}
    set output to ""
    repeat with m in matches
        set elemRole to role of m
        set elemTitle to ""
        try
            set elemTitle to description of m
        end try
        try
            set elemTitle to name of m
        end try
        set elemPos to position of m
        set elemSize to size of m
        set output to output & elemRole & "|" & elemTitle & "|" & (item 1 of elemPos as integer) & "|" & (item 2 of elemPos as integer) & "|" & (item 1 of elemSize as integer) & "|" & (item 2 of elemSize as integer) & "|true|true" & linefeed
    end repeat
    return output
end tell"#
            )
        }

        /// Parse a single `role|title|x|y|w|h|enabled|focused` line.
        fn parse_line(line: &str) -> Option<UiElement> {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 6 {
                return None;
            }

            let role = parts[0].trim().to_string();
            let title = parts[1].trim().to_string();
            let x: i32 = parts[2].trim().parse().ok()?;
            let y: i32 = parts[3].trim().parse().ok()?;
            let w: u32 = parts[4].trim().parse().ok()?;
            let h: u32 = parts[5].trim().parse().ok()?;

            Some(UiElement {
                text: title,
                bounds: ElementBounds {
                    x,
                    y,
                    width: w,
                    height: h,
                },
                role: Some(role),
                confidence: 0.95, // native API — high confidence
                source: FinderSource::Accessibility,
            })
        }

        /// Check whether the circuit breaker allows a call.
        fn circuit_breaker_allows() -> bool {
            let failures = CONSECUTIVE_FAILURES.load(Ordering::Relaxed);
            if failures >= CIRCUIT_BREAKER_THRESHOLD {
                if failures % CIRCUIT_BREAKER_RETRY_INTERVAL != 0 {
                    CONSECUTIVE_FAILURES.fetch_add(1, Ordering::Relaxed);
                    return false;
                }
                warn!(
                    "MacOsAccessibilityFinder: circuit breaker retry after {} skipped calls",
                    failures - CIRCUIT_BREAKER_THRESHOLD
                );
            }
            true
        }

        fn record_success() {
            CONSECUTIVE_FAILURES.store(0, Ordering::Relaxed);
        }

        fn record_failure() {
            CONSECUTIVE_FAILURES.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[allow(deprecated)]
    #[async_trait]
    impl ElementFinder for MacOsAccessibilityFinder {
        async fn find_element(
            &self,
            text: Option<&str>,
            role: Option<&str>,
            _region: Option<&ElementBounds>,
        ) -> Result<Vec<UiElement>, CoreError> {
            if !Self::circuit_breaker_allows() {
                debug!("MacOsAccessibilityFinder: circuit breaker open, skipping");
                return Ok(vec![]);
            }

            let script = Self::build_script(text, role);

            let output = match timeout(
                Duration::from_secs(SUBPROCESS_TIMEOUT_SECS),
                Command::new("osascript").arg("-e").arg(&script).output(),
            )
            .await
            {
                Ok(Ok(out)) => out,
                Ok(Err(e)) => {
                    Self::record_failure();
                    debug!(error = %e, "osascript spawn failed");
                    return Ok(vec![]);
                }
                Err(_) => {
                    Self::record_failure();
                    debug!("osascript timed out — Accessibility permission may be missing");
                    return Ok(vec![]);
                }
            };

            if !output.status.success() {
                Self::record_failure();
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!(stderr = %stderr, "osascript exited with error");
                return Ok(vec![]);
            }

            Self::record_success();

            let stdout = String::from_utf8_lossy(&output.stdout);
            let elements: Vec<UiElement> = stdout
                .lines()
                .filter(|l| !l.trim().is_empty())
                .filter_map(Self::parse_line)
                .collect();

            debug!(count = elements.len(), "MacOsAccessibilityFinder results");
            Ok(elements)
        }

        fn name(&self) -> &str {
            "macos-accessibility"
        }
    }

    #[cfg(test)]
    #[allow(deprecated)]
    mod tests {
        use super::*;

        #[test]
        fn parse_line_valid() {
            let line = "AXButton|Save|100|200|80|30|true|true";
            let elem = MacOsAccessibilityFinder::parse_line(line).unwrap();
            assert_eq!(elem.text, "Save");
            assert_eq!(elem.role, Some("AXButton".to_string()));
            assert_eq!(elem.bounds.x, 100);
            assert_eq!(elem.bounds.y, 200);
            assert_eq!(elem.bounds.width, 80);
            assert_eq!(elem.bounds.height, 30);
            assert_eq!(elem.source, FinderSource::Accessibility);
            assert!(elem.confidence > 0.9);
        }

        #[test]
        fn parse_line_minimal_fields() {
            let line = "AXTextField|Search|10|20|200|25";
            let elem = MacOsAccessibilityFinder::parse_line(line).unwrap();
            assert_eq!(elem.text, "Search");
            assert_eq!(elem.bounds.width, 200);
        }

        #[test]
        fn parse_line_insufficient_fields() {
            let line = "AXButton|Save|100";
            assert!(MacOsAccessibilityFinder::parse_line(line).is_none());
        }

        #[test]
        fn parse_line_non_numeric() {
            let line = "AXButton|Save|abc|200|80|30";
            assert!(MacOsAccessibilityFinder::parse_line(line).is_none());
        }

        #[test]
        fn parse_line_empty_title() {
            let line = "AXGroup||0|0|100|50|true|false";
            let elem = MacOsAccessibilityFinder::parse_line(line).unwrap();
            assert_eq!(elem.text, "");
            assert_eq!(elem.role, Some("AXGroup".to_string()));
        }

        #[test]
        fn build_script_no_query_returns_focused_element() {
            let script = MacOsAccessibilityFinder::build_script(None, None);
            assert!(script.contains("focused UI element"));
        }

        #[test]
        fn build_script_text_query_searches_children() {
            let script = MacOsAccessibilityFinder::build_script(Some("Save"), None);
            assert!(script.contains("Save"));
            assert!(script.contains("repeat with elem"));
        }

        #[test]
        fn build_script_role_query_filters_by_role() {
            let script = MacOsAccessibilityFinder::build_script(None, Some("AXButton"));
            assert!(script.contains("AXButton"));
        }

        #[test]
        fn build_script_both_queries() {
            let script = MacOsAccessibilityFinder::build_script(Some("OK"), Some("AXButton"));
            assert!(script.contains("OK"));
            assert!(script.contains("AXButton"));
        }

        /// Integration test — requires Accessibility permission.
        /// Run manually: `cargo test -p oneshim-vision -- accessibility_macos --ignored`
        #[tokio::test]
        #[ignore]
        async fn find_focused_element_integration() {
            let finder = MacOsAccessibilityFinder::new();
            let result = finder.find_element(None, None, None).await;
            // Should succeed (possibly empty if no window focused)
            assert!(result.is_ok());
        }
    }
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
pub use inner::MacOsAccessibilityFinder;
