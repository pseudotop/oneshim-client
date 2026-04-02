//! Port for speech-to-text transcription providers.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::audio::{AudioBuffer, TranscriptionResult};

#[async_trait]
pub trait SttProvider: Send + Sync {
    /// Transcribe audio buffer to text. Takes ownership to avoid clone.
    async fn transcribe(&self, audio: AudioBuffer) -> Result<TranscriptionResult, CoreError>;

    /// Provider name for logging/UI.
    fn provider_name(&self) -> &str;
}
