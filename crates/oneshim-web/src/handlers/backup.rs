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
        let settings = backup_settings_from_state(&state);
        archive.settings = Some(settings);
    }

    // 태그 백업
    if params.include_tags {
        let tags = state
            .storage
            .list_backup_tags()
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .into_iter()
            .map(|tag| TagBackup {
                id: tag.id,
                name: tag.name,
                color: tag.color,
                created_at: tag.created_at,
            })
            .collect();

        let frame_tags = state
            .storage
            .list_backup_frame_tags()
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .into_iter()
            .map(|ft| FrameTagBackup {
                frame_id: ft.frame_id,
                tag_id: ft.tag_id,
                created_at: ft.created_at,
            })
            .collect();

        archive.tags = Some(tags);
        archive.frame_tags = Some(frame_tags);
    }

    // 이벤트 백업
    if params.include_events {
        let events = state
            .storage
            .list_event_exports("0000-01-01T00:00:00Z", "9999-12-31T23:59:59Z")
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .into_iter()
            .map(|event| EventBackup {
                event_id: event.event_id,
                event_type: event.event_type,
                timestamp: event.timestamp,
                app_name: event.app_name,
                window_title: event.window_title,
            })
            .collect();
        archive.events = Some(events);
    }

    // 프레임 메타데이터 백업
    if params.include_frames {
        let frames = state
            .storage
            .list_frame_exports("0000-01-01T00:00:00Z", "9999-12-31T23:59:59Z")
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .into_iter()
            .map(|frame| FrameBackup {
                id: frame.id,
                timestamp: frame.timestamp,
                trigger_type: frame.trigger_type,
                app_name: frame.app_name,
                window_title: frame.window_title,
                importance: frame.importance,
                width: frame.resolution_w as i32,
                height: frame.resolution_h as i32,
                ocr_text: frame.ocr_text,
            })
            .collect();
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
        match restore_settings_to_state(&state, settings) {
            Ok(_) => restored.settings = true,
            Err(e) => errors.push(format!("설정 복원 실패: {e}")),
        }
    }

    // 태그 복원 (기존 태그 유지, 새 태그만 추가)
    if let Some(tags) = &archive.tags {
        for tag in tags {
            match state
                .storage
                .upsert_backup_tag(tag.id, &tag.name, &tag.color, &tag.created_at)
            {
                Ok(_) => restored.tags += 1,
                Err(e) => errors.push(format!("태그 '{}' 복원 실패: {e}", tag.name)),
            }
        }
    }

    // 프레임-태그 연결 복원
    if let Some(frame_tags) = &archive.frame_tags {
        for ft in frame_tags {
            match state
                .storage
                .upsert_backup_frame_tag(ft.frame_id, ft.tag_id, &ft.created_at)
            {
                Ok(_) => restored.frame_tags += 1,
                Err(e) => errors.push(format!("프레임-태그 연결 복원 실패: {e}")),
            }
        }
    }

    // 이벤트 복원
    if let Some(events) = &archive.events {
        for event in events {
            match state.storage.upsert_backup_event(
                &event.event_id,
                &event.event_type,
                &event.timestamp,
                event.app_name.as_deref(),
                event.window_title.as_deref(),
            ) {
                Ok(_) => restored.events += 1,
                Err(e) => errors.push(format!("이벤트 복원 실패: {e}")),
            }
        }
    }

    // 프레임 메타데이터 복원
    if let Some(frames) = &archive.frames {
        for frame in frames {
            match state.storage.upsert_backup_frame(
                frame.id,
                &frame.timestamp,
                &frame.trigger_type,
                &frame.app_name,
                &frame.window_title,
                frame.importance,
                frame.width,
                frame.height,
                frame.ocr_text.as_deref(),
            ) {
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
fn backup_settings_from_state(state: &AppState) -> SettingsBackup {
    if let Some(ref config_manager) = state.config_manager {
        let config = config_manager.get();
        SettingsBackup {
            capture_enabled: config.vision.capture_enabled,
            capture_interval_secs: (config.vision.capture_throttle_ms / 1000).max(1),
            idle_threshold_secs: config.monitor.idle_threshold_secs,
            metrics_interval_secs: config.monitor.poll_interval_ms / 1000,
            web_port: config.web.port,
            notification_enabled: config.notification.enabled,
            idle_notification_mins: config.notification.idle_notification_mins as u64,
            long_session_notification_mins: config.notification.long_session_mins as u64,
            high_usage_threshold_percent: config.notification.high_usage_threshold as u8,
        }
    } else {
        SettingsBackup {
            capture_enabled: true,
            capture_interval_secs: 60,
            idle_threshold_secs: 300,
            metrics_interval_secs: 5,
            web_port: 9090,
            notification_enabled: true,
            idle_notification_mins: 30,
            long_session_notification_mins: 60,
            high_usage_threshold_percent: 90,
        }
    }
}

fn restore_settings_to_state(state: &AppState, settings: &SettingsBackup) -> Result<(), ApiError> {
    let config_manager = state
        .config_manager
        .as_ref()
        .ok_or_else(|| ApiError::Internal("설정 관리자가 없어 복원할 수 없습니다".to_string()))?;

    config_manager
        .update_with(|config| {
            config.vision.capture_enabled = settings.capture_enabled;
            config.vision.capture_throttle_ms = settings.capture_interval_secs.saturating_mul(1000);
            config.monitor.idle_threshold_secs = settings.idle_threshold_secs;
            config.monitor.poll_interval_ms = settings.metrics_interval_secs.saturating_mul(1000);
            config.web.port = settings.web_port;
            config.notification.enabled = settings.notification_enabled;
            config.notification.idle_notification_mins = settings.idle_notification_mins as u32;
            config.notification.long_session_mins =
                settings.long_session_notification_mins as u32;
            config.notification.high_usage_threshold =
                settings.high_usage_threshold_percent as u32;
        })
        .map_err(|e| ApiError::Internal(format!("설정 저장 실패: {e}")))?;

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
