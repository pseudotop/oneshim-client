//! Audio capture and speech-to-text data types.

use serde::{Deserialize, Serialize};

/// Raw 16kHz mono f32 PCM audio buffer.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub duration_secs: f32,
}

impl AudioBuffer {
    /// Create a new buffer from 16kHz mono samples.
    pub fn new(samples: Vec<f32>) -> Self {
        let duration_secs = samples.len() as f32 / 16000.0;
        Self {
            samples,
            sample_rate: 16000,
            duration_secs,
        }
    }

    /// Whether the buffer contains no audio data.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// STT transcription result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub language: Option<String>,
    pub duration_secs: f32,
    pub processing_secs: f32,
}

use crate::config::WhisperModelSize;

/// Download progress event sent via channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub progress_pct: Option<u8>,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
}

/// Model download/install status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum ModelDownloadStatus {
    NotInstalled,
    Downloading {
        progress_pct: Option<u8>,
        bytes_downloaded: u64,
        total_bytes: Option<u64>,
    },
    Ready {
        path: String,
        size_bytes: u64,
    },
    Error {
        message: String,
    },
}

/// Combined audio subsystem status for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStatus {
    pub enabled: bool,
    pub selected_model: WhisperModelSize,
    pub model_status: ModelDownloadStatus,
    pub stt_provider_loaded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_calculates_duration() {
        let buf = AudioBuffer::new(vec![0.0; 16000]);
        assert_eq!(buf.duration_secs, 1.0);
        assert_eq!(buf.sample_rate, 16000);
        assert!(!buf.is_empty());
    }

    #[test]
    fn new_half_second() {
        let buf = AudioBuffer::new(vec![0.0; 8000]);
        assert!((buf.duration_secs - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn empty_buffer() {
        let buf = AudioBuffer::new(vec![]);
        assert!(buf.is_empty());
        assert_eq!(buf.duration_secs, 0.0);
    }

    #[test]
    fn whisper_model_size_serde_round_trip() {
        let size = crate::config::WhisperModelSize::Small;
        let json = serde_json::to_string(&size).unwrap();
        assert_eq!(json, "\"small\"");
        let restored: crate::config::WhisperModelSize = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, size);
    }

    #[test]
    fn whisper_model_size_default_is_base() {
        assert_eq!(
            crate::config::WhisperModelSize::default(),
            crate::config::WhisperModelSize::Base
        );
    }

    #[test]
    fn model_download_status_serde_tagged() {
        let ready = ModelDownloadStatus::Ready {
            path: "/tmp/model.bin".into(),
            size_bytes: 142_000_000,
        };
        let json = serde_json::to_string(&ready).unwrap();
        assert!(json.contains("\"state\":\"ready\""));
        let restored: ModelDownloadStatus = serde_json::from_str(&json).unwrap();
        matches!(restored, ModelDownloadStatus::Ready { .. });
    }

    #[test]
    fn model_download_status_not_installed_serde() {
        let status = ModelDownloadStatus::NotInstalled;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "{\"state\":\"not_installed\"}");
    }

    #[test]
    fn download_progress_with_none_pct() {
        let p = DownloadProgress {
            progress_pct: None,
            bytes_downloaded: 1024,
            total_bytes: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"progress_pct\":null"));
    }

    #[test]
    fn audio_status_serde() {
        let status = AudioStatus {
            enabled: true,
            selected_model: crate::config::WhisperModelSize::Base,
            model_status: ModelDownloadStatus::NotInstalled,
            stt_provider_loaded: false,
        };
        let json = serde_json::to_string(&status).unwrap();
        let restored: AudioStatus = serde_json::from_str(&json).unwrap();
        assert!(restored.enabled);
        assert!(!restored.stt_provider_loaded);
    }
}
