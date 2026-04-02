//! Tauri IPC commands for audio capture and speech-to-text.

use std::sync::Arc;

use tauri::{command, Emitter};

use oneshim_core::models::audio::TranscriptionResult;

use crate::runtime_state::AppState;

/// Start microphone capture (Push-to-Talk begin).
#[command]
pub async fn start_audio_capture(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let capture = state
        .audio
        .capture
        .as_ref()
        .ok_or_else(|| "audio capture not available".to_string())?;
    capture.start().map_err(|e| e.to_string())
}

/// Stop capture and transcribe the recorded audio.
#[command]
pub async fn stop_and_transcribe(
    state: tauri::State<'_, AppState>,
) -> Result<TranscriptionResult, String> {
    let capture = state
        .audio
        .capture
        .as_ref()
        .ok_or_else(|| "audio capture not available".to_string())?;

    let stt = {
        let guard = state.audio.stt_engine.read().await;
        guard
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| "STT engine not available (model may not be loaded)".to_string())?
    };

    let buffer = capture.stop().map_err(|e| e.to_string())?;

    if buffer.is_empty() {
        return Ok(TranscriptionResult {
            text: String::new(),
            language: None,
            duration_secs: 0.0,
            processing_secs: 0.0,
        });
    }

    stt.transcribe(buffer).await.map_err(|e| e.to_string())
}

use std::sync::atomic::Ordering;

use oneshim_core::config::WhisperModelSize;
use oneshim_core::models::audio::{AudioStatus, ModelDownloadStatus};

/// Get combined audio subsystem status.
#[command]
pub async fn get_audio_status(state: tauri::State<'_, AppState>) -> Result<AudioStatus, String> {
    let model_status = match &state.audio.model_downloader {
        Some(dl) => dl.model_status(state.config.audio.model_size, &state.audio.model_dir),
        None => ModelDownloadStatus::NotInstalled,
    };
    let stt_loaded = state.audio.stt_engine.read().await.is_some();
    Ok(AudioStatus {
        enabled: state.config.audio.enabled,
        selected_model: state.config.audio.model_size,
        model_status,
        stt_provider_loaded: stt_loaded,
        stt_provider: format!("{:?}", state.config.audio.stt_provider).to_lowercase(),
    })
}

/// Start downloading a Whisper model with progress events.
#[command]
pub async fn download_whisper_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    model_size: WhisperModelSize,
) -> Result<(), String> {
    // Guard: reject if already downloading
    if state.audio.downloading.swap(true, Ordering::SeqCst) {
        return Err("a download is already in progress".into());
    }
    // Reset cancel flag
    state.audio.download_cancel.store(false, Ordering::SeqCst);

    let downloader = state
        .audio
        .model_downloader
        .as_ref()
        .ok_or_else(|| "model downloader not available".to_string())?
        .clone();
    let model_dir = state.audio.model_dir.clone();
    let cancel = state.audio.download_cancel.clone();
    let downloading = state.audio.downloading.clone();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Bridge progress channel -> Tauri events
    let app_handle = app.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_handle.emit("audio-model-progress", &progress);
        }
    });

    // Spawn download task
    let app_clone = app.clone();
    tokio::spawn(async move {
        let result = downloader
            .download(model_size, &model_dir, tx, cancel)
            .await;
        downloading.store(false, Ordering::SeqCst);
        match result {
            Ok(path) => {
                let _ = app_clone.emit(
                    "audio-model-complete",
                    serde_json::json!({
                        "path": path.to_string_lossy(),
                        "model_size": model_size,
                        "size_bytes": std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                    }),
                );
            }
            Err(e) => {
                let _ = app_clone.emit(
                    "audio-model-error",
                    serde_json::json!({ "message": e.to_string() }),
                );
            }
        }
    });

    Ok(())
}

/// Cancel an active model download.
#[command]
pub async fn cancel_model_download(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.audio.download_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// Delete a downloaded Whisper model.
#[command]
pub async fn delete_whisper_model(
    state: tauri::State<'_, AppState>,
    model_size: WhisperModelSize,
) -> Result<(), String> {
    let dl = state
        .audio
        .model_downloader
        .as_ref()
        .ok_or_else(|| "model downloader not available".to_string())?;
    dl.delete_model(model_size, &state.audio.model_dir)
        .map_err(|e| e.to_string())
}

/// Reload STT engine with current config — creates Local, Cloud, or Fallback provider.
#[command]
pub async fn reload_stt_engine(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    use oneshim_core::config::SttProviderKind;

    let config = &state.config.audio;

    // Build local provider (if model available)
    let local_provider: Option<Arc<dyn oneshim_core::ports::stt_provider::SttProvider>> = {
        #[cfg(feature = "stt")]
        {
            #[cfg(feature = "download")]
            let model_path =
                state
                    .audio
                    .model_dir
                    .join(oneshim_audio::model_downloader::model_filename(
                        config.model_size,
                    ));
            #[cfg(not(feature = "download"))]
            let model_path = std::path::PathBuf::from(&config.whisper_model_path);

            if model_path.exists() {
                match oneshim_audio::WhisperSttProvider::new(&model_path, config.language) {
                    Ok(p) => Some(Arc::new(p) as _),
                    Err(e) => {
                        tracing::warn!("Failed to load local Whisper: {e}");
                        None
                    }
                }
            } else {
                None
            }
        }
        #[cfg(not(feature = "stt"))]
        {
            None
        }
    };

    // Build cloud provider (if key configured)
    let cloud_provider: Option<Arc<dyn oneshim_core::ports::stt_provider::SttProvider>> = {
        #[cfg(feature = "cloud-stt")]
        {
            if !config.cloud_api_key.is_empty() {
                match oneshim_audio::CloudSttProvider::new(
                    config.cloud_api_key.clone(),
                    config.cloud_stt_endpoint.clone(),
                    config.language,
                    config.cloud_timeout_secs,
                ) {
                    Ok(p) => Some(Arc::new(p) as _),
                    Err(e) => {
                        tracing::warn!("Failed to create cloud STT: {e}");
                        None
                    }
                }
            } else {
                None
            }
        }
        #[cfg(not(feature = "cloud-stt"))]
        {
            None
        }
    };

    // Assemble final provider based on config preference
    let provider: Option<Arc<dyn oneshim_core::ports::stt_provider::SttProvider>> =
        match config.stt_provider {
            SttProviderKind::Cloud => match (cloud_provider, local_provider) {
                (Some(cloud), Some(local)) => {
                    Some(Arc::new(crate::fallback_stt::FallbackSttProvider::new(cloud, local)) as _)
                }
                (Some(cloud), None) => Some(cloud),
                (None, Some(local)) => {
                    tracing::warn!("Cloud STT unavailable, using local");
                    Some(local)
                }
                (None, None) => None,
            },
            SttProviderKind::Local => local_provider,
        };

    let loaded = provider.is_some();
    let mut guard = state.audio.stt_engine.write().await;
    *guard = provider;

    if loaded {
        tracing::info!("STT engine reloaded (provider: {:?})", config.stt_provider);
    }
    Ok(loaded)
}
