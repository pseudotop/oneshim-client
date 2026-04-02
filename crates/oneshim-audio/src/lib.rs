//! Audio capture and speech-to-text adapter crate.
//!
//! - `AudioCapture`: microphone input via cpal + rubato resampling to 16kHz
//! - `VadDetector`: energy-based voice activity detection
//! - `WhisperSttProvider`: local Whisper STT (feature-gated behind `whisper`)

mod capture;
pub mod vad;

#[cfg(feature = "whisper")]
mod whisper;

#[cfg(feature = "download")]
pub mod model_downloader;

#[cfg(feature = "cloud-stt")]
mod cloud_stt;

pub use capture::AudioCapture;
pub use vad::{VadDetector, VadEvent, VadState};

#[cfg(feature = "whisper")]
pub use whisper::WhisperSttProvider;

#[cfg(feature = "download")]
pub use model_downloader::WhisperModelDownloader;

#[cfg(feature = "cloud-stt")]
pub use cloud_stt::CloudSttProvider;
