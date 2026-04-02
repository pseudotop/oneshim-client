//! Audio capture and speech-to-text adapter crate.
//!
//! - `AudioCapture`: microphone input via cpal + rubato resampling to 16kHz
//! - `WhisperSttProvider`: local Whisper STT (feature-gated behind `whisper`)

mod capture;

#[cfg(feature = "whisper")]
mod whisper;

pub use capture::AudioCapture;

#[cfg(feature = "whisper")]
pub use whisper::WhisperSttProvider;
