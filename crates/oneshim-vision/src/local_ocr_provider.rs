//! 로컬 OCR 제공자 — Tesseract 래퍼.
//!
//! 기존 `OcrExtractor`를 `OcrProvider` 트레이트로 래핑한다.
//! Privacy Gateway를 거치지 않는다 (is_external=false).

use async_trait::async_trait;

use oneshim_core::error::CoreError;
use oneshim_core::ports::ocr_provider::{OcrProvider, OcrResult};

// ============================================================
// LocalOcrProvider — Tesseract 래퍼
// ============================================================

/// 로컬 OCR 제공자 (Tesseract 기반)
///
/// 기존 `OcrExtractor`의 추출 결과를 표준 `OcrResult`로 변환한다.
/// 로컬 처리이므로 Privacy Gateway를 거치지 않는다.
pub struct LocalOcrProvider;

impl LocalOcrProvider {
    /// 새 로컬 OCR 제공자 생성
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalOcrProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OcrProvider for LocalOcrProvider {
    async fn extract_elements(
        &self,
        image: &[u8],
        _image_format: &str,
    ) -> Result<Vec<OcrResult>, CoreError> {
        #[cfg(feature = "ocr")]
        {
            use crate::ocr::OcrExtractor;

            // 이미지 디코딩
            let img = image::load_from_memory(image)
                .map_err(|e| CoreError::OcrError(format!("이미지 디코딩 실패: {e}")))?;

            // OCR 워드 박스 추출
            let extractor = OcrExtractor::new(None);
            let word_boxes = extractor
                .extract_words_with_boxes(&img)
                .await
                .map_err(|e| CoreError::OcrError(format!("OCR 추출 실패: {e}")))?;

            // OcrWordBox → OcrResult 변환
            let results: Vec<OcrResult> = word_boxes
                .into_iter()
                .map(|wb| OcrResult {
                    text: wb.text,
                    x: wb.x,
                    y: wb.y,
                    width: wb.w.max(0) as u32,
                    height: wb.h.max(0) as u32,
                    confidence: 0.0, // Tesseract word-level confidence는 별도 API 필요
                })
                .collect();

            Ok(results)
        }

        #[cfg(not(feature = "ocr"))]
        {
            let _ = image;
            Ok(vec![])
        }
    }

    fn provider_name(&self) -> &str {
        "local-tesseract"
    }

    fn is_external(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_ocr_provider_name() {
        let provider = LocalOcrProvider::new();
        assert_eq!(provider.provider_name(), "local-tesseract");
        assert!(!provider.is_external());
    }

    #[tokio::test]
    async fn local_ocr_provider_invalid_image() {
        let provider = LocalOcrProvider::new();
        let result = provider.extract_elements(b"fake-image", "png").await;
        // OCR feature 비활성화: Ok(empty), 활성화: Err(디코딩 실패)
        #[cfg(not(feature = "ocr"))]
        {
            assert!(result.is_ok());
            assert!(result.unwrap().is_empty());
        }
        #[cfg(feature = "ocr")]
        {
            assert!(result.is_err());
        }
    }
}
