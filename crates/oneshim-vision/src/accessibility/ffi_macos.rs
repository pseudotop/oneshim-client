//! Raw FFI bindings for macOS Accessibility API functions.
//!
//! These functions are not exposed by the `core-foundation` crate.
//! We link to the ApplicationServices framework which provides them.
//!
//! Reference: Apple Developer Documentation -- Accessibility Reference

#![allow(non_snake_case, non_upper_case_globals)]

#[cfg(target_os = "macos")]
#[allow(dead_code)] // consumed by sibling `macos.rs` module
pub(crate) mod ax {
    use std::ffi::c_void;

    use core_foundation_sys::base::CFTypeRef;
    use core_foundation_sys::string::CFStringRef;

    /// Opaque type for an accessibility element (same layout as CFTypeRef).
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

    // Extract a CGPoint / CGSize from an AXValue.
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXValueGetValue(value: CFTypeRef, value_type: u32, value_ptr: *mut c_void) -> bool;
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
