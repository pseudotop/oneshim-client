//! Cloud STT provider using OpenAI Whisper API.
//!
//! Gated behind `#[cfg(feature = "cloud-stt")]`.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::multipart;
use tracing::debug;

use oneshim_core::config::SttLanguage;
use oneshim_core::error::CoreError;
use oneshim_core::models::audio::{AudioBuffer, TranscriptionResult};
use oneshim_core::ports::stt_provider::SttProvider;

pub struct CloudSttProvider {
    client: reqwest::Client,
    api_key: String,
    endpoint: String,
    language: SttLanguage,
    timeout_secs: u32,
}

impl CloudSttProvider {
    pub fn new(
        api_key: String,
        endpoint: String,
        language: SttLanguage,
        timeout_secs: u32,
    ) -> Result<Self, CoreError> {
        if api_key.is_empty() {
            return Err(CoreError::SpeechToText("cloud STT API key is empty".into()));
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(u64::from(timeout_secs)))
            .build()
            .map_err(|e| CoreError::SpeechToText(format!("build HTTP client: {e}")))?;

        Ok(Self {
            client,
            api_key,
            endpoint,
            language,
            timeout_secs,
        })
    }
}

#[async_trait]
impl SttProvider for CloudSttProvider {
    async fn transcribe(&self, audio: AudioBuffer) -> Result<TranscriptionResult, CoreError> {
        if audio.is_empty() {
            return Ok(TranscriptionResult {
                text: String::new(),
                language: None,
                duration_secs: 0.0,
                processing_secs: 0.0,
            });
        }

        let duration_secs = audio.duration_secs;
        let start = Instant::now();

        // Convert to WAV for upload
        let wav_bytes = audio.to_wav_bytes();

        let file_part = multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| CoreError::SpeechToText(format!("create multipart: {e}")))?;

        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("model", "whisper-1");

        // Add language hint if not auto
        match self.language {
            SttLanguage::En => {
                form = form.text("language", "en");
            }
            SttLanguage::Ko => {
                form = form.text("language", "ko");
            }
            SttLanguage::Auto => {}
        }

        debug!(endpoint = %self.endpoint, "sending cloud STT request");

        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    CoreError::RequestTimeout {
                        timeout_ms: u64::from(self.timeout_secs) * 1000,
                    }
                } else {
                    CoreError::Network(format!("cloud STT request: {e}"))
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(CoreError::SpeechToText(format!(
                "cloud STT error: HTTP {status} — {body}"
            )));
        }

        #[derive(serde::Deserialize)]
        struct OpenAiResponse {
            text: String,
        }

        let result: OpenAiResponse = response
            .json()
            .await
            .map_err(|e| CoreError::SpeechToText(format!("parse cloud response: {e}")))?;

        let processing_secs = start.elapsed().as_secs_f32();
        debug!(
            text_len = result.text.len(),
            processing_secs, "cloud STT complete"
        );

        Ok(TranscriptionResult {
            text: result.text,
            language: None,
            duration_secs,
            processing_secs,
        })
    }

    fn provider_name(&self) -> &str {
        "openai-whisper-cloud"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty_api_key() {
        let result =
            CloudSttProvider::new(String::new(), "http://test".into(), SttLanguage::Auto, 10);
        assert!(result.is_err());
    }

    #[test]
    fn provider_name_correct() {
        let provider = CloudSttProvider::new(
            "sk-test".into(),
            "http://test".into(),
            SttLanguage::Auto,
            10,
        )
        .unwrap();
        assert_eq!(provider.provider_name(), "openai-whisper-cloud");
    }
}
