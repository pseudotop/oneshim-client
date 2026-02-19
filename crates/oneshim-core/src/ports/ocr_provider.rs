//! OCR 제공자 포트.
//!
//! 내장 Tesseract 또는 외부 AI OCR API를 추상화하는 인터페이스를 정의한다.
//! Privacy Gateway를 통해 외부 전송 시 데이터 세정이 적용된다.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// OCR 결과 (제공자 무관 표준 구조)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    /// 인식된 텍스트
    pub text: String,
    /// 바운딩 박스 X 좌표
    pub x: i32,
    /// 바운딩 박스 Y 좌표
    pub y: i32,
    /// 바운딩 박스 너비
    pub width: u32,
    /// 바운딩 박스 높이
    pub height: u32,
    /// 인식 신뢰도 (0.0 ~ 1.0)
    pub confidence: f64,
}

/// OCR 제공자 — 내장(Tesseract) 또는 외부 AI API
///
/// 구현체: `LocalOcrProvider` (Tesseract), `RemoteOcrProvider` (Claude Vision 등)
#[async_trait]
pub trait OcrProvider: Send + Sync {
    /// 이미지에서 텍스트 + 바운딩 박스 추출
    ///
    /// - `image`: 이미지 바이트
    /// - `image_format`: 이미지 형식 ("png", "jpeg" 등)
    async fn extract_elements(
        &self,
        image: &[u8],
        image_format: &str,
    ) -> Result<Vec<OcrResult>, CoreError>;

    /// 제공자 이름 (예: "local-tesseract", "claude-vision", "google-vision")
    fn provider_name(&self) -> &str;

    /// 외부 API인지 여부 (Privacy Gateway 적용 결정)
    fn is_external(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_result_serde() {
        let result = OcrResult {
            text: "저장".to_string(),
            x: 100,
            y: 200,
            width: 60,
            height: 25,
            confidence: 0.92,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deser: OcrResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.text, "저장");
        assert_eq!(deser.x, 100);
        assert!((deser.confidence - 0.92).abs() < f64::EPSILON);
    }

    #[test]
    fn ocr_result_vec_serde() {
        let results = vec![
            OcrResult {
                text: "파일".to_string(),
                x: 0,
                y: 0,
                width: 40,
                height: 20,
                confidence: 0.88,
            },
            OcrResult {
                text: "편집".to_string(),
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
        assert_eq!(deser[1].text, "편집");
    }
}
