use serde::Serialize;
use tauri::command;

use crate::runtime_state::SyncRuntimeState;

#[derive(Serialize)]
pub struct SyncStatusDto {
    pub enabled: bool,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Serialize, Default)]
pub struct SyncResultDto {
    pub applied: usize,
    pub skipped: usize,
    pub tombstoned: usize,
}

#[derive(Serialize)]
pub struct SyncPeerDto {
    pub device_id: String,
    pub device_name: String,
    pub last_sync_at: String,
}

#[command]
pub async fn get_sync_status(
    state: tauri::State<'_, SyncRuntimeState>,
) -> Result<SyncStatusDto, String> {
    match state.engine() {
        Some(engine) => Ok(SyncStatusDto {
            enabled: true,
            device_id: engine.device_id().to_string(),
            device_name: engine.device_name().to_string(),
        }),
        None => Ok(SyncStatusDto {
            enabled: false,
            device_id: String::new(),
            device_name: String::new(),
        }),
    }
}

#[command]
pub async fn trigger_sync_cycle(
    state: tauri::State<'_, SyncRuntimeState>,
) -> Result<SyncResultDto, String> {
    let engine = state.engine().ok_or("Sync not enabled")?;

    match engine.run_cycle().await {
        Ok(Some(result)) => Ok(SyncResultDto {
            applied: result.applied,
            skipped: result.skipped_lww + result.skipped_dup,
            tombstoned: result.tombstoned,
        }),
        Ok(None) => Ok(SyncResultDto::default()),
        Err(e) => Err(e.to_string()),
    }
}

#[command]
pub async fn discover_sync_peers(
    state: tauri::State<'_, SyncRuntimeState>,
) -> Result<Vec<SyncPeerDto>, String> {
    let engine = state.engine().ok_or("Sync not enabled")?;

    let peers = engine.discover_peers().await.map_err(|e| e.to_string())?;

    Ok(peers
        .into_iter()
        .map(|p| SyncPeerDto {
            device_id: p.device_id,
            device_name: p.device_name,
            last_sync_at: p.last_sync_at,
        })
        .collect())
}
