//! Port for audio capture devices.

use crate::error::CoreError;
use crate::models::audio::AudioBuffer;

/// Port trait for microphone capture (Push-to-Talk).
/// Implementations: `oneshim_audio::AudioCapture` (cpal).
pub trait AudioCapturePort: Send + Sync {
    /// Start capturing audio from the default input device.
    fn start(&self) -> Result<(), CoreError>;

    /// Stop capturing and return the accumulated audio buffer (16kHz mono).
    fn stop(&self) -> Result<AudioBuffer, CoreError>;

    /// Whether currently capturing.
    fn is_capturing(&self) -> bool;
}
