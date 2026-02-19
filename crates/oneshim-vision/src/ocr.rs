//! OCR 텍스트 추출 모듈.
//!
//! `leptess` 기반 Tesseract OCR 래퍼.
//! `ocr` feature flag 활성화 시에만 빌드된다.
//!
//! Phase 31 최적화:
//! - Lazy initialization: 첫 호출 시에만 Tesseract 초기화
//! - Async wrapper: spawn_blocking으로 메인 스레드 블로킹 제거
//! - ROI 추출: 고해상도 이미지에서 중앙 영역만 처리

use std::path::PathBuf;
use std::sync::OnceLock;
use thiserror::Error;
use tracing::debug;

/// OCR 에러 타입
#[derive(Debug, Error)]
pub enum OcrError {
    /// Tesseract 초기화 실패
    #[error("OCR 초기화 실패: {0}")]
    Init(String),

    /// 이미지 설정 실패
    #[error("OCR 이미지 설정 실패: {0}")]
    ImageSetup(String),

    /// 텍스트 추출 실패
    #[error("OCR 텍스트 추출 실패: {0}")]
    Extraction(String),

    /// 빈 이미지 입력
    #[error("빈 이미지: 너비 또는 높이가 0")]
    EmptyImage,

    /// 비동기 작업 실패
    #[error("OCR 비동기 작업 실패: {0}")]
    Async(String),
}

/// Lazy-initialized Tesseract 인스턴스 보관
/// OnceLock으로 첫 호출 시에만 초기화
static TESSDATA_PATH: OnceLock<Option<String>> = OnceLock::new();

/// OCR 텍스트 추출기
///
/// Phase 31 최적화: Lazy initialization으로 첫 호출 시에만 Tesseract 초기화
pub struct OcrExtractor {
    /// Tesseract 데이터 경로 (None이면 시스템 기본값)
    tessdata_path: Option<PathBuf>,
    /// 최대 추출 문자 수 (0이면 무제한)
    max_chars: usize,
}

impl OcrExtractor {
    /// 새 OCR 추출기 생성
    pub fn new(tessdata_path: Option<PathBuf>) -> Self {
        // 경로를 전역 OnceLock에 저장 (lazy init용)
        let path_str = tessdata_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        let _ = TESSDATA_PATH.set(path_str);

        Self {
            tessdata_path,
            max_chars: 0,
        }
    }

    /// 최대 문자 수 제한 설정
    pub fn with_max_chars(mut self, max_chars: usize) -> Self {
        self.max_chars = max_chars;
        self
    }

    /// 이미지에서 텍스트 추출 (동기)
    pub fn extract(&self, image: &image::DynamicImage) -> Result<String, OcrError> {
        let rgba = image.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());

        if w == 0 || h == 0 {
            return Err(OcrError::EmptyImage);
        }

        let tessdata = self
            .tessdata_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());

        let tessdata_ref = tessdata.as_deref();

        let mut lt = leptess::LepTess::new(tessdata_ref, "eng")
            .map_err(|e| OcrError::Init(format!("{e}")))?;

        lt.set_image_from_mem(rgba.as_raw(), w as i32, h as i32, 4, (w * 4) as i32)
            .map_err(|_| OcrError::ImageSetup("이미지 메모리 설정 실패".to_string()))?;

        let text = lt
            .get_utf8_text()
            .map_err(|e| OcrError::Extraction(format!("{e}")))?;

        let result = text.trim().to_string();

        // 최대 문자 수 제한
        if self.max_chars > 0 && result.len() > self.max_chars {
            Ok(result.chars().take(self.max_chars).collect())
        } else {
            Ok(result)
        }
    }

    /// 이미지에서 텍스트 추출 (비동기)
    ///
    /// Phase 31 최적화: spawn_blocking으로 메인 스레드 블로킹 제거
    pub async fn extract_async(&self, image: &image::DynamicImage) -> Result<String, OcrError> {
        let rgba = image.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());

        if w == 0 || h == 0 {
            return Err(OcrError::EmptyImage);
        }

        let tessdata = self
            .tessdata_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());

        let max_chars = self.max_chars;
        let raw_data = rgba.into_raw();

        // 별도 스레드에서 OCR 실행
        let result = tokio::task::spawn_blocking(move || {
            let tessdata_ref = tessdata.as_deref();

            let mut lt = leptess::LepTess::new(tessdata_ref, "eng")
                .map_err(|e| OcrError::Init(format!("{e}")))?;

            lt.set_image_from_mem(&raw_data, w as i32, h as i32, 4, (w * 4) as i32)
                .map_err(|_| OcrError::ImageSetup("이미지 메모리 설정 실패".to_string()))?;

            let text = lt
                .get_utf8_text()
                .map_err(|e| OcrError::Extraction(format!("{e}")))?;

            let result = text.trim().to_string();

            if max_chars > 0 && result.len() > max_chars {
                Ok(result.chars().take(max_chars).collect())
            } else {
                Ok(result)
            }
        })
        .await
        .map_err(|e| OcrError::Async(format!("작업 조인 실패: {e}")))?;

        result
    }

    /// ROI(Region of Interest) 추출 후 OCR 수행
    ///
    /// Phase 31 최적화: 고해상도 이미지에서 중앙 영역만 추출하여 처리 시간 단축
    pub async fn extract_roi_async(
        &self,
        image: &image::DynamicImage,
        roi_ratio: f32,
    ) -> Result<String, OcrError> {
        use image::GenericImageView;

        let (w, h) = image.dimensions();

        if w == 0 || h == 0 {
            return Err(OcrError::EmptyImage);
        }

        // ROI 크기 계산 (중앙 영역)
        let roi_ratio = roi_ratio.clamp(0.1, 1.0);
        let roi_w = ((w as f32) * roi_ratio) as u32;
        let roi_h = ((h as f32) * roi_ratio) as u32;
        let roi_x = (w - roi_w) / 2;
        let roi_y = (h - roi_h) / 2;

        debug!(
            "OCR ROI: 원본 {}x{} → ROI {}x{} at ({}, {})",
            w, h, roi_w, roi_h, roi_x, roi_y
        );

        // ROI 추출
        let roi_image = image.crop_imm(roi_x, roi_y, roi_w, roi_h);

        self.extract_async(&roi_image).await
    }

    /// tessdata 경로 반환
    pub fn tessdata_path(&self) -> Option<&PathBuf> {
        self.tessdata_path.as_ref()
    }
}

/// OCR 워드 + 바운딩 박스 결과
#[derive(Debug, Clone)]
pub struct OcrWordBox {
    /// 추출된 텍스트
    pub text: String,
    /// X 좌표
    pub x: i32,
    /// Y 좌표
    pub y: i32,
    /// 너비
    pub w: i32,
    /// 높이
    pub h: i32,
}

impl OcrExtractor {
    /// 워드 단위 텍스트 + 바운딩 박스 추출 (비동기)
    ///
    /// 각 단어의 위치 정보를 함께 반환하여 PII 블러 처리에 활용한다.
    pub async fn extract_words_with_boxes(
        &self,
        image: &image::DynamicImage,
    ) -> Result<Vec<OcrWordBox>, OcrError> {
        let rgba = image.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());

        if w == 0 || h == 0 {
            return Err(OcrError::EmptyImage);
        }

        let tessdata = self
            .tessdata_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());

        let raw_data = rgba.into_raw();

        tokio::task::spawn_blocking(move || {
            let tessdata_ref = tessdata.as_deref();

            let mut lt = leptess::LepTess::new(tessdata_ref, "eng")
                .map_err(|e| OcrError::Init(format!("{e}")))?;

            lt.set_image_from_mem(&raw_data, w as i32, h as i32, 4, (w * 4) as i32)
                .map_err(|_| OcrError::ImageSetup("이미지 메모리 설정 실패".to_string()))?;

            // 워드 레벨 바운딩 박스 추출
            let boxes = lt
                .get_component_boxes(leptess::capi::TessPageIteratorLevel_RIL_WORD, true)
                .ok_or_else(|| OcrError::Extraction("워드 박스 추출 실패".to_string()))?;

            // 전체 텍스트에서 워드 단위로 매핑
            let full_text = lt
                .get_utf8_text()
                .map_err(|e| OcrError::Extraction(format!("{e}")))?;
            let words: Vec<&str> = full_text.split_whitespace().collect();

            let mut result = Vec::new();
            for (i, b) in boxes.iter().enumerate() {
                let geom = b.get_geometry();
                let word_text = words.get(i).unwrap_or(&"").to_string();
                if !word_text.is_empty() {
                    result.push(OcrWordBox {
                        text: word_text,
                        x: geom.x,
                        y: geom.y,
                        w: geom.w,
                        h: geom.h,
                    });
                }
            }

            Ok(result)
        })
        .await
        .map_err(|e| OcrError::Async(format!("작업 조인 실패: {e}")))?
    }
}

/// 간편 OCR 함수 (동기)
pub fn extract_text(image: &image::DynamicImage) -> Result<String, OcrError> {
    let extractor = OcrExtractor::new(None);
    extractor.extract(image)
}

/// 간편 OCR 함수 (비동기)
pub async fn extract_text_async(image: &image::DynamicImage) -> Result<String, OcrError> {
    let extractor = OcrExtractor::new(None);
    extractor.extract_async(image).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_image_returns_error() {
        let extractor = OcrExtractor::new(None);
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(0, 0));
        let result = extractor.extract(&img);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OcrError::EmptyImage));
    }

    #[test]
    fn error_display_messages() {
        let e1 = OcrError::Init("테스트".to_string());
        assert!(e1.to_string().contains("초기화"));

        let e2 = OcrError::ImageSetup("테스트".to_string());
        assert!(e2.to_string().contains("이미지"));

        let e3 = OcrError::Extraction("테스트".to_string());
        assert!(e3.to_string().contains("추출"));

        let e4 = OcrError::EmptyImage;
        assert!(e4.to_string().contains("빈 이미지"));

        let e5 = OcrError::Async("테스트".to_string());
        assert!(e5.to_string().contains("비동기"));
    }

    #[test]
    fn extractor_creation() {
        let extractor = OcrExtractor::new(None);
        assert!(extractor.tessdata_path().is_none());

        let path = PathBuf::from("/usr/share/tessdata");
        let extractor = OcrExtractor::new(Some(path.clone()));
        assert_eq!(extractor.tessdata_path(), Some(&path));
    }

    #[test]
    fn max_chars_builder() {
        let extractor = OcrExtractor::new(None).with_max_chars(100);
        assert_eq!(extractor.max_chars, 100);
    }

    #[tokio::test]
    async fn empty_image_async_returns_error() {
        let extractor = OcrExtractor::new(None);
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(0, 0));
        let result = extractor.extract_async(&img).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OcrError::EmptyImage));
    }

    #[test]
    fn ocr_word_box_creation() {
        let wb = OcrWordBox {
            text: "hello".to_string(),
            x: 10,
            y: 20,
            w: 50,
            h: 15,
        };
        assert_eq!(wb.text, "hello");
        assert_eq!(wb.x, 10);
        assert_eq!(wb.y, 20);
        assert_eq!(wb.w, 50);
        assert_eq!(wb.h, 15);
    }

    #[tokio::test]
    async fn extract_words_with_boxes_empty_image() {
        let extractor = OcrExtractor::new(None);
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(0, 0));
        let result = extractor.extract_words_with_boxes(&img).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OcrError::EmptyImage));
    }

    #[tokio::test]
    async fn roi_extraction_invalid_ratio() {
        let extractor = OcrExtractor::new(None);
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(0, 0));

        // 빈 이미지에서 ROI 추출 시 에러
        let result = extractor.extract_roi_async(&img, 0.5).await;
        assert!(result.is_err());
    }
}
