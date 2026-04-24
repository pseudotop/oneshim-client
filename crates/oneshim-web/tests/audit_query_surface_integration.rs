//! Audit query surface integration tests (Task 9.3).
//!
//! Per plan L3602 / spec §9.2 L1403-1405. Covers D25 `entries_by_command_id`
//! port method + `GET /api/audit/export` REST endpoint filtering.
//!
//! Test 1 exercises the `AuditLogPort::entries_by_command_id` contract via
//! the production `AuditLogAdapter` (the only `AuditLogPort` impl wired in
//! production today). It verifies that with 3 matching + 2 non-matching
//! entries, exactly 3 are returned, newest-first.
//!
//! Test 2 exercises the `GET /api/audit/export?command_id=...` REST endpoint
//! end-to-end through the production Axum router via `tower::ServiceExt`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Method, Request, StatusCode};
use oneshim_automation::audit::{AuditLogAdapter, AuditLogger};
use oneshim_core::models::audit::{AuditEntry, AuditLevel};
use oneshim_core::ports::audit_log::AuditLogPort;
use oneshim_storage::sqlite::SqliteStorage;
use tokio::sync::{broadcast, RwLock};
use tower::ServiceExt;

use oneshim_web::app_state::AppState;
use oneshim_web::WebServer;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Build an `AuditLogAdapter` wrapping a fresh in-memory `AuditLogger`.
///
/// Buffer capacity 100 is large enough for any of these tests (5/4 entries).
fn fresh_audit_adapter() -> Arc<AuditLogAdapter> {
    let logger = Arc::new(RwLock::new(AuditLogger::new(100, 10)));
    Arc::new(AuditLogAdapter::new(logger))
}

/// Seed `n` entries with the given `command_id` via `log_start_if` (which uses
/// `Utc::now()` at insertion time). Inserts a `tokio::time::sleep(1ms)` between
/// successive entries so timestamps are strictly monotonic — required for the
/// "newest-first" ordering assertion since the prod impl orders by insertion
/// order (which equals timestamp order when monotonic).
async fn seed_entries(adapter: &Arc<AuditLogAdapter>, command_id: &str, n: usize) {
    for i in 0..n {
        adapter
            .log_start_if(
                AuditLevel::Basic,
                command_id,
                "session-test",
                &format!("action-{i}"),
            )
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
}

/// Build an `AppState` with the supplied audit adapter wired into
/// `state.automation.audit_logger`. Backed by in-memory SQLite + a fresh
/// broadcast channel; nothing else from production is required for the
/// `/api/audit/export` route.
fn app_state_with_audit(audit: Arc<dyn AuditLogPort>) -> AppState {
    let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory SQLite"));
    let (event_tx, _) = broadcast::channel(16);
    let mut state = AppState::with_core(storage, event_tx);
    state.automation.audit_logger = Some(audit);
    state
}

/// Build the full production Axum router (loopback gating included) and attach
/// a `MockConnectInfo` so `require_loopback_client` middleware passes.
fn loopback_app(state: AppState) -> axum::Router {
    WebServer::build_router(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
}

// ── Test 1: AuditLogPort::entries_by_command_id ──────────────────────────────

/// Spec §9.2 L1403 — D25 port method:
///
/// Insert 3 audit entries with the same `command_id` + 2 with different
/// command_ids. Calling `AuditLogPort::entries_by_command_id(cmd, 10)` must
/// return exactly the 3 matching rows, newest first.
///
/// Drives the production `AuditLogAdapter` (the wired prod impl) — not a mock.
#[tokio::test]
async fn audit_entries_by_command_id_returns_matching_rows() {
    let adapter = fresh_audit_adapter();

    // Seed 3 entries with the target command_id, then 2 with a different one.
    // Timestamps are forced strictly monotonic by the 1ms sleep in seed_entries.
    seed_entries(&adapter, "cmd-target-123", 3).await;
    seed_entries(&adapter, "cmd-other-456", 2).await;

    // Query: limit 10 (well above the 3 matching rows).
    let results: Vec<AuditEntry> = adapter.entries_by_command_id("cmd-target-123", 10).await;

    // Exactly 3 rows match the target command_id.
    assert_eq!(
        results.len(),
        3,
        "expected exactly 3 entries matching cmd-target-123, got {}",
        results.len()
    );

    // All returned rows have the target command_id (no leakage from cmd-other-456).
    for entry in &results {
        assert_eq!(
            entry.command_id, "cmd-target-123",
            "result leaked non-matching command_id: {:?}",
            entry.command_id
        );
    }

    // Newest-first ordering: timestamps strictly non-increasing across the
    // returned slice (each pair satisfies `prev >= next`).
    for window in results.windows(2) {
        assert!(
            window[0].timestamp >= window[1].timestamp,
            "expected newest-first ordering; got {} before {}",
            window[0].timestamp,
            window[1].timestamp,
        );
    }
}

// ── Test 2: GET /api/audit/export?command_id=X ───────────────────────────────

/// Spec §9.2 L1404-1405 — D25 REST endpoint:
///
/// Pre-populate the audit log with rows for two different command_ids; call
/// `GET /api/audit/export?command_id=cmd-target-789`; assert that all returned
/// rows have the target `command_id` (no leakage from the other command_id).
///
/// Drives the production Axum router end-to-end via `tower::ServiceExt::oneshot`,
/// proving that:
///   1. The route `/api/audit/export` is reachable through `WebServer::build_router`.
///   2. `command_id` is parsed from the query string.
///   3. The handler dispatches to `AuditLogPort::entries_by_command_id` (not
///      `recent_entries`) when the param is present + non-empty.
///   4. Only matching rows are serialized into the JSON body.
#[tokio::test]
async fn audit_export_rest_endpoint_filters_by_command_id() {
    let adapter = fresh_audit_adapter();

    // Seed 2 matching + 2 non-matching rows.
    seed_entries(&adapter, "cmd-target-789", 2).await;
    seed_entries(&adapter, "cmd-other-321", 2).await;

    let state = app_state_with_audit(adapter as Arc<dyn AuditLogPort>);
    let app = loopback_app(state);

    // GET /api/audit/export?command_id=cmd-target-789
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/audit/export?command_id=cmd-target-789")
                .body(Body::empty())
                .expect("build GET request"),
        )
        .await
        .expect("oneshot");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET /api/audit/export with command_id filter must return 200"
    );

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let entries: Vec<AuditEntry> =
        serde_json::from_slice(&body_bytes).expect("response body must be Vec<AuditEntry> JSON");

    // Exactly 2 rows match the target command_id.
    assert_eq!(
        entries.len(),
        2,
        "expected exactly 2 entries matching cmd-target-789, got {}",
        entries.len()
    );

    // No leakage from cmd-other-321.
    for entry in &entries {
        assert_eq!(
            entry.command_id, "cmd-target-789",
            "REST response leaked non-matching command_id: {:?}",
            entry.command_id
        );
    }
}
