//!
//!

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use tracing::{debug, info, warn};

fn get_mtm() -> Option<MainThreadMarker> {
    MainThreadMarker::new()
}

///
pub fn hide_app() {
    let Some(mtm) = get_mtm() else {
        warn!("macOS: app hide failure");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    app.hide(None);
    info!("macOS: app (NSApplication.hide)");
}

///
#[allow(deprecated)]
#[allow(unused_unsafe)]
pub fn show_app() {
    let Some(mtm) = get_mtm() else {
        warn!("macOS: app display failure");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    unsafe { app.unhide(None) };
    app.activateIgnoringOtherApps(true);
    info!("macOS: app display (NSApplication.unhide + activate)");
}

#[allow(unused_unsafe)]
pub fn is_app_hidden() -> bool {
    let Some(mtm) = get_mtm() else {
        warn!("macOS: state check failure");
        return false;
    };

    let app = NSApplication::sharedApplication(mtm);
    let hidden = unsafe { app.isHidden() };
    debug!("macOS: app state = {}", hidden);
    hidden
}

///
#[allow(dead_code)]
pub fn set_accessory_mode() {
    let Some(mtm) = get_mtm() else {
        warn!("macOS: mode change failure");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    info!("macOS: Accessory mode settings (Dock )");
}

#[allow(dead_code)]
pub fn set_regular_mode() {
    let Some(mtm) = get_mtm() else {
        warn!("macOS: mode change failure");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    info!("macOS: Regular mode settings (Dock display)");
}

#[cfg(test)]
mod tests {
}
