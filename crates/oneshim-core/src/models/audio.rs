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
