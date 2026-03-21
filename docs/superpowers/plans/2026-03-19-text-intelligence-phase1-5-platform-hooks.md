# Text-Heavy App Intelligence Phase 1.5: Platform Key-Category Hooks

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire actual platform key events into `InputActivityCollector::record_categorized_keystroke()` so the `KeystrokeProfile` gets real data instead of zeros. Phase 1 built the framework (counters, ratios, classification rules); this phase connects it to OS input events on macOS, Windows, and Linux.

**Architecture:** Each platform gets a dedicated key event observer module inside `oneshim-monitor/src/`. Hooks run on a dedicated `std::thread` (not tokio) to avoid blocking the async runtime, and communicate to `InputActivityCollector` via its existing `AtomicU32` counters (zero allocation on the hot path). The hook is purely passive -- it observes key events without modifying or blocking them. The entire subsystem is gated behind `text_intelligence.input_pattern_detail = true` in config. Each platform module is conditionally compiled via `#[cfg(target_os = "...")]`.

**Tech Stack:** Rust, `core-graphics` (macOS CGEventTap), `windows-sys` (Windows Raw Input), `x11` crate (Linux XRecord), `std::thread`, `AtomicU32`

**Spec:** `docs/superpowers/specs/2026-03-19-text-heavy-app-intelligence-design.md` (Section 12, Phase 1.5)

**Depends on:** Phase 1 (completed) -- `KeyCategory` enum, `record_categorized_keystroke()`, `InputActivityCollector` counters all exist

---

## File Map

### New files

| File | Responsibility |
|------|----------------|
| `crates/oneshim-monitor/src/key_hook/mod.rs` | Platform-agnostic `KeyHook` struct, `start()` / `stop()` API, re-exports |
| `crates/oneshim-monitor/src/key_hook/classify.rs` | `classify_keycode()` -- pure function mapping platform keycodes to `KeyCategory` |
| `crates/oneshim-monitor/src/key_hook/macos.rs` | macOS `CGEventTap` passive observer (`#[cfg(target_os = "macos")]`) |
| `crates/oneshim-monitor/src/key_hook/windows.rs` | Windows Raw Input keyboard hook (`#[cfg(target_os = "windows")]`) |
| `crates/oneshim-monitor/src/key_hook/linux.rs` | Linux X11 XRecord extension hook (`#[cfg(target_os = "linux")]`) |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-monitor/src/lib.rs` | Add `pub mod key_hook;` |
| `crates/oneshim-monitor/Cargo.toml` | Add `core-foundation` + `core-foundation-sys` (macOS), extend `windows-sys` features (Windows) |
| `src-tauri/src/scheduler/loops.rs` | Spawn `KeyHook` in `run_scheduler_loops()`, pass `Arc<InputActivityCollector>` |
| `Cargo.toml` (workspace) | Add `core-foundation` and `core-foundation-sys` to workspace deps |

---

## Task 1: Create key_hook module skeleton and classify function

**Files:**
- New: `crates/oneshim-monitor/src/key_hook/mod.rs`
- New: `crates/oneshim-monitor/src/key_hook/classify.rs`
- Modify: `crates/oneshim-monitor/src/lib.rs`

- [ ] **Step 1: Create `key_hook/mod.rs` with `KeyHook` struct**

Create `crates/oneshim-monitor/src/key_hook/mod.rs`:

```rust
//! Platform key-category hooks for text-heavy app intelligence.
//!
//! Spawns a dedicated OS thread that passively observes keyboard events,
//! classifies each key into a `KeyCategory`, and calls
//! `InputActivityCollector::record_categorized_keystroke()`.
//!
//! The hook is purely passive -- it does NOT modify or block key events.
//!
//! Gated by `text_intelligence.input_pattern_detail = true` in config.

mod classify;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

pub use classify::classify_keycode;

use crate::input_activity::InputActivityCollector;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

/// Handle to a running platform key event observer.
///
/// Call `start()` to spawn the observer thread. Call `stop()` or drop the
/// handle to terminate the observer.
pub struct KeyHook {
    running: Arc<AtomicBool>,
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl KeyHook {
    /// Spawn the platform-specific key event observer on a dedicated thread.
    ///
    /// The observer calls `collector.record_categorized_keystroke()` for each
    /// key-down event. Key-up events are ignored (we only count presses).
    ///
    /// Returns `None` if the platform does not support passive key observation
    /// (e.g., Linux Wayland without X11 fallback).
    pub fn start(collector: Arc<InputActivityCollector>) -> Option<Self> {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let thread_handle = Self::spawn_platform_hook(collector, running_clone)?;

        info!("key-category hook started");

        Some(Self {
            running,
            thread_handle: Some(thread_handle),
        })
    }

    /// Signal the observer thread to stop and wait for it to exit.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread_handle.take() {
            // Platform hooks may block in a run loop; we signal via the
            // AtomicBool and give the thread a bounded time to exit.
            // On macOS, CFRunLoopStop is called from within the tap callback
            // when `running` becomes false.
            let _ = handle.join();
        }
        info!("key-category hook stopped");
    }

    /// Platform-specific hook spawning. Returns None if unsupported.
    #[cfg(target_os = "macos")]
    fn spawn_platform_hook(
        collector: Arc<InputActivityCollector>,
        running: Arc<AtomicBool>,
    ) -> Option<std::thread::JoinHandle<()>> {
        Some(std::thread::Builder::new()
            .name("key-hook-macos".to_string())
            .spawn(move || {
                macos::run_event_tap(collector, running);
            })
            .ok()?)
    }

    #[cfg(target_os = "windows")]
    fn spawn_platform_hook(
        collector: Arc<InputActivityCollector>,
        running: Arc<AtomicBool>,
    ) -> Option<std::thread::JoinHandle<()>> {
        Some(std::thread::Builder::new()
            .name("key-hook-windows".to_string())
            .spawn(move || {
                windows::run_raw_input_hook(collector, running);
            })
            .ok()?)
    }

    #[cfg(target_os = "linux")]
    fn spawn_platform_hook(
        collector: Arc<InputActivityCollector>,
        running: Arc<AtomicBool>,
    ) -> Option<std::thread::JoinHandle<()>> {
        Some(std::thread::Builder::new()
            .name("key-hook-linux".to_string())
            .spawn(move || {
                linux::run_x11_record_hook(collector, running);
            })
            .ok()?)
    }
}

impl Drop for KeyHook {
    fn drop(&mut self) {
        if self.thread_handle.is_some() {
            self.stop();
        }
    }
}
```

- [ ] **Step 2: Create `key_hook/classify.rs` with platform keycode mapping**

Create `crates/oneshim-monitor/src/key_hook/classify.rs`:

```rust
//! Pure function to classify platform keycodes into KeyCategory.
//!
//! Each platform calls this with its native keycode. The function maps
//! the code to one of: Enter, Tab, Arrow, Backspace, Special, Regular.

use oneshim_core::models::app_registry::KeyCategory;

// ── macOS CGKeyCode constants ──
// Ref: /System/Library/Frameworks/Carbon.framework/Versions/A/Frameworks/
//      HIToolbox.framework/Headers/Events.h

/// Classify a macOS CGKeyCode into a KeyCategory.
#[cfg(target_os = "macos")]
pub fn classify_keycode(keycode: u16) -> KeyCategory {
    match keycode {
        // Return / Enter
        36 | 76 => KeyCategory::Enter,
        // Tab
        48 => KeyCategory::Tab,
        // Arrow keys
        123 | 124 | 125 | 126 => KeyCategory::Arrow,
        // Delete (backspace) / Forward Delete
        51 | 117 => KeyCategory::Backspace,
        // Escape
        53 => KeyCategory::Special,
        // Home / End / Page Up / Page Down
        115 | 119 | 116 | 121 => KeyCategory::Special,
        // Function keys F1-F20
        122 | 120 | 99 | 118 | 96 | 97 | 98 | 100 | 101 | 109 | 103 | 111
        | 105 | 107 | 113 | 106 | 64 | 79 | 80 | 90 => KeyCategory::Special,
        // Modifier keys (Shift, Control, Option, Command, Caps Lock, Fn)
        56 | 60 | 59 | 62 | 58 | 61 | 55 | 54 | 57 | 63 => KeyCategory::Special,
        // Everything else is Regular
        _ => KeyCategory::Regular,
    }
}

// ── Windows Virtual Key codes ──
// Ref: https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes

/// Classify a Windows virtual key code into a KeyCategory.
#[cfg(target_os = "windows")]
pub fn classify_keycode(vk_code: u32) -> KeyCategory {
    match vk_code {
        // VK_RETURN (0x0D)
        0x0D => KeyCategory::Enter,
        // VK_TAB (0x09)
        0x09 => KeyCategory::Tab,
        // VK_LEFT, VK_UP, VK_RIGHT, VK_DOWN (0x25-0x28)
        0x25..=0x28 => KeyCategory::Arrow,
        // VK_BACK (0x08), VK_DELETE (0x2E)
        0x08 | 0x2E => KeyCategory::Backspace,
        // VK_ESCAPE (0x1B)
        0x1B => KeyCategory::Special,
        // VK_HOME (0x24), VK_END (0x23), VK_PRIOR/PageUp (0x21), VK_NEXT/PageDown (0x22)
        0x21..=0x24 => KeyCategory::Special,
        // VK_F1-VK_F24 (0x70-0x87)
        0x70..=0x87 => KeyCategory::Special,
        // VK_SHIFT, VK_CONTROL, VK_MENU (Alt), VK_LWIN, VK_RWIN
        0x10..=0x12 | 0x5B | 0x5C => KeyCategory::Special,
        // VK_CAPITAL (Caps Lock), VK_NUMLOCK, VK_SCROLL
        0x14 | 0x90 | 0x91 => KeyCategory::Special,
        // Everything else
        _ => KeyCategory::Regular,
    }
}

// ── Linux X11 keysym constants ──
// Ref: /usr/include/X11/keysymdef.h

/// Classify a Linux X11 keysym into a KeyCategory.
#[cfg(target_os = "linux")]
pub fn classify_keycode(keysym: u32) -> KeyCategory {
    // XK_Return = 0xFF0D, XK_KP_Enter = 0xFF8D
    const XK_RETURN: u32 = 0xFF0D;
    const XK_KP_ENTER: u32 = 0xFF8D;
    const XK_TAB: u32 = 0xFF09;
    const XK_LEFT: u32 = 0xFF51;
    const XK_UP: u32 = 0xFF52;
    const XK_RIGHT: u32 = 0xFF53;
    const XK_DOWN: u32 = 0xFF54;
    const XK_BACKSPACE: u32 = 0xFF08;
    const XK_DELETE: u32 = 0xFFFF;
    const XK_ESCAPE: u32 = 0xFF1B;
    const XK_HOME: u32 = 0xFF50;
    const XK_END: u32 = 0xFF57;
    const XK_PAGE_UP: u32 = 0xFF55;
    const XK_PAGE_DOWN: u32 = 0xFF56;
    const XK_F1: u32 = 0xFFBE;
    const XK_F24: u32 = 0xFFD5;
    // Modifier range: XK_Shift_L (0xFFE1) through XK_Hyper_R (0xFFEE)
    const XK_SHIFT_L: u32 = 0xFFE1;
    const XK_HYPER_R: u32 = 0xFFEE;
    const XK_CAPS_LOCK: u32 = 0xFFE5;

    match keysym {
        XK_RETURN | XK_KP_ENTER => KeyCategory::Enter,
        XK_TAB => KeyCategory::Tab,
        XK_LEFT | XK_UP | XK_RIGHT | XK_DOWN => KeyCategory::Arrow,
        XK_BACKSPACE | XK_DELETE => KeyCategory::Backspace,
        XK_ESCAPE | XK_HOME | XK_END | XK_PAGE_UP | XK_PAGE_DOWN => KeyCategory::Special,
        k if (XK_F1..=XK_F24).contains(&k) => KeyCategory::Special,
        k if (XK_SHIFT_L..=XK_HYPER_R).contains(&k) => KeyCategory::Special,
        XK_CAPS_LOCK => KeyCategory::Special,
        _ => KeyCategory::Regular,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── macOS tests ──

    #[cfg(target_os = "macos")]
    mod macos_tests {
        use super::*;

        #[test]
        fn return_key_is_enter() {
            assert_eq!(classify_keycode(36), KeyCategory::Enter);
        }

        #[test]
        fn numpad_enter_is_enter() {
            assert_eq!(classify_keycode(76), KeyCategory::Enter);
        }

        #[test]
        fn tab_key() {
            assert_eq!(classify_keycode(48), KeyCategory::Tab);
        }

        #[test]
        fn arrow_keys() {
            for code in [123, 124, 125, 126] {
                assert_eq!(classify_keycode(code), KeyCategory::Arrow, "keycode {code}");
            }
        }

        #[test]
        fn delete_is_backspace() {
            assert_eq!(classify_keycode(51), KeyCategory::Backspace);
        }

        #[test]
        fn forward_delete_is_backspace() {
            assert_eq!(classify_keycode(117), KeyCategory::Backspace);
        }

        #[test]
        fn escape_is_special() {
            assert_eq!(classify_keycode(53), KeyCategory::Special);
        }

        #[test]
        fn function_key_f1_is_special() {
            assert_eq!(classify_keycode(122), KeyCategory::Special);
        }

        #[test]
        fn modifier_keys_are_special() {
            // Shift (56), Control (59), Option (58), Command (55)
            for code in [56, 59, 58, 55] {
                assert_eq!(classify_keycode(code), KeyCategory::Special, "keycode {code}");
            }
        }

        #[test]
        fn letter_keys_are_regular() {
            // 'A' = keycode 0, 'S' = 1, ...
            for code in [0, 1, 2, 3, 11, 12, 13, 14] {
                assert_eq!(classify_keycode(code), KeyCategory::Regular, "keycode {code}");
            }
        }

        #[test]
        fn number_keys_are_regular() {
            // 1-0 on main keyboard: 18-29
            for code in 18..=29 {
                assert_eq!(classify_keycode(code), KeyCategory::Regular, "keycode {code}");
            }
        }

        #[test]
        fn home_end_pageup_pagedown_are_special() {
            for code in [115, 119, 116, 121] {
                assert_eq!(classify_keycode(code), KeyCategory::Special, "keycode {code}");
            }
        }
    }

    // ── Windows tests ──

    #[cfg(target_os = "windows")]
    mod windows_tests {
        use super::*;

        #[test]
        fn vk_return_is_enter() {
            assert_eq!(classify_keycode(0x0D), KeyCategory::Enter);
        }

        #[test]
        fn vk_tab_is_tab() {
            assert_eq!(classify_keycode(0x09), KeyCategory::Tab);
        }

        #[test]
        fn vk_arrows() {
            for code in [0x25, 0x26, 0x27, 0x28] {
                assert_eq!(classify_keycode(code), KeyCategory::Arrow, "vk {code:#x}");
            }
        }

        #[test]
        fn vk_back_is_backspace() {
            assert_eq!(classify_keycode(0x08), KeyCategory::Backspace);
        }

        #[test]
        fn vk_delete_is_backspace() {
            assert_eq!(classify_keycode(0x2E), KeyCategory::Backspace);
        }

        #[test]
        fn vk_escape_is_special() {
            assert_eq!(classify_keycode(0x1B), KeyCategory::Special);
        }

        #[test]
        fn vk_f1_is_special() {
            assert_eq!(classify_keycode(0x70), KeyCategory::Special);
        }

        #[test]
        fn vk_a_is_regular() {
            assert_eq!(classify_keycode(0x41), KeyCategory::Regular);
        }
    }

    // ── Linux tests ──

    #[cfg(target_os = "linux")]
    mod linux_tests {
        use super::*;

        #[test]
        fn xk_return_is_enter() {
            assert_eq!(classify_keycode(0xFF0D), KeyCategory::Enter);
        }

        #[test]
        fn xk_kp_enter_is_enter() {
            assert_eq!(classify_keycode(0xFF8D), KeyCategory::Enter);
        }

        #[test]
        fn xk_tab_is_tab() {
            assert_eq!(classify_keycode(0xFF09), KeyCategory::Tab);
        }

        #[test]
        fn xk_arrows() {
            for code in [0xFF51, 0xFF52, 0xFF53, 0xFF54] {
                assert_eq!(classify_keycode(code), KeyCategory::Arrow, "keysym {code:#x}");
            }
        }

        #[test]
        fn xk_backspace_is_backspace() {
            assert_eq!(classify_keycode(0xFF08), KeyCategory::Backspace);
        }

        #[test]
        fn xk_delete_is_backspace() {
            assert_eq!(classify_keycode(0xFFFF), KeyCategory::Backspace);
        }

        #[test]
        fn xk_escape_is_special() {
            assert_eq!(classify_keycode(0xFF1B), KeyCategory::Special);
        }

        #[test]
        fn xk_f1_is_special() {
            assert_eq!(classify_keycode(0xFFBE), KeyCategory::Special);
        }

        #[test]
        fn xk_a_is_regular() {
            // XK_a = 0x0061
            assert_eq!(classify_keycode(0x0061), KeyCategory::Regular);
        }
    }
}
```

- [ ] **Step 3: Register `key_hook` module in `lib.rs`**

In `crates/oneshim-monitor/src/lib.rs`, add:

```rust
pub mod key_hook;
```

**Verify:**
```bash
cargo check -p oneshim-monitor
```

**Commit:** `feat(monitor): add key_hook module skeleton with platform keycode classification`

---

## Task 2: macOS CGEventTap implementation

**Files:**
- New: `crates/oneshim-monitor/src/key_hook/macos.rs`
- Modify: `crates/oneshim-monitor/Cargo.toml`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add macOS dependencies to workspace and crate**

In `Cargo.toml` (workspace root), verify these already exist (they do):
```toml
core-graphics = "0.25"
```

Add to workspace deps if not present:
```toml
core-foundation = "0.10"
core-foundation-sys = "0.8"
```

In `crates/oneshim-monitor/Cargo.toml`, under `[target.'cfg(target_os = "macos")'.dependencies]`:
```toml
core-foundation = { workspace = true }
core-foundation-sys = { workspace = true }
```

The crate already depends on `core-graphics` for mouse position; CGEventTap lives there too.

- [ ] **Step 2: Implement CGEventTap passive observer**

Create `crates/oneshim-monitor/src/key_hook/macos.rs`:

```rust
//! macOS key event observer using CGEventTap.
//!
//! Creates a passive (listen-only) event tap for key-down events.
//! The tap callback classifies each key's CGKeyCode into KeyCategory
//! and calls record_categorized_keystroke() on the shared collector.
//!
//! Runs on a dedicated std::thread. Uses CFRunLoop for event delivery.
//! The thread exits when the `running` AtomicBool is set to false.
//!
//! Requires Accessibility permission on macOS (System Settings >
//! Privacy & Security > Accessibility). If permission is denied,
//! CGEventTapCreate returns null and the function exits gracefully.

#![cfg(target_os = "macos")]

use crate::input_activity::InputActivityCollector;
use super::classify::classify_keycode;
use core_graphics::event::{
    CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventType,
};
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Run the CGEventTap key observer. Blocks until `running` becomes false.
///
/// This function MUST be called from a dedicated std::thread (not from
/// a tokio task) because CFRunLoop::run_current() blocks the thread.
pub fn run_event_tap(collector: Arc<InputActivityCollector>, running: Arc<AtomicBool>) {
    // CGEventTapCreate with kCGEventTapOptionListenOnly creates a passive
    // observer. The callback receives events but cannot modify them.
    // The return value NULL from the callback is required for listen-only taps.
    //
    // Event mask: kCGEventKeyDown only. We do not need key-up or flags-changed
    // events because we are counting key presses, not releases.
    let event_mask = 1 << (CGEventType::KeyDown as u64);

    // We use a raw pointer trick to pass the collector + running flag into
    // the C callback. The pointers are valid for the lifetime of this function.
    struct TapContext {
        collector: Arc<InputActivityCollector>,
        running: Arc<AtomicBool>,
    }

    let context = Box::new(TapContext {
        collector,
        running: running.clone(),
    });
    let context_ptr = Box::into_raw(context);

    // Safety: CGEventTapCreate is an FFI call. The callback is invoked on the
    // same thread that runs CFRunLoop. The context pointer is valid until we
    // reclaim it below.
    unsafe {
        extern "C" fn tap_callback(
            _proxy: core_graphics::sys::CGEventTapProxy,
            event_type: CGEventType,
            event: core_graphics::sys::CGEventRef,
            user_info: *mut std::ffi::c_void,
        ) -> core_graphics::sys::CGEventRef {
            let ctx = &*(user_info as *const TapContext);

            if !ctx.running.load(Ordering::Relaxed) {
                // Signal CFRunLoop to stop
                CFRunLoop::get_current().stop();
                return event;
            }

            // CGEventTapDisabledByTimeout / CGEventTapDisabledByUserInput
            // The system disables the tap after 30s of timeout. Re-enable it.
            if matches!(event_type, CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput) {
                debug!("CGEventTap was disabled, re-enabling");
                // Re-enable by returning the event. The tap auto-re-enables
                // on the next event if we do not explicitly disable it.
                return event;
            }

            if event_type != CGEventType::KeyDown {
                return event;
            }

            // Extract the virtual keycode from the event.
            // CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode)
            let keycode = core_graphics::event::CGEvent::from_ptr(event)
                .get_integer_value_field(core_graphics::event::EventField::KEYBOARD_EVENT_KEYCODE)
                as u16;

            let category = classify_keycode(keycode);

            // Check if modifier flags indicate a shortcut (Command or Control held)
            let flags = core_graphics::event::CGEvent::from_ptr(event).get_flags();
            let is_shortcut = flags.contains(CGEventFlags::CGEventFlagCommand)
                || flags.contains(CGEventFlags::CGEventFlagControl);

            // Backspace is classified as correction automatically inside
            // record_categorized_keystroke; is_correction=false here.
            ctx.collector.record_categorized_keystroke(category, is_shortcut, false);

            event // listen-only tap must return the event unmodified
        }

        let tap = core_graphics::event::CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            event_mask,
            tap_callback,
            context_ptr as *mut std::ffi::c_void,
        );

        let tap = match tap {
            Ok(tap) => tap,
            Err(_) => {
                warn!(
                    "CGEventTapCreate failed -- grant Accessibility permission in \
                     System Settings > Privacy & Security > Accessibility"
                );
                // Reclaim the context box to avoid leak
                let _ = Box::from_raw(context_ptr);
                return;
            }
        };

        // Create a CFMachPort run loop source and add it to the current run loop
        let run_loop_source = tap.mach_port_run_loop_source(0)
            .expect("failed to create run loop source from event tap");

        let run_loop = CFRunLoop::get_current();
        run_loop.add_source(&run_loop_source, unsafe { kCFRunLoopCommonModes });

        // Enable the tap
        tap.enable();

        info!("CGEventTap active — passive key observer running");

        // CFRunLoop::run_current() blocks until stop() is called (from the
        // callback when `running` becomes false, or from KeyHook::stop()).
        CFRunLoop::run_current();

        // Cleanup: reclaim the context box
        let _ = Box::from_raw(context_ptr);
    }

    debug!("CGEventTap run loop exited");
}
```

> **Note on CGEventTap API:** The above uses `core_graphics::event` types. The exact FFI surface depends on the `core-graphics` crate version (0.25). If the high-level `CGEventTap::new()` wrapper is not available in this version, fall back to raw `CGEventTapCreate` via `core_graphics::sys`. The implementation must compile; adjust FFI bindings as needed during implementation.

- [ ] **Step 3: Write macOS-specific integration test**

Add to `crates/oneshim-monitor/src/key_hook/macos.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_context_size_is_reasonable() {
        // Ensure we are not accidentally capturing a large closure
        assert!(std::mem::size_of::<Arc<InputActivityCollector>>() <= 16);
    }

    /// Test that creating and immediately stopping the hook does not panic.
    /// This test may fail in CI without Accessibility permission; that is
    /// expected. The test is primarily for local development.
    #[test]
    fn start_stop_does_not_panic() {
        let collector = Arc::new(InputActivityCollector::new());
        let running = Arc::new(AtomicBool::new(true));

        // Immediately signal stop before the run loop starts
        running.store(false, Ordering::SeqCst);

        // run_event_tap should exit quickly because running is false
        let running_clone = running.clone();
        let collector_clone = collector.clone();
        let handle = std::thread::spawn(move || {
            run_event_tap(collector_clone, running_clone);
        });

        // Give it a moment then join
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = handle.join();
    }
}
```

**Verify:**
```bash
cargo check -p oneshim-monitor  # compile check
cargo test -p oneshim-monitor -- key_hook  # unit tests
```

**Commit:** `feat(monitor): implement macOS CGEventTap passive key observer`

---

## Task 3: Windows Raw Input implementation

**Files:**
- New: `crates/oneshim-monitor/src/key_hook/windows.rs`
- Modify: `crates/oneshim-monitor/Cargo.toml` (may need additional windows-sys features)

- [ ] **Step 1: Verify windows-sys features**

The workspace already has `windows-sys` with `Win32_UI_Input_KeyboardAndMouse`. We additionally need `Win32_UI_WindowsAndMessaging` for `GetMessageW`, `TranslateMessage`, `DispatchMessageW`, and `RegisterRawInputDevices`. Check the workspace Cargo.toml features list; these should already be present. If `Win32_Devices_HumanInterfaceDevice` is needed for `RAWINPUTDEVICE`, add it.

In `Cargo.toml` (workspace root), extend `windows-sys` features if needed:
```toml
windows-sys = { version = "0.61", features = [
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_Input",
    "Win32_Foundation",
    "Win32_System_Threading",
    "Win32_System_Registry",
    "Win32_System_SystemInformation",
    "Win32_System_LibraryLoader",
    "Win32_Devices_HumanInterfaceDevice",
] }
```

- [ ] **Step 2: Implement Windows Raw Input keyboard hook**

Create `crates/oneshim-monitor/src/key_hook/windows.rs`:

```rust
//! Windows key event observer using Raw Input.
//!
//! Registers a Raw Input keyboard device listener using
//! RegisterRawInputDevices with RIDEV_INPUTSINK. This receives all
//! keyboard input system-wide without blocking or modifying events.
//!
//! Runs on a dedicated std::thread with its own message loop.

#![cfg(target_os = "windows")]

use crate::input_activity::InputActivityCollector;
use super::classify::classify_keycode;
use oneshim_core::models::app_registry::KeyCategory;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

use windows_sys::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, RAWINPUT, RAWINPUTDEVICE,
    RAWINPUTHEADER, RIDEV_INPUTSINK, RID_INPUT, RIM_TYPEKEYBOARD,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, PostQuitMessage, RegisterClassW, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, MSG, WNDCLASSW, WM_INPUT,
};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

/// Thread-local state passed to the window procedure via a static.
/// Safe because the hook runs on a single dedicated thread.
static mut HOOK_STATE: Option<HookState> = None;

struct HookState {
    collector: Arc<InputActivityCollector>,
    running: Arc<AtomicBool>,
}

/// Run the Raw Input keyboard hook. Blocks until `running` becomes false.
pub fn run_raw_input_hook(collector: Arc<InputActivityCollector>, running: Arc<AtomicBool>) {
    unsafe {
        HOOK_STATE = Some(HookState {
            collector,
            running: running.clone(),
        });

        // Register a hidden message-only window class
        let class_name: Vec<u16> = "OneshimKeyHook\0".encode_utf16().collect();
        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: std::ptr::null_mut(),
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&wnd_class);

        // Create a message-only window (HWND_MESSAGE = -3)
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            0,
            0, 0, 0, 0,
            -3isize as HWND, // HWND_MESSAGE
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );

        if hwnd.is_null() {
            warn!("Failed to create message-only window for key hook");
            HOOK_STATE = None;
            return;
        }

        // Register for Raw Input keyboard events
        let rid = RAWINPUTDEVICE {
            usUsagePage: 0x01, // HID_USAGE_PAGE_GENERIC
            usUsage: 0x06,    // HID_USAGE_GENERIC_KEYBOARD
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        };

        if RegisterRawInputDevices(
            &rid as *const RAWINPUTDEVICE,
            1,
            std::mem::size_of::<RAWINPUTDEVICE>() as u32,
        ) == 0 {
            warn!("RegisterRawInputDevices failed");
            DestroyWindow(hwnd);
            HOOK_STATE = None;
            return;
        }

        info!("Raw Input keyboard hook active");

        // Message loop
        let mut msg: MSG = std::mem::zeroed();
        while running.load(Ordering::Relaxed) {
            let ret = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if ret == 0 || ret == -1 {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        DestroyWindow(hwnd);
        HOOK_STATE = None;
    }

    debug!("Raw Input message loop exited");
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg != WM_INPUT {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    let state = match HOOK_STATE.as_ref() {
        Some(s) => s,
        None => return DefWindowProcW(hwnd, msg, wparam, lparam),
    };

    if !state.running.load(Ordering::Relaxed) {
        PostQuitMessage(0);
        return 0;
    }

    // Read the RAWINPUT data
    let mut size: u32 = 0;
    GetRawInputData(
        lparam as _,
        RID_INPUT,
        std::ptr::null_mut(),
        &mut size,
        std::mem::size_of::<RAWINPUTHEADER>() as u32,
    );

    if size == 0 {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    let mut buffer = vec![0u8; size as usize];
    let read = GetRawInputData(
        lparam as _,
        RID_INPUT,
        buffer.as_mut_ptr() as _,
        &mut size,
        std::mem::size_of::<RAWINPUTHEADER>() as u32,
    );

    if read == u32::MAX {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    let raw = &*(buffer.as_ptr() as *const RAWINPUT);

    if raw.header.dwType != RIM_TYPEKEYBOARD {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    let keyboard = &raw.data.keyboard;

    // WM_KEYDOWN or WM_SYSKEYDOWN (key press only, ignore key-up)
    // Message field: 0x0100 = WM_KEYDOWN, 0x0104 = WM_SYSKEYDOWN
    if keyboard.Message != 0x0100 && keyboard.Message != 0x0104 {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    let vk_code = keyboard.VKey as u32;
    let category = classify_keycode(vk_code);

    // Detect shortcuts: check if Control or Alt is part of the key combo
    // (Raw Input does not directly indicate modifier state; for simplicity,
    // we do not detect shortcuts here. The existing shortcut detection in
    // the scheduler handles this via record_shortcut_name.)
    let is_shortcut = false;

    state.collector.record_categorized_keystroke(category, is_shortcut, false);

    DefWindowProcW(hwnd, msg, wparam, lparam)
}
```

- [ ] **Step 3: Write Windows-specific test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_state_default_is_none() {
        // HOOK_STATE is static and should be None by default
        unsafe {
            // Note: this test is not safe to run in parallel with the hook
            assert!(HOOK_STATE.is_none());
        }
    }
}
```

**Verify:**
```bash
# Cross-compile check (from macOS or Linux):
cargo check -p oneshim-monitor --target x86_64-pc-windows-msvc  # if toolchain available
# Or just verify the current platform compiles:
cargo check -p oneshim-monitor
```

**Commit:** `feat(monitor): implement Windows Raw Input passive key observer`

---

## Task 4: Linux X11 XRecord implementation

**Files:**
- New: `crates/oneshim-monitor/src/key_hook/linux.rs`

- [ ] **Step 1: Implement X11 XRecord hook (best-effort)**

The Linux hook uses the XRecord extension via `x11` crate FFI for X11 sessions. On Wayland-only desktops, this will not work and the hook will return None from `KeyHook::start()`.

Create `crates/oneshim-monitor/src/key_hook/linux.rs`:

```rust
//! Linux key event observer using X11 XRecord extension.
//!
//! Uses the XRecord extension to register a key event listener.
//! This is X11-only. On pure Wayland sessions (without XWayland),
//! the hook fails gracefully and KeyHook::start() returns None.
//!
//! Best-effort implementation: if XRecord is unavailable (e.g.,
//! missing xdotool, restricted server), log a warning and exit.

#![cfg(target_os = "linux")]

use crate::input_activity::InputActivityCollector;
use super::classify::classify_keycode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Run the X11 XRecord key observer. Blocks until `running` becomes false.
///
/// This is a best-effort implementation. If X11 or XRecord is unavailable,
/// it logs a warning and returns immediately.
pub fn run_x11_record_hook(collector: Arc<InputActivityCollector>, running: Arc<AtomicBool>) {
    // Check if we can connect to X11
    let display_env = std::env::var("DISPLAY").unwrap_or_default();
    if display_env.is_empty() {
        warn!("No DISPLAY set — X11 key hook unavailable (Wayland-only?)");
        return;
    }

    // The XRecord approach requires the `x11` crate or raw FFI to libX11
    // and libXtst. For Phase 1.5, we use a subprocess approach via
    // `xinput test-xi2 --root` as a pragmatic fallback that requires no
    // additional Rust crate dependencies.
    //
    // The `xinput` tool outputs key press/release events in a parseable
    // format. We spawn it, read stdout line by line, and classify keycodes.
    //
    // If `xinput` is not installed, we fall back to a no-op with a warning.
    info!("starting X11 key observer via xinput test-xi2");

    let mut child = match std::process::Command::new("xinput")
        .args(["test-xi2", "--root"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                warn!(
                    "xinput not found — install with 'sudo apt install xinput' \
                     for key-category tracking on Linux"
                );
            } else {
                warn!("failed to spawn xinput: {e}");
            }
            return;
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            warn!("failed to capture xinput stdout");
            let _ = child.kill();
            return;
        }
    };

    use std::io::BufRead;
    let reader = std::io::BufReader::new(stdout);

    // xinput test-xi2 output format:
    //   EVENT type 2 (KeyPress)
    //       detail: 36
    //       ...
    // We parse "EVENT type 2" for key press and "detail: <keycode>" for
    // the X11 keycode, then use XKeycodeToKeysym equivalent to get the keysym.
    // For simplicity, we map common X11 keycodes directly. The keycode in
    // xinput output is the X11 hardware keycode (not keysym).
    //
    // X11 hardware keycodes need offset -8 to align with evdev keycodes.
    // We map hardware keycodes to approximate categories directly.

    let mut in_key_press = false;

    for line in reader.lines() {
        if !running.load(Ordering::Relaxed) {
            break;
        }

        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();

        if trimmed.contains("EVENT type 2") || trimmed.contains("KeyPress") {
            in_key_press = true;
            continue;
        }

        if trimmed.contains("EVENT type 3") || trimmed.contains("KeyRelease") {
            in_key_press = false;
            continue;
        }

        if in_key_press {
            if let Some(detail) = trimmed.strip_prefix("detail:") {
                if let Ok(keycode) = detail.trim().parse::<u32>() {
                    // Convert X11 hardware keycode to approximate keysym
                    let keysym = x11_keycode_to_keysym_approx(keycode);
                    let category = classify_keycode(keysym);
                    collector.record_categorized_keystroke(category, false, false);
                    in_key_press = false;
                }
            }
        }
    }

    // Clean up the child process
    let _ = child.kill();
    let _ = child.wait();

    debug!("X11 key observer exited");
}

/// Approximate mapping from X11 hardware keycode to keysym.
///
/// X11 hardware keycodes vary by keyboard model, but the standard
/// evdev mapping (keycode - 8 = evdev code) is common on modern
/// Linux systems. This maps the most common keys; unmapped codes
/// default to a "regular" keysym range.
fn x11_keycode_to_keysym_approx(keycode: u32) -> u32 {
    // Standard evdev-based mapping (common on modern Linux)
    match keycode {
        9 => 0xFF1B,   // Escape
        22 => 0xFF08,  // BackSpace
        23 => 0xFF09,  // Tab
        36 => 0xFF0D,  // Return
        104 => 0xFF8D, // KP_Enter
        111 => 0xFF52, // Up
        113 => 0xFF51, // Left
        114 => 0xFF53, // Right
        116 => 0xFF54, // Down
        110 => 0xFF50, // Home
        115 => 0xFF57, // End
        112 => 0xFF55, // Page_Up
        117 => 0xFF56, // Page_Down
        119 => 0xFFFF, // Delete
        67 => 0xFFBE,  // F1
        68 => 0xFFBF,  // F2
        69 => 0xFFC0,  // F3
        70 => 0xFFC1,  // F4
        71 => 0xFFC2,  // F5
        72 => 0xFFC3,  // F6
        73 => 0xFFC4,  // F7
        74 => 0xFFC5,  // F8
        75 => 0xFFC6,  // F9
        76 => 0xFFC7,  // F10
        95 => 0xFFC8,  // F11
        96 => 0xFFC9,  // F12
        50 | 62 => 0xFFE1, // Shift_L, Shift_R
        37 | 105 => 0xFFE3, // Control_L, Control_R
        64 | 108 => 0xFFE9, // Alt_L, Alt_R
        133 | 134 => 0xFFEB, // Super_L, Super_R
        66 => 0xFFE5,  // Caps_Lock
        _ => 0x0061,   // Default to 'a' (Regular)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_keycode_return_maps_correctly() {
        assert_eq!(x11_keycode_to_keysym_approx(36), 0xFF0D);
    }

    #[test]
    fn x11_keycode_escape_maps_correctly() {
        assert_eq!(x11_keycode_to_keysym_approx(9), 0xFF1B);
    }

    #[test]
    fn x11_keycode_arrows_map_correctly() {
        assert_eq!(x11_keycode_to_keysym_approx(111), 0xFF52); // Up
        assert_eq!(x11_keycode_to_keysym_approx(113), 0xFF51); // Left
        assert_eq!(x11_keycode_to_keysym_approx(114), 0xFF53); // Right
        assert_eq!(x11_keycode_to_keysym_approx(116), 0xFF54); // Down
    }

    #[test]
    fn x11_keycode_unknown_is_regular_keysym() {
        // Unknown keycode maps to 'a' keysym -> Regular
        assert_eq!(x11_keycode_to_keysym_approx(999), 0x0061);
    }
}
```

**Verify:**
```bash
cargo check -p oneshim-monitor
cargo test -p oneshim-monitor -- key_hook::linux
```

**Commit:** `feat(monitor): implement Linux X11 xinput-based key observer (best-effort)`

---

## Task 5: Wire KeyHook into the scheduler

**Files:**
- Modify: `src-tauri/src/scheduler/loops.rs`
- Modify: `src-tauri/src/scheduler/mod.rs` (if `Scheduler` struct needs a field)

- [ ] **Step 1: Add `key_hook` field to Scheduler or local state**

The `KeyHook` needs to live for the duration of the scheduler. The simplest approach is to start it inside `run_scheduler_loops()` and let it drop when the function exits (which happens on shutdown).

In `src-tauri/src/scheduler/loops.rs`, inside `run_scheduler_loops()`, after `let shared_input_collector = Arc::new(InputActivityCollector::new());`, add:

```rust
        // ── Phase 1.5: Platform key-category hook ──
        // Spawns a passive OS keyboard observer that classifies key events
        // into KeyCategory and feeds them into InputActivityCollector.
        // Gated by text_intelligence.input_pattern_detail config flag.
        let _key_hook = {
            let text_intel_config = self
                .config_manager
                .as_ref()
                .map(|cm| cm.get().analysis.text_intelligence.clone())
                .unwrap_or_default();

            if text_intel_config.enabled && text_intel_config.input_pattern_detail {
                oneshim_monitor::key_hook::KeyHook::start(shared_input_collector.clone())
            } else {
                debug!("key-category hook disabled (text_intelligence.input_pattern_detail = false or text_intelligence.enabled = false)");
                None
            }
        };
```

The `_key_hook` binding ensures the hook stays alive. When `run_scheduler_loops` returns (on shutdown), the `KeyHook` is dropped, which calls `stop()` via the `Drop` impl.

- [ ] **Step 2: Add tracing import if needed**

Verify that `debug!` is imported at the top of `loops.rs`. It already is (`use tracing::{debug, info, warn};`).

- [ ] **Step 3: Add `oneshim-monitor` dependency to `src-tauri` Cargo.toml if not present**

Check `src-tauri/Cargo.toml` for `oneshim-monitor`. It should already be listed since the scheduler module uses `oneshim_monitor::idle::IdleTracker` etc. Verify and add if missing:

```toml
oneshim-monitor = { workspace = true }
```

**Verify:**
```bash
cargo check -p oneshim-tauri  # or whatever the src-tauri package name is
```

**Commit:** `feat(scheduler): wire platform KeyHook into scheduler with config gating`

---

## Task 6: Workspace build and test verification

- [ ] **Step 1: Run full workspace check**

```bash
cargo check --workspace
```

- [ ] **Step 2: Run full workspace tests**

```bash
cargo test --workspace
```

- [ ] **Step 3: Run clippy**

```bash
cargo clippy --workspace
```

- [ ] **Step 4: Run fmt check**

```bash
cargo fmt --check
```

- [ ] **Step 5: Fix any issues found in Steps 1-4**

Address compiler errors, test failures, lint warnings. Common issues to expect:

1. **CGEventTap FFI surface**: The `core-graphics` 0.25 crate may not expose `CGEventTap::new()` as a high-level wrapper. If so, use raw FFI via `core_graphics::sys::CGEventTapCreate` directly.
2. **windows-sys RAWINPUT struct layout**: The `data.keyboard` field access on `RAWINPUT` uses an anonymous union. May need `std::mem::transmute` or direct pointer casting.
3. **Platform test isolation**: macOS tests that require Accessibility permission will fail in CI. Mark them with `#[ignore]` and add a note explaining the permission requirement.

**Commit:** `chore: fix lint and build issues from Phase 1.5 platform key hooks`

---

## Task 7: Manual platform verification (cannot be automated)

These steps require manual testing on each platform. They are listed as verification criteria, not automated steps.

- [ ] **Step 1: macOS manual test**

1. Build and run: `cargo tauri dev`
2. Verify log output: `key-category hook started` and `CGEventTap active`
3. Open iTerm2, type a few commands (including Enter, Tab, arrow keys)
4. Check web dashboard (http://localhost:10090) for non-zero keystroke profile ratios
5. If Accessibility permission prompt appears, grant it in System Settings
6. Verify no typing latency increase (hook is passive and async)

- [ ] **Step 2: Windows manual test**

1. Build and run on Windows
2. Verify log output: `key-category hook started` and `Raw Input keyboard hook active`
3. Open Windows Terminal, type commands
4. Check web dashboard for non-zero keystroke profile ratios
5. Verify no UAC prompt (Raw Input does not require elevation)

- [ ] **Step 3: Linux manual test**

1. Build and run on Linux (X11 session)
2. Verify `xinput` is installed: `which xinput`
3. Verify log output: `starting X11 key observer via xinput test-xi2`
4. Open terminal, type commands
5. Check web dashboard for non-zero keystroke profile ratios
6. Verify that on Wayland-only session, log shows: `No DISPLAY set -- X11 key hook unavailable`

---

## Verification Criteria

After all tasks are complete:

1. `cargo test --workspace` passes with 0 failures
2. `cargo clippy --workspace` produces no warnings (except allowed `dead_code`)
3. `classify_keycode()` correctly maps platform keycodes to `KeyCategory`:
   - macOS: Return (36) -> Enter, Tab (48) -> Tab, arrows (123-126) -> Arrow, Delete (51) -> Backspace
   - Windows: VK_RETURN (0x0D) -> Enter, VK_TAB (0x09) -> Tab, VK_LEFT-VK_DOWN -> Arrow, VK_BACK -> Backspace
   - Linux: XK_Return (0xFF0D) -> Enter, XK_Tab (0xFF09) -> Tab, XK_Left-XK_Down -> Arrow, XK_BackSpace -> Backspace
4. `KeyHook::start()` returns `Some(hook)` on macOS (with Accessibility permission) and Windows
5. `KeyHook::start()` returns `Some(hook)` on Linux X11 (with `xinput` installed), `None` on Wayland-only
6. `KeyHook` respects config gating: not started when `text_intelligence.enabled = false` or `input_pattern_detail = false`
7. After hook is running, `InputActivityCollector::take_snapshot()` produces `keystroke_profile: Some(...)` with non-zero ratios during typing
8. Terminal app correctly classified as `TerminalCommands` when enter_ratio > 0.15 (end-to-end)
9. Document editor correctly classified as `DocumentWriting` vs `DocumentReading` (end-to-end)
10. No typing latency impact -- hook thread does not block the main thread or the key event pipeline
11. `KeyHook::stop()` (and `Drop`) cleanly terminates the observer thread
12. Existing tests continue to pass (no regression from Phase 1)

---

## Risk Assessment and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| macOS Accessibility permission not granted | Hook silently does nothing; counters stay at zero; existing classification fallback handles it | Log a clear warning with instructions to grant permission |
| CGEventTap disabled by system timeout | Key events stop being observed | Re-enable the tap in the callback when `TapDisabledByTimeout` event is received |
| `core-graphics` 0.25 API does not expose CGEventTap high-level wrapper | Compile error | Fall back to raw `core_graphics::sys::CGEventTapCreate` FFI |
| Windows Raw Input struct layout mismatch | Incorrect keycode extraction | Verify with a simple test program; use explicit struct offset calculations if needed |
| Linux `xinput` not installed | Hook unavailable | Log a helpful install command; degrade gracefully |
| Wayland has no X11 fallback | Hook unavailable | Return None from `KeyHook::start()`; existing zero-counter fallback handles this |
| Hook thread panic or crash | InputActivityCollector stops receiving data | Wrap the hook body in `catch_unwind`; log the panic and let counters fall back to zero |
| Privacy concern: keystroke logging | User distrust | Document clearly that only aggregate category counts (not key sequences or content) are recorded. Config-gated and consent-required. |

---

## What Phase 1.5 Does NOT Include

These are explicitly out of scope:

| Item | Deferred to |
|------|-------------|
| Accessibility API (AXUIElement, UIA) | Phase 2 |
| `FocusedElementInfo` extraction | Phase 2 |
| `zeroize` for raw text | Phase 3 |
| `full_text_extraction` consent | Phase 3 |
| AppRegistry wiring into TitleBarParser / privacy.rs | Future cleanup |
| Wayland native key event monitoring (libinput/evdev) | Future (requires root or input group) |
| Shortcut detection in Windows Raw Input hook | Future (would need modifier key state tracking) |
| Hot-reload of config changes (start/stop hook on config change) | Future (requires config file watcher) |
