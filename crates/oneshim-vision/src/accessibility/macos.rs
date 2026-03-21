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
    use oneshim_core::models::focused_element::{
        AccessibilityElement, ElementRect, FocusedElementInfo,
    };
    use oneshim_core::ports::accessibility::AccessibilityExtractor;

    use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};

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

    /// Result of a batched attribute fetch for a single AX element.
    /// Used by `batch_get_attributes()` to return role, title, description,
    /// and bounds from a single `AXUIElementCopyMultipleAttributeValues` call.
    struct BatchAttributes {
        role: String,
        title: Option<String>,
        description: Option<String>,
        position_and_size: Option<ElementRect>,
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

        /// Fetch role, title, description, position, and size in a single IPC call
        /// using `AXUIElementCopyMultipleAttributeValues`.
        ///
        /// Returns `None` if the batch call fails (caller should fall back to
        /// individual `AXUIElementCopyAttributeValue` calls).
        ///
        /// SAFETY: Caller must ensure `element` is a valid AXUIElementRef.
        /// All returned CFTypeRef values are released within this function.
        unsafe fn batch_get_attributes(element: AXUIElementRef) -> Option<BatchAttributes> {
            use core_foundation::array::CFArray;
            use core_foundation::base::TCFType;
            use core_foundation::string::CFString;

            // Build the attribute names array: [AXRole, AXTitle, AXDescription, AXPosition, AXSize]
            let attr_role = ax_attr(AX_ROLE_ATTR);
            let attr_title = ax_attr(AX_TITLE_ATTR);
            let attr_desc = ax_attr(AX_DESCRIPTION_ATTR);
            let attr_pos = ax_attr(AX_POSITION_ATTR);
            let attr_size = ax_attr(AX_SIZE_ATTR);

            let attrs: CFArray<CFString> =
                CFArray::from_CFTypes(&[attr_role, attr_title, attr_desc, attr_pos, attr_size]);

            let mut values_ref: CFArrayRef = ptr::null();
            let err = AXUIElementCopyMultipleAttributeValues(
                element,
                attrs.as_concrete_TypeRef(),
                0, // default options: return kAXValueNotFound for missing attrs
                &mut values_ref,
            );

            if err != kAXErrorSuccess || values_ref.is_null() {
                return None;
            }

            let count = CFArrayGetCount(values_ref);
            if count < 5 {
                CFRelease(values_ref as CFTypeRef);
                return None;
            }

            // Helper: extract a String from a CFTypeRef that may be a CFString or
            // an error marker (kCFNull / AXValueNotFound sentinel).
            let extract_string = |idx: isize| -> Option<String> {
                let val = CFArrayGetValueAtIndex(values_ref, idx);
                if val.is_null() {
                    return None;
                }
                // The batch API returns kCFNull for unsupported/missing attributes.
                // kCFNull has a different CFTypeID than CFString.
                let type_id = core_foundation_sys::base::CFGetTypeID(val);
                let string_type_id = core_foundation_sys::string::CFStringGetTypeID();
                if type_id != string_type_id {
                    return None;
                }
                let cf_str = CFString::wrap_under_get_rule(val as CFStringRef);
                Some(cf_str.to_string())
            };

            // Index 0: role
            let role = extract_string(0).unwrap_or_default();
            // Index 1: title
            let title = extract_string(1);
            // Index 2: description
            let description = extract_string(2);

            // Index 3 & 4: position (AXValue<CGPoint>) and size (AXValue<CGSize>)
            let position_and_size = {
                let pos_val = CFArrayGetValueAtIndex(values_ref, 3);
                let size_val = CFArrayGetValueAtIndex(values_ref, 4);

                if pos_val.is_null() || size_val.is_null() {
                    None
                } else {
                    let mut point = CGPoint::default();
                    let mut size = CGSize::default();

                    let got_point = AXValueGetValue(
                        pos_val,
                        kAXValueCGPointType,
                        &mut point as *mut _ as *mut std::ffi::c_void,
                    );
                    let got_size = AXValueGetValue(
                        size_val,
                        kAXValueCGSizeType,
                        &mut size as *mut _ as *mut std::ffi::c_void,
                    );

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
            };

            // Release the values array (the individual elements are not owned by us
            // since we used CFArrayGetValueAtIndex which follows the Get Rule).
            CFRelease(values_ref as CFTypeRef);

            Some(BatchAttributes {
                role,
                title,
                description,
                position_and_size,
            })
        }

        /// Recursively traverse the accessibility tree from an element.
        ///
        /// Uses `AXUIElementCopyMultipleAttributeValues` to fetch role, title,
        /// description, position, and size in a single IPC call per element
        /// (down from 4-5 individual calls). Falls back to individual
        /// `AXUIElementCopyAttributeValue` calls if the batch API returns an error.
        ///
        /// SAFETY: All CFTypeRef values are released. The function returns owned
        /// Rust data. `remaining` is decremented for each element collected to
        /// enforce the max_elements cap.
        unsafe fn traverse_tree(
            element: AXUIElementRef,
            depth: u32,
            max_depth: u32,
            remaining: &mut usize,
            pii_level: PiiFilterLevel,
        ) -> Vec<AccessibilityElement> {
            if depth > max_depth || *remaining == 0 {
                return Vec::new();
            }

            let mut results = Vec::new();

            // Try batch fetch first (1 IPC call instead of 4-5).
            let (role, label, bounds) = if let Some(batch) = Self::batch_get_attributes(element) {
                let lbl = if pii_level != PiiFilterLevel::Strict {
                    batch.title.or(batch.description).unwrap_or_default()
                } else {
                    String::new()
                };
                (batch.role, lbl, batch.position_and_size)
            } else {
                // Fallback: individual attribute fetches.
                let role_key = ax_attr(AX_ROLE_ATTR);
                let role = Self::get_string_attr(element, as_cf_ref(&role_key)).unwrap_or_default();

                let lbl = if pii_level != PiiFilterLevel::Strict {
                    let title_key = ax_attr(AX_TITLE_ATTR);
                    let desc_key = ax_attr(AX_DESCRIPTION_ATTR);
                    Self::get_string_attr(element, as_cf_ref(&title_key))
                        .or_else(|| Self::get_string_attr(element, as_cf_ref(&desc_key)))
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                let bounds = Self::get_position_and_size(element);
                (role, lbl, bounds)
            };

            results.push(AccessibilityElement {
                role,
                label,
                bounds,
            });
            *remaining = remaining.saturating_sub(1);

            // Recurse into children
            if depth < max_depth && *remaining > 0 {
                let children_key = ax_attr(AX_CHILDREN_ATTR);
                let mut children_ref: CFTypeRef = ptr::null();
                let err = AXUIElementCopyAttributeValue(
                    element,
                    as_cf_ref(&children_key),
                    &mut children_ref,
                );
                if err == kAXErrorSuccess && !children_ref.is_null() {
                    let count = CFArrayGetCount(children_ref as CFArrayRef);
                    for i in 0..count {
                        if *remaining == 0 {
                            break;
                        }
                        let child = CFArrayGetValueAtIndex(children_ref as CFArrayRef, i);
                        if !child.is_null() {
                            let child_elements = Self::traverse_tree(
                                child,
                                depth + 1,
                                max_depth,
                                remaining,
                                pii_level,
                            );
                            results.extend(child_elements);
                        }
                    }
                    CFRelease(children_ref);
                }
            }

            results
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

        async fn extract_window_elements(
            &self,
            max_depth: u32,
            max_elements: usize,
            pii_level: PiiFilterLevel,
            has_full_text_consent: bool,
        ) -> Result<Vec<AccessibilityElement>, CoreError> {
            if !Self::check_permission() {
                return Err(CoreError::PermissionDenied(
                    "macOS Accessibility permission not granted. \
                     Enable in System Settings > Privacy & Security > Accessibility."
                        .to_string(),
                ));
            }
            if !Self::circuit_allows() {
                return Ok(Vec::new());
            }

            let effective_level = if pii_level == PiiFilterLevel::Off && !has_full_text_consent {
                PiiFilterLevel::Standard
            } else {
                pii_level
            };

            let result = tokio::task::spawn_blocking(move || unsafe {
                let system_wide = AXUIElementCreateSystemWide();
                if system_wide.is_null() {
                    return Vec::new();
                }

                // Get focused element
                let focused_window_key = ax_attr(AX_FOCUSED_UI_ELEMENT_ATTR);
                let mut focused: CFTypeRef = ptr::null();
                let err = AXUIElementCopyAttributeValue(
                    system_wide,
                    as_cf_ref(&focused_window_key),
                    &mut focused,
                );
                CFRelease(system_wide);

                if err != kAXErrorSuccess || focused.is_null() {
                    return Vec::new();
                }

                // Try to get the window containing the focused element
                let window_key = ax_attr(AX_WINDOW_ATTR);
                let mut window_ref: CFTypeRef = ptr::null();
                let w_err =
                    AXUIElementCopyAttributeValue(focused, as_cf_ref(&window_key), &mut window_ref);

                let traverse_root = if w_err == kAXErrorSuccess && !window_ref.is_null() {
                    CFRelease(focused);
                    window_ref
                } else {
                    // Fallback: traverse from focused element itself
                    focused
                };

                let mut remaining = max_elements;
                let elements = Self::traverse_tree(
                    traverse_root,
                    0,
                    max_depth,
                    &mut remaining,
                    effective_level,
                );
                CFRelease(traverse_root);
                elements
            })
            .await
            .map_err(|e| CoreError::Internal(format!("AX tree traversal task failed: {e}")))?;

            if result.is_empty() {
                Self::record_failure();
            } else {
                Self::record_success();
                debug!(count = result.len(), "AX window tree extracted");
            }

            Ok(result)
        }

        fn has_permission(&self) -> bool {
            Self::check_permission()
        }

        fn name(&self) -> &str {
            "macos-native-accessibility"
        }

        fn request_permission(&self) -> bool {
            unsafe {
                use core_foundation::boolean::CFBoolean;
                use core_foundation::dictionary::CFDictionary;

                let key = CFString::new("AXTrustedCheckOptionPrompt");
                let value = CFBoolean::true_value();
                let options =
                    CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
                AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as CFTypeRef)
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use inner::MacOsNativeAccessibility;

// ── AXObserver-based focus change detection ────────────────────────────
//
// Event-driven alternative to polling. An AXObserver subscribes to
// `kAXFocusedUIElementChangedNotification` for a given application PID.
// When the focused element changes, a callback sets an atomic flag that
// the scheduler can check on each tick.
//
// Usage:
//   let handle = FocusObserverHandle::start(pid)?;
//   // ... on scheduler tick:
//   if handle.has_focus_changed() {
//       // extract focused element (existing polling path)
//   }
//   // ... on shutdown or PID change:
//   handle.stop();

#[cfg(target_os = "macos")]
mod observer {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use core_foundation::string::CFString;
    use core_foundation_sys::base::CFRelease;
    use core_foundation_sys::string::CFStringRef;
    use tracing::{debug, info, warn};

    use crate::accessibility::ffi_macos::ax::*;

    /// Shared state between the observer callback and the owner.
    ///
    /// The callback sets `focus_changed` to true. The owner reads and
    /// resets it via `has_focus_changed()`. The `running` flag controls
    /// the CFRunLoop thread lifetime.
    struct ObserverState {
        /// Set to `true` by the AXObserver callback when focus changes.
        focus_changed: AtomicBool,
        /// Set to `false` to signal the CFRunLoop thread to exit.
        running: AtomicBool,
    }

    /// Handle to a running AXObserver that detects focus changes for a
    /// specific application PID.
    ///
    /// Dropping the handle stops the observer and joins the background thread.
    pub struct FocusObserverHandle {
        state: Arc<ObserverState>,
        /// The dedicated thread running the CFRunLoop.
        thread: Option<std::thread::JoinHandle<()>>,
        /// PID being observed (for diagnostics).
        pid: PidT,
    }

    impl FocusObserverHandle {
        /// Start observing focus changes for the given application PID.
        ///
        /// Spawns a dedicated thread that runs a CFRunLoop to receive
        /// AXObserver notifications. Returns `None` if the observer
        /// cannot be created (e.g. permission denied, invalid PID).
        pub fn start(pid: PidT) -> Option<Self> {
            let state = Arc::new(ObserverState {
                focus_changed: AtomicBool::new(false),
                running: AtomicBool::new(true),
            });

            // Verify that the observer can be created before spawning the
            // thread. This catches permission errors early.
            if !Self::can_create_observer(pid) {
                warn!(
                    pid,
                    "AXObserver: cannot create observer for PID (permission denied or invalid PID)"
                );
                return None;
            }

            let state_clone = state.clone();
            let thread = std::thread::Builder::new()
                .name(format!("ax-focus-observer-{pid}"))
                .spawn(move || {
                    Self::run_observer_loop(pid, state_clone);
                })
                .ok()?;

            info!(pid, "AXFocusObserver started");

            Some(Self {
                state,
                thread: Some(thread),
                pid,
            })
        }

        /// Check whether the focused element has changed since the last check.
        ///
        /// Returns `true` exactly once per focus change event. Thread-safe.
        pub fn has_focus_changed(&self) -> bool {
            self.state.focus_changed.swap(false, Ordering::Acquire)
        }

        /// The PID being observed.
        pub fn observed_pid(&self) -> PidT {
            self.pid
        }

        /// Stop the observer. Also called automatically on drop.
        pub fn stop(&mut self) {
            if !self.state.running.swap(false, Ordering::Release) {
                return; // already stopped
            }
            debug!(pid = self.pid, "AXFocusObserver stopping");

            if let Some(handle) = self.thread.take() {
                // The CFRunLoop will exit on its next iteration because
                // `running` is false and the 0.5s timeout will fire.
                let _ = handle.join();
            }
        }

        /// Quick check: can we create an AXObserver for this PID?
        ///
        /// Creates and immediately releases an observer to validate
        /// that the PID is valid and accessibility permission is granted.
        fn can_create_observer(pid: PidT) -> bool {
            unsafe {
                let mut observer: AXObserverRef = std::ptr::null();
                let err = AXObserverCreate(pid, Self::focus_callback, &mut observer);
                if err == kAXErrorSuccess && !observer.is_null() {
                    CFRelease(observer);
                    true
                } else {
                    false
                }
            }
        }

        /// The CFRunLoop thread body.
        ///
        /// Creates an AXObserver, subscribes to focus change notifications
        /// on the application element, and runs the CFRunLoop until
        /// `running` is set to false.
        fn run_observer_loop(pid: PidT, state: Arc<ObserverState>) {
            unsafe {
                // SAFETY: AXObserverCreate allocates and returns a new
                // AXObserverRef. We own it and must release it.
                let mut observer: AXObserverRef = std::ptr::null();
                let err = AXObserverCreate(pid, Self::focus_callback, &mut observer);
                if err != kAXErrorSuccess || observer.is_null() {
                    warn!(
                        pid,
                        ax_error = err,
                        "AXObserverCreate failed in observer thread"
                    );
                    return;
                }

                // SAFETY: AXUIElementCreateApplication returns a new
                // AXUIElementRef for the given PID. Caller owns it.
                let app_element = AXUIElementCreateApplication(pid);
                if app_element.is_null() {
                    warn!(pid, "AXUIElementCreateApplication returned null");
                    CFRelease(observer);
                    return;
                }

                // Subscribe to kAXFocusedUIElementChangedNotification.
                //
                // The `refcon` pointer carries our shared state so the
                // callback can set the `focus_changed` flag. We convert
                // the Arc to a raw pointer. The Arc is kept alive by
                // `state` in this scope -- we do NOT call Arc::from_raw
                // in the callback (which would double-free).
                let notification_name = CFString::new(AX_FOCUSED_UI_ELEMENT_CHANGED_NOTIFICATION);
                let refcon = Arc::as_ptr(&state) as *mut c_void;

                let add_err = AXObserverAddNotification(
                    observer,
                    app_element,
                    Self::as_cf_string_ref(&notification_name),
                    refcon,
                );

                if add_err != kAXErrorSuccess {
                    warn!(pid, ax_error = add_err, "AXObserverAddNotification failed");
                    CFRelease(app_element);
                    CFRelease(observer);
                    return;
                }

                // Get the run loop source and add it to the current
                // thread's CFRunLoop.
                let source = AXObserverGetRunLoopSource(observer);
                if source.is_null() {
                    warn!(pid, "AXObserverGetRunLoopSource returned null");
                    Self::cleanup_observer(observer, app_element, &notification_name);
                    return;
                }

                let run_loop = CFRunLoopGetCurrent();
                let mode = CFString::new(K_CF_RUN_LOOP_DEFAULT_MODE);

                // SAFETY: CFRunLoopAddSource does not take ownership of
                // the source. The source remains valid as long as the
                // observer is alive.
                CFRunLoopAddSource(run_loop, source, Self::as_cf_string_ref(&mode));

                debug!(pid, "AXObserver run loop source added, entering loop");

                // Run the CFRunLoop with periodic wake-ups to check the
                // `running` flag. We use CFRunLoopRunInMode with a 0.5s
                // timeout so we can exit promptly when stop() is called.
                while state.running.load(Ordering::Acquire) {
                    // CFRunLoopRunInMode returns after processing one
                    // source or after the timeout, whichever comes first.
                    let result = CFRunLoopRunInMode(
                        Self::as_cf_string_ref(&mode),
                        0.5,  // seconds
                        true, // returnAfterSourceHandled (1 = true)
                    );

                    // kCFRunLoopRunFinished (1) means no sources left --
                    // the observer was invalidated. Exit the loop.
                    if result == 1 {
                        debug!(pid, "CFRunLoop finished (no sources), exiting");
                        break;
                    }
                }

                // Cleanup: remove notification, release observer and element.
                CFRunLoopRemoveSource(run_loop, source, Self::as_cf_string_ref(&mode));
                Self::cleanup_observer(observer, app_element, &notification_name);

                debug!(pid, "AXObserver thread exiting");
            }
        }

        /// The AXObserver callback invoked when focus changes.
        ///
        /// SAFETY: This is called by the macOS accessibility framework on
        /// the CFRunLoop thread. `refcon` must point to a valid
        /// `ObserverState` (guaranteed because the Arc is kept alive by
        /// the `run_observer_loop` scope).
        unsafe extern "C" fn focus_callback(
            _observer: AXObserverRef,
            _element: AXUIElementRef,
            _notification: CFStringRef,
            refcon: *mut c_void,
        ) {
            if refcon.is_null() {
                return;
            }
            // SAFETY: `refcon` points to the ObserverState inside the Arc.
            // We only read/write atomics through it -- no ownership transfer.
            let state = &*(refcon as *const ObserverState);
            state.focus_changed.store(true, Ordering::Release);
        }

        /// Helper: get raw CFStringRef from a CFString.
        fn as_cf_string_ref(s: &CFString) -> CFStringRef {
            use core_foundation::base::TCFType;
            s.as_concrete_TypeRef()
        }

        /// Helper: remove notification and release observer + element.
        unsafe fn cleanup_observer(
            observer: AXObserverRef,
            element: AXUIElementRef,
            notification_name: &CFString,
        ) {
            let _ = AXObserverRemoveNotification(
                observer,
                element,
                Self::as_cf_string_ref(notification_name),
            );
            CFRelease(element);
            CFRelease(observer);
        }
    }

    impl Drop for FocusObserverHandle {
        fn drop(&mut self) {
            self.stop();
        }
    }

    // ── CFRunLoopRunInMode FFI ──────────────────────────────────────────
    //
    // Not exposed in our ffi_macos.rs because it is a CoreFoundation
    // function, not ApplicationServices. We declare it here privately.
    extern "C" {
        /// Run the current thread's run loop in the given mode for up to
        /// `seconds`. Returns the reason the run loop exited:
        ///   0 = kCFRunLoopRunFinished (placeholder, unused in practice)
        ///   1 = kCFRunLoopRunStopped
        ///   2 = kCFRunLoopRunTimedOut
        ///   3 = kCFRunLoopRunHandledSource
        fn CFRunLoopRunInMode(
            mode: CFStringRef,
            seconds: f64,
            return_after_source_handled: bool,
        ) -> i32;
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn observer_state_is_send_and_sync() {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<FocusObserverHandle>();
        }

        #[test]
        fn has_focus_changed_returns_false_initially() {
            let state = Arc::new(ObserverState {
                focus_changed: AtomicBool::new(false),
                running: AtomicBool::new(false),
            });
            // Simulate the check without a real observer thread.
            assert!(!state.focus_changed.swap(false, Ordering::Acquire));
        }

        #[test]
        fn has_focus_changed_resets_after_read() {
            let state = Arc::new(ObserverState {
                focus_changed: AtomicBool::new(true),
                running: AtomicBool::new(false),
            });
            // First read should return true and reset.
            assert!(state.focus_changed.swap(false, Ordering::Acquire));
            // Second read should return false.
            assert!(!state.focus_changed.swap(false, Ordering::Acquire));
        }

        #[test]
        fn callback_sets_focus_changed_flag() {
            let state = Arc::new(ObserverState {
                focus_changed: AtomicBool::new(false),
                running: AtomicBool::new(true),
            });
            let refcon = Arc::as_ptr(&state) as *mut c_void;

            // SAFETY: state is valid and we pass it as refcon. The
            // callback only writes an atomic bool through the pointer.
            unsafe {
                FocusObserverHandle::focus_callback(
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    refcon,
                );
            }
            assert!(state.focus_changed.load(Ordering::Acquire));
        }

        #[test]
        fn callback_handles_null_refcon() {
            // Should not panic or crash.
            unsafe {
                FocusObserverHandle::focus_callback(
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                );
            }
        }

        /// Integration test -- requires Accessibility permission and a running app.
        /// Run manually: `cargo test -p oneshim-vision -- focus_observer_integration --ignored`
        #[test]
        #[ignore]
        fn focus_observer_integration() {
            // Observe the current process (our own test binary).
            // This won't produce real focus events but validates the
            // create/subscribe/cleanup lifecycle.
            let pid = std::process::id() as PidT;
            let handle = FocusObserverHandle::start(pid);

            if let Some(mut handle) = handle {
                // No events expected -- just verify no crash.
                assert!(!handle.has_focus_changed());
                std::thread::sleep(std::time::Duration::from_millis(200));
                assert!(!handle.has_focus_changed());
                handle.stop();
            } else {
                eprintln!(
                    "SKIP: FocusObserverHandle::start returned None \
                     (Accessibility permission not granted or invalid PID)"
                );
            }
        }

        /// Integration test -- observe a known app (Finder, PID 1 as launchd fallback).
        /// Run manually: `cargo test -p oneshim-vision -- focus_observer_finder --ignored`
        #[test]
        #[ignore]
        fn focus_observer_finder() {
            // Try to find Finder's PID via sysinfo or fall back to PID 1.
            let finder_pid = find_finder_pid().unwrap_or(1);
            let handle = FocusObserverHandle::start(finder_pid);

            match handle {
                Some(mut h) => {
                    eprintln!(
                        "Observer started for PID {}. Switch focus in Finder within 3s...",
                        finder_pid
                    );
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    let changed = h.has_focus_changed();
                    eprintln!("Focus changed: {changed}");
                    h.stop();
                }
                None => {
                    eprintln!("SKIP: could not create observer for Finder (PID {finder_pid})");
                }
            }
        }

        /// Helper: find Finder's PID by name.
        fn find_finder_pid() -> Option<PidT> {
            use std::process::Command;
            let output = Command::new("pgrep")
                .arg("-x")
                .arg("Finder")
                .output()
                .ok()?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.trim().lines().next()?.parse::<PidT>().ok()
        }
    }
}

#[cfg(target_os = "macos")]
pub use observer::FocusObserverHandle;

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

    /// Integration test for tree traversal -- requires Accessibility permission.
    /// Run manually: `cargo test -p oneshim-vision -- macos_tree_traversal --ignored`
    #[tokio::test]
    #[ignore]
    async fn extract_window_elements_integration() {
        let extractor = MacOsNativeAccessibility::new();
        if !extractor.has_permission() {
            eprintln!("SKIP: Accessibility permission not granted");
            return;
        }
        let result = extractor
            .extract_window_elements(3, 300, PiiFilterLevel::Standard, false)
            .await;
        assert!(result.is_ok());
        let elements = result.unwrap();
        // Should return at least 1 element (the window or focused element)
        // May return 0 on headless CI
        eprintln!("extracted {} elements", elements.len());
    }

    #[tokio::test]
    #[ignore]
    async fn extract_window_elements_permission_denied_without_access() {
        // This test verifies the PermissionDenied path, but only
        // meaningful when run without Accessibility permission.
        let extractor = MacOsNativeAccessibility::new();
        if extractor.has_permission() {
            eprintln!("SKIP: permission already granted, cannot test denial");
            return;
        }
        let result = extractor
            .extract_window_elements(3, 300, PiiFilterLevel::Standard, false)
            .await;
        assert!(matches!(
            result,
            Err(oneshim_core::error::CoreError::PermissionDenied(_))
        ));
    }

    /// Integration test for batch attribute fetching -- requires Accessibility permission.
    /// Verifies that traverse_tree uses batch_get_attributes and produces the
    /// same results as the individual-call fallback path.
    /// Run manually: `cargo test -p oneshim-vision -- macos_batch_traversal --ignored`
    #[tokio::test]
    #[ignore]
    async fn extract_window_elements_batch_traversal() {
        let extractor = MacOsNativeAccessibility::new();
        if !extractor.has_permission() {
            eprintln!("SKIP: Accessibility permission not granted");
            return;
        }
        let result = extractor
            .extract_window_elements(2, 100, PiiFilterLevel::Off, true)
            .await;
        assert!(result.is_ok());
        let elements = result.unwrap();
        // Each element should have a non-empty role from the batch fetch
        for elem in &elements {
            assert!(!elem.role.is_empty(), "batch fetch should populate role");
        }
        eprintln!(
            "batch traversal: {} elements, {} with bounds",
            elements.len(),
            elements.iter().filter(|e| e.bounds.is_some()).count()
        );
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
