//! Edge Intelligence 집중도 API 핸들러.
//!
//! 집중도 메트릭, 작업 세션, 인터럽션, 로컬 제안 API.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::handlers::TimeRangeQuery;
use crate::AppState;

// ============================================================
// 응답 타입 정의
// ============================================================

/// 집중도 메트릭 응답
#[derive(Debug, Serialize)]
pub struct FocusMetricsDto {
    /// 날짜 (YYYY-MM-DD)
    pub date: String,
    /// 총 활동 시간 (초)
    pub total_active_secs: u64,
    /// 깊은 작업 시간 (초)
    pub deep_work_secs: u64,
    /// 커뮤니케이션 시간 (초)
    pub communication_secs: u64,
    /// 컨텍스트 전환 횟수
    pub context_switches: u32,
    /// 인터럽션 횟수
    pub interruption_count: u32,
    /// 평균 집중 지속 시간 (초)
    pub avg_focus_duration_secs: u64,
    /// 최대 집중 지속 시간 (초)
    pub max_focus_duration_secs: u64,
    /// 집중도 점수 (0.0 ~ 100.0)
    pub focus_score: f32,
}

/// 집중도 메트릭 + 히스토리 응답
#[derive(Debug, Serialize)]
pub struct FocusMetricsResponse {
    /// 오늘 메트릭
    pub today: FocusMetricsDto,
    /// 히스토리 (최근 7일)
    pub history: Vec<FocusMetricsDto>,
}

/// 작업 세션 응답
#[derive(Debug, Serialize)]
pub struct WorkSessionDto {
    /// 세션 ID
    pub id: i64,
    /// 시작 시각 (RFC3339)
    pub started_at: String,
    /// 종료 시각 (RFC3339, None이면 진행 중)
    pub ended_at: Option<String>,
    /// 주요 앱
    pub primary_app: String,
    /// 앱 카테고리
    pub category: String,
    /// 상태 (active, completed)
    pub state: String,
    /// 인터럽션 횟수
    pub interruption_count: u32,
    /// 깊은 작업 시간 (초)
    pub deep_work_secs: u64,
    /// 총 시간 (초)
    pub duration_secs: u64,
}

/// 인터럽션 응답
#[derive(Debug, Serialize)]
pub struct InterruptionDto {
    /// ID
    pub id: i64,
    /// 중단 시각 (RFC3339)
    pub interrupted_at: String,
    /// 이전 앱
    pub from_app: String,
    /// 이전 앱 카테고리
    pub from_category: String,
    /// 전환된 앱
    pub to_app: String,
    /// 전환된 앱 카테고리
    pub to_category: String,
    /// 복귀 시각 (RFC3339, None이면 미복귀)
    pub resumed_at: Option<String>,
    /// 복귀한 앱
    pub resumed_to_app: Option<String>,
    /// 중단 지속 시간 (초)
    pub duration_secs: Option<u64>,
}

/// 로컬 제안 응답
#[derive(Debug, Serialize)]
pub struct LocalSuggestionDto {
    /// 제안 ID
    pub id: i64,
    /// 제안 유형
    pub suggestion_type: String,
    /// 제안 내용 (JSON)
    pub payload: serde_json::Value,
    /// 생성 시각 (RFC3339)
    pub created_at: String,
    /// 표시 시각 (RFC3339)
    pub shown_at: Option<String>,
    /// 무시 시각 (RFC3339)
    pub dismissed_at: Option<String>,
    /// 실행 시각 (RFC3339)
    pub acted_at: Option<String>,
}

/// 제안 피드백 요청
#[derive(Debug, Deserialize)]
pub struct SuggestionFeedbackRequest {
    /// 액션: "shown", "dismissed", "acted"
    pub action: String,
}

// ============================================================
// API 핸들러
// ============================================================

/// GET /api/focus/metrics - 집중도 메트릭 조회 (오늘 + 최근 7일)
pub async fn get_focus_metrics(
    State(state): State<AppState>,
) -> Result<Json<FocusMetricsResponse>, (StatusCode, String)> {
    debug!("GET /api/focus/metrics");

    let storage = &state.storage;
    let today = Utc::now().format("%Y-%m-%d").to_string();

    // 오늘 메트릭 조회 (없으면 생성)
    let today_metrics = storage
        .get_or_create_focus_metrics(&today)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 최근 7일 메트릭 조회
    let history_raw = storage
        .get_recent_focus_metrics(7)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // DTO로 변환
    let today_dto = FocusMetricsDto {
        date: today.clone(),
        total_active_secs: today_metrics.total_active_secs,
        deep_work_secs: today_metrics.deep_work_secs,
        communication_secs: today_metrics.communication_secs,
        context_switches: today_metrics.context_switches,
        interruption_count: today_metrics.interruption_count,
        avg_focus_duration_secs: today_metrics.avg_focus_duration_secs,
        max_focus_duration_secs: today_metrics.max_focus_duration_secs,
        focus_score: today_metrics.focus_score,
    };

    let history: Vec<FocusMetricsDto> = history_raw
        .into_iter()
        .filter(|(date, _)| date != &today) // 오늘 제외
        .map(|(date, m)| FocusMetricsDto {
            date,
            total_active_secs: m.total_active_secs,
            deep_work_secs: m.deep_work_secs,
            communication_secs: m.communication_secs,
            context_switches: m.context_switches,
            interruption_count: m.interruption_count,
            avg_focus_duration_secs: m.avg_focus_duration_secs,
            max_focus_duration_secs: m.max_focus_duration_secs,
            focus_score: m.focus_score,
        })
        .collect();

    Ok(Json(FocusMetricsResponse {
        today: today_dto,
        history,
    }))
}

/// GET /api/focus/sessions - 작업 세션 목록 조회
pub async fn get_work_sessions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<TimeRangeQuery>,
) -> Result<Json<Vec<WorkSessionDto>>, (StatusCode, String)> {
    debug!("GET /api/focus/sessions");

    let storage = &state.storage;
    let from = query.from_datetime();
    let to = query.to_datetime();
    let limit = query.limit_or_default();

    let conn = storage.conn_ref().lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("잠금 획득 실패: {e}"),
        )
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT id, started_at, ended_at, primary_app, category, state, 
                    interruption_count, deep_work_secs, duration_secs
             FROM work_sessions 
             WHERE started_at >= ?1 AND started_at <= ?2
             ORDER BY started_at DESC
             LIMIT ?3",
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("쿼리 준비 실패: {e}"),
            )
        })?;

    let sessions = stmt
        .query_map(
            rusqlite::params![from.to_rfc3339(), to.to_rfc3339(), limit as i64],
            |row| {
                Ok(WorkSessionDto {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    ended_at: row.get(2)?,
                    primary_app: row.get(3)?,
                    category: row.get(4)?,
                    state: row.get(5)?,
                    interruption_count: row.get(6)?,
                    deep_work_secs: row.get(7)?,
                    duration_secs: row.get(8)?,
                })
            },
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("쿼리 실행 실패: {e}"),
            )
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(sessions))
}

/// GET /api/focus/interruptions - 인터럽션 목록 조회
pub async fn get_interruptions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<TimeRangeQuery>,
) -> Result<Json<Vec<InterruptionDto>>, (StatusCode, String)> {
    debug!("GET /api/focus/interruptions");

    let storage = &state.storage;
    let from = query.from_datetime();
    let to = query.to_datetime();
    let limit = query.limit_or_default();

    let conn = storage.conn_ref().lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("잠금 획득 실패: {e}"),
        )
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT id, interrupted_at, from_app, from_category, to_app, to_category,
                    resumed_at, resumed_to_app,
                    CASE WHEN resumed_at IS NOT NULL 
                         THEN CAST((julianday(resumed_at) - julianday(interrupted_at)) * 86400 AS INTEGER)
                         ELSE NULL END as duration_secs
             FROM interruptions 
             WHERE interrupted_at >= ?1 AND interrupted_at <= ?2
             ORDER BY interrupted_at DESC
             LIMIT ?3",
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("쿼리 준비 실패: {e}")))?;

    let interruptions = stmt
        .query_map(
            rusqlite::params![from.to_rfc3339(), to.to_rfc3339(), limit as i64],
            |row| {
                Ok(InterruptionDto {
                    id: row.get(0)?,
                    interrupted_at: row.get(1)?,
                    from_app: row.get(2)?,
                    from_category: row.get(3)?,
                    to_app: row.get(4)?,
                    to_category: row.get(5)?,
                    resumed_at: row.get(6)?,
                    resumed_to_app: row.get(7)?,
                    duration_secs: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                })
            },
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("쿼리 실행 실패: {e}"),
            )
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(interruptions))
}

/// GET /api/focus/suggestions - 로컬 제안 목록 조회
pub async fn get_suggestions(
    State(state): State<AppState>,
) -> Result<Json<Vec<LocalSuggestionDto>>, (StatusCode, String)> {
    debug!("GET /api/focus/suggestions");

    let storage = &state.storage;

    // 최근 24시간 내 제안만 조회
    let cutoff = (Utc::now() - Duration::hours(24)).to_rfc3339();

    let conn = storage.conn_ref().lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("잠금 획득 실패: {e}"),
        )
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT id, suggestion_type, payload, created_at, shown_at, dismissed_at, acted_at
             FROM local_suggestions 
             WHERE created_at >= ?1
             ORDER BY created_at DESC
             LIMIT 50",
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("쿼리 준비 실패: {e}"),
            )
        })?;

    let suggestions = stmt
        .query_map(rusqlite::params![cutoff], |row| {
            let payload_str: String = row.get(2)?;
            let payload: serde_json::Value =
                serde_json::from_str(&payload_str).unwrap_or(serde_json::json!({}));
            Ok(LocalSuggestionDto {
                id: row.get(0)?,
                suggestion_type: row.get(1)?,
                payload,
                created_at: row.get(3)?,
                shown_at: row.get(4)?,
                dismissed_at: row.get(5)?,
                acted_at: row.get(6)?,
            })
        })
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("쿼리 실행 실패: {e}"),
            )
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(suggestions))
}

/// POST /api/focus/suggestions/:id/feedback - 제안 피드백 제출
pub async fn submit_suggestion_feedback(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<SuggestionFeedbackRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    debug!(
        "POST /api/focus/suggestions/{}/feedback action={}",
        id, request.action
    );

    let storage = &state.storage;

    match request.action.as_str() {
        "shown" => storage
            .mark_suggestion_shown(id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        "dismissed" => storage
            .mark_suggestion_dismissed(id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        "acted" => storage
            .mark_suggestion_acted(id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("유효하지 않은 액션: {}", request.action),
            ))
        }
    }

    Ok(StatusCode::NO_CONTENT)
}
