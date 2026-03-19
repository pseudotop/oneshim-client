//! Raw FFI bindings for macOS Accessibility API functions.
//!
//! These functions are not exposed by the `core-foundation` crate.
//! We link to the ApplicationServices framework which provides them.
//!
//! Attribute key constants are constructed as `CFString` at call time rather
//! than imported as extern statics, because the Rust linker does not reliably
//! resolve `kAX*Attribute` symbols from the ApplicationServices `.tbd` stubs
//! in all toolchain versions.
//!
//! Reference: Apple Developer Documentation -- Accessibility Reference

#![allow(non_snake_case, non_upper_case_globals)]

#[cfg(target_os = "macos")]
#[allow(dead_code)] // consumed by sibling `macos.rs` module
pub(crate) mod ax {
    use std::ffi::c_void;

    use core_foundation::string::CFString;
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

    // Attribute key string values -- constructed at call time.
    // These correspond to the Apple-defined kAX*Attribute constants.
    pub const AX_FOCUSED_UI_ELEMENT_ATTR: &str = "AXFocusedUIElement";
    pub const AX_ROLE_ATTR: &str = "AXRole";
    pub const AX_TITLE_ATTR: &str = "AXTitle";
    pub const AX_VALUE_ATTR: &str = "AXValue";
    pub const AX_DESCRIPTION_ATTR: &str = "AXDescription";
    pub const AX_POSITION_ATTR: &str = "AXPosition";
    pub const AX_SIZE_ATTR: &str = "AXSize";
    pub const AX_PLACEHOLDER_VALUE_ATTR: &str = "AXPlaceholderValue";

    /// Create a CFStringRef from a Rust string constant.
    /// The returned CFString is autoreleased/owned by the caller through
    /// the core-foundation wrapper.
    pub fn ax_attr(name: &str) -> CFString {
        CFString::new(name)
    }

    /// Get the raw CFStringRef pointer from a CFString for use with AX API.
    pub fn as_cf_ref(s: &CFString) -> CFStringRef {
        use core_foundation::base::TCFType;
        s.as_concrete_TypeRef()
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
