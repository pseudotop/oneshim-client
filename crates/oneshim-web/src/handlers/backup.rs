//! 백업/복원 API 핸들러.
//!
//! 설정, 태그, 프레임 메타데이터를 JSON 아카이브로 백업/복원합니다.

use axum::{
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, AppState};

/// 백업 쿼리 파라미터
#[derive(Debug, Deserialize)]
pub struct BackupQuery {
    /// 설정 포함 여부 (기본: true)
    #[serde(default = "default_true")]
    pub include_settings: bool,
    /// 태그 포함 여부 (기본: true)
    #[serde(default = "default_true")]
    pub include_tags: bool,
    /// 이벤트 포함 여부 (기본: false)
    #[serde(default)]
    pub include_events: bool,
    /// 프레임 메타데이터 포함 여부 (기본: false)
    #[serde(default)]
    pub include_frames: bool,
}

fn default_true() -> bool {
    true
}

/// 백업 메타데이터
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupMetadata {
    /// 백업 버전
    pub version: String,
    /// 생성 시각
    pub created_at: String,
    /// 앱 버전
    pub app_version: String,
    /// 포함된 데이터 유형
    pub includes: BackupIncludes,
}

/// 백업 포함 여부
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupIncludes {
    pub settings: bool,
    pub tags: bool,
    pub events: bool,
    pub frames: bool,
}

/// 태그 백업 레코드
#[derive(Debug, Serialize, Deserialize)]
pub struct TagBackup {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub created_at: String,
}

/// 프레임-태그 연결 백업
#[derive(Debug, Serialize, Deserialize)]
pub struct FrameTagBackup {
    pub frame_id: i64,
    pub tag_id: i64,
    pub created_at: String,
}

/// 설정 백업
#[derive(Debug, Serialize, Deserialize)]
pub struct SettingsBackup {
    pub capture_enabled: bool,
    pub capture_interval_secs: u64,
    pub idle_threshold_secs: u64,
    pub metrics_interval_secs: u64,
    pub web_port: u16,
    pub notification_enabled: bool,
    pub idle_notification_mins: u64,
    pub long_session_notification_mins: u64,
    pub high_usage_threshold_percent: u8,
}

/// 이벤트 백업 레코드
#[derive(Debug, Serialize, Deserialize)]
pub struct EventBackup {
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
}

/// 프레임 메타데이터 백업 (이미지 제외)
#[derive(Debug, Serialize, Deserialize)]
pub struct FrameBackup {
    pub id: i64,
    pub timestamp: String,
    pub trigger_type: String,
    pub app_name: String,
    pub window_title: String,
    pub importance: f32,
    pub width: i32,
    pub height: i32,
    pub ocr_text: Option<String>,
}

/// 전체 백업 아카이브
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupArchive {
    pub metadata: BackupMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<SettingsBackup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<TagBackup>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_tags: Option<Vec<FrameTagBackup>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<EventBackup>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<Vec<FrameBackup>>,
}

/// 복원 결과
#[derive(Debug, Serialize)]
pub struct RestoreResult {
    pub success: bool,
    pub restored: RestoredCounts,
    pub errors: Vec<String>,
}

/// 복원된 항목 수
#[derive(Debug, Serialize)]
pub struct RestoredCounts {
    pub settings: bool,
    pub tags: u64,
    pub frame_tags: u64,
    pub events: u64,
    pub frames: u64,
}

/// GET /api/backup - 백업 생성
pub async fn create_backup(
    State(state): State<AppState>,
    Query(params): Query<BackupQuery>,
) -> Result<Response, ApiError> {
    let conn = state
        .storage
        .conn_ref()
        .lock()
        .map_err(|e| ApiError::Internal(format!("DB 잠금 실패: {e}")))?;

    let mut archive = BackupArchive {
        metadata: BackupMetadata {
            version: "1.0".to_string(),
            created_at: Utc::now().to_rfc3339(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            includes: BackupIncludes {
                settings: params.include_settings,
                tags: params.include_tags,
                events: params.include_events,
                frames: params.include_frames,
            },
        },
        settings: None,
        tags: None,
        frame_tags: None,
        events: None,
        frames: None,
    };

    // 설정 백업
    if params.include_settings {
        let settings = backup_settings(&conn)?;
        archive.settings = Some(settings);
    }

    // 태그 백업
    if params.include_tags {
        let (tags, frame_tags) = backup_tags(&conn)?;
        archive.tags = Some(tags);
        archive.frame_tags = Some(frame_tags);
    }

    // 이벤트 백업
    if params.include_events {
        let events = backup_events(&conn)?;
        archive.events = Some(events);
    }

    // 프레임 메타데이터 백업
    if params.include_frames {
        let frames = backup_frames(&conn)?;
        archive.frames = Some(frames);
    }

    let json = serde_json::to_string_pretty(&archive)
        .map_err(|e| ApiError::Internal(format!("JSON 직렬화 실패: {e}")))?;

    let now = Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("oneshim_backup_{now}.json");

    Ok((
        [
            (header::CONTENT_TYPE, "application/json; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                &format!("attachment; filename=\"{filename}\""),
            ),
        ],
        json,
    )
        .into_response())
}

/// POST /api/backup/restore - 백업 복원
pub async fn restore_backup(
    State(state): State<AppState>,
    Json(archive): Json<BackupArchive>,
) -> Result<Json<RestoreResult>, ApiError> {
    let conn = state
        .storage
        .conn_ref()
        .lock()
        .map_err(|e| ApiError::Internal(format!("DB 잠금 실패: {e}")))?;

    let mut errors = Vec::new();
    let mut restored = RestoredCounts {
        settings: false,
        tags: 0,
        frame_tags: 0,
        events: 0,
        frames: 0,
    };

    // 설정 복원
    if let Some(settings) = &archive.settings {
        match restore_settings(&conn, settings) {
            Ok(_) => restored.settings = true,
            Err(e) => errors.push(format!("설정 복원 실패: {e}")),
        }
    }

    // 태그 복원 (기존 태그 유지, 새 태그만 추가)
    if let Some(tags) = &archive.tags {
        for tag in tags {
            match restore_tag(&conn, tag) {
                Ok(_) => restored.tags += 1,
                Err(e) => errors.push(format!("태그 '{}' 복원 실패: {e}", tag.name)),
            }
        }
    }

    // 프레임-태그 연결 복원
    if let Some(frame_tags) = &archive.frame_tags {
        for ft in frame_tags {
            match restore_frame_tag(&conn, ft) {
                Ok(_) => restored.frame_tags += 1,
                Err(e) => errors.push(format!("프레임-태그 연결 복원 실패: {e}")),
            }
        }
    }

    // 이벤트 복원
    if let Some(events) = &archive.events {
        for event in events {
            match restore_event(&conn, event) {
                Ok(_) => restored.events += 1,
                Err(e) => errors.push(format!("이벤트 복원 실패: {e}")),
            }
        }
    }

    // 프레임 메타데이터 복원
    if let Some(frames) = &archive.frames {
        for frame in frames {
            match restore_frame(&conn, frame) {
                Ok(_) => restored.frames += 1,
                Err(e) => errors.push(format!("프레임 복원 실패: {e}")),
            }
        }
    }

    Ok(Json(RestoreResult {
        success: errors.is_empty(),
        restored,
        errors,
    }))
}

/// 설정 백업
fn backup_settings(conn: &rusqlite::Connection) -> Result<SettingsBackup, ApiError> {
    let mut stmt = conn
        .prepare(
            "SELECT key, value FROM settings WHERE key IN (
                'capture_enabled', 'capture_interval_secs', 'idle_threshold_secs',
                'metrics_interval_secs', 'web_port', 'notification_enabled',
                'idle_notification_mins', 'long_session_notification_mins', 'high_usage_threshold_percent'
            )",
        )
        .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

    let mut settings = SettingsBackup {
        capture_enabled: true,
        capture_interval_secs: 60,
        idle_threshold_secs: 300,
        metrics_interval_secs: 5,
        web_port: 9090,
        notification_enabled: true,
        idle_notification_mins: 30,
        long_session_notification_mins: 60,
        high_usage_threshold_percent: 90,
    };

    let rows = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            let value: String = row.get(1)?;
            Ok((key, value))
        })
        .map_err(|e| ApiError::Internal(format!("쿼리 실행 실패: {e}")))?;

    for row in rows.flatten() {
        let (key, value) = row;
        match key.as_str() {
            "capture_enabled" => settings.capture_enabled = value == "true",
            "capture_interval_secs" => {
                settings.capture_interval_secs = value.parse().unwrap_or(60);
            }
            "idle_threshold_secs" => {
                settings.idle_threshold_secs = value.parse().unwrap_or(300);
            }
            "metrics_interval_secs" => {
                settings.metrics_interval_secs = value.parse().unwrap_or(5);
            }
            "web_port" => {
                settings.web_port = value.parse().unwrap_or(9090);
            }
            "notification_enabled" => settings.notification_enabled = value == "true",
            "idle_notification_mins" => {
                settings.idle_notification_mins = value.parse().unwrap_or(30);
            }
            "long_session_notification_mins" => {
                settings.long_session_notification_mins = value.parse().unwrap_or(60);
            }
            "high_usage_threshold_percent" => {
                settings.high_usage_threshold_percent = value.parse().unwrap_or(90);
            }
            _ => {}
        }
    }

    Ok(settings)
}

/// 태그 및 프레임-태그 연결 백업
fn backup_tags(
    conn: &rusqlite::Connection,
) -> Result<(Vec<TagBackup>, Vec<FrameTagBackup>), ApiError> {
    // 태그 백업
    let mut stmt = conn
        .prepare("SELECT id, name, color, created_at FROM tags ORDER BY id")
        .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

    let tags: Vec<TagBackup> = stmt
        .query_map([], |row| {
            Ok(TagBackup {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| ApiError::Internal(format!("쿼리 실행 실패: {e}")))?
        .filter_map(|r| r.ok())
        .collect();

    // 프레임-태그 연결 백업
    let mut stmt = conn
        .prepare("SELECT frame_id, tag_id, created_at FROM frame_tags ORDER BY frame_id, tag_id")
        .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

    let frame_tags: Vec<FrameTagBackup> = stmt
        .query_map([], |row| {
            Ok(FrameTagBackup {
                frame_id: row.get(0)?,
                tag_id: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(|e| ApiError::Internal(format!("쿼리 실행 실패: {e}")))?
        .filter_map(|r| r.ok())
        .collect();

    Ok((tags, frame_tags))
}

/// 이벤트 백업
fn backup_events(conn: &rusqlite::Connection) -> Result<Vec<EventBackup>, ApiError> {
    let mut stmt = conn
        .prepare(
            "SELECT event_id, event_type, timestamp, app_name, window_title
             FROM events ORDER BY timestamp",
        )
        .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

    let events: Vec<EventBackup> = stmt
        .query_map([], |row| {
            Ok(EventBackup {
                event_id: row.get(0)?,
                event_type: row.get(1)?,
                timestamp: row.get(2)?,
                app_name: row.get(3)?,
                window_title: row.get(4)?,
            })
        })
        .map_err(|e| ApiError::Internal(format!("쿼리 실행 실패: {e}")))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(events)
}

/// 프레임 메타데이터 백업 (이미지 제외)
fn backup_frames(conn: &rusqlite::Connection) -> Result<Vec<FrameBackup>, ApiError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, trigger_type, app_name, window_title, importance, width, height, ocr_text
             FROM frames ORDER BY timestamp",
        )
        .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

    let frames: Vec<FrameBackup> = stmt
        .query_map([], |row| {
            Ok(FrameBackup {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                trigger_type: row.get(2)?,
                app_name: row.get(3)?,
                window_title: row.get(4)?,
                importance: row.get(5)?,
                width: row.get(6)?,
                height: row.get(7)?,
                ocr_text: row.get(8)?,
            })
        })
        .map_err(|e| ApiError::Internal(format!("쿼리 실행 실패: {e}")))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(frames)
}

/// 설정 복원
fn restore_settings(
    conn: &rusqlite::Connection,
    settings: &SettingsBackup,
) -> Result<(), ApiError> {
    let settings_map = [
        ("capture_enabled", settings.capture_enabled.to_string()),
        (
            "capture_interval_secs",
            settings.capture_interval_secs.to_string(),
        ),
        (
            "idle_threshold_secs",
            settings.idle_threshold_secs.to_string(),
        ),
        (
            "metrics_interval_secs",
            settings.metrics_interval_secs.to_string(),
        ),
        ("web_port", settings.web_port.to_string()),
        (
            "notification_enabled",
            settings.notification_enabled.to_string(),
        ),
        (
            "idle_notification_mins",
            settings.idle_notification_mins.to_string(),
        ),
        (
            "long_session_notification_mins",
            settings.long_session_notification_mins.to_string(),
        ),
        (
            "high_usage_threshold_percent",
            settings.high_usage_threshold_percent.to_string(),
        ),
    ];

    for (key, value) in settings_map {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            [key, &value],
        )
        .map_err(|e| ApiError::Internal(format!("설정 저장 실패: {e}")))?;
    }

    Ok(())
}

/// 태그 복원 (중복 무시)
fn restore_tag(conn: &rusqlite::Connection, tag: &TagBackup) -> Result<(), ApiError> {
    conn.execute(
        "INSERT OR IGNORE INTO tags (id, name, color, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![tag.id, tag.name, tag.color, tag.created_at],
    )
    .map_err(|e| ApiError::Internal(format!("태그 저장 실패: {e}")))?;

    Ok(())
}

/// 프레임-태그 연결 복원 (중복 무시)
fn restore_frame_tag(conn: &rusqlite::Connection, ft: &FrameTagBackup) -> Result<(), ApiError> {
    conn.execute(
        "INSERT OR IGNORE INTO frame_tags (frame_id, tag_id, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![ft.frame_id, ft.tag_id, ft.created_at],
    )
    .map_err(|e| ApiError::Internal(format!("프레임-태그 연결 저장 실패: {e}")))?;

    Ok(())
}

/// 이벤트 복원 (중복 무시)
fn restore_event(conn: &rusqlite::Connection, event: &EventBackup) -> Result<(), ApiError> {
    conn.execute(
        "INSERT OR IGNORE INTO events (event_id, event_type, timestamp, app_name, window_title)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            event.event_id,
            event.event_type,
            event.timestamp,
            event.app_name,
            event.window_title
        ],
    )
    .map_err(|e| ApiError::Internal(format!("이벤트 저장 실패: {e}")))?;

    Ok(())
}

/// 프레임 메타데이터 복원 (중복 무시, 이미지 없음)
fn restore_frame(conn: &rusqlite::Connection, frame: &FrameBackup) -> Result<(), ApiError> {
    // 프레임 존재 여부 확인
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM frames WHERE id = ?1)",
            [frame.id],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !exists {
        // 이미지 없이 메타데이터만 삽입 (이미지는 빈 바이트)
        conn.execute(
            "INSERT INTO frames (id, timestamp, trigger_type, app_name, window_title, importance, width, height, ocr_text, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                frame.id,
                frame.timestamp,
                frame.trigger_type,
                frame.app_name,
                frame.window_title,
                frame.importance,
                frame.width,
                frame.height,
                frame.ocr_text,
                Vec::<u8>::new() // 빈 이미지 데이터
            ],
        )
        .map_err(|e| ApiError::Internal(format!("프레임 저장 실패: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_query_defaults() {
        let query: BackupQuery = serde_json::from_str("{}").unwrap();
        assert!(query.include_settings);
        assert!(query.include_tags);
        assert!(!query.include_events);
        assert!(!query.include_frames);
    }

    #[test]
    fn backup_archive_serializes() {
        let archive = BackupArchive {
            metadata: BackupMetadata {
                version: "1.0".to_string(),
                created_at: "2024-01-30T10:00:00Z".to_string(),
                app_version: "0.1.0".to_string(),
                includes: BackupIncludes {
                    settings: true,
                    tags: true,
                    events: false,
                    frames: false,
                },
            },
            settings: None,
            tags: Some(vec![TagBackup {
                id: 1,
                name: "Work".to_string(),
                color: "#3b82f6".to_string(),
                created_at: "2024-01-30T10:00:00Z".to_string(),
            }]),
            frame_tags: None,
            events: None,
            frames: None,
        };

        let json = serde_json::to_string(&archive).unwrap();
        assert!(json.contains("\"version\":\"1.0\""));
        assert!(json.contains("\"Work\""));
    }

    #[test]
    fn restore_result_serializes() {
        let result = RestoreResult {
            success: true,
            restored: RestoredCounts {
                settings: true,
                tags: 5,
                frame_tags: 10,
                events: 0,
                frames: 0,
            },
            errors: vec![],
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"tags\":5"));
    }
}
