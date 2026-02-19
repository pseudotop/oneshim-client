//! Edge 프레임 처리 오케스트레이터.
//!
//! `FrameProcessor` 포트 구현. 중요도별 처리 분기.

use async_trait::async_trait;
use chrono::Utc;
use image::DynamicImage;
use oneshim_core::error::CoreError;
use oneshim_core::models::frame::{FrameMetadata, ImagePayload, ProcessedFrame};
use oneshim_core::ports::vision::{CaptureRequest, FrameProcessor};
use std::path::PathBuf;
use tracing::debug;

use crate::capture::ScreenCapture;
use crate::delta;
use crate::encoder::{self, WebPQuality};
use crate::privacy;
use crate::thumbnail;

/// Edge 프레임 처리기 — `FrameProcessor` 포트 구현
pub struct EdgeFrameProcessor {
    capture: ScreenCapture,
    prev_frame: Option<DynamicImage>,
    thumbnail_width: u32,
    thumbnail_height: u32,
    /// OCR 추출기 (ocr feature 활성화 시)
    #[cfg(feature = "ocr")]
    ocr_extractor: Option<crate::ocr::OcrExtractor>,
}

impl EdgeFrameProcessor {
    /// 새 프레임 처리기 생성
    #[allow(unused_variables)]
    pub fn new(thumbnail_width: u32, thumbnail_height: u32, ocr_tessdata: Option<PathBuf>) -> Self {
        Self {
            capture: ScreenCapture::new(),
            prev_frame: None,
            thumbnail_width,
            thumbnail_height,
            #[cfg(feature = "ocr")]
            ocr_extractor: ocr_tessdata
                .map(|p| crate::ocr::OcrExtractor::new(Some(p)))
                .or_else(|| Some(crate::ocr::OcrExtractor::new(None))),
        }
    }
}

/// OCR 텍스트 추출 (feature 비활성화 시 항상 None)
#[cfg(not(feature = "ocr"))]
fn extract_ocr_text(_frame: &DynamicImage, _processor: &EdgeFrameProcessor) -> Option<String> {
    None
}

/// OCR 텍스트 추출 (feature 활성화 시)
#[cfg(feature = "ocr")]
fn extract_ocr_text(frame: &DynamicImage, processor: &EdgeFrameProcessor) -> Option<String> {
    let extractor = processor.ocr_extractor.as_ref()?;
    match extractor.extract(frame) {
        Ok(text) if !text.is_empty() => {
            let sanitized = privacy::sanitize_title(&text);
            Some(sanitized)
        }
        Ok(_) => None,
        Err(e) => {
            tracing::warn!("OCR 추출 실패 (무시): {e}");
            None
        }
    }
}

#[async_trait]
impl FrameProcessor for EdgeFrameProcessor {
    async fn capture_and_process(
        &mut self,
        capture_request: &CaptureRequest,
    ) -> Result<ProcessedFrame, CoreError> {
        let sanitized_title = privacy::sanitize_title(&capture_request.window_title);
        let importance = capture_request.importance;

        // 스크린 캡처
        let current_frame = self.capture.capture_primary()?;
        let (w, h) = (current_frame.width(), current_frame.height());

        let metadata = FrameMetadata {
            timestamp: Utc::now(),
            trigger_type: capture_request.trigger_type.clone(),
            app_name: capture_request.app_name.clone(),
            window_title: sanitized_title,
            resolution: (w, h),
            importance,
        };

        // 중요도별 처리 분기
        let image_payload = if importance >= 0.8 {
            // 전체 프레임 + OCR
            debug!("전체 프레임 처리 (중요도 {:.1})", importance);
            let encoded = encoder::encode_webp_base64(&current_frame, WebPQuality::High)?;
            let ocr_text = extract_ocr_text(&current_frame, self);
            Some(ImagePayload::Full {
                data: encoded,
                format: "webp".to_string(),
                ocr_text,
            })
        } else if importance >= 0.5 {
            // 델타 인코딩
            debug!("델타 처리 (중요도 {:.1})", importance);
            if let Some(prev) = &self.prev_frame {
                if let Some(delta_region) = delta::compute_delta(prev, &current_frame) {
                    let encoded = encoder::encode_webp_base64(&current_frame, WebPQuality::Medium)?;
                    Some(ImagePayload::Delta {
                        data: encoded,
                        region: delta_region.region,
                        changed_ratio: delta_region.changed_ratio,
                    })
                } else {
                    None // 변경 없음 → 메타만
                }
            } else {
                // 이전 프레임 없으면 전체 프레임
                let encoded = encoder::encode_webp_base64(&current_frame, WebPQuality::Medium)?;
                Some(ImagePayload::Full {
                    data: encoded,
                    format: "webp".to_string(),
                    ocr_text: None,
                })
            }
        } else if importance >= 0.3 {
            // 썸네일만
            debug!("썸네일 처리 (중요도 {:.1})", importance);
            let thumb = thumbnail::fast_resize(
                &current_frame,
                self.thumbnail_width,
                self.thumbnail_height,
            )?;
            let encoded = encoder::encode_webp_base64(&thumb, WebPQuality::Low)?;
            Some(ImagePayload::Thumbnail {
                data: encoded,
                width: self.thumbnail_width,
                height: self.thumbnail_height,
            })
        } else {
            // 메타데이터만
            debug!("메타만 (중요도 {:.1})", importance);
            None
        };

        // 이전 프레임 업데이트
        self.prev_frame = Some(current_frame);

        Ok(ProcessedFrame {
            metadata,
            image_payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use image::{DynamicImage, RgbaImage};

    fn make_test_image(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            w,
            h,
            image::Rgba([100, 150, 200, 255]),
        ))
    }

    #[test]
    fn processor_creation() {
        let proc = EdgeFrameProcessor::new(480, 270, None);
        assert_eq!(proc.thumbnail_width, 480);
        assert_eq!(proc.thumbnail_height, 270);
        assert!(proc.prev_frame.is_none());
    }

    #[test]
    fn full_frame_encoding_high_importance() {
        let img = make_test_image(640, 480);
        let encoded = encoder::encode_webp_base64(&img, WebPQuality::High).unwrap();
        assert!(!encoded.is_empty());
        // Base64 디코딩 가능한지 검증
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .unwrap();
        assert!(!decoded.is_empty());
    }

    #[test]
    fn delta_encoding_medium_importance() {
        let img1 = make_test_image(100, 100);
        // 다른 색으로 두 번째 이미지 생성
        let img2 = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            100,
            100,
            image::Rgba([200, 50, 50, 255]),
        ));
        // 델타 계산 — 완전히 다른 이미지이므로 변경 감지됨
        let result = delta::compute_delta(&img1, &img2);
        assert!(result.is_some());
        let dr = result.unwrap();
        assert!(dr.changed_ratio > 0.0);
    }

    #[test]
    fn thumbnail_generation_low_importance() {
        let img = make_test_image(1920, 1080);
        let thumb = thumbnail::fast_resize(&img, 480, 270).unwrap();
        assert_eq!(thumb.width(), 480);
        assert_eq!(thumb.height(), 270);
    }

    #[test]
    fn privacy_sanitization_in_pipeline() {
        let title = "Login - admin@company.com - Firefox";
        let sanitized = privacy::sanitize_title(title);
        assert!(sanitized.contains("[EMAIL]"));
        assert!(!sanitized.contains("admin@company.com"));
    }
}
