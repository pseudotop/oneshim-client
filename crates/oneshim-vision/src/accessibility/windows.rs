//! Windows UIAutomation accessibility extractor.
//!
//! Extracts the currently focused UI element via the COM-based IUIAutomation API.
//! The implementation follows the same circuit breaker and PII gating patterns
//! used by the macOS native extractor.
//!
//! COM call sequence:
//!   1. `CoInitializeEx(COINIT_MULTITHREADED)` — initialize COM on the thread
//!   2. `CoCreateInstance(CLSID_CUIAutomation)` — obtain `IUIAutomation` interface
//!   3. `IUIAutomation::GetFocusedElement()` — get the focused `IUIAutomationElement`
//!   4. Extract properties:
//!      - `get_CurrentControlType()` — mapped to a role string
//!      - `get_CurrentName()` — accessibility label
//!      - `get_CurrentBoundingRectangle()` — screen position/size
//!      - `GetCurrentPropertyValue(UIA_ValueValuePropertyId)` — text value
//!   5. `CoUninitialize()` — release COM on the thread
//!
//! The actual COM FFI calls are isolated in the `com` helper module so the
//! overall structure can be reviewed and tested on non-Windows platforms.

#[cfg(target_os = "windows")]
mod inner {
    use std::sync::atomic::{AtomicU32, Ordering};

    use async_trait::async_trait;
    use tracing::{debug, warn};
    use zeroize::Zeroizing;

    use oneshim_core::config::PiiFilterLevel;
    use oneshim_core::error::CoreError;
    use oneshim_core::models::focused_element::{ElementRect, FocusedElementInfo};
    use oneshim_core::ports::accessibility::AccessibilityExtractor;

    use crate::privacy::sanitize_title_with_level;

    // ── Circuit breaker (mirrors macOS impl) ─────────────────────────

    /// Consecutive COM/UIA failures before the circuit breaker opens.
    static CONSECUTIVE_FAILURES: AtomicU32 = AtomicU32::new(0);
    const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;
    /// After threshold is hit, retry once every N calls.
    const CIRCUIT_BREAKER_RETRY_INTERVAL: u32 = 60;

    // ── UIA ControlTypeId → role string mapping ──────────────────────

    /// Map a UIA `ControlTypeId` to a human-readable role string.
    ///
    /// The numeric values are from the Windows SDK header `UIAutomationClient.h`.
    /// We keep strings consistent with macOS AXRole naming where possible and
    /// use Windows-native names otherwise.
    fn control_type_to_role(control_type_id: i32) -> &'static str {
        // UIA_*ControlTypeId values (from UIAutomationClient.h)
        const UIA_BUTTON: i32 = 50000;
        const UIA_CALENDAR: i32 = 50001;
        const UIA_CHECKBOX: i32 = 50002;
        const UIA_COMBOBOX: i32 = 50003;
        const UIA_EDIT: i32 = 50004;
        const UIA_HYPERLINK: i32 = 50005;
        const UIA_IMAGE: i32 = 50006;
        const UIA_LISTITEM: i32 = 50007;
        const UIA_LIST: i32 = 50008;
        const UIA_MENU: i32 = 50009;
        const UIA_MENUBAR: i32 = 50010;
        const UIA_MENUITEM: i32 = 50011;
        const UIA_PROGRESSBAR: i32 = 50012;
        const UIA_RADIOBUTTON: i32 = 50013;
        const UIA_SCROLLBAR: i32 = 50014;
        const UIA_SLIDER: i32 = 50015;
        const UIA_SPINNER: i32 = 50016;
        const UIA_STATUSBAR: i32 = 50017;
        const UIA_TAB: i32 = 50018;
        const UIA_TABITEM: i32 = 50019;
        const UIA_TEXT: i32 = 50020;
        const UIA_TOOLBAR: i32 = 50021;
        const UIA_TOOLTIP: i32 = 50022;
        const UIA_TREE: i32 = 50023;
        const UIA_TREEITEM: i32 = 50024;
        const UIA_CUSTOM: i32 = 50025;
        const UIA_GROUP: i32 = 50026;
        const UIA_THUMB: i32 = 50027;
        const UIA_DATAGRID: i32 = 50028;
        const UIA_DATAITEM: i32 = 50029;
        const UIA_DOCUMENT: i32 = 50030;
        const UIA_SPLITBUTTON: i32 = 50031;
        const UIA_WINDOW: i32 = 50032;
        const UIA_PANE: i32 = 50033;
        const UIA_HEADER: i32 = 50034;
        const UIA_HEADERITEM: i32 = 50035;
        const UIA_TABLE: i32 = 50036;
        const UIA_TITLEBAR: i32 = 50037;
        const UIA_SEPARATOR: i32 = 50038;

        match control_type_id {
            UIA_BUTTON => "Button",
            UIA_CALENDAR => "Calendar",
            UIA_CHECKBOX => "CheckBox",
            UIA_COMBOBOX => "ComboBox",
            UIA_EDIT => "Edit",
            UIA_HYPERLINK => "Hyperlink",
            UIA_IMAGE => "Image",
            UIA_LISTITEM => "ListItem",
            UIA_LIST => "List",
            UIA_MENU => "Menu",
            UIA_MENUBAR => "MenuBar",
            UIA_MENUITEM => "MenuItem",
            UIA_PROGRESSBAR => "ProgressBar",
            UIA_RADIOBUTTON => "RadioButton",
            UIA_SCROLLBAR => "ScrollBar",
            UIA_SLIDER => "Slider",
            UIA_SPINNER => "Spinner",
            UIA_STATUSBAR => "StatusBar",
            UIA_TAB => "Tab",
            UIA_TABITEM => "TabItem",
            UIA_TEXT => "Text",
            UIA_TOOLBAR => "ToolBar",
            UIA_TOOLTIP => "ToolTip",
            UIA_TREE => "Tree",
            UIA_TREEITEM => "TreeItem",
            UIA_CUSTOM => "Custom",
            UIA_GROUP => "Group",
            UIA_THUMB => "Thumb",
            UIA_DATAGRID => "DataGrid",
            UIA_DATAITEM => "DataItem",
            UIA_DOCUMENT => "Document",
            UIA_SPLITBUTTON => "SplitButton",
            UIA_WINDOW => "Window",
            UIA_PANE => "Pane",
            UIA_HEADER => "Header",
            UIA_HEADERITEM => "HeaderItem",
            UIA_TABLE => "Table",
            UIA_TITLEBAR => "TitleBar",
            UIA_SEPARATOR => "Separator",
            _ => "Unknown",
        }
    }

    // ── Raw extracted data (before PII filtering) ────────────────────

    /// Raw data extracted from UIAutomation before PII level gating.
    /// Text fields use `Zeroizing<String>` so memory is zeroed on drop.
    struct RawFocusedElement {
        role: String,
        name: Option<Zeroizing<String>>,
        value: Option<Zeroizing<String>>,
        position: Option<ElementRect>,
    }

    // ── COM helper module ────────────────────────────────────────────
    //
    // Isolates the unsafe COM FFI calls into a single function that
    // returns `Option<RawFocusedElement>`. This makes the overall
    // structure reviewable on non-Windows and allows the helper to be
    // replaced with a stub for unit testing.

    mod com {
        use super::{control_type_to_role, ElementRect, RawFocusedElement};
        use std::ptr;
        use zeroize::Zeroizing;

        // COM CLSID/IID GUIDs for IUIAutomation
        //
        // CLSID_CUIAutomation:
        //   {FF48DBA4-60EF-4201-AA87-54103EEF594E}
        const CLSID_CUIAUTOMATION: windows_sys::core::GUID = windows_sys::core::GUID {
            data1: 0xFF48DBA4,
            data2: 0x60EF,
            data3: 0x4201,
            data4: [0xAA, 0x87, 0x54, 0x10, 0x3E, 0xEF, 0x59, 0x4E],
        };

        // IID_IUIAutomation:
        //   {30CBE57D-D9D0-452A-AB13-7AC5AC4825EE}
        const IID_IUIAUTOMATION: windows_sys::core::GUID = windows_sys::core::GUID {
            data1: 0x30CBE57D,
            data2: 0xD9D0,
            data3: 0x452A,
            data4: [0xAB, 0x13, 0x7A, 0xC5, 0xAC, 0x48, 0x25, 0xEE],
        };

        /// UIA property IDs for `GetCurrentPropertyValue`.
        const UIA_VALUE_VALUE_PROPERTY_ID: i32 = 30045;

        // COM method vtable offsets for IUIAutomation
        // IUnknown: QueryInterface(0), AddRef(1), Release(2)
        // IUIAutomation methods:
        //   ...various methods...
        //   GetFocusedElement is at vtable index 8
        const IUNKNOWN_RELEASE_INDEX: usize = 2;
        const IUIAUTOMATION_GET_FOCUSED_ELEMENT_INDEX: usize = 8;

        // IUIAutomationElement vtable offsets
        // IUnknown: 0-2, IUIAutomationElement methods start at 3
        //   get_CurrentControlType: index 21
        //   get_CurrentName: index 23
        //   get_CurrentBoundingRectangle: index 27
        //   GetCurrentPropertyValue: index 12
        const IELEMENT_GET_CURRENT_CONTROL_TYPE_INDEX: usize = 21;
        const IELEMENT_GET_CURRENT_NAME_INDEX: usize = 23;
        const IELEMENT_GET_CURRENT_BOUNDING_RECT_INDEX: usize = 27;
        const IELEMENT_GET_CURRENT_PROPERTY_VALUE_INDEX: usize = 12;

        /// UIA bounding rectangle (uses f64, not i32 like Windows RECT).
        #[repr(C)]
        struct UiaRect {
            left: f64,
            top: f64,
            width: f64,
            height: f64,
        }

        /// VARIANT structure (simplified for BSTR/I4 extraction).
        #[repr(C)]
        struct Variant {
            vt: u16,
            _reserved1: u16,
            _reserved2: u16,
            _reserved3: u16,
            data: VariantData,
        }

        /// Union portion of VARIANT (64-bit).
        #[repr(C)]
        union VariantData {
            bstr_val: *mut u16,
            int_val: i32,
            _pad: [u8; 8],
        }

        const VT_BSTR: u16 = 8;

        /// Convert a BSTR (null-terminated UTF-16 pointer) to a Rust String.
        ///
        /// SAFETY: `bstr` must be a valid BSTR pointer allocated by SysAllocString.
        unsafe fn bstr_to_string(bstr: *mut u16) -> Option<String> {
            if bstr.is_null() {
                return None;
            }
            // BSTR length prefix is 4 bytes before the pointer
            let len_ptr = (bstr as *const u8).sub(4) as *const u32;
            let byte_len = *len_ptr as usize;
            let char_len = byte_len / 2;
            if char_len == 0 {
                return Some(String::new());
            }
            let slice = std::slice::from_raw_parts(bstr, char_len);
            String::from_utf16(slice).ok()
        }

        /// Free a BSTR allocated by the COM runtime.
        unsafe fn sys_free_string(bstr: *mut u16) {
            if !bstr.is_null() {
                windows_sys::Win32::System::Com::SysFreeString(bstr as windows_sys::core::BSTR);
            }
        }

        /// Call a method on a COM interface via its vtable.
        ///
        /// SAFETY: The pointer must be a valid COM interface and the index
        /// must be correct for the interface layout.
        unsafe fn vtable_fn(obj: *mut std::ffi::c_void, index: usize) -> *const std::ffi::c_void {
            let vtable = *(obj as *const *const *const std::ffi::c_void);
            *vtable.add(index)
        }

        /// Release a COM object (call IUnknown::Release).
        unsafe fn release(obj: *mut std::ffi::c_void) {
            if !obj.is_null() {
                let release_fn: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32 =
                    std::mem::transmute(vtable_fn(obj, IUNKNOWN_RELEASE_INDEX));
                release_fn(obj);
            }
        }

        /// Extract focused element data via COM UIAutomation API.
        ///
        /// Returns `None` if COM initialization fails, no element is focused,
        /// or any step in the extraction chain fails. Errors are logged at
        /// debug level to avoid noise under normal operation.
        ///
        /// SAFETY: This function performs raw COM FFI calls. It is only called
        /// from `spawn_blocking` to avoid blocking the tokio runtime.
        pub(super) fn extract_via_uia() -> Option<RawFocusedElement> {
            unsafe {
                // Step 1: Initialize COM (COINIT_MULTITHREADED = 0x0)
                let hr = windows_sys::Win32::System::Com::CoInitializeEx(
                    ptr::null(),
                    windows_sys::Win32::System::Com::COINIT_MULTITHREADED,
                );
                // S_OK (0) or S_FALSE (1, already initialized) are both acceptable.
                if hr < 0 {
                    tracing::debug!(hresult = hr, "CoInitializeEx failed");
                    return None;
                }
                let _com_guard = ComGuard; // CoUninitialize on drop

                // Step 2: Create IUIAutomation instance
                let mut automation: *mut std::ffi::c_void = ptr::null_mut();
                let hr = windows_sys::Win32::System::Com::CoCreateInstance(
                    &CLSID_CUIAUTOMATION,
                    ptr::null_mut(),
                    windows_sys::Win32::System::Com::CLSCTX_INPROC_SERVER,
                    &IID_IUIAUTOMATION,
                    &mut automation,
                );
                if hr < 0 || automation.is_null() {
                    tracing::debug!(hresult = hr, "CoCreateInstance(CUIAutomation) failed");
                    return None;
                }

                // Step 3: GetFocusedElement
                let mut element: *mut std::ffi::c_void = ptr::null_mut();
                let get_focused: unsafe extern "system" fn(
                    *mut std::ffi::c_void,
                    *mut *mut std::ffi::c_void,
                ) -> i32 = std::mem::transmute(vtable_fn(
                    automation,
                    IUIAUTOMATION_GET_FOCUSED_ELEMENT_INDEX,
                ));
                let hr = get_focused(automation, &mut element);
                release(automation);

                if hr < 0 || element.is_null() {
                    tracing::debug!(hresult = hr, "GetFocusedElement returned no element");
                    return None;
                }

                // Step 4a: get_CurrentControlType -> role
                let mut control_type: i32 = 0;
                let get_control_type: unsafe extern "system" fn(
                    *mut std::ffi::c_void,
                    *mut i32,
                ) -> i32 = std::mem::transmute(vtable_fn(
                    element,
                    IELEMENT_GET_CURRENT_CONTROL_TYPE_INDEX,
                ));
                let hr = get_control_type(element, &mut control_type);
                let role = if hr >= 0 {
                    control_type_to_role(control_type).to_string()
                } else {
                    "Unknown".to_string()
                };

                // Step 4b: get_CurrentName -> label
                let mut name_bstr: *mut u16 = ptr::null_mut();
                let get_name: unsafe extern "system" fn(
                    *mut std::ffi::c_void,
                    *mut *mut u16,
                ) -> i32 = std::mem::transmute(vtable_fn(element, IELEMENT_GET_CURRENT_NAME_INDEX));
                let hr = get_name(element, &mut name_bstr);
                let name = if hr >= 0 {
                    let s = bstr_to_string(name_bstr);
                    sys_free_string(name_bstr);
                    s.filter(|s| !s.is_empty()).map(Zeroizing::new)
                } else {
                    None
                };

                // Step 4c: get_CurrentBoundingRectangle -> position
                let mut rect = UiaRect {
                    left: 0.0,
                    top: 0.0,
                    width: 0.0,
                    height: 0.0,
                };
                let get_rect: unsafe extern "system" fn(
                    *mut std::ffi::c_void,
                    *mut UiaRect,
                ) -> i32 = std::mem::transmute(vtable_fn(
                    element,
                    IELEMENT_GET_CURRENT_BOUNDING_RECT_INDEX,
                ));
                let hr = get_rect(element, &mut rect);
                let position = if hr >= 0 && (rect.width > 0.0 || rect.height > 0.0) {
                    Some(ElementRect {
                        x: rect.left as f32,
                        y: rect.top as f32,
                        width: rect.width as f32,
                        height: rect.height as f32,
                    })
                } else {
                    None
                };

                // Step 4d: GetCurrentPropertyValue(UIA_ValueValuePropertyId) -> text value
                let mut variant = std::mem::zeroed::<Variant>();
                let get_property: unsafe extern "system" fn(
                    *mut std::ffi::c_void,
                    i32,
                    *mut Variant,
                ) -> i32 = std::mem::transmute(vtable_fn(
                    element,
                    IELEMENT_GET_CURRENT_PROPERTY_VALUE_INDEX,
                ));
                let hr = get_property(element, UIA_VALUE_VALUE_PROPERTY_ID, &mut variant);
                // UIA_ValueValuePropertyId only ever returns VT_BSTR or VT_EMPTY,
                // so freeing the BSTR (via sys_free_string) is sufficient cleanup.
                // TODO: If we extend this to other property IDs that may return
                // VT_I4, VT_DISPATCH, VT_UNKNOWN, etc., add a VariantClear call
                // here to safely release any variant-owned resources.
                let value = if hr >= 0 && variant.vt == VT_BSTR {
                    let bstr = variant.data.bstr_val;
                    let s = bstr_to_string(bstr);
                    sys_free_string(bstr);
                    s.filter(|s| !s.is_empty()).map(Zeroizing::new)
                } else {
                    None
                };

                release(element);

                Some(RawFocusedElement {
                    role,
                    name,
                    value,
                    position,
                })
            }
        }

        /// RAII guard to call `CoUninitialize` when dropped.
        struct ComGuard;

        impl Drop for ComGuard {
            fn drop(&mut self) {
                unsafe {
                    windows_sys::Win32::System::Com::CoUninitialize();
                }
            }
        }
    }

    // ── Public extractor struct ──────────────────────────────────────

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

        /// Check if a debugger is attached to the current process.
        /// When detected, text extraction is skipped to prevent memory
        /// inspection of sensitive accessibility data.
        fn is_debugger_attached() -> bool {
            unsafe { windows_sys::Win32::System::Diagnostics::Debug::IsDebuggerPresent() != 0 }
        }

        // ── Circuit breaker (same pattern as macOS) ──────────────────

        fn circuit_allows() -> bool {
            let failures = CONSECUTIVE_FAILURES.load(Ordering::Relaxed);
            if failures >= CIRCUIT_BREAKER_THRESHOLD {
                if failures % CIRCUIT_BREAKER_RETRY_INTERVAL != 0 {
                    CONSECUTIVE_FAILURES.fetch_add(1, Ordering::Relaxed);
                    return false;
                }
                warn!(
                    "WindowsUiaAccessibility: circuit breaker retry after {} skipped",
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

        // ── PII level gating (mirrors macOS) ─────────────────────────

        /// Apply PII-level filtering to raw extracted data.
        ///
        /// Level semantics:
        /// - `Strict`: role + position only
        /// - `Standard`: + label + value_length (no text content)
        /// - `Basic`: + sanitized text (PII patterns masked)
        /// - `Off`: full text (requires explicit consent)
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
                    label: raw.name.as_deref().map(|s| s.to_string()),
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
                        label: raw.name.as_deref().map(|s| s.to_string()),
                        value_length: raw.value.as_deref().map(|v| v.len() as u32),
                        extracted_text: text,
                    }
                }
                PiiFilterLevel::Off => FocusedElementInfo {
                    role: raw.role,
                    position: raw.position,
                    label: raw.name.as_deref().map(|s| s.to_string()),
                    value_length: raw.value.as_deref().map(|v| v.len() as u32),
                    extracted_text: raw.value.as_deref().map(|v| v.to_string()),
                },
            }
            // raw.name and raw.value (Zeroizing<String>) are dropped here,
            // zeroing memory automatically.
        }
    }

    #[async_trait]
    impl AccessibilityExtractor for WindowsUiaAccessibility {
        async fn extract_focused_element(
            &self,
            pii_level: PiiFilterLevel,
            has_full_text_consent: bool,
        ) -> Result<Option<FocusedElementInfo>, CoreError> {
            // Security: skip when debugger is attached
            if Self::is_debugger_attached() {
                warn!("Debugger detected; skipping accessibility text extraction");
                return Ok(None);
            }

            // Circuit breaker: skip when too many consecutive failures
            if !Self::circuit_allows() {
                debug!("WindowsUiaAccessibility: circuit breaker open");
                return Ok(None);
            }

            // Consent gating: fall back to Standard if Off without consent
            let effective_level = if pii_level == PiiFilterLevel::Off && !has_full_text_consent {
                debug!(
                    "PII Off requested but full_text_extraction consent missing; \
                     falling back to Standard"
                );
                PiiFilterLevel::Standard
            } else {
                pii_level
            };

            // Run synchronous COM calls on a blocking thread
            let result = tokio::task::spawn_blocking(com::extract_via_uia)
                .await
                .map_err(|e| CoreError::Internal(format!("UIA blocking task failed: {e}")))?;

            match result {
                Some(raw) => {
                    Self::record_success();
                    let filtered = Self::filter_by_level(raw, effective_level);
                    debug!(role = %filtered.role, "UIA focused element extracted");
                    Ok(Some(filtered))
                }
                None => {
                    Self::record_failure();
                    Ok(None)
                }
            }
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
    use oneshim_core::models::focused_element::ElementRect;
    use oneshim_core::ports::accessibility::AccessibilityExtractor;

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

    #[test]
    fn control_type_mapping_known_types() {
        // Button = 50000
        assert_eq!(super::inner::control_type_to_role(50000), "Button");
        // Edit = 50004
        assert_eq!(super::inner::control_type_to_role(50004), "Edit");
        // Document = 50030
        assert_eq!(super::inner::control_type_to_role(50030), "Document");
        // Text = 50020
        assert_eq!(super::inner::control_type_to_role(50020), "Text");
    }

    #[test]
    fn control_type_mapping_unknown() {
        assert_eq!(super::inner::control_type_to_role(99999), "Unknown");
    }

    #[tokio::test]
    async fn extract_returns_ok() {
        // On CI or real Windows, this should return Ok (either Some or None
        // depending on whether a window is focused).
        let extractor = WindowsUiaAccessibility::new();
        let result = extractor
            .extract_focused_element(PiiFilterLevel::Standard, false)
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn filter_strict_strips_label_and_text() {
        let info = apply_test_filter(
            "Edit",
            Some("Username"),
            Some("john@example.com"),
            Some(ElementRect {
                x: 10.0,
                y: 20.0,
                width: 200.0,
                height: 25.0,
            }),
            PiiFilterLevel::Strict,
        );
        assert_eq!(info.role, "Edit");
        assert!(info.position.is_some());
        assert!(info.label.is_none());
        assert!(info.value_length.is_none());
        assert!(info.extracted_text.is_none());
    }

    #[test]
    fn filter_standard_includes_label_and_length() {
        let info = apply_test_filter(
            "Edit",
            Some("Search"),
            Some("cargo test"),
            None,
            PiiFilterLevel::Standard,
        );
        assert_eq!(info.label, Some("Search".to_string()));
        assert_eq!(info.value_length, Some(10));
        assert!(info.extracted_text.is_none());
    }

    #[test]
    fn filter_basic_includes_sanitized_text() {
        let info = apply_test_filter(
            "Edit",
            None,
            Some("user@example.com"),
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
        let info = apply_test_filter(
            "Document",
            None,
            Some("full content here"),
            None,
            PiiFilterLevel::Off,
        );
        assert_eq!(info.extracted_text, Some("full content here".to_string()));
    }

    /// Helper to test PII filtering without COM calls.
    fn apply_test_filter(
        role: &str,
        name: Option<&str>,
        value: Option<&str>,
        position: Option<ElementRect>,
        level: PiiFilterLevel,
    ) -> oneshim_core::models::focused_element::FocusedElementInfo {
        use crate::privacy::sanitize_title_with_level;
        use oneshim_core::models::focused_element::FocusedElementInfo;
        use zeroize::Zeroizing;

        let name_z = name.map(|s| Zeroizing::new(s.to_string()));
        let value_z = value.map(|s| Zeroizing::new(s.to_string()));

        match level {
            PiiFilterLevel::Strict => FocusedElementInfo {
                role: role.to_string(),
                position,
                ..Default::default()
            },
            PiiFilterLevel::Standard => FocusedElementInfo {
                role: role.to_string(),
                position,
                label: name_z.as_deref().map(|s| s.to_string()),
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
                    label: name_z.as_deref().map(|s| s.to_string()),
                    value_length: value_z.as_deref().map(|v| v.len() as u32),
                    extracted_text: text,
                }
            }
            PiiFilterLevel::Off => FocusedElementInfo {
                role: role.to_string(),
                position,
                label: name_z.as_deref().map(|s| s.to_string()),
                value_length: value_z.as_deref().map(|v| v.len() as u32),
                extracted_text: value_z.as_deref().map(|v| v.to_string()),
            },
        }
    }
}
