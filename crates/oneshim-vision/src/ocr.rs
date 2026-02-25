//!
//!

use std::path::PathBuf;
use std::sync::OnceLock;
use thiserror::Error;
use tracing::debug;

#[derive(Debug, Error)]
pub enum OcrError {
    #[error("OCR initialize failure: {0}")]
    Init(String),

    #[error("OCR image setup failed: {0}")]
    ImageSetup(String),

    #[error("OCR text extraction failed: {0}")]
    Extraction(String),

    #[error("Empty image: width or height is 0")]
    EmptyImage,

    #[error("OCR async task failed: {0}")]
    Async(String),
}

static TESSDATA_PATH: OnceLock<Option<String>> = OnceLock::new();

///
pub struct OcrExtractor {
    tessdata_path: Option<PathBuf>,
    max_chars: usize,
}

impl OcrExtractor {
    pub fn new(tessdata_path: Option<PathBuf>) -> Self {
        let path_str = tessdata_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        let _ = TESSDATA_PATH.set(path_str);

        Self {
            tessdata_path,
            max_chars: 0,
        }
    }

    pub fn with_max_chars(mut self, max_chars: usize) -> Self {
        self.max_chars = max_chars;
        self
    }

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
            .map_err(|_| OcrError::ImageSetup("Image memory setup failed".to_string()))?;

        let text = lt
            .get_utf8_text()
            .map_err(|e| OcrError::Extraction(format!("{e}")))?;

        let result = text.trim().to_string();

        if self.max_chars > 0 && result.len() > self.max_chars {
            Ok(result.chars().take(self.max_chars).collect())
        } else {
            Ok(result)
        }
    }

    ///
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

        let result = tokio::task::spawn_blocking(move || {
            let tessdata_ref = tessdata.as_deref();

            let mut lt = leptess::LepTess::new(tessdata_ref, "eng")
                .map_err(|e| OcrError::Init(format!("{e}")))?;

            lt.set_image_from_mem(&raw_data, w as i32, h as i32, 4, (w * 4) as i32)
                .map_err(|_| OcrError::ImageSetup("Image memory setup failed".to_string()))?;

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
        .map_err(|e| OcrError::Async(format!("Task join failed: {e}")))?;

        result
    }

    ///
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

        let roi_ratio = roi_ratio.clamp(0.1, 1.0);
        let roi_w = ((w as f32) * roi_ratio) as u32;
        let roi_h = ((h as f32) * roi_ratio) as u32;
        let roi_x = (w - roi_w) / 2;
        let roi_y = (h - roi_h) / 2;

        debug!(
            "OCR ROI: source {}x{} -> ROI {}x{} at ({}, {})",
            w, h, roi_w, roi_h, roi_x, roi_y
        );

        let roi_image = image.crop_imm(roi_x, roi_y, roi_w, roi_h);

        self.extract_async(&roi_image).await
    }

    pub fn tessdata_path(&self) -> Option<&PathBuf> {
        self.tessdata_path.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct OcrWordBox {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl OcrExtractor {
    ///
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
                .map_err(|_| OcrError::ImageSetup("Image memory setup failed".to_string()))?;

            let boxes = lt
                .get_component_boxes(leptess::capi::TessPageIteratorLevel_RIL_WORD, true)
                .ok_or_else(|| OcrError::Extraction("Failed to extract word boxes".to_string()))?;

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
        .map_err(|e| OcrError::Async(format!("Task join failed: {e}")))?
    }
}

pub fn extract_text(image: &image::DynamicImage) -> Result<String, OcrError> {
    let extractor = OcrExtractor::new(None);
    extractor.extract(image)
}

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
        let e1 = OcrError::Init("test".to_string());
        assert!(e1.to_string().contains("initialize"));

        let e2 = OcrError::ImageSetup("test".to_string());
        assert!(e2.to_string().contains("image"));

        let e3 = OcrError::Extraction("test".to_string());
        assert!(e3.to_string().contains("extraction"));

        let e4 = OcrError::EmptyImage;
        assert!(e4.to_string().contains("Empty image"));

        let e5 = OcrError::Async("test".to_string());
        assert!(e5.to_string().contains("async"));
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

        let result = extractor.extract_roi_async(&img, 0.5).await;
        assert!(result.is_err());
    }
}
