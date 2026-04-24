//! REST handlers for the tracking-schedule configuration and status endpoints.
//!
//! Three endpoints mirror the Tauri IPC commands defined in
//! `src-tauri/src/commands/tracking_schedule.rs`:
//!
//! - `GET  /api/tracking-schedule`         — return current config
//! - `PUT  /api/tracking-schedule`         — validate + persist new config
//! - `GET  /api/tracking-schedule/status`  — real-time status snapshot
//!
//! A.15 (TDD red): these stubs exist so the integration test file compiles and
//! the routes resolve (returning 501 Not Implemented), causing all 4 tests to
//! fail with wrong status codes until A.16 supplies the real logic.

use axum::http::StatusCode;

pub async fn get_config() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn put_config() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn get_status() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
