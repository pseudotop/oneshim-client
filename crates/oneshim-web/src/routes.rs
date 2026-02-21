//! API 라우트 정의.

use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::handlers;
use crate::AppState;

/// API 라우트 생성
pub fn api_routes() -> Router<AppState> {
    Router::new()
        // 시스템 메트릭
        .route("/metrics", get(handlers::metrics::get_metrics))
        .route(
            "/metrics/hourly",
            get(handlers::metrics::get_hourly_metrics),
        )
        // 프로세스 스냅샷
        .route("/processes", get(handlers::processes::get_processes))
        // 유휴 기간
        .route("/idle", get(handlers::idle::get_idle_periods))
        // 세션
        .route("/sessions", get(handlers::sessions::list_sessions))
        .route("/sessions/:id", get(handlers::sessions::get_session))
        // 프레임 (스크린샷)
        .route("/frames", get(handlers::frames::get_frames))
        .route("/frames/:id/image", get(handlers::frames::get_frame_image))
        // 이벤트
        .route("/events", get(handlers::events::get_events))
        // 통계
        .route("/stats/summary", get(handlers::stats::get_summary))
        .route("/stats/apps", get(handlers::stats::get_app_usage))
        .route("/stats/heatmap", get(handlers::stats::get_heatmap))
        // 리포트
        .route("/reports", get(handlers::reports::generate_report))
        // 설정
        .route("/settings", get(handlers::settings::get_settings))
        .route("/settings", post(handlers::settings::update_settings))
        .route("/storage/stats", get(handlers::settings::get_storage_stats))
        // 데이터 삭제
        .route("/data/range", delete(handlers::data::delete_data_range))
        .route("/data/all", delete(handlers::data::delete_all_data))
        // 검색
        .route("/search", get(handlers::search::search))
        // 실시간 스트림 (SSE)
        .route("/stream", get(handlers::stream::event_stream))
        // 데이터 내보내기
        .route("/export/metrics", get(handlers::export::export_metrics))
        .route("/export/events", get(handlers::export::export_events))
        .route("/export/frames", get(handlers::export::export_frames))
        // 백업/복원
        .route("/backup", get(handlers::backup::create_backup))
        .route("/backup/restore", post(handlers::backup::restore_backup))
        // 태그
        .route("/tags", get(handlers::tags::list_tags))
        .route("/tags", post(handlers::tags::create_tag))
        .route("/tags/:id", get(handlers::tags::get_tag))
        .route("/tags/:id", put(handlers::tags::update_tag))
        .route("/tags/:id", delete(handlers::tags::delete_tag))
        // 프레임 태그
        .route(
            "/frames/:frame_id/tags",
            get(handlers::tags::get_frame_tags),
        )
        // 통합 타임라인 (세션 리플레이)
        .route("/timeline", get(handlers::timeline::get_timeline))
        .route(
            "/frames/:frame_id/tags/:tag_id",
            post(handlers::tags::add_tag_to_frame),
        )
        .route(
            "/frames/:frame_id/tags/:tag_id",
            delete(handlers::tags::remove_tag_from_frame),
        )
        // Edge Intelligence (집중도)
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
        // 자동화
        .route(
            "/automation/status",
            get(handlers::automation::get_automation_status),
        )
        .route(
            "/automation/audit",
            get(handlers::automation::get_audit_logs),
        )
        .route(
            "/automation/policies",
            get(handlers::automation::get_policies),
        )
        .route(
            "/automation/stats",
            get(handlers::automation::get_automation_stats),
        )
        // 워크플로우 프리셋
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
            update_control: None,
        };
        let _app: Router<()> = api_routes().with_state(state);
    }
}
