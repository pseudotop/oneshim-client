//! Native screen recording border indicator.
//!
//! Platform: macOS only (Phase 1). Other platforms gracefully degrade (no border).

#[cfg(target_os = "macos")]
mod colors;
#[cfg(target_os = "macos")]
mod macos;

use std::sync::atomic::{AtomicBool, Ordering};

/// Native screen recording border indicator.
///
/// Creates a dedicated NSWindow with a CAShapeLayer border on macOS.
/// Completely independent of the WebView overlay pipeline.
///
/// Thread safety: `MainThreadBound` wraps the NSWindow/CAShapeLayer.
/// AtomicBool tracks visible/paused state from any thread.
/// All native mutations dispatch to the main thread via `get_on_main`.
pub struct NativeBorderIndicator {
    #[cfg(target_os = "macos")]
    inner: dispatch2::MainThreadBound<macos::BorderInner>,
    visible: AtomicBool,
    paused: AtomicBool,
}

impl NativeBorderIndicator {
    /// Create the border indicator on macOS.
    ///
    /// Uses `NSScreen::mainScreen()` to get screen bounds in native points.
    /// Returns `None` if window creation fails.
    #[cfg(target_os = "macos")]
    pub fn new(mtm: objc2::MainThreadMarker) -> Option<Self> {
        let inner = macos::create_border_window(mtm)?;
        Some(Self {
            inner: dispatch2::MainThreadBound::new(inner, mtm),
            visible: AtomicBool::new(false),
            paused: AtomicBool::new(false),
        })
    }

    /// Non-macOS stub — always returns `None`.
    #[cfg(not(target_os = "macos"))]
    pub fn new() -> Option<Self> {
        None
    }

    /// Show the border window (capturing state — teal with pulse animation).
    /// No-op if already visible.
    pub fn show(&self) {
        if self.visible.swap(true, Ordering::Relaxed) {
            return;
        }
        #[cfg(target_os = "macos")]
        self.inner.get_on_main(|inner| {
            inner.window.orderFront(None);
        });
    }

    /// Hide the border window. No-op if already hidden.
    pub fn hide(&self) {
        if !self.visible.swap(false, Ordering::Relaxed) {
            return;
        }
        #[cfg(target_os = "macos")]
        self.inner.get_on_main(|inner| {
            inner.window.orderOut(None);
        });
    }

    /// Update paused state. Switches between:
    /// - `false` (capturing): teal border with pulse animation
    /// - `true`  (paused): gray border, static (no animation)
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        #[cfg(target_os = "macos")]
        self.inner.get_on_main(|inner| {
            use objc2_foundation::ns_string;
            if paused {
                // Border stroke → gray, static
                inner
                    .border_layer
                    .removeAnimationForKey(ns_string!("borderPulse"));
                let gray = colors::gray_cgcolor();
                inner.border_layer.setStrokeColor(Some(&gray));

                // Glow bands → gray, static (keep relative opacity)
                for glow in &inner.glow_layers {
                    glow.removeAnimationForKey(ns_string!("glowPulse"));
                    let gray = colors::gray_cgcolor();
                    glow.setStrokeColor(Some(&gray));
                }
            } else {
                // Border stroke → teal, pulsing
                let anim = macos::create_stroke_pulse_animation();
                inner
                    .border_layer
                    .addAnimation_forKey(&anim, Some(ns_string!("borderPulse")));
                let teal = colors::teal_cgcolor_full();
                inner.border_layer.setStrokeColor(Some(&teal));

                // Glow bands → teal, pulsing
                for (i, glow) in inner.glow_layers.iter().enumerate() {
                    let teal = colors::teal_cgcolor_full();
                    glow.setStrokeColor(Some(&teal));
                    let base = macos::GLOW_OPACITIES[i];
                    glow.setOpacity(base);
                    let anim = macos::create_opacity_pulse(base, base * 0.25);
                    glow.addAnimation_forKey(&anim, Some(ns_string!("glowPulse")));
                }
            }
        });
    }

    /// Query current visibility (lock-free atomic read).
    #[allow(dead_code)]
    pub fn is_visible(&self) -> bool {
        self.visible.load(Ordering::Relaxed)
    }

    /// Query current paused state (lock-free atomic read).
    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
}

#[cfg(target_os = "macos")]
impl Drop for NativeBorderIndicator {
    fn drop(&mut self) {
        self.inner.get_on_main(|inner| {
            inner.window.orderOut(None);
        });
    }
}

/// Tauri managed state wrapper for the native border indicator.
///
/// Separate from AppState to avoid ordering issues — AppState is registered
/// before `setup.rs` runs, but the border window must be created during setup
/// on the main thread.
pub struct NativeBorderState(pub NativeBorderIndicator);
