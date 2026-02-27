use axum::{
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use oneshim_api_contracts::backup::{
    BackupArchive, BackupIncludes, BackupMetadata, BackupQuery, EventBackup, FrameBackup,
    FrameTagBackup, RestoreResult, RestoredCounts, SettingsBackup, TagBackup,
};

use crate::{error::ApiError, AppState};

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

    if params.include_settings {
        let settings = backup_settings_from_state(&state);
        archive.settings = Some(settings);
    }

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
        .map_err(|e| ApiError::Internal(format!("JSON serialization failed: {e}")))?;

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

    if let Some(settings) = &archive.settings {
        match restore_settings_to_state(&state, settings) {
            Ok(_) => restored.settings = true,
            Err(e) => errors.push(format!("Failed to restore settings: {e}")),
        }
    }

    if let Some(tags) = &archive.tags {
        for tag in tags {
            match state
                .storage
                .upsert_backup_tag(tag.id, &tag.name, &tag.color, &tag.created_at)
            {
                Ok(_) => restored.tags += 1,
                Err(e) => errors.push(format!("Failed to restore tag '{}': {e}", tag.name)),
            }
        }
    }

    if let Some(frame_tags) = &archive.frame_tags {
        for ft in frame_tags {
            match state
                .storage
                .upsert_backup_frame_tag(ft.frame_id, ft.tag_id, &ft.created_at)
            {
                Ok(_) => restored.frame_tags += 1,
                Err(e) => errors.push(format!("Failed to restore frame-tag relation: {e}")),
            }
        }
    }

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
                Err(e) => errors.push(format!("Failed to restore event: {e}")),
            }
        }
    }

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
                Err(e) => errors.push(format!("Failed to restore frame: {e}")),
            }
        }
    }

    Ok(Json(RestoreResult {
        success: errors.is_empty(),
        restored,
        errors,
    }))
}

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
        .ok_or_else(|| ApiError::Internal("Cannot restore without config manager".to_string()))?;

    config_manager
        .update_with(|config| {
            config.vision.capture_enabled = settings.capture_enabled;
            config.vision.capture_throttle_ms = settings.capture_interval_secs.saturating_mul(1000);
            config.monitor.idle_threshold_secs = settings.idle_threshold_secs;
            config.monitor.poll_interval_ms = settings.metrics_interval_secs.saturating_mul(1000);
            config.web.port = settings.web_port;
            config.notification.enabled = settings.notification_enabled;
            config.notification.idle_notification_mins = settings.idle_notification_mins as u32;
            config.notification.long_session_mins = settings.long_session_notification_mins as u32;
            config.notification.high_usage_threshold = settings.high_usage_threshold_percent as u32;
        })
        .map_err(|e| ApiError::Internal(format!("Failed to save settings: {e}")))?;

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
