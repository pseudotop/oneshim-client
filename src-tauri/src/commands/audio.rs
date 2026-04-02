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

/// Reload STT engine with current config model.
#[command]
pub async fn reload_stt_engine(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    #[cfg(feature = "download")]
    let model_path = state
        .audio
        .model_dir
        .join(oneshim_audio::model_downloader::model_filename(
            state.config.audio.model_size,
        ));
    #[cfg(not(feature = "download"))]
    let model_path = std::path::PathBuf::from(&state.config.audio.whisper_model_path);

    if !model_path.exists() {
        let mut guard = state.audio.stt_engine.write().await;
        *guard = None;
        return Ok(false);
    }

    #[cfg(feature = "stt")]
    {
        match oneshim_audio::WhisperSttProvider::new(&model_path, state.config.audio.language) {
            Ok(provider) => {
                let mut guard = state.audio.stt_engine.write().await;
                *guard = Some(Arc::new(provider) as _);
                tracing::info!("STT engine reloaded: {}", model_path.display());
                Ok(true)
            }
            Err(e) => {
                tracing::warn!("Failed to reload STT: {e}");
                Err(e.to_string())
            }
        }
    }
    #[cfg(not(feature = "stt"))]
    {
        Ok(false)
    }
}
