// 오디오 캡처 및 음성-텍스트 변환 설정
use super::super::enums::SttLanguage;
use serde::{Deserialize, Serialize};

fn default_max_recording_secs() -> u32 {
    60
}

/// Audio capture and speech-to-text configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Enable audio capture and STT features (opt-in).
    #[serde(default)]
    pub enabled: bool,
    /// Path to Whisper GGML model file. Empty = auto-detect bundled model.
    #[serde(default)]
    pub whisper_model_path: String,
    /// Language hint for Whisper transcription.
    #[serde(default)]
    pub language: SttLanguage,
    /// Maximum recording duration in seconds.
    #[serde(default = "default_max_recording_secs")]
    pub max_recording_secs: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            whisper_model_path: String::new(),
            language: SttLanguage::Auto,
            max_recording_secs: default_max_recording_secs(),
        }
    }
}
