//! Tauri IPC command for receiving frontend route error reports.
//!
//! Classifies severity, logs via tracing, emits desktop notifications
//! (with per-route cooldown), and signals recovery strategies back to
//! the frontend via `frontend-recovery` events.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::Emitter;
use tracing::{error, info, warn};

/// Payload emitted to the frontend via `frontend-recovery` event.
#[derive(Debug, Serialize, Clone)]
struct RecoveryPayload {
    strategy: String,
    route: String,
    reason: String,
}

/// Payload emitted via `desktop-notification` to trigger the notification pipeline.
#[derive(Debug, Serialize, Clone)]
struct NotificationPayload {
    title: String,
    body: String,
}

static NOTIFICATION_COOLDOWN: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
static RECOVERY_COOLDOWN: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

const NOTIFICATION_COOLDOWN_SECS: u64 = 30;
const RECOVERY_COOLDOWN_SECS: u64 = 5;

/// Emit a desktop notification for the given route, respecting a 30-second per-route cooldown.
fn maybe_notify(app: &tauri::AppHandle, route: &str, message: &str) {
    let now = Instant::now();
    let cooldown = Duration::from_secs(NOTIFICATION_COOLDOWN_SECS);

    let mut guard = match NOTIFICATION_COOLDOWN.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let map = guard.get_or_insert_with(HashMap::new);

    if let Some(last) = map.get(route) {
        if now.duration_since(*last) < cooldown {
            return;
        }
    }

    map.insert(route.to_owned(), now);
    drop(guard);

    let payload = NotificationPayload {
        title: format!("Route error: {route}"),
        body: message.to_owned(),
    };

    if let Err(e) = app.emit("desktop-notification", &payload) {
        warn!("failed to emit desktop-notification: {e}");
    }
}

/// Emit a recovery signal to the frontend, respecting a 5-second per-route cooldown.
fn maybe_emit_recovery(app: &tauri::AppHandle, route: &str, strategy: &str, reason: &str) {
    let now = Instant::now();
    let cooldown = Duration::from_secs(RECOVERY_COOLDOWN_SECS);

    let mut guard = match RECOVERY_COOLDOWN.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let map = guard.get_or_insert_with(HashMap::new);

    if let Some(last) = map.get(route) {
        if now.duration_since(*last) < cooldown {
            return;
        }
    }

    map.insert(route.to_owned(), now);
    drop(guard);

    let payload = RecoveryPayload {
        strategy: strategy.to_owned(),
        route: route.to_owned(),
        reason: reason.to_owned(),
    };

    if let Err(e) = app.emit("frontend-recovery", &payload) {
        warn!("failed to emit frontend-recovery: {e}");
    }
}

/// Receive a frontend route error, log it, optionally notify, and signal recovery.
///
/// Severity mapping:
/// - `info`     — log only
/// - `warning`  — log + desktop notification
/// - `error`    — log + desktop notification + `reset-route` recovery
/// - `critical` — log + desktop notification + `full-reload` recovery
#[tauri::command]
pub async fn report_frontend_error(
    app: tauri::AppHandle,
    error_message: String,
    route: String,
    severity: String,
    stack: Option<String>,
) -> Result<(), String> {
    match severity.as_str() {
        "info" => {
            info!(route, error_message, "frontend info");
        }
        "warning" => {
            warn!(route, error_message, "frontend warning");
            maybe_notify(&app, &route, &error_message);
        }
        "error" => {
            error!(route, error_message, ?stack, "frontend error");
            maybe_notify(&app, &route, &error_message);
            maybe_emit_recovery(&app, &route, "reset-route", &error_message);
        }
        "critical" => {
            error!(route, error_message, ?stack, "CRITICAL frontend error");
            maybe_notify(&app, &route, &error_message);
            maybe_emit_recovery(&app, &route, "full-reload", &error_message);
        }
        _ => {
            warn!(route, error_message, severity, "unknown severity");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_cooldown_suppresses_rapid_calls() {
        // Reset the global state for this test.
        {
            let mut guard = NOTIFICATION_COOLDOWN.lock().unwrap();
            let map = guard.get_or_insert_with(HashMap::new);
            map.clear();
        }

        let route = "/test-route";
        let now = Instant::now();

        // First insert should succeed (no prior entry).
        {
            let mut guard = NOTIFICATION_COOLDOWN.lock().unwrap();
            let map = guard.get_or_insert_with(HashMap::new);
            assert!(map.get(route).is_none());
            map.insert(route.to_owned(), now);
        }

        // Immediate second check should be within cooldown.
        {
            let guard = NOTIFICATION_COOLDOWN.lock().unwrap();
            let map = guard.as_ref().unwrap();
            let last = map.get(route).unwrap();
            let elapsed = Instant::now().duration_since(*last);
            assert!(elapsed < Duration::from_secs(NOTIFICATION_COOLDOWN_SECS));
        }
    }

    #[test]
    fn recovery_cooldown_suppresses_rapid_calls() {
        {
            let mut guard = RECOVERY_COOLDOWN.lock().unwrap();
            let map = guard.get_or_insert_with(HashMap::new);
            map.clear();
        }

        let route = "/test-recovery";
        let now = Instant::now();

        {
            let mut guard = RECOVERY_COOLDOWN.lock().unwrap();
            let map = guard.get_or_insert_with(HashMap::new);
            assert!(map.get(route).is_none());
            map.insert(route.to_owned(), now);
        }

        {
            let guard = RECOVERY_COOLDOWN.lock().unwrap();
            let map = guard.as_ref().unwrap();
            let last = map.get(route).unwrap();
            let elapsed = Instant::now().duration_since(*last);
            assert!(elapsed < Duration::from_secs(RECOVERY_COOLDOWN_SECS));
        }
    }

    #[test]
    fn recovery_payload_serializes_correctly() {
        let payload = RecoveryPayload {
            strategy: "reset-route".to_owned(),
            route: "/focus".to_owned(),
            reason: "component crashed".to_owned(),
        };

        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["strategy"], "reset-route");
        assert_eq!(json["route"], "/focus");
        assert_eq!(json["reason"], "component crashed");
    }

    #[test]
    fn notification_payload_serializes_correctly() {
        let payload = NotificationPayload {
            title: "Route error: /reports".to_owned(),
            body: "render failed".to_owned(),
        };

        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["title"], "Route error: /reports");
        assert_eq!(json["body"], "render failed");
    }
}
