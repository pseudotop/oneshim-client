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
}
