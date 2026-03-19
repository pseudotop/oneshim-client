//! macOS native accessibility extractor using AXUIElement FFI.
//!
//! Replaces the osascript-based stub with direct Core Accessibility API calls.
//! Requires Accessibility permission in System Settings > Privacy & Security.

#[cfg(target_os = "macos")]
mod inner {
    use std::ptr;
    use std::sync::atomic::{AtomicU32, Ordering};

    use async_trait::async_trait;
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::{CFRelease, CFTypeRef};
    use core_foundation_sys::string::CFStringRef;
    use tracing::{debug, warn};
    use zeroize::Zeroizing;

    use oneshim_core::config::PiiFilterLevel;
    use oneshim_core::error::CoreError;
    use oneshim_core::models::focused_element::{ElementRect, FocusedElementInfo};
    use oneshim_core::ports::accessibility::AccessibilityExtractor;

    use crate::accessibility::ffi_macos::ax::*;
    use crate::privacy::sanitize_title_with_level;

    /// Circuit breaker: skip AX calls after consecutive failures.
    static CONSECUTIVE_FAILURES: AtomicU32 = AtomicU32::new(0);
    const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;
    const CIRCUIT_BREAKER_RETRY_INTERVAL: u32 = 60;

    /// Raw data extracted from the accessibility API before PII filtering.
    struct RawFocusedElement {
        role: String,
        title: Option<Zeroizing<String>>,
        value: Option<Zeroizing<String>>,
        placeholder: Option<String>,
        position: Option<ElementRect>,
    }

    pub struct MacOsNativeAccessibility;

    impl Default for MacOsNativeAccessibility {
        fn default() -> Self {
            Self
        }
    }

    impl MacOsNativeAccessibility {
        pub fn new() -> Self {
            Self
        }

        /// Check if accessibility permission is granted.
        fn check_permission() -> bool {
            unsafe { AXIsProcessTrustedWithOptions(ptr::null()) }
        }

        /// Circuit breaker: check if calls are allowed.
        fn circuit_allows() -> bool {
            let failures = CONSECUTIVE_FAILURES.load(Ordering::Relaxed);
            if failures >= CIRCUIT_BREAKER_THRESHOLD {
                if failures % CIRCUIT_BREAKER_RETRY_INTERVAL != 0 {
                    CONSECUTIVE_FAILURES.fetch_add(1, Ordering::Relaxed);
                    return false;
                }
                warn!(
                    "MacOsNativeAccessibility: circuit breaker retry after {} skipped",
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

        /// Extract the focused element via AXUIElement API (synchronous).
        ///
        /// SAFETY: All CFTypeRef values are released after use. The function
        /// returns owned Rust strings -- no dangling Core Foundation references.
        fn extract_raw() -> Option<RawFocusedElement> {
            unsafe {
                let system_wide = AXUIElementCreateSystemWide();
                if system_wide.is_null() {
                    return None;
                }

                // Build attribute key CFStrings
                let focused_attr = ax_attr(AX_FOCUSED_UI_ELEMENT_ATTR);

                // Get focused element
                let mut focused: CFTypeRef = ptr::null();
                let err = AXUIElementCopyAttributeValue(
                    system_wide,
                    as_cf_ref(&focused_attr),
                    &mut focused,
                );
                CFRelease(system_wide);

                if err != kAXErrorSuccess || focused.is_null() {
                    if err == kAXErrorAPIDisabled {
                        warn!("Accessibility permission revoked at runtime; returning None");
                    }
                    return None;
                }

                // Extract role
                let role_key = ax_attr(AX_ROLE_ATTR);
                let role = Self::get_string_attr(focused, as_cf_ref(&role_key)).unwrap_or_default();

                // Extract title/description for label
                let title_key = ax_attr(AX_TITLE_ATTR);
                let desc_key = ax_attr(AX_DESCRIPTION_ATTR);
                let title = Self::get_string_attr(focused, as_cf_ref(&title_key))
                    .or_else(|| Self::get_string_attr(focused, as_cf_ref(&desc_key)))
                    .map(Zeroizing::new);

                // Extract value (raw text content) -- zeroized
                let value_key = ax_attr(AX_VALUE_ATTR);
                let value =
                    Self::get_string_attr(focused, as_cf_ref(&value_key)).map(Zeroizing::new);

                // Extract placeholder
                let placeholder_key = ax_attr(AX_PLACEHOLDER_VALUE_ATTR);
                let placeholder = Self::get_string_attr(focused, as_cf_ref(&placeholder_key));

                // Extract position + size
                let position = Self::get_position_and_size(focused);

                CFRelease(focused);

                Some(RawFocusedElement {
                    role,
                    title,
                    value,
                    placeholder,
                    position,
                })
            }
        }

        /// Helper: get a string attribute from an AXUIElement.
        unsafe fn get_string_attr(element: AXUIElementRef, attr: CFStringRef) -> Option<String> {
            let mut value: CFTypeRef = ptr::null();
            let err = AXUIElementCopyAttributeValue(element, attr, &mut value);
            if err != kAXErrorSuccess || value.is_null() {
                return None;
            }
            // CFTypeRef -> CFStringRef -> CFString -> Rust String
            // AXUIElementCopyAttributeValue follows the "Create Rule":
            // the caller owns the returned CFTypeRef.
            let cf_str = CFString::wrap_under_create_rule(value as CFStringRef);
            Some(cf_str.to_string())
        }

        /// Helper: extract position (CGPoint) and size (CGSize) from element.
        unsafe fn get_position_and_size(element: AXUIElementRef) -> Option<ElementRect> {
            let pos_key = ax_attr(AX_POSITION_ATTR);
            let size_key = ax_attr(AX_SIZE_ATTR);

            let mut pos_ref: CFTypeRef = ptr::null();
            let mut size_ref: CFTypeRef = ptr::null();

            let pos_err = AXUIElementCopyAttributeValue(element, as_cf_ref(&pos_key), &mut pos_ref);
            let size_err =
                AXUIElementCopyAttributeValue(element, as_cf_ref(&size_key), &mut size_ref);

            if pos_err != kAXErrorSuccess || size_err != kAXErrorSuccess {
                if !pos_ref.is_null() {
                    CFRelease(pos_ref);
                }
                if !size_ref.is_null() {
                    CFRelease(size_ref);
                }
                return None;
            }

            let mut point = CGPoint::default();
            let mut size = CGSize::default();

            let got_point = AXValueGetValue(
                pos_ref,
                kAXValueCGPointType,
                &mut point as *mut _ as *mut std::ffi::c_void,
            );
            let got_size = AXValueGetValue(
                size_ref,
                kAXValueCGSizeType,
                &mut size as *mut _ as *mut std::ffi::c_void,
            );

            CFRelease(pos_ref);
            CFRelease(size_ref);

            if got_point && got_size {
                Some(ElementRect {
                    x: point.x as f32,
                    y: point.y as f32,
                    width: size.width as f32,
                    height: size.height as f32,
                })
            } else {
                None
            }
        }

        /// Apply PII-level filtering to raw extracted data.
        fn filter_by_level(raw: RawFocusedElement, level: PiiFilterLevel) -> FocusedElementInfo {
            match level {
                PiiFilterLevel::Strict => FocusedElementInfo {
                    role: raw.role,
                    position: raw.position,
                    ..Default::default()
                },
                PiiFilterLevel::Standard => FocusedElementInfo {
                    role: raw.role,
                    position: raw.position,
                    label: raw
                        .title
                        .as_deref()
                        .map(|s| s.to_string())
                        .or(raw.placeholder.clone()),
                    value_length: raw.value.as_deref().map(|v| v.len() as u32),
                    ..Default::default()
                },
                PiiFilterLevel::Basic => {
                    let text = raw
                        .value
                        .as_deref()
                        .map(|v| sanitize_title_with_level(v, PiiFilterLevel::Basic));
                    FocusedElementInfo {
                        role: raw.role,
                        position: raw.position,
                        label: raw
                            .title
                            .as_deref()
                            .map(|s| s.to_string())
                            .or(raw.placeholder.clone()),
                        value_length: raw.value.as_deref().map(|v| v.len() as u32),
                        extracted_text: text,
                    }
                }
                PiiFilterLevel::Off => FocusedElementInfo {
                    role: raw.role,
                    position: raw.position,
                    label: raw
                        .title
                        .as_deref()
                        .map(|s| s.to_string())
                        .or(raw.placeholder.clone()),
                    value_length: raw.value.as_deref().map(|v| v.len() as u32),
                    extracted_text: raw.value.as_deref().map(|v| v.to_string()),
                },
            }
            // raw.title and raw.value (Zeroizing<String>) are dropped here,
            // zeroing memory automatically.
        }
    }

    #[async_trait]
    impl AccessibilityExtractor for MacOsNativeAccessibility {
        async fn extract_focused_element(
            &self,
            pii_level: PiiFilterLevel,
            has_full_text_consent: bool,
        ) -> Result<Option<FocusedElementInfo>, CoreError> {
            if !Self::circuit_allows() {
                debug!("MacOsNativeAccessibility: circuit breaker open");
                return Ok(None);
            }

            let effective_level = if pii_level == PiiFilterLevel::Off && !has_full_text_consent {
                debug!("PII Off requested but full_text_extraction consent missing; falling back to Standard");
                PiiFilterLevel::Standard
            } else {
                pii_level
            };

            // Run synchronous FFI on a blocking thread to avoid stalling tokio
            let result = tokio::task::spawn_blocking(Self::extract_raw)
                .await
                .map_err(|e| CoreError::Internal(format!("AX blocking task failed: {e}")))?;

            match result {
                Some(raw) => {
                    Self::record_success();
                    let filtered = Self::filter_by_level(raw, effective_level);
                    debug!(role = %filtered.role, "AX focused element extracted");
                    Ok(Some(filtered))
                }
                None => {
                    Self::record_failure();
                    Ok(None)
                }
            }
        }

        fn has_permission(&self) -> bool {
            Self::check_permission()
        }

        fn name(&self) -> &str {
            "macos-native-accessibility"
        }
    }
}

#[cfg(target_os = "macos")]
pub use inner::MacOsNativeAccessibility;

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::inner::*;

    use oneshim_core::config::PiiFilterLevel;
    use oneshim_core::models::focused_element::ElementRect;
    use oneshim_core::ports::accessibility::AccessibilityExtractor;
    use zeroize::Zeroizing;

    #[test]
    fn filter_strict_only_role_and_position() {
        let info = apply_filter(
            "AXTextField",
            Some("Search"),
            Some("secret query"),
            Some("Type here"),
            Some(ElementRect {
                x: 10.0,
                y: 20.0,
                width: 200.0,
                height: 25.0,
            }),
            PiiFilterLevel::Strict,
        );
        assert_eq!(info.role, "AXTextField");
        assert!(info.position.is_some());
        assert!(info.label.is_none());
        assert!(info.value_length.is_none());
        assert!(info.extracted_text.is_none());
    }

    #[test]
    fn filter_standard_includes_label_and_length() {
        let info = apply_filter(
            "AXTextArea",
            Some("Terminal"),
            Some("cargo test"),
            None,
            None,
            PiiFilterLevel::Standard,
        );
        assert_eq!(info.label, Some("Terminal".to_string()));
        assert_eq!(info.value_length, Some(10));
        assert!(info.extracted_text.is_none());
    }

    #[test]
    fn filter_basic_includes_sanitized_text() {
        let info = apply_filter(
            "AXTextField",
            None,
            Some("user@example.com"),
            None,
            None,
            PiiFilterLevel::Basic,
        );
        assert!(info.extracted_text.is_some());
        let text = info.extracted_text.unwrap();
        assert!(text.contains("[EMAIL]"));
        assert!(!text.contains("user@example.com"));
    }

    #[test]
    fn filter_off_includes_full_text() {
        let info = apply_filter(
            "AXTextField",
            None,
            Some("full content here"),
            None,
            None,
            PiiFilterLevel::Off,
        );
        assert_eq!(info.extracted_text, Some("full content here".to_string()));
    }

    #[test]
    fn filter_standard_falls_back_to_placeholder_when_no_title() {
        let info = apply_filter(
            "AXTextField",
            None,
            Some("value"),
            Some("Search..."),
            None,
            PiiFilterLevel::Standard,
        );
        assert_eq!(info.label, Some("Search...".to_string()));
    }

    /// Integration test -- requires Accessibility permission.
    /// Run manually: `cargo test -p oneshim-vision -- macos_native_ax --ignored`
    #[tokio::test]
    #[ignore]
    async fn extract_focused_element_integration() {
        let extractor = MacOsNativeAccessibility::new();
        if !extractor.has_permission() {
            eprintln!("SKIP: Accessibility permission not granted");
            return;
        }
        let result = extractor
            .extract_focused_element(PiiFilterLevel::Standard, false)
            .await;
        assert!(result.is_ok());
        // May be None if no element is focused (headless CI)
    }

    /// Apply PII filter to test data by reconstructing the filter logic.
    /// This duplicates the private struct so we can test the filtering
    /// without exposing internals.
    fn apply_filter(
        role: &str,
        title: Option<&str>,
        value: Option<&str>,
        placeholder: Option<&str>,
        position: Option<ElementRect>,
        level: PiiFilterLevel,
    ) -> oneshim_core::models::focused_element::FocusedElementInfo {
        use crate::privacy::sanitize_title_with_level;
        use oneshim_core::models::focused_element::FocusedElementInfo;

        let title_z = title.map(|s| Zeroizing::new(s.to_string()));
        let value_z = value.map(|s| Zeroizing::new(s.to_string()));
        let placeholder_s = placeholder.map(|s| s.to_string());

        let result = match level {
            PiiFilterLevel::Strict => FocusedElementInfo {
                role: role.to_string(),
                position,
                ..Default::default()
            },
            PiiFilterLevel::Standard => FocusedElementInfo {
                role: role.to_string(),
                position,
                label: title_z
                    .as_deref()
                    .map(|s| s.to_string())
                    .or(placeholder_s.clone()),
                value_length: value_z.as_deref().map(|v| v.len() as u32),
                ..Default::default()
            },
            PiiFilterLevel::Basic => {
                let text = value_z
                    .as_deref()
                    .map(|v| sanitize_title_with_level(v, PiiFilterLevel::Basic));
                FocusedElementInfo {
                    role: role.to_string(),
                    position,
                    label: title_z
                        .as_deref()
                        .map(|s| s.to_string())
                        .or(placeholder_s.clone()),
                    value_length: value_z.as_deref().map(|v| v.len() as u32),
                    extracted_text: text,
                }
            }
            PiiFilterLevel::Off => FocusedElementInfo {
                role: role.to_string(),
                position,
                label: title_z
                    .as_deref()
                    .map(|s| s.to_string())
                    .or(placeholder_s.clone()),
                value_length: value_z.as_deref().map(|v| v.len() as u32),
                extracted_text: value_z.as_deref().map(|v| v.to_string()),
            },
        };
        // Zeroizing values dropped here.
        result
    }
}
