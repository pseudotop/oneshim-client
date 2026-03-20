use oneshim_core::config::OverlayMode;
use oneshim_core::models::coaching::{CoachingMessage, DismissAction, GoalProgressView};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

const OVERLAY_LABEL: &str = "magic-overlay";
const OVERLAY_URL: &str = "overlay.html";

/// Serializable payload emitted to the overlay WebView via Tauri events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayCoachingPayload {
    pub message_id: String,
    pub profile: String,
    pub trigger_type: String,
    pub text: String,
    pub auto_dismiss_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayUpgradePayload {
    pub message_id: String,
    pub personalized_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayFocusPayload {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub border_color: String,
    pub opacity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayGoalPayload {
    pub goals: Vec<GoalProgressView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayModePayload {
    pub mode: OverlayMode,
}

#[derive(Debug)]
struct OverlayState {
    mode: OverlayMode,
    visible: bool,
    current_message_id: Option<String>,
}

/// Handle for managing the MagicOverlay Tauri WebView window.
///
/// Created once during app setup. The overlay window is lazily created
/// on the first `show_coaching()` call and kept alive (hidden when idle).
#[derive(Clone)]
pub struct MagicOverlayHandle {
    app_handle: AppHandle,
    state: Arc<RwLock<OverlayState>>,
}

impl MagicOverlayHandle {
    pub fn new(app_handle: AppHandle, initial_mode: OverlayMode) -> Self {
        Self {
            app_handle,
            state: Arc::new(RwLock::new(OverlayState {
                mode: initial_mode,
                visible: false,
                current_message_id: None,
            })),
        }
    }

    /// Create the overlay window if it does not yet exist.
    ///
    /// Gracefully degrades on Linux/Wayland (overlay not supported).
    /// macOS requires `macos-private-api` feature flag for transparent windows.
    /// Windows requires `shadow: false` to avoid rendering artifacts.
    fn ensure_window(&self) -> Result<(), String> {
        // Wayland graceful degradation: skip overlay creation entirely
        #[cfg(target_os = "linux")]
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            warn!("Wayland detected — MagicOverlay disabled, using notification fallback");
            return Err("Wayland does not support transparent overlay windows".to_string());
        }

        // Check if window already exists
        if self.app_handle.get_webview_window(OVERLAY_LABEL).is_some() {
            return Ok(());
        }

        let monitor = self
            .app_handle
            .primary_monitor()
            .map_err(|e| format!("Failed to get primary monitor: {e}"))?
            .ok_or_else(|| "No primary monitor found".to_string())?;

        let size = monitor.size();

        let builder = WebviewWindowBuilder::new(
            &self.app_handle,
            OVERLAY_LABEL,
            WebviewUrl::App(OVERLAY_URL.into()),
        )
        .title("ONESHIM Overlay")
        .inner_size(size.width as f64, size.height as f64)
        .position(0.0, 0.0)
        .transparent(true)
        .always_on_top(true)
        .decorations(false)
        .resizable(false)
        .visible(false)
        .skip_taskbar(true)
        // Windows: shadow must be false for transparent windows to render correctly
        .shadow(false);

        let window = builder
            .build()
            .map_err(|e| format!("Failed to create overlay window: {e}"))?;

        // Default: click-through. User presses Cmd+Shift+O to make interactive.
        let _ = window.set_ignore_cursor_events(true);

        info!("MagicOverlay window created");
        Ok(())
    }

    /// Show a coaching message on the overlay.
    ///
    /// Creates the overlay window if needed, emits the event, and sets
    /// the window to visible.
    pub async fn show_coaching(&self, message: &CoachingMessage) {
        if let Err(e) = self.ensure_window() {
            debug!("overlay unavailable, skipping show_coaching: {e}");
            return;
        }

        let payload = OverlayCoachingPayload {
            message_id: message.message_id.clone(),
            profile: format!("{:?}", message.profile),
            trigger_type: oneshim_core::models::coaching::trigger_type_name(&message.trigger),
            text: message.display_text().to_string(),
            auto_dismiss_secs: 15,
        };

        if let Err(e) = self.app_handle.emit("overlay:show-coaching", &payload) {
            warn!("failed to emit overlay:show-coaching: {e}");
            return;
        }

        // Show the window
        if let Some(window) = self.app_handle.get_webview_window(OVERLAY_LABEL) {
            let _ = window.show();
        }

        let mut state = self.state.write().await;
        state.visible = true;
        state.current_message_id = Some(message.message_id.clone());
    }

    /// Upgrade the coaching message text with LLM-personalized content.
    ///
    /// Only emits if the current message matches and the window is still visible.
    pub async fn upgrade_message(&self, message_id: &str, personalized_text: &str) {
        let state = self.state.read().await;
        if !state.visible {
            return;
        }
        if state.current_message_id.as_deref() != Some(message_id) {
            return;
        }
        drop(state);

        let payload = OverlayUpgradePayload {
            message_id: message_id.to_string(),
            personalized_text: personalized_text.to_string(),
        };

        if let Err(e) = self.app_handle.emit("overlay:upgrade-message", &payload) {
            warn!("failed to emit overlay:upgrade-message: {e}");
        }
    }

    /// Dismiss a coaching message from the overlay.
    ///
    /// Clears the current message ID and hides the window.
    pub async fn dismiss(&self, message_id: &str, _action: DismissAction) {
        let mut state = self.state.write().await;
        if state.current_message_id.as_deref() == Some(message_id) {
            state.current_message_id = None;
        }
        state.visible = false;
        drop(state);

        if let Some(window) = self.app_handle.get_webview_window(OVERLAY_LABEL) {
            let _ = window.hide();
        }

        if let Err(e) = self.app_handle.emit("overlay:dismiss", message_id) {
            warn!("failed to emit overlay:dismiss: {e}");
        }
    }

    /// Update focus highlight overlay element.
    pub async fn update_focus_highlight(&self, highlight: OverlayFocusPayload) {
        if let Err(e) = self.app_handle.emit("overlay:update-focus", &highlight) {
            warn!("failed to emit overlay:update-focus: {e}");
        }
    }

    /// Update goal progress data on the overlay.
    pub async fn update_goal_progress(&self, goals: Vec<GoalProgressView>) {
        let payload = OverlayGoalPayload { goals };
        if let Err(e) = self.app_handle.emit("overlay:update-goals", &payload) {
            warn!("failed to emit overlay:update-goals: {e}");
        }
    }

    /// Set the overlay display mode (Minimal/Rich/Adaptive).
    pub async fn set_mode(&self, mode: OverlayMode) {
        let mut state = self.state.write().await;
        state.mode = mode;
        drop(state);

        let payload = OverlayModePayload { mode };
        if let Err(e) = self.app_handle.emit("overlay:set-mode", &payload) {
            warn!("failed to emit overlay:set-mode: {e}");
        }
    }

    /// Get the current overlay display mode.
    pub async fn get_mode(&self) -> OverlayMode {
        self.state.read().await.mode
    }

    /// Check if the overlay window is currently visible.
    pub async fn is_visible(&self) -> bool {
        self.state.read().await.visible
    }

    /// Cycle through overlay modes: Minimal -> Rich -> Adaptive -> Minimal.
    pub async fn toggle_mode(&self) {
        let new_mode = {
            let state = self.state.read().await;
            match state.mode {
                OverlayMode::Minimal => OverlayMode::Rich,
                OverlayMode::Rich => OverlayMode::Adaptive,
                OverlayMode::Adaptive => OverlayMode::Minimal,
            }
        };
        self.set_mode(new_mode).await;
    }

    /// Toggle overlay interactivity.
    ///
    /// When `interactive = true`, the overlay captures mouse/keyboard input
    /// (user can interact with popup buttons).
    /// When `interactive = false`, all events pass through to underlying windows.
    ///
    /// Triggered by:
    ///   - Global shortcut Cmd+Shift+O: toggle to interactive
    ///   - Coaching popup dismissed: return to click-through
    ///   - 5-second no-interaction timeout: return to click-through
    pub async fn set_interactive(&self, interactive: bool) {
        if let Some(window) = self.app_handle.get_webview_window(OVERLAY_LABEL) {
            // set_ignore_cursor_events is the inverse of "interactive"
            let _ = window.set_ignore_cursor_events(!interactive);
            debug!("Overlay interactive={interactive}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_state_default_mode() {
        let state = OverlayState {
            mode: OverlayMode::Minimal,
            visible: false,
            current_message_id: None,
        };
        assert_eq!(state.mode, OverlayMode::Minimal);
        assert!(!state.visible);
        assert!(state.current_message_id.is_none());
    }

    #[test]
    fn overlay_coaching_payload_serde_roundtrip() {
        let payload = OverlayCoachingPayload {
            message_id: "msg-001".to_string(),
            profile: "FocusGuard".to_string(),
            trigger_type: "RegimeDrift".to_string(),
            text: "Take a break from coding.".to_string(),
            auto_dismiss_secs: 15,
        };

        let json = serde_json::to_string(&payload).expect("serialize");
        let restored: OverlayCoachingPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.message_id, "msg-001");
        assert_eq!(restored.auto_dismiss_secs, 15);
    }

    #[test]
    fn overlay_upgrade_payload_serde_roundtrip() {
        let payload = OverlayUpgradePayload {
            message_id: "msg-002".to_string(),
            personalized_text: "Great focus session! Time for a well-earned break.".to_string(),
        };

        let json = serde_json::to_string(&payload).expect("serialize");
        let restored: OverlayUpgradePayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.message_id, "msg-002");
        assert!(restored.personalized_text.contains("well-earned"));
    }

    #[test]
    fn overlay_focus_payload_serde_roundtrip() {
        let payload = OverlayFocusPayload {
            x: 100,
            y: 200,
            width: 800,
            height: 600,
            border_color: "#3b82f6".to_string(),
            opacity: 0.8,
        };

        let json = serde_json::to_string(&payload).expect("serialize");
        let restored: OverlayFocusPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.x, 100);
        assert_eq!(restored.width, 800);
        assert!((restored.opacity - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn overlay_goal_payload_serde_roundtrip() {
        let payload = OverlayGoalPayload {
            goals: vec![GoalProgressView {
                regime_label: "Deep Work".to_string(),
                current_minutes: 45,
                target_minutes: 120,
                percentage: 37,
                display_color: "#3b82f6".to_string(),
            }],
        };

        let json = serde_json::to_string(&payload).expect("serialize");
        let restored: OverlayGoalPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.goals.len(), 1);
        assert_eq!(restored.goals[0].regime_label, "Deep Work");
    }
}
