//! Windows key event observer using Raw Input.
//!
//! Registers a Raw Input keyboard device listener using
//! RegisterRawInputDevices with RIDEV_INPUTSINK. This receives all
//! keyboard input system-wide without blocking or modifying events.
//!
//! Runs on a dedicated std::thread with its own message loop.
//!
//! **Status**: Stub implementation. Full implementation pending Windows
//! build/test environment. Logs a warning and returns immediately.

use crate::input_activity::InputActivityCollector;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::warn;

/// Run the Raw Input keyboard hook. Blocks until `running` becomes false.
///
/// Currently a stub that logs a platform warning. Full Raw Input
/// implementation requires testing on a Windows machine with the
/// Win32_UI_Input and Win32_Devices_HumanInterfaceDevice features.
pub fn run_raw_input_hook(collector: Arc<InputActivityCollector>, running: Arc<AtomicBool>) {
    let _ = (collector, running);
    warn!("Windows Raw Input key hook not yet implemented -- platform hook not active");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_does_not_panic() {
        let collector = Arc::new(InputActivityCollector::new());
        let running = Arc::new(AtomicBool::new(false));
        // Should return immediately without panic
        run_raw_input_hook(collector, running);
    }
}
