//! OCR provider port — defines the contract for extracting text elements
//! and their bounding boxes from screen capture images.
//! Implemented by `RemoteOcrProvider` in `oneshim-network` and
//! `OcrExtractor` (Tesseract) in `oneshim-vision`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub confidence: f64,
}

/// OCR adapters emit `CoreError::OcrError` (wire: `provider.ocr_failed`) for
/// provider-side failures (malformed OCR response, empty output). HTTP-layer
/// failures follow the canonical semantic status mapping (`auth.failed` /
/// `network.timeout` / `network.rate_limit` / `service.unavailable`). See
/// `docs/guides/http-status-error-mapping.md`. Local OCR (Tesseract) emits
/// `CoreError::OcrError` for library failures and `CoreError::PermissionDenied`
/// if screen-capture permission is missing upstream.
#[async_trait]
pub trait OcrProvider: Send + Sync {
    async fn extract_elements(
        &self,
        image: &[u8],
        image_format: &str,
    ) -> Result<Vec<OcrResult>, CoreError>;

    fn provider_name(&self) -> &str;

    fn is_external(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_result_serde() {
        let result = OcrResult {
            text: "save".to_string(),
            x: 100,
            y: 200,
            width: 60,
            height: 25,
            confidence: 0.92,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deser: OcrResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.text, "save");
        assert_eq!(deser.x, 100);
        assert!((deser.confidence - 0.92).abs() < f64::EPSILON);
    }

    #[test]
    fn ocr_result_vec_serde() {
        let results = vec![
            OcrResult {
                text: "file".to_string(),
                x: 0,
                y: 0,
                width: 40,
                height: 20,
                confidence: 0.88,
            },
            OcrResult {
                text: "edit".to_string(),
                x: 50,
                y: 0,
                width: 40,
                height: 20,
                confidence: 0.91,
            },
        ];
        let json = serde_json::to_string(&results).unwrap();
        let deser: Vec<OcrResult> = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.len(), 2);
        assert_eq!(deser[1].text, "edit");
    }
}
