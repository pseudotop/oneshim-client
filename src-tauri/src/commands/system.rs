use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::Utc;
use oneshim_api_contracts::support::RuntimeLogSnapshotDto;
use tauri::command;

use crate::feature_capabilities::{
    build_feature_capability_snapshot,
    probe_provider_surface_endpoint as probe_provider_surface_endpoint_impl,
    FeatureCapabilitySnapshot, FeatureCapabilityState, ProviderEndpointProbeResult,
};
use crate::runtime_state::{AppState, SecretBackendCapabilities, SecretBackendState};

const DEFAULT_LOG_LINE_LIMIT: usize = 200;
const MAX_LOG_LINE_LIMIT: usize = 500;

fn runtime_log_dir() -> PathBuf {
    oneshim_core::config_manager::ConfigManager::data_dir()
        .map(|d| d.join("logs"))
        .unwrap_or_else(|_| PathBuf::from("logs"))
}

fn newest_log_file(log_dir: &Path) -> Result<Option<PathBuf>, String> {
    if !log_dir.exists() {
        return Ok(None);
    }

    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    let entries = std::fs::read_dir(log_dir).map_err(|err| {
        format!(
            "Failed to read runtime log directory '{}': {err}",
            log_dir.display()
        )
    })?;

    for entry in entries {
        let entry =
            entry.map_err(|err| format!("Failed to inspect runtime log directory entry: {err}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        match newest.as_ref() {
            Some((current_modified, _)) if modified <= *current_modified => {}
            _ => newest = Some((modified, path)),
        }
    }

    Ok(newest.map(|(_, path)| path))
}

fn tail_log_file(path: &Path, line_limit: usize) -> Result<(usize, String), String> {
    let file = File::open(path).map_err(|err| {
        format!(
            "Failed to open runtime log file '{}': {err}",
            path.display()
        )
    })?;
    let reader = BufReader::new(file);
    let mut lines = VecDeque::with_capacity(line_limit);

    for line in reader.lines() {
        let line = line.map_err(|err| {
            format!(
                "Failed to read runtime log file '{}': {err}",
                path.display()
            )
        })?;
        if lines.len() == line_limit {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    let count = lines.len();
    let recent_text = lines.into_iter().collect::<Vec<_>>().join("\n");
    Ok((count, recent_text))
}

fn runtime_log_snapshot_from_dir(
    log_dir: &Path,
    line_limit: usize,
) -> Result<RuntimeLogSnapshotDto, String> {
    let latest_log = newest_log_file(log_dir)?;
    let (log_file, line_count, recent_text) = if let Some(path) = latest_log {
        let (line_count, recent_text) = tail_log_file(&path, line_limit)?;
        (Some(path.display().to_string()), line_count, recent_text)
    } else {
        (None, 0, String::new())
    };

    Ok(RuntimeLogSnapshotDto {
        generated_at: Utc::now().to_rfc3339(),
        log_dir: log_dir.display().to_string(),
        log_file,
        line_count,
        recent_text,
    })
}

/// 자동화 상태 조회 — 사용자 설정 기반 반환
#[command]
pub async fn get_automation_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(state.config_manager.get().automation.enabled)
}

/// Secret backend capability snapshot for desktop runtime surfaces.
#[command]
pub async fn get_secret_backend_capabilities(
    state: tauri::State<'_, SecretBackendState>,
) -> Result<SecretBackendCapabilities, String> {
    Ok(state.0.clone())
}

/// Generic feature capability + maturity snapshot for desktop runtime surfaces.
#[command]
pub async fn get_feature_capabilities(
    state: tauri::State<'_, FeatureCapabilityState>,
) -> Result<FeatureCapabilitySnapshot, String> {
    let secret_backend = state.0.clone();
    Ok(build_feature_capability_snapshot(&secret_backend).await)
}

/// Probe the currently configured provider endpoint for a direct/self-hosted surface.
#[command]
pub async fn probe_provider_surface_endpoint(
    surface_id: String,
    endpoint_kind: String,
    endpoint: String,
) -> Result<ProviderEndpointProbeResult, String> {
    Ok(probe_provider_surface_endpoint_impl(&surface_id, &endpoint_kind, &endpoint).await)
}

#[command]
pub async fn get_runtime_log_snapshot(
    line_limit: Option<usize>,
) -> Result<RuntimeLogSnapshotDto, String> {
    let line_limit = line_limit
        .unwrap_or(DEFAULT_LOG_LINE_LIMIT)
        .clamp(10, MAX_LOG_LINE_LIMIT);
    runtime_log_snapshot_from_dir(&runtime_log_dir(), line_limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn runtime_log_snapshot_returns_empty_when_directory_is_missing() {
        let dir = PathBuf::from("/nonexistent/oneshim-log-tests");
        let snapshot =
            runtime_log_snapshot_from_dir(&dir, 50).expect("snapshot should still succeed");

        assert_eq!(snapshot.log_dir, dir.display().to_string());
        assert!(snapshot.log_file.is_none());
        assert_eq!(snapshot.line_count, 0);
        assert!(snapshot.recent_text.is_empty());
    }

    #[test]
    fn runtime_log_snapshot_reads_tail_of_newest_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let older = temp.path().join("oneshim.log.older");
        let newer = temp.path().join("oneshim.log.newer");

        fs::write(&older, "old-1\nold-2\n").expect("write older log");
        thread::sleep(Duration::from_millis(20));
        fs::write(&newer, "new-1\nnew-2\nnew-3\n").expect("write newer log");

        let snapshot =
            runtime_log_snapshot_from_dir(temp.path(), 2).expect("snapshot should succeed");
        let log_file = snapshot.log_file.expect("newest file should be selected");

        assert_eq!(snapshot.log_dir, temp.path().display().to_string());
        assert!(log_file.ends_with("oneshim.log.newer"));
        assert_eq!(snapshot.line_count, 2);
        assert_eq!(snapshot.recent_text, "new-2\nnew-3");
    }
}
