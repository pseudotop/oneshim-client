use serde::Serialize;
use tauri::command;

use crate::runtime_state::{ConfigRuntimeState, SyncRuntimeState};

#[derive(Serialize)]
pub struct SyncStatusDto {
    pub enabled: bool,
    pub device_id: String,
    pub device_name: String,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
    /// Known peers discovered during the last discovery scan.
    pub peers: Vec<SyncPeerDto>,
}

#[derive(Serialize, Default)]
pub struct SyncResultDto {
    pub applied: usize,
    pub skipped: usize,
    pub tombstoned: usize,
}

#[derive(Serialize, Clone)]
pub struct SyncPeerDto {
    pub device_id: String,
    pub device_name: String,
    pub last_sync_at: String,
}

#[command]
pub async fn get_sync_status(
    state: tauri::State<'_, SyncRuntimeState>,
    config_state: tauri::State<'_, ConfigRuntimeState>,
) -> Result<SyncStatusDto, String> {
    // config.sync.enabled is the authoritative master switch regardless of
    // whether the engine is currently wired up.
    let config_enabled = config_state.config_manager().get().sync.enabled;

    match state.engine() {
        Some(engine) => {
            let (sync_at, error) = engine.health_status();
            // Attempt a lightweight peer discovery to populate the status; ignore
            // errors so that a discovery failure does not fail the status query.
            let peers = engine
                .discover_peers()
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|p| SyncPeerDto {
                    device_id: p.device_id,
                    device_name: p.device_name,
                    last_sync_at: p.last_sync_at,
                })
                .collect();

            Ok(SyncStatusDto {
                enabled: config_enabled,
                device_id: engine.device_id().to_string(),
                device_name: engine.device_name().to_string(),
                last_sync_at: sync_at,
                last_error: error,
                peers,
            })
        }
        None => Ok(SyncStatusDto {
            enabled: config_enabled,
            device_id: String::new(),
            device_name: String::new(),
            last_sync_at: None,
            last_error: None,
            peers: Vec::new(),
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

/// Enable or disable cross-device sync.
///
/// Persists the change to the config file. The engine itself is started/stopped
/// at the next app launch — a live toggle of the background loop is not yet
/// supported and is handled by the scheduler on startup.
#[command]
pub fn set_sync_enabled(
    enabled: bool,
    config_state: tauri::State<'_, ConfigRuntimeState>,
) -> Result<(), String> {
    config_state
        .config_manager()
        .update_with(|config| {
            config.sync.enabled = enabled;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    tracing::info!(enabled, "sync enabled flag updated");
    Ok(())
}

/// Remove a peer from the known-peers list.
///
/// Delegates to the sync engine's transport to evict the peer from the
/// active peer registry (LAN verified-peers map, remote REST endpoint,
/// or file-transport changeset files).
#[command]
pub async fn forget_peer(
    device_id: String,
    state: tauri::State<'_, SyncRuntimeState>,
) -> Result<(), String> {
    let engine = state.engine().ok_or("Sync not enabled")?;
    engine
        .forget_peer(&device_id)
        .await
        .map_err(|e| e.to_string())
}
