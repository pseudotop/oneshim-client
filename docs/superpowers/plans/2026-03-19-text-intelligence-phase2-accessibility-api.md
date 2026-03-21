# Text-Heavy App Intelligence Phase 2: Accessibility API Structure Extraction

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the osascript-based `MacOsAccessibilityFinder` stub with native AXUIElement FFI on macOS and a structurally complete UIAutomation stub on Windows, introduce the `FocusedElementInfo` domain model and `AccessibilityExtractor` port trait, enforce PII-level gating on extracted element data, apply `zeroize` memory protection to raw accessibility text, and wire the focused-element signal into the existing GUI pipeline as a supplementary context source alongside OCR regions.

**Architecture:** New port trait `AccessibilityExtractor` in `oneshim-core` (separate from the existing `ElementFinder` trait -- `ElementFinder` is for click-target lookup; `AccessibilityExtractor` is for passive focused-element extraction on each scheduler tick). macOS implementation uses `AXUIElementCreateSystemWide()` + `AXUIElementCopyAttributeValue()` via `core-foundation` + raw FFI bindings (no new crate dependency -- link to `ApplicationServices` framework). Windows implementation uses `IUIAutomation::GetFocusedElement()` via `windows-sys`. The `ChainedElementFinder` pattern is not involved -- `AccessibilityExtractor` is a distinct pipeline feeding `FocusedElementInfo` into the analysis tick, not the automation element-finding chain. Config gated by `text_intelligence.accessibility_extraction = true` + `activity_pattern_learning` consent. Linux returns `None` (AT-SPI2 deferred).

**Tech Stack:** Rust, `core-foundation` (already in workspace), `core-foundation-sys` (already in workspace), `core-graphics` (already in workspace), `windows-sys` (already in workspace, extend features), `zeroize` (new workspace dep), `serde`, `tracing`

**Spec:** `docs/superpowers/specs/2026-03-19-text-heavy-app-intelligence-design.md` (Sections 6, 9.1, 9.2, 12 Phase 2)

**Depends on:** Phase 1 (completed -- `TextIntelligenceConfig` exists with `accessibility_extraction` field), Phase 1.5 (completed -- platform key hooks exist, proving the `#[cfg(target_os)]` pattern)

---

## File Map

### New files

| File | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/models/focused_element.rs` | `FocusedElementInfo`, `ElementRect` structs |
| `crates/oneshim-core/src/ports/accessibility.rs` | `AccessibilityExtractor` port trait |
| `crates/oneshim-vision/src/accessibility/mod.rs` | Platform dispatcher + `PiiFilteredExtractor` wrapper |
| `crates/oneshim-vision/src/accessibility/macos.rs` | `MacOsNativeAccessibility` -- AXUIElement FFI (`#[cfg(target_os = "macos")]`) |
| `crates/oneshim-vision/src/accessibility/windows.rs` | `WindowsUiaAccessibility` -- UIAutomation stub (`#[cfg(target_os = "windows")]`) |
| `crates/oneshim-vision/src/accessibility/ffi_macos.rs` | Raw FFI bindings for AX functions not exposed by `core-foundation` |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-core/src/models/mod.rs` | Add `pub mod focused_element;` |
| `crates/oneshim-core/src/ports/mod.rs` | Add `pub mod accessibility;` |
| `crates/oneshim-vision/src/lib.rs` | Add `pub mod accessibility;`, keep existing `accessibility_macos` + `accessibility_windows` as deprecated |
| `crates/oneshim-vision/Cargo.toml` | Add `zeroize` dependency |
| `Cargo.toml` (workspace root) | Add `zeroize = "1"` to `[workspace.dependencies]` |
| `crates/oneshim-core/src/consent.rs` | Add `full_text_extraction: bool` to `ConsentPermissions` (Tier 6) |
| `src-tauri/src/scheduler/loops.rs` | Construct + call `AccessibilityExtractor` in monitor loop, pass result to analysis pipeline |
| `src-tauri/src/scheduler/mod.rs` | Add `accessibility_extractor: Option<Arc<dyn AccessibilityExtractor>>` to `Scheduler` |
| `src-tauri/src/main.rs` | Wire `AccessibilityExtractor` in DI setup |

---

## Task 1: Add FocusedElementInfo and ElementRect models (oneshim-core)

**Files:**
- New: `crates/oneshim-core/src/models/focused_element.rs`
- Modify: `crates/oneshim-core/src/models/mod.rs`

- [ ] **Step 1: Create `focused_element.rs` with domain models**

Create `crates/oneshim-core/src/models/focused_element.rs`:

```rust
//! Focused UI element information from OS accessibility APIs.
//!
//! Domain model consumed by the analysis pipeline to provide element-level
//! context for text-heavy applications. PII filtering is applied before
//! these structs are persisted or transmitted.

use serde::{Deserialize, Serialize};

/// Screen rectangle for an accessibility element.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ElementRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Information about the currently focused UI element, extracted via
/// OS accessibility API. All text fields are PII-filtered before storage.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FocusedElementInfo {
    /// Accessibility role (e.g., "AXTextField", "AXTextArea", "AXButton",
    /// "AXStaticText", "edit", "document").
    pub role: String,

    /// Position and size of the element on screen.
    pub position: Option<ElementRect>,

    /// Accessibility label (e.g., "Search", "Terminal", "Message input").
    /// Filtered by PII level. None at Strict level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// Length of the element's text value in characters (not the content itself).
    /// Useful for distinguishing empty fields from filled ones.
    /// Available at Standard and Basic levels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_length: Option<u32>,

    /// Extracted text content from the element.
    /// Only available at Basic level (with email/phone masking) or Off level
    /// (full text, requires additional consent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracted_text: Option<String>,
}
```

- [ ] **Step 2: Add module declaration to `models/mod.rs`**

Add `pub mod focused_element;` to `crates/oneshim-core/src/models/mod.rs` alongside existing module declarations.

- [ ] **Step 3: Unit tests**

Append to `focused_element.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_element_info_default() {
        let info = FocusedElementInfo::default();
        assert_eq!(info.role, "");
        assert!(info.position.is_none());
        assert!(info.label.is_none());
        assert!(info.value_length.is_none());
        assert!(info.extracted_text.is_none());
    }

    #[test]
    fn serde_roundtrip_full() {
        let info = FocusedElementInfo {
            role: "AXTextField".to_string(),
            position: Some(ElementRect { x: 100.0, y: 200.0, width: 300.0, height: 25.0 }),
            label: Some("Search".to_string()),
            value_length: Some(42),
            extracted_text: Some("cargo test --workspace".to_string()),
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: FocusedElementInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, decoded);
    }

    #[test]
    fn serde_roundtrip_minimal() {
        let info = FocusedElementInfo {
            role: "AXButton".to_string(),
            position: Some(ElementRect { x: 10.0, y: 20.0, width: 80.0, height: 30.0 }),
            ..Default::default()
        };
        let json = serde_json::to_string(&info).unwrap();
        // None fields are skipped
        assert!(!json.contains("label"));
        assert!(!json.contains("value_length"));
        assert!(!json.contains("extracted_text"));
        let decoded: FocusedElementInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, decoded);
    }

    #[test]
    fn backward_compat_missing_fields() {
        // Old JSON without focused_element fields deserializes to defaults
        let json = r#"{"role":"AXGroup"}"#;
        let info: FocusedElementInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.role, "AXGroup");
        assert!(info.position.is_none());
    }
}
```

**Verify:** `cargo test -p oneshim-core -- focused_element`

---

## Task 2: Add AccessibilityExtractor port trait (oneshim-core)

**Files:**
- New: `crates/oneshim-core/src/ports/accessibility.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`

- [ ] **Step 1: Create the port trait**

Create `crates/oneshim-core/src/ports/accessibility.rs`:

```rust
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
```

- [ ] **Step 2: Add module declaration to `ports/mod.rs`**

Add `pub mod accessibility;` to `crates/oneshim-core/src/ports/mod.rs`.

**Verify:** `cargo check -p oneshim-core`

---

## Task 3: Add full_text_extraction consent field (oneshim-core)

**Files:**
- Modify: `crates/oneshim-core/src/consent.rs`

- [ ] **Step 1: Add Tier 6 consent field**

In `ConsentPermissions`, add after the `cross_device_sync` field:

```rust
    // --- Tier 6: Text Intelligence ---
    /// Permits extraction of full text content from focused UI elements.
    /// Required only when pii_extraction_level is set to Off.
    /// GDPR Article 6 -- explicit consent for processing text content
    /// that may contain personal data.
    #[serde(default)]
    pub full_text_extraction: bool,
```

- [ ] **Step 2: Backward compatibility test**

Append to existing consent tests:

```rust
#[test]
fn consent_without_full_text_extraction_deserializes() {
    let json = r#"{"screen_capture":true,"activity_pattern_learning":true}"#;
    let perms: ConsentPermissions = serde_json::from_str(json).unwrap();
    assert!(!perms.full_text_extraction);
    assert!(perms.activity_pattern_learning);
}
```

**Verify:** `cargo test -p oneshim-core -- consent`

---

## Task 4: Add zeroize to workspace dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/oneshim-vision/Cargo.toml`

- [ ] **Step 1: Add zeroize to workspace**

In `Cargo.toml` workspace `[workspace.dependencies]` section, add:

```toml
zeroize = { version = "1", features = ["derive"] }
```

Place it near the security/crypto dependencies (`sha2`, `hmac`, `ed25519-dalek`).

- [ ] **Step 2: Add zeroize to oneshim-vision**

In `crates/oneshim-vision/Cargo.toml` `[dependencies]` section, add:

```toml
zeroize = { workspace = true }
```

**Verify:** `cargo check -p oneshim-vision`

---

## Task 5: Create macOS FFI bindings for AXUIElement (oneshim-vision)

**Files:**
- New: `crates/oneshim-vision/src/accessibility/ffi_macos.rs`

- [ ] **Step 1: Define FFI bindings for AX functions**

Create `crates/oneshim-vision/src/accessibility/ffi_macos.rs`:

```rust
//! Raw FFI bindings for macOS Accessibility API functions.
//!
//! These functions are not exposed by the `core-foundation` crate.
//! We link to the ApplicationServices framework which provides them.
//!
//! Reference: Apple Developer Documentation — Accessibility Reference

#![allow(non_snake_case, non_upper_case_globals)]

#[cfg(target_os = "macos")]
pub(crate) mod ax {
    use core_foundation::base::{CFTypeRef, TCFType};
    use core_foundation::string::CFStringRef;
    use std::ffi::c_void;

    /// Opaque type for an accessibility element.
    pub type AXUIElementRef = CFTypeRef;

    /// AXError codes.
    pub type AXError = i32;
    pub const kAXErrorSuccess: AXError = 0;
    pub const kAXErrorAPIDisabled: AXError = -25211;
    pub const kAXErrorNoValue: AXError = -25212;
    pub const kAXErrorAttributeUnsupported: AXError = -25205;

    // Attribute key constants -- declared as CFStringRef, resolved at link time.
    extern "C" {
        pub static kAXFocusedUIElementAttribute: CFStringRef;
        pub static kAXRoleAttribute: CFStringRef;
        pub static kAXTitleAttribute: CFStringRef;
        pub static kAXValueAttribute: CFStringRef;
        pub static kAXDescriptionAttribute: CFStringRef;
        pub static kAXPositionAttribute: CFStringRef;
        pub static kAXSizeAttribute: CFStringRef;
        pub static kAXPlaceholderValueAttribute: CFStringRef;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        /// Create a system-wide accessibility element (the root of the
        /// accessibility tree across all applications).
        pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;

        /// Copy the value of an attribute from an accessibility element.
        pub fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;

        /// Check whether the calling process has been granted accessibility
        /// permission. `options` can be NULL or a dictionary containing
        /// kAXTrustedCheckOptionPrompt.
        pub fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    }

    /// Extract a `CGPoint` from an AXValue.
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXValueGetValue(
            value: CFTypeRef,
            value_type: u32,
            value_ptr: *mut c_void,
        ) -> bool;
    }

    // AXValueType constants
    pub const kAXValueCGPointType: u32 = 1;
    pub const kAXValueCGSizeType: u32 = 2;

    /// CGPoint for position extraction.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }

    /// CGSize for size extraction.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub struct CGSize {
        pub width: f64,
        pub height: f64,
    }
}
```

- [ ] **Step 2: Verify FFI compiles on macOS**

**Verify:** `cargo check -p oneshim-vision` (on macOS)

---

## Task 6: Implement macOS native AccessibilityExtractor (oneshim-vision)

**Files:**
- New: `crates/oneshim-vision/src/accessibility/macos.rs`

- [ ] **Step 1: Implement `MacOsNativeAccessibility`**

Create `crates/oneshim-vision/src/accessibility/macos.rs`:

```rust
//! macOS native accessibility extractor using AXUIElement FFI.
//!
//! Replaces the osascript-based stub with direct Core Accessibility API calls.
//! Requires Accessibility permission in System Settings > Privacy & Security.

#[cfg(target_os = "macos")]
mod inner {
    use std::sync::atomic::{AtomicU32, Ordering};

    use async_trait::async_trait;
    use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
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

    impl MacOsNativeAccessibility {
        pub fn new() -> Self {
            Self
        }

        /// Check if accessibility permission is granted.
        fn check_permission() -> bool {
            unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) }
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

                // Get focused element
                let mut focused: CFTypeRef = std::ptr::null();
                let err = AXUIElementCopyAttributeValue(
                    system_wide,
                    kAXFocusedUIElementAttribute,
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
                let role = Self::get_string_attr(focused, kAXRoleAttribute)
                    .unwrap_or_default();

                // Extract title/description for label
                let title = Self::get_string_attr(focused, kAXTitleAttribute)
                    .or_else(|| Self::get_string_attr(focused, kAXDescriptionAttribute))
                    .map(Zeroizing::new);

                // Extract value (raw text content) -- zeroized
                let value = Self::get_string_attr(focused, kAXValueAttribute)
                    .map(Zeroizing::new);

                // Extract placeholder
                let placeholder = Self::get_string_attr(focused, kAXPlaceholderValueAttribute);

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
            let mut value: CFTypeRef = std::ptr::null();
            let err = AXUIElementCopyAttributeValue(element, attr, &mut value);
            if err != kAXErrorSuccess || value.is_null() {
                return None;
            }
            // CFTypeRef -> CFString -> Rust String
            let cf_str = CFString::wrap_under_create_rule(value as *const _);
            Some(cf_str.to_string())
        }

        /// Helper: extract position (CGPoint) and size (CGSize) from element.
        unsafe fn get_position_and_size(element: AXUIElementRef) -> Option<ElementRect> {
            let mut pos_ref: CFTypeRef = std::ptr::null();
            let mut size_ref: CFTypeRef = std::ptr::null();

            let pos_err = AXUIElementCopyAttributeValue(
                element, kAXPositionAttribute, &mut pos_ref,
            );
            let size_err = AXUIElementCopyAttributeValue(
                element, kAXSizeAttribute, &mut size_ref,
            );

            if pos_err != kAXErrorSuccess || size_err != kAXErrorSuccess {
                if !pos_ref.is_null() { CFRelease(pos_ref); }
                if !size_ref.is_null() { CFRelease(size_ref); }
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
                    label: raw.title.as_deref().map(|s| s.to_string())
                        .or(raw.placeholder.clone()),
                    value_length: raw.value.as_deref().map(|v| v.len() as u32),
                    ..Default::default()
                },
                PiiFilterLevel::Basic => {
                    let text = raw.value.as_deref()
                        .map(|v| sanitize_title_with_level(v, PiiFilterLevel::Basic));
                    FocusedElementInfo {
                        role: raw.role,
                        position: raw.position,
                        label: raw.title.as_deref().map(|s| s.to_string())
                            .or(raw.placeholder.clone()),
                        value_length: raw.value.as_deref().map(|v| v.len() as u32),
                        extracted_text: text,
                    }
                }
                PiiFilterLevel::Off => FocusedElementInfo {
                    role: raw.role,
                    position: raw.position,
                    label: raw.title.as_deref().map(|s| s.to_string())
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
```

- [ ] **Step 2: Unit tests (in same file, bottom)**

```rust
#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::inner::*;

    #[test]
    fn filter_strict_only_role_and_position() {
        let raw = RawFocusedElement {
            role: "AXTextField".to_string(),
            title: Some(zeroize::Zeroizing::new("Search".to_string())),
            value: Some(zeroize::Zeroizing::new("secret query".to_string())),
            placeholder: Some("Type here".to_string()),
            position: Some(oneshim_core::models::focused_element::ElementRect {
                x: 10.0, y: 20.0, width: 200.0, height: 25.0,
            }),
        };
        let info = MacOsNativeAccessibility::filter_by_level(
            raw,
            oneshim_core::config::PiiFilterLevel::Strict,
        );
        assert_eq!(info.role, "AXTextField");
        assert!(info.position.is_some());
        assert!(info.label.is_none());
        assert!(info.value_length.is_none());
        assert!(info.extracted_text.is_none());
    }

    #[test]
    fn filter_standard_includes_label_and_length() {
        let raw = RawFocusedElement {
            role: "AXTextArea".to_string(),
            title: Some(zeroize::Zeroizing::new("Terminal".to_string())),
            value: Some(zeroize::Zeroizing::new("cargo test".to_string())),
            placeholder: None,
            position: None,
        };
        let info = MacOsNativeAccessibility::filter_by_level(
            raw,
            oneshim_core::config::PiiFilterLevel::Standard,
        );
        assert_eq!(info.label, Some("Terminal".to_string()));
        assert_eq!(info.value_length, Some(10));
        assert!(info.extracted_text.is_none());
    }

    #[test]
    fn filter_basic_includes_sanitized_text() {
        let raw = RawFocusedElement {
            role: "AXTextField".to_string(),
            title: None,
            value: Some(zeroize::Zeroizing::new("user@example.com".to_string())),
            placeholder: None,
            position: None,
        };
        let info = MacOsNativeAccessibility::filter_by_level(
            raw,
            oneshim_core::config::PiiFilterLevel::Basic,
        );
        assert!(info.extracted_text.is_some());
        let text = info.extracted_text.unwrap();
        assert!(text.contains("[EMAIL]"));
        assert!(!text.contains("user@example.com"));
    }

    #[test]
    fn filter_off_includes_full_text() {
        let raw = RawFocusedElement {
            role: "AXTextField".to_string(),
            title: None,
            value: Some(zeroize::Zeroizing::new("full content here".to_string())),
            placeholder: None,
            position: None,
        };
        let info = MacOsNativeAccessibility::filter_by_level(
            raw,
            oneshim_core::config::PiiFilterLevel::Off,
        );
        assert_eq!(info.extracted_text, Some("full content here".to_string()));
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
            .extract_focused_element(
                oneshim_core::config::PiiFilterLevel::Standard,
                false,
            )
            .await;
        assert!(result.is_ok());
        // May be None if no element is focused (headless CI)
    }
}
```

**Verify:** `cargo test -p oneshim-vision -- filter_strict` (unit tests), `cargo check -p oneshim-vision` (compilation)

---

## Task 7: Implement Windows UIAutomation stub (oneshim-vision)

**Files:**
- New: `crates/oneshim-vision/src/accessibility/windows.rs`

- [ ] **Step 1: Create structurally complete stub**

Create `crates/oneshim-vision/src/accessibility/windows.rs`:

```rust
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
}
```

**Verify:** `cargo check -p oneshim-vision`

---

## Task 8: Create accessibility module dispatcher (oneshim-vision)

**Files:**
- New: `crates/oneshim-vision/src/accessibility/mod.rs`
- Modify: `crates/oneshim-vision/src/lib.rs`

- [ ] **Step 1: Create `accessibility/mod.rs`**

Create `crates/oneshim-vision/src/accessibility/mod.rs`:

```rust
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

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

use std::sync::Arc;
use oneshim_core::ports::accessibility::AccessibilityExtractor;

#[cfg(target_os = "macos")]
pub use macos::MacOsNativeAccessibility;

#[cfg(target_os = "windows")]
pub use windows::WindowsUiaAccessibility;

/// Create the platform-appropriate accessibility extractor.
///
/// Returns `None` on Linux (AT-SPI2 deferred) or if the platform module
/// is unavailable.
pub fn create_extractor() -> Option<Arc<dyn AccessibilityExtractor>> {
    #[cfg(target_os = "macos")]
    {
        let extractor = MacOsNativeAccessibility::new();
        if extractor.has_permission() {
            Some(Arc::new(extractor))
        } else {
            tracing::warn!(
                "macOS Accessibility permission not granted; \
                 accessibility extraction disabled. Grant permission in \
                 System Settings > Privacy & Security > Accessibility."
            );
            // Still return the extractor -- it will gracefully return None
            // on each call and log when the circuit breaker triggers.
            Some(Arc::new(extractor))
        }
    }

    #[cfg(target_os = "windows")]
    {
        Some(Arc::new(WindowsUiaAccessibility::new()))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        tracing::debug!("Accessibility extraction not available on this platform");
        None
    }
}
```

- [ ] **Step 2: Update `lib.rs` to expose the new module**

In `crates/oneshim-vision/src/lib.rs`, add:

```rust
pub mod accessibility;
```

Keep the existing `accessibility_macos` and `accessibility_windows` modules with a deprecation comment -- they implement `ElementFinder` for the automation pipeline and will be separately migrated later.

- [ ] **Step 3: Verify**

**Verify:** `cargo check -p oneshim-vision`

---

## Task 9: Wire AccessibilityExtractor into Scheduler DI (src-tauri)

**Files:**
- Modify: `src-tauri/src/scheduler/mod.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add field to Scheduler struct**

In `src-tauri/src/scheduler/mod.rs`, add to the `Scheduler` struct:

```rust
    /// Accessibility API extractor for focused element context (Phase 2).
    /// `None` when `text_intelligence.accessibility_extraction` is disabled
    /// or platform does not support it.
    pub(crate) accessibility_extractor: Option<Arc<dyn oneshim_core::ports::accessibility::AccessibilityExtractor>>,
```

- [ ] **Step 2: Wire in DI (main.rs)**

In `src-tauri/src/main.rs`, during Scheduler construction:

```rust
// Phase 2: Accessibility extractor (gated by config + consent)
let accessibility_extractor = {
    let text_config = config_manager.get().analysis.text_intelligence.clone();
    let consent = consent_manager.current_permissions();

    if text_config.enabled
        && text_config.accessibility_extraction
        && consent.activity_pattern_learning
    {
        oneshim_vision::accessibility::create_extractor()
    } else {
        None
    }
};
```

Pass `accessibility_extractor` to the `Scheduler` constructor.

**Verify:** `cargo check -p src-tauri` (or `cargo check --workspace`)

---

## Task 10: Wire AccessibilityExtractor into monitor loop (src-tauri)

**Files:**
- Modify: `src-tauri/src/scheduler/loops.rs`

- [ ] **Step 1: Add `accessibility_extractor` parameter to `spawn_monitor_loop()`**

Clone the `Arc` from `self.accessibility_extractor` alongside other clones at the top of `spawn_monitor_loop()`:

```rust
let accessibility_extractor = self.accessibility_extractor.clone();
```

- [ ] **Step 2: Add extraction call in the monitor loop body**

Inside the monitor loop, after `input_collector.set_current_app(&app_name)` and before the adaptive trigger state / analysis pipeline section, add:

```rust
// ── Accessibility API extraction (Phase 2) ──
let focused_element: Option<oneshim_core::models::focused_element::FocusedElementInfo> = {
    if let Some(ref ax) = accessibility_extractor {
        let text_config = /* read from config_manager or cached */ ;
        let consent = /* read from consent_manager or cached */ ;
        match ax.extract_focused_element(
            text_config.pii_extraction_level,
            consent.full_text_extraction,
        ).await {
            Ok(info) => info,
            Err(e) => {
                debug!("accessibility extraction failed: {e}");
                None
            }
        }
    } else {
        None
    }
};
```

- [ ] **Step 3: Pass `focused_element` into the analysis pipeline**

Extend `run_analysis_tick()` call to accept `Option<&FocusedElementInfo>`. The analysis pipeline can use the role/label to supplement `WorkTypeClassifier::classify_extended()`:

- If `focused_element.role` contains "TextArea" or "TextField" and the app is an IDE, this confirms the user is in a text editing pane (not a file tree or terminal).
- If `focused_element.role` is "AXGroup" or "AXScrollArea", the user may be browsing, not editing.

This is a supplementary signal -- the exact classification rules are deferred to the analysis module and do not need to be finalized in this plan.

- [ ] **Step 4: Verify full integration**

**Verify:** `cargo check --workspace`, `cargo test --workspace`

---

## Task 11: Extend windows-sys features for UIAutomation (future proofing)

**Files:**
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add UIAutomation-related features to windows-sys**

Extend the `windows-sys` feature list in workspace dependencies:

```toml
windows-sys = { version = "0.61", features = [
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_Accessibility",
    "Win32_Foundation",
    "Win32_System_Threading",
    "Win32_System_Registry",
    "Win32_System_SystemInformation",
    "Win32_System_Com",
    "Win32_System_LibraryLoader",
] }
```

Added features:
- `Win32_UI_Accessibility` -- UIAutomation interfaces
- `Win32_System_Com` -- `CoCreateInstance` for UIAutomation factory
- `Win32_System_LibraryLoader` -- `SetDllDirectory` for DLL search order hardening (Spec Section 9.2)

**Verify:** `cargo check --workspace`

---

## Task 12: Security hardening (Windows DLL protection)

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add DLL search order restriction at startup**

At the very beginning of `main()`, before any other initialization:

```rust
// Windows DLL search order hardening (Spec Section 9.2):
// Remove CWD from DLL search path to prevent DLL hijacking.
#[cfg(target_os = "windows")]
unsafe {
    windows_sys::Win32::System::LibraryLoader::SetDllDirectoryW(
        windows_sys::core::w!("")
    );
}
```

- [ ] **Step 2: Add optional debugger detection guard**

Add a helper function for Windows anti-debugging (used when extracting accessibility text):

```rust
#[cfg(target_os = "windows")]
fn is_debugger_attached() -> bool {
    unsafe { windows_sys::Win32::System::Diagnostics::Debug::IsDebuggerPresent() != 0 }
}
```

This is called in the Windows `AccessibilityExtractor` implementation. When a debugger is detected, skip text extraction and log `warn!("Debugger detected; skipping accessibility text extraction")`.

**Verify:** `cargo check --workspace`

---

## Task 13: Deprecate old accessibility stubs (oneshim-vision)

**Files:**
- Modify: `crates/oneshim-vision/src/accessibility_macos.rs`
- Modify: `crates/oneshim-vision/src/accessibility_windows.rs`

- [ ] **Step 1: Add deprecation notice to `accessibility_macos.rs`**

Add at the top of the file:

```rust
//! DEPRECATED: Use `accessibility::MacOsNativeAccessibility` (Phase 2 native AX FFI)
//! instead. This osascript-based stub is retained for the `ElementFinder` trait
//! (automation click-target lookup) but should not be used for focused-element
//! extraction. Will be removed when the `ChainedElementFinder` is updated to
//! use the new native implementation.
```

- [ ] **Step 2: Add deprecation notice to `accessibility_windows.rs`**

Add at the top of the file:

```rust
//! DEPRECATED: Use `accessibility::WindowsUiaAccessibility` (Phase 2 UIAutomation)
//! instead. This empty stub is retained for the `ElementFinder` trait. Will be
//! removed when the `ChainedElementFinder` is updated.
```

**Verify:** `cargo check -p oneshim-vision`

---

## Task 14: Integration tests with mock accessibility data

**Files:**
- Add to existing test modules in `src-tauri/tests/` or `crates/oneshim-vision/`

- [ ] **Step 1: Test PII-level fallback chain**

```rust
#[tokio::test]
async fn pii_off_without_consent_falls_back_to_standard() {
    // Use a mock AccessibilityExtractor that returns known data
    let extractor = MockAccessibilityExtractor::new(FocusedElementInfo {
        role: "AXTextField".to_string(),
        position: Some(ElementRect { x: 0.0, y: 0.0, width: 100.0, height: 25.0 }),
        label: Some("Input".to_string()),
        value_length: Some(10),
        extracted_text: None, // Standard level -> no text
    });

    let result = extractor
        .extract_focused_element(PiiFilterLevel::Off, false) // no consent!
        .await
        .unwrap();

    // Should NOT have extracted_text because consent is missing
    assert!(result.is_some());
    let info = result.unwrap();
    assert!(info.extracted_text.is_none());
}
```

- [ ] **Step 2: Test config gating**

```rust
#[test]
fn accessibility_disabled_when_config_false() {
    let config = TextIntelligenceConfig {
        enabled: true,
        accessibility_extraction: false,
        ..Default::default()
    };
    // Scheduler should NOT construct an AccessibilityExtractor
    assert!(!config.accessibility_extraction);
}

#[test]
fn accessibility_disabled_when_consent_missing() {
    let consent = ConsentPermissions {
        activity_pattern_learning: false,
        ..Default::default()
    };
    // Even if config says enabled, missing consent blocks construction
    assert!(!consent.activity_pattern_learning);
}
```

- [ ] **Step 3: Test zeroize behavior**

```rust
#[test]
fn zeroizing_string_clears_on_drop() {
    use zeroize::Zeroizing;

    let secret = Zeroizing::new("sensitive text".to_string());
    let ptr = secret.as_ptr();
    let len = secret.len();
    drop(secret);
    // After drop, the memory at ptr should be zeroed.
    // NOTE: This test is best-effort -- the allocator may have reused
    // the memory. In practice, zeroize guarantees the drop impl zeros
    // the buffer before deallocation.
}
```

**Verify:** `cargo test --workspace`

---

## Verification Checklist

After all tasks are complete, run these commands and verify all pass:

- [ ] `cargo check --workspace` -- no compilation errors
- [ ] `cargo test --workspace` -- all tests pass (0 failures)
- [ ] `cargo clippy --workspace` -- no warnings (allow `dead_code` for future-use variants)
- [ ] `cargo fmt --check` -- formatting clean

### Manual verification (requires macOS with Accessibility permission):

- [ ] `cargo test -p oneshim-vision -- macos_native_ax --ignored` -- integration test runs without panic
- [ ] Focus a text field in any app, verify that `extract_focused_element()` returns role + position
- [ ] Test with Accessibility permission revoked -- verify graceful `None` return, no crash

### Architecture validation:

- [ ] `AccessibilityExtractor` trait is in `oneshim-core/src/ports/` (port layer)
- [ ] `FocusedElementInfo` and `ElementRect` are in `oneshim-core/src/models/` (domain layer)
- [ ] macOS/Windows implementations are in `oneshim-vision/src/accessibility/` (adapter layer)
- [ ] No direct dependency between adapter crates -- all cross-crate communication through `oneshim-core` traits
- [ ] `zeroize` is used for all raw text from accessibility APIs before PII filtering
- [ ] `full_text_extraction` consent field uses `#[serde(default)]` for backward compatibility
- [ ] Circuit breaker pattern reused from existing `accessibility_macos.rs`

---

## Performance Budget

| Operation | Budget | Approach |
|-----------|--------|----------|
| `AXUIElement` focused element query | < 5ms | Single attribute query, no tree traversal |
| `spawn_blocking` overhead | < 1ms | tokio blocking thread pool |
| PII filtering on element text | < 0.1ms | Reuses existing `sanitize_title_with_level()` |
| `Zeroizing<String>` drop (zeroize) | < 0.01ms | Memset zero on buffer |
| Circuit breaker check | < 0.001ms | Single atomic load |

Total per-tick overhead when accessibility is enabled: < 7ms (within the spec's < 5ms budget for the AX call itself, plus overhead).

---

## Risk Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| AXUIElement FFI crashes on edge cases | Monitor loop crash | `spawn_blocking` isolates FFI; circuit breaker limits retries; `catch_unwind` as last resort |
| Accessibility permission revoked at runtime | Silent data loss | `kAXErrorAPIDisabled` detected, returns `None`, logs `warn!` |
| Memory leak from un-released CFTypeRef | Process memory growth | Every `CFTypeRef` is released via `CFRelease()` in the same scope |
| Raw text in memory between extraction and PII filter | Security exposure | `Zeroizing<String>` zeros on drop; exposure window < 1ms |
| Windows stub returns None forever | No accessibility data on Windows | Structurally complete code with TODO comments; no regression from current behavior |
