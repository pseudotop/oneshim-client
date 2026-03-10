//! macOS-specific runtime integration for bare binary (non-.app bundle) execution.
//!
//! When the ONESHIM binary runs directly (e.g., installed via `install.sh`),
//! macOS has no Info.plist to read the dock icon from. This module sets it
//! programmatically via NSApplication API.

/// Set the macOS dock icon from the embedded icon.png at runtime.
///
/// Bare binaries (not .app bundles) don't have Info.plist, so macOS shows
/// a generic "exec" icon in the dock. This fixes that by calling
/// `[NSApplication setApplicationIconImage:]` with the brand icon.
pub fn set_dock_icon() {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    let icon_bytes: &[u8] = include_bytes!("../icons/dock_icon.png");

    unsafe {
        let ns_data_class = match Class::get("NSData") {
            Some(c) => c,
            None => return,
        };
        let ns_image_class = match Class::get("NSImage") {
            Some(c) => c,
            None => return,
        };
        let ns_app_class = match Class::get("NSApplication") {
            Some(c) => c,
            None => return,
        };

        let data: *mut Object = msg_send![
            ns_data_class,
            dataWithBytes:icon_bytes.as_ptr()
            length:icon_bytes.len()
        ];
        if data.is_null() {
            return;
        }

        let image: *mut Object = msg_send![ns_image_class, alloc];
        let image: *mut Object = msg_send![image, initWithData:data];
        if image.is_null() {
            return;
        }

        let app: *mut Object = msg_send![ns_app_class, sharedApplication];
        if !app.is_null() {
            let _: () = msg_send![app, setApplicationIconImage:image];
        }
    }
}
