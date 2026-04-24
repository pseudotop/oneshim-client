//! Tauri IPC commands for tracking-schedule configuration and status.
//!
//! These stubs exist for A.13 TDD-red contract tests; A.14 provides the
//! real implementations.  The three commands mirror the REST surface
//! (`GET/PUT /config/tracking_schedule`, `GET /config/tracking_schedule/status`)
//! so the WebView and REST callers share the same semantics.
//!
//! A.14 will wire these commands into `generate_handler!` in `main.rs`.

// Tauri commands are wired to generate_handler! in A.14; suppress dead_code
// lint on the pub async fns until then.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use tauri::command;

use oneshim_core::config::TrackingScheduleConfig;

use crate::ipc_error::IpcError;
use crate::runtime_state::ConfigRuntimeState;

// ── TrackingScheduleStatus ──────────────────────────────────────────────────

/// Snapshot of the current tracking-schedule state returned by
/// `get_tracking_schedule_status`.
///
/// A.14 may refine or move this struct to `oneshim-api-contracts`; for now it
/// lives here so A.13's tests can compile and exercise the contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackingScheduleStatus {
    /// Whether tracking is currently muted by the schedule.
    pub active_now: bool,
    /// RFC 3339 timestamp when the current mute window ends, if active.
    pub ends_at: Option<String>,
    /// RFC 3339 timestamp when the next mute window begins, within 7 days.
    pub next_starts_at: Option<String>,
    /// Human-readable label of the currently active window, or empty string.
    pub label: String,
}

// ── IPC command stubs ───────────────────────────────────────────────────────

/// Return the current tracking-schedule configuration.
///
/// A.14 impl: reads from `ConfigRuntimeState::config_manager().get()`.
#[command]
pub async fn get_tracking_schedule(
    state: tauri::State<'_, ConfigRuntimeState>,
) -> Result<TrackingScheduleConfig, IpcError> {
    get_tracking_schedule_inner(state.config_manager().get().tracking_schedule.clone())
}

/// Inner logic extracted so tests can call it without constructing `tauri::State`.
pub(crate) fn get_tracking_schedule_inner(
    _cfg: TrackingScheduleConfig,
) -> Result<TrackingScheduleConfig, IpcError> {
    todo!("A.14 impl")
}

/// Persist a new tracking-schedule configuration.
///
/// Validation mirrors the REST PUT handler: HH:MM format, valid IANA timezone,
/// overnight windows ≤ 16 h.  Returns `IpcError` with code
/// `"validation.invalid_arguments"` on invalid input.
///
/// A.14 impl: validates, deep-merges into config, persists via ConfigManager.
#[command]
pub async fn set_tracking_schedule(
    cfg: TrackingScheduleConfig,
    _state: tauri::State<'_, ConfigRuntimeState>,
) -> Result<(), IpcError> {
    set_tracking_schedule_inner(cfg)
}

/// Inner logic extracted so tests can call it without constructing `tauri::State`.
pub(crate) fn set_tracking_schedule_inner(_cfg: TrackingScheduleConfig) -> Result<(), IpcError> {
    todo!("A.14 impl")
}

/// Return a real-time status snapshot for the tracking schedule.
///
/// Uses wall-clock `now` internally (UTC, converted to configured timezone).
/// A.14 impl: calls `TrackingWindow::window_is_active` for each window, walks
/// the next 7 × 24 h to find `next_starts_at`.
#[command]
pub async fn get_tracking_schedule_status(
    state: tauri::State<'_, ConfigRuntimeState>,
) -> Result<TrackingScheduleStatus, IpcError> {
    let cfg = state.config_manager().get().tracking_schedule.clone();
    get_tracking_schedule_status_inner(cfg)
}

/// Inner logic extracted so tests can call it without constructing `tauri::State`.
pub(crate) fn get_tracking_schedule_status_inner(
    _cfg: TrackingScheduleConfig,
) -> Result<TrackingScheduleStatus, IpcError> {
    todo!("A.14 impl")
}

// ── validate_hhmm ──────────────────────────────────────────────────────────

/// Validate a single `"HH:MM"` string at the IPC boundary.
///
/// Returns `Err(IpcError)` with code `"validation.invalid_arguments"` if the
/// string does not match the strict HH:MM format accepted by the config layer.
/// A.14 uses this to produce early errors before the full serde path.
pub(crate) fn validate_hhmm(_s: &str, _field: &str) -> Result<(), IpcError> {
    todo!("A.14 impl")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid `TrackingScheduleConfig` for roundtrip tests.
    fn schedule_with_window(start: &str, end: &str, label: &str) -> TrackingScheduleConfig {
        // Construct via JSON so the validated TryFrom path fires and we get a
        // properly constructed value without calling private constructors.
        let json = format!(
            r#"{{
                "enabled": true,
                "windows": [{{
                    "start": "{start}",
                    "end": "{end}",
                    "days_of_week": ["Mon","Tue","Wed","Thu","Fri"],
                    "label": "{label}"
                }}],
                "timezone": "Asia/Seoul"
            }}"#
        );
        serde_json::from_str(&json).expect("valid tracking schedule")
    }

    // ── 1. set_then_get_roundtrip ───────────────────────────────────────────

    /// Calling `set_tracking_schedule` followed by `get_tracking_schedule`
    /// returns exactly the config that was set (deep equality).
    ///
    /// Red: both stubs panic with `todo!("A.14 impl")`.
    #[test]
    fn set_then_get_roundtrip() {
        let cfg = schedule_with_window("09:00", "18:00", "Work hours");

        // set — expect Ok(())
        let set_result = set_tracking_schedule_inner(cfg.clone());
        assert!(
            set_result.is_ok(),
            "set_tracking_schedule should succeed for valid config"
        );

        // get — expect the same config back
        let get_result = get_tracking_schedule_inner(cfg.clone());
        let returned = get_result.expect("get_tracking_schedule should succeed");
        assert_eq!(
            returned, cfg,
            "get_tracking_schedule must return the config that was set"
        );
    }

    // ── 2. get_status_returns_active_when_now_in_window ────────────────────

    /// When `now` falls inside a configured window, `active_now` is true and
    /// `ends_at` / `label` are populated.
    ///
    /// Red: stub panics with `todo!("A.14 impl")`.
    #[test]
    fn get_status_returns_active_when_now_in_window() {
        // 09:00–18:00 Mon–Fri; status logic will determine current window via
        // wall-clock.  We pass a schedule that covers the full work-week so the
        // test is window-independent of which day the CI runs on.
        let cfg = schedule_with_window("00:00", "23:59", "Always-on window");

        let status = get_tracking_schedule_status_inner(cfg)
            .expect("get_tracking_schedule_status should succeed");

        assert!(
            status.active_now,
            "active_now must be true when now is inside the window"
        );
        assert!(
            status.ends_at.is_some(),
            "ends_at must be populated when active_now is true"
        );
        assert!(
            !status.label.is_empty(),
            "label must be non-empty when a named window is active"
        );
    }

    // ── 3. get_status_returns_inactive_outside_window ──────────────────────

    /// When the schedule is disabled, `active_now` is false and `ends_at` is None.
    ///
    /// Red: stub panics with `todo!("A.14 impl")`.
    #[test]
    fn get_status_returns_inactive_outside_window() {
        // A schedule with `enabled: false` — the engine must never activate it.
        let cfg: TrackingScheduleConfig =
            serde_json::from_str(r#"{"enabled": false, "windows": [], "timezone": "Local"}"#)
                .expect("valid disabled schedule");

        let status = get_tracking_schedule_status_inner(cfg)
            .expect("get_tracking_schedule_status should succeed");

        assert!(
            !status.active_now,
            "active_now must be false when schedule is disabled"
        );
        assert!(
            status.ends_at.is_none(),
            "ends_at must be None when not in any window"
        );
    }

    // ── 4. get_status_returns_next_starts_at_within_7_days ─────────────────

    /// When `active_now` is false and a future window exists within 7 days,
    /// `next_starts_at` is Some and within a 7-day horizon.
    ///
    /// Red: stub panics with `todo!("A.14 impl")`.
    #[test]
    fn get_status_returns_next_starts_at_within_7_days() {
        // A schedule that covers Mon–Sun for a narrow 1-minute window at 23:58.
        // Since 23:58 almost certainly hasn't happened yet today (or at most ~2
        // minutes per day), `next_starts_at` must be Some for a CI run at any
        // time of day.  If by bad luck `now` falls in that 1-min window the
        // test checks the invariant still holds (Some within 7 days).
        let json = r#"{
            "enabled": true,
            "windows": [{
                "start": "23:58",
                "end": "23:59",
                "days_of_week": ["Mon","Tue","Wed","Thu","Fri","Sat","Sun"],
                "label": "Night tick"
            }],
            "timezone": "Local"
        }"#;
        let cfg: TrackingScheduleConfig =
            serde_json::from_str(json).expect("valid tight-window schedule");

        let status = get_tracking_schedule_status_inner(cfg)
            .expect("get_tracking_schedule_status should succeed");

        // Either we are currently in the 1-min window (active) or the next one
        // is within 7 days.  Either way, `next_starts_at` must be Some.
        assert!(
            status.next_starts_at.is_some(),
            "next_starts_at must be Some when a future window exists within 7 days"
        );

        // Validate it parses as RFC 3339 and is within 7 days.
        if let Some(ref ts) = status.next_starts_at {
            let next: chrono::DateTime<chrono::Utc> = ts
                .parse()
                .expect("next_starts_at must be a valid RFC 3339 timestamp");
            let horizon = chrono::Utc::now() + chrono::Duration::days(7);
            assert!(
                next <= horizon,
                "next_starts_at must be within the 7-day lookahead horizon"
            );
        }
    }

    // ── 5. ipc_error_on_invalid_hhmm_format ────────────────────────────────

    /// Passing a window with an invalid HH:MM value (`"12:XX"`) must cause
    /// `set_tracking_schedule` to return an `IpcError` whose `code` is
    /// `"validation.invalid_arguments"`.
    ///
    /// Red: serde deserialisation rejects the payload before `todo!()` is reached,
    /// but A.14 must surface the correct wire code either way.
    ///
    /// Note: because `TrackingScheduleConfig` validates on deserialise (via
    /// `TryFrom<TrackingWindowRaw>`), we construct the invalid config via the
    /// `validate_hhmm` helper which is the IPC-boundary validator A.14 will wire.
    #[test]
    fn ipc_error_on_invalid_hhmm_format() {
        let err = validate_hhmm("12:XX", "start").expect_err("validate_hhmm must reject '12:XX'");

        assert_eq!(
            err.code, "validation.invalid_arguments",
            "IpcError code must be 'validation.invalid_arguments', got '{}'",
            err.code
        );
    }
}
