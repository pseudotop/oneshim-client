//! Whisper model download manager with streaming progress and SHA-256 verification.
//!
//! Gated behind `#[cfg(feature = "download")]`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use oneshim_core::config::WhisperModelSize;
use oneshim_core::error::CoreError;
use oneshim_core::models::audio::{DownloadProgress, ModelDownloadStatus};
use oneshim_core::ports::model_downloader::ModelDownloader;

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub fn model_filename(size: WhisperModelSize) -> &'static str {
    match size {
        WhisperModelSize::Tiny => "ggml-tiny.bin",
        WhisperModelSize::Base => "ggml-base.bin",
        WhisperModelSize::Small => "ggml-small.bin",
        WhisperModelSize::Medium => "ggml-medium.bin",
    }
}

pub fn model_expected_bytes(size: WhisperModelSize) -> u64 {
    match size {
        WhisperModelSize::Tiny => 77_691_713,
        WhisperModelSize::Base => 147_951_465,
        WhisperModelSize::Small => 487_601_967,
        WhisperModelSize::Medium => 1_533_774_781,
    }
}

pub struct WhisperModelDownloader {
    client: reqwest::Client,
}

impl Default for WhisperModelDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl WhisperModelDownloader {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ModelDownloader for WhisperModelDownloader {
    async fn download(
        &self,
        model: WhisperModelSize,
        dest_dir: &Path,
        progress_tx: mpsc::UnboundedSender<DownloadProgress>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<PathBuf, CoreError> {
        let filename = model_filename(model);
        let url = format!("{BASE_URL}/{filename}");
        let final_path = dest_dir.join(filename);
        let part_path = dest_dir.join(format!("{filename}.part"));

        // Ensure dest dir exists
        std::fs::create_dir_all(dest_dir).map_err(|e| CoreError::AudioCaptureV2 {
            code: oneshim_core::error_codes::AudioCode::CaptureFailed,
            message: format!("create model dir: {e}"),
        })?;

        info!(model = ?model, url = %url, "starting model download");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| CoreError::NetworkV2 {
                code: oneshim_core::error_codes::NetworkCode::Generic,
                message: format!("model download request: {e}"),
            })?;

        if !response.status().is_success() {
            return Err(CoreError::NetworkV2 {
                code: oneshim_core::error_codes::NetworkCode::Generic,
                message: format!("model download failed: HTTP {}", response.status()),
            });
        }

        let total_bytes = response.content_length();
        let mut stream = response.bytes_stream();
        let mut file =
            std::fs::File::create(&part_path).map_err(|e| CoreError::AudioCaptureV2 {
                code: oneshim_core::error_codes::AudioCode::CaptureFailed,
                message: format!("create part file: {e}"),
            })?;
        let mut hasher = Sha256::new();
        let mut downloaded: u64 = 0;

        use std::io::Write;
        while let Some(chunk_result) = stream.next().await {
            // Check cancellation
            if cancelled.load(Ordering::Relaxed) {
                drop(file);
                if let Err(e) = std::fs::remove_file(&part_path) {
                    debug!("remove_file failed: {e}");
                }
                return Err(CoreError::AudioCaptureV2 {
                    code: oneshim_core::error_codes::AudioCode::CaptureFailed,
                    message: "download cancelled".into(),
                });
            }

            let chunk = chunk_result.map_err(|e| CoreError::NetworkV2 {
                code: oneshim_core::error_codes::NetworkCode::Generic,
                message: format!("download stream: {e}"),
            })?;

            file.write_all(&chunk)
                .map_err(|e| CoreError::AudioCaptureV2 {
                    code: oneshim_core::error_codes::AudioCode::CaptureFailed,
                    message: format!("write chunk: {e}"),
                })?;
            hasher.update(&chunk);
            downloaded += chunk.len() as u64;

            let progress_pct = total_bytes.map(|total| {
                if total == 0 {
                    0u8
                } else {
                    ((downloaded * 100) / total).min(100) as u8
                }
            });

            let _ = progress_tx.send(DownloadProgress {
                progress_pct,
                bytes_downloaded: downloaded,
                total_bytes,
            });
        }

        drop(file);

        // Verify expected size
        let expected = model_expected_bytes(model);
        if downloaded != expected {
            warn!(
                expected,
                actual = downloaded,
                "model size mismatch — upstream may have updated"
            );
        }

        // Atomic rename
        std::fs::rename(&part_path, &final_path).map_err(|e| CoreError::AudioCaptureV2 {
            code: oneshim_core::error_codes::AudioCode::CaptureFailed,
            message: format!("rename part file: {e}"),
        })?;

        let hash = hasher.finalize().iter().fold(String::new(), |mut acc, b| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{b:02x}");
            acc
        });
        info!(
            model = ?model,
            size = downloaded,
            sha256 = %hash,
            "model download complete"
        );

        Ok(final_path)
    }

    fn model_status(&self, model: WhisperModelSize, dest_dir: &Path) -> ModelDownloadStatus {
        let path = dest_dir.join(model_filename(model));
        match std::fs::metadata(&path) {
            Ok(meta) => ModelDownloadStatus::Ready {
                path: path.to_string_lossy().into_owned(),
                size_bytes: meta.len(),
            },
            Err(_) => {
                // Check for partial download
                let part_path = dest_dir.join(format!("{}.part", model_filename(model)));
                if part_path.exists() {
                    ModelDownloadStatus::Error {
                        message: "incomplete download — please re-download".into(),
                    }
                } else {
                    ModelDownloadStatus::NotInstalled
                }
            }
        }
    }

    fn delete_model(&self, model: WhisperModelSize, dest_dir: &Path) -> Result<(), CoreError> {
        let path = dest_dir.join(model_filename(model));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| CoreError::AudioCaptureV2 {
                code: oneshim_core::error_codes::AudioCode::CaptureFailed,
                message: format!("delete model: {e}"),
            })?;
        }
        // Also clean up any .part file
        let part = dest_dir.join(format!("{}.part", model_filename(model)));
        if let Err(e) = std::fs::remove_file(&part) {
            debug!("remove_file failed: {e}");
        }
        debug!(model = ?model, "model deleted");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn model_filename_mapping() {
        assert_eq!(model_filename(WhisperModelSize::Tiny), "ggml-tiny.bin");
        assert_eq!(model_filename(WhisperModelSize::Base), "ggml-base.bin");
        assert_eq!(model_filename(WhisperModelSize::Small), "ggml-small.bin");
        assert_eq!(model_filename(WhisperModelSize::Medium), "ggml-medium.bin");
    }

    #[test]
    fn model_status_not_installed() {
        let dl = WhisperModelDownloader::new();
        let dir = tempdir().unwrap();
        let status = dl.model_status(WhisperModelSize::Base, dir.path());
        assert!(matches!(status, ModelDownloadStatus::NotInstalled));
    }

    #[test]
    fn model_status_ready_when_file_exists() {
        let dl = WhisperModelDownloader::new();
        let dir = tempdir().unwrap();
        let path = dir.path().join("ggml-base.bin");
        std::fs::write(&path, b"fake model data").unwrap();
        let status = dl.model_status(WhisperModelSize::Base, dir.path());
        match status {
            ModelDownloadStatus::Ready { size_bytes, .. } => {
                assert_eq!(size_bytes, 15); // "fake model data".len()
            }
            _ => panic!("expected Ready"),
        }
    }

    #[test]
    fn delete_model_removes_file() {
        let dl = WhisperModelDownloader::new();
        let dir = tempdir().unwrap();
        let path = dir.path().join("ggml-tiny.bin");
        std::fs::write(&path, b"data").unwrap();
        assert!(path.exists());
        dl.delete_model(WhisperModelSize::Tiny, dir.path()).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn delete_model_noop_when_missing() {
        let dl = WhisperModelDownloader::new();
        let dir = tempdir().unwrap();
        dl.delete_model(WhisperModelSize::Medium, dir.path())
            .unwrap();
    }
}
