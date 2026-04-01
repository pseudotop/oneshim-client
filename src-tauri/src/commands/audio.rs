//! Tauri IPC commands for audio capture and speech-to-text.

use tauri::command;

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

    let stt = state
        .audio
        .stt_engine
        .as_ref()
        .ok_or_else(|| "STT engine not available (model may not be loaded)".to_string())?;

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
