//! 데이터 삭제 API 핸들러.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, AppState};

/// 날짜 범위 삭제 요청
#[derive(Debug, Deserialize)]
pub struct DeleteRangeRequest {
    /// 시작 날짜 (RFC3339 또는 YYYY-MM-DD)
    pub from: String,
    /// 종료 날짜 (RFC3339 또는 YYYY-MM-DD)
    pub to: String,
    /// 삭제할 데이터 유형 (비어있으면 모두 삭제)
    #[serde(default)]
    pub data_types: Vec<String>,
}

/// 삭제 결과 응답
#[derive(Debug, Serialize)]
pub struct DeleteResult {
    /// 성공 여부
    pub success: bool,
    /// 삭제된 이벤트 수
    pub events_deleted: u64,
    /// 삭제된 프레임 수
    pub frames_deleted: u64,
    /// 삭제된 메트릭 수
    pub metrics_deleted: u64,
    /// 삭제된 프로세스 스냅샷 수
    pub process_snapshots_deleted: u64,
    /// 삭제된 유휴 기록 수
    pub idle_periods_deleted: u64,
    /// 메시지
    pub message: String,
}

impl DeleteResult {
    fn empty() -> Self {
        Self {
            success: true,
            events_deleted: 0,
            frames_deleted: 0,
            metrics_deleted: 0,
            process_snapshots_deleted: 0,
            idle_periods_deleted: 0,
            message: String::new(),
        }
    }

    fn total(&self) -> u64 {
        self.events_deleted
            + self.frames_deleted
            + self.metrics_deleted
            + self.process_snapshots_deleted
            + self.idle_periods_deleted
    }
}

/// DELETE /api/data/range - 날짜 범위로 데이터 삭제
pub async fn delete_data_range(
    State(state): State<AppState>,
    Json(request): Json<DeleteRangeRequest>,
) -> Result<Json<DeleteResult>, ApiError> {
    // 날짜 유효성 검사
    if request.from.is_empty() || request.to.is_empty() {
        return Err(ApiError::BadRequest(
            "시작 날짜와 종료 날짜가 필요합니다".to_string(),
        ));
    }

    let conn = state
        .storage
        .conn_ref()
        .lock()
        .map_err(|e| ApiError::Internal(format!("DB 잠금 실패: {e}")))?;

    let mut result = DeleteResult::empty();

    // 데이터 유형이 지정되지 않으면 모두 삭제
    let delete_all = request.data_types.is_empty();
    let data_types = &request.data_types;

    // 이벤트 삭제
    if delete_all || data_types.iter().any(|t| t == "events") {
        let deleted = conn
            .execute(
                "DELETE FROM events WHERE timestamp >= ? AND timestamp <= ?",
                [&request.from, &request.to],
            )
            .map_err(|e| ApiError::Internal(format!("이벤트 삭제 실패: {e}")))?;
        result.events_deleted = deleted as u64;
    }

    // 프레임 삭제 (이미지 파일도 함께 삭제)
    if delete_all || data_types.iter().any(|t| t == "frames") {
        // 먼저 삭제할 프레임의 파일 경로 조회
        if let Some(ref frames_dir) = state.frames_dir {
            let mut stmt = conn
                .prepare("SELECT file_path FROM frames WHERE timestamp >= ? AND timestamp <= ?")
                .map_err(|e| ApiError::Internal(format!("프레임 조회 실패: {e}")))?;

            let paths: Vec<String> = stmt
                .query_map([&request.from, &request.to], |row| row.get(0))
                .map_err(|e| ApiError::Internal(format!("프레임 경로 조회 실패: {e}")))?
                .filter_map(|r| r.ok())
                .collect();

            // 파일 삭제
            for path in paths {
                let full_path = frames_dir.join(&path);
                let _ = std::fs::remove_file(full_path);
            }
        }

        let deleted = conn
            .execute(
                "DELETE FROM frames WHERE timestamp >= ? AND timestamp <= ?",
                [&request.from, &request.to],
            )
            .map_err(|e| ApiError::Internal(format!("프레임 삭제 실패: {e}")))?;
        result.frames_deleted = deleted as u64;
    }

    // 메트릭 삭제
    if delete_all || data_types.iter().any(|t| t == "metrics") {
        let deleted = conn
            .execute(
                "DELETE FROM system_metrics WHERE timestamp >= ? AND timestamp <= ?",
                [&request.from, &request.to],
            )
            .map_err(|e| ApiError::Internal(format!("메트릭 삭제 실패: {e}")))?;
        result.metrics_deleted = deleted as u64;

        // 시간별 메트릭도 삭제
        let _ = conn.execute(
            "DELETE FROM system_metrics_hourly WHERE hour >= ? AND hour <= ?",
            [&request.from, &request.to],
        );
    }

    // 프로세스 스냅샷 삭제
    if delete_all || data_types.iter().any(|t| t == "processes") {
        let deleted = conn
            .execute(
                "DELETE FROM process_snapshots WHERE timestamp >= ? AND timestamp <= ?",
                [&request.from, &request.to],
            )
            .map_err(|e| ApiError::Internal(format!("프로세스 스냅샷 삭제 실패: {e}")))?;
        result.process_snapshots_deleted = deleted as u64;
    }

    // 유휴 기록 삭제
    if delete_all || data_types.iter().any(|t| t == "idle") {
        let deleted = conn
            .execute(
                "DELETE FROM idle_periods WHERE start_time >= ? AND start_time <= ?",
                [&request.from, &request.to],
            )
            .map_err(|e| ApiError::Internal(format!("유휴 기록 삭제 실패: {e}")))?;
        result.idle_periods_deleted = deleted as u64;
    }

    result.message = format!("{}개의 레코드가 삭제되었습니다", result.total());

    Ok(Json(result))
}

/// DELETE /api/data/all - 모든 데이터 삭제
pub async fn delete_all_data(
    State(state): State<AppState>,
) -> Result<Json<DeleteResult>, ApiError> {
    let conn = state
        .storage
        .conn_ref()
        .lock()
        .map_err(|e| ApiError::Internal(format!("DB 잠금 실패: {e}")))?;

    let mut result = DeleteResult::empty();

    // 프레임 이미지 파일 모두 삭제
    if let Some(ref frames_dir) = state.frames_dir {
        if frames_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(frames_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
    }

    // 모든 테이블 데이터 삭제
    result.events_deleted =
        conn.execute("DELETE FROM events", [])
            .map_err(|e| ApiError::Internal(format!("이벤트 삭제 실패: {e}")))? as u64;

    result.frames_deleted =
        conn.execute("DELETE FROM frames", [])
            .map_err(|e| ApiError::Internal(format!("프레임 삭제 실패: {e}")))? as u64;

    result.metrics_deleted =
        conn.execute("DELETE FROM system_metrics", [])
            .map_err(|e| ApiError::Internal(format!("메트릭 삭제 실패: {e}")))? as u64;

    // 시간별 메트릭도 삭제
    let _ = conn.execute("DELETE FROM system_metrics_hourly", []);

    result.process_snapshots_deleted = conn
        .execute("DELETE FROM process_snapshots", [])
        .map_err(|e| ApiError::Internal(format!("프로세스 스냅샷 삭제 실패: {e}")))?
        as u64;

    result.idle_periods_deleted =
        conn.execute("DELETE FROM idle_periods", [])
            .map_err(|e| ApiError::Internal(format!("유휴 기록 삭제 실패: {e}")))? as u64;

    // 세션 통계도 초기화
    let _ = conn.execute("DELETE FROM session_stats", []);

    result.message = format!("모든 데이터가 삭제되었습니다 ({}개 레코드)", result.total());

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_result_total() {
        let result = DeleteResult {
            success: true,
            events_deleted: 10,
            frames_deleted: 5,
            metrics_deleted: 100,
            process_snapshots_deleted: 20,
            idle_periods_deleted: 3,
            message: String::new(),
        };

        assert_eq!(result.total(), 138);
    }

    #[test]
    fn delete_result_empty() {
        let result = DeleteResult::empty();
        assert!(result.success);
        assert_eq!(result.total(), 0);
    }

    #[test]
    fn delete_range_request_deserializes() {
        let json =
            r#"{"from": "2024-01-01", "to": "2024-01-31", "data_types": ["events", "frames"]}"#;
        let request: DeleteRangeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.from, "2024-01-01");
        assert_eq!(request.to, "2024-01-31");
        assert_eq!(request.data_types.len(), 2);
    }

    #[test]
    fn delete_range_request_default_data_types() {
        let json = r#"{"from": "2024-01-01", "to": "2024-01-31"}"#;
        let request: DeleteRangeRequest = serde_json::from_str(json).unwrap();
        assert!(request.data_types.is_empty());
    }
}
