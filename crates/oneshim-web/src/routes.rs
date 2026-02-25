use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::handlers;
use crate::AppState;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/metrics", get(handlers::metrics::get_metrics))
        .route(
            "/metrics/hourly",
            get(handlers::metrics::get_hourly_metrics),
        )
        .route("/processes", get(handlers::processes::get_processes))
        .route("/idle", get(handlers::idle::get_idle_periods))
        .route("/sessions", get(handlers::sessions::list_sessions))
        .route("/sessions/:id", get(handlers::sessions::get_session))
        .route("/frames", get(handlers::frames::get_frames))
        .route("/frames/:id/image", get(handlers::frames::get_frame_image))
        .route("/events", get(handlers::events::get_events))
        .route("/stats/summary", get(handlers::stats::get_summary))
        .route("/stats/apps", get(handlers::stats::get_app_usage))
        .route("/stats/heatmap", get(handlers::stats::get_heatmap))
        .route("/reports", get(handlers::reports::generate_report))
        .route("/settings", get(handlers::settings::get_settings))
        .route("/settings", post(handlers::settings::update_settings))
        .route("/storage/stats", get(handlers::settings::get_storage_stats))
        .route("/data/range", delete(handlers::data::delete_data_range))
        .route("/data/all", delete(handlers::data::delete_all_data))
        .route("/search", get(handlers::search::search))
        .route("/stream", get(handlers::stream::event_stream))
        .route("/export/metrics", get(handlers::export::export_metrics))
        .route("/export/events", get(handlers::export::export_events))
        .route("/export/frames", get(handlers::export::export_frames))
        .route("/backup", get(handlers::backup::create_backup))
        .route("/backup/restore", post(handlers::backup::restore_backup))
        .route("/tags", get(handlers::tags::list_tags))
        .route("/tags", post(handlers::tags::create_tag))
        .route("/tags/:id", get(handlers::tags::get_tag))
        .route("/tags/:id", put(handlers::tags::update_tag))
        .route("/tags/:id", delete(handlers::tags::delete_tag))
        .route(
            "/frames/:frame_id/tags",
            get(handlers::tags::get_frame_tags),
        )
        .route("/timeline", get(handlers::timeline::get_timeline))
        .route(
            "/frames/:frame_id/tags/:tag_id",
            post(handlers::tags::add_tag_to_frame),
        )
        .route(
            "/frames/:frame_id/tags/:tag_id",
            delete(handlers::tags::remove_tag_from_frame),
        )
        .route("/focus/metrics", get(handlers::focus::get_focus_metrics))
        .route("/focus/sessions", get(handlers::focus::get_work_sessions))
        .route(
            "/focus/interruptions",
            get(handlers::focus::get_interruptions),
        )
        .route("/focus/suggestions", get(handlers::focus::get_suggestions))
        .route(
            "/focus/suggestions/:id/feedback",
            post(handlers::focus::submit_suggestion_feedback),
        )
        .route(
            "/automation/status",
            get(handlers::automation::get_automation_status),
        )
        .route(
            "/automation/contracts",
            get(handlers::automation::get_contract_versions),
        )
        .route(
            "/automation/audit",
            get(handlers::automation::get_audit_logs),
        )
        .route(
            "/automation/policy-events",
            get(handlers::automation::get_policy_events),
        )
        .route(
            "/automation/policies",
            get(handlers::automation::get_policies),
        )
        .route(
            "/automation/stats",
            get(handlers::automation::get_automation_stats),
        )
        .route(
            "/automation/presets",
            get(handlers::automation::list_presets),
        )
        .route(
            "/automation/presets",
            post(handlers::automation::create_preset),
        )
        .route(
            "/automation/presets/:id",
            put(handlers::automation::update_preset),
        )
        .route(
            "/automation/presets/:id",
            delete(handlers::automation::delete_preset),
        )
        .route(
            "/automation/presets/:id/run",
            post(handlers::automation::run_preset),
        )
        .route(
            "/automation/execute-hint",
            post(handlers::automation::execute_intent_hint),
        )
        .route(
            "/automation/execute-scene-action",
            post(handlers::automation::execute_scene_action),
        )
        .route(
            "/automation/scene",
            get(handlers::automation::get_automation_scene),
        )
        .route(
            "/automation/scene/calibration",
            get(handlers::automation::get_automation_scene_calibration),
        )
        .route(
            "/automation/gui/sessions",
            post(handlers::automation_gui::create_gui_session),
        )
        .route(
            "/automation/gui/sessions/:id",
            get(handlers::automation_gui::get_gui_session)
                .delete(handlers::automation_gui::delete_gui_session),
        )
        .route(
            "/automation/gui/sessions/:id/highlight",
            post(handlers::automation_gui::highlight_gui_session),
        )
        .route(
            "/automation/gui/sessions/:id/confirm",
            post(handlers::automation_gui::confirm_gui_session),
        )
        .route(
            "/automation/gui/sessions/:id/execute",
            post(handlers::automation_gui::execute_gui_session),
        )
        .route(
            "/automation/gui/sessions/:id/events",
            get(handlers::automation_gui::gui_session_event_stream),
        )
        .route(
            "/onboarding/quickstart",
            get(handlers::onboarding::get_quickstart),
        )
        .route(
            "/support/diagnostics",
            get(handlers::support::get_diagnostics),
        )
        .route("/update/status", get(handlers::update::get_update_status))
        .route("/update/action", post(handlers::update::post_update_action))
        .route("/update/stream", get(handlers::update::get_update_stream))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use oneshim_storage::sqlite::SqliteStorage;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    #[test]
    fn routes_compile() {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
        let (event_tx, _) = broadcast::channel(16);
        let state = AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            update_control: None,
        };
        let _app: Router<()> = api_routes().with_state(state);
    }
}
