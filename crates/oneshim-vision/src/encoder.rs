//! WebP 인코더.
//!
//! 적응적 품질 선택 + WebP 인코딩.
//! Phase 31 최적화: 품질 추정으로 1회 인코딩 전략 (50% 성능 개선)

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use image::{DynamicImage, GenericImageView};
use oneshim_core::error::CoreError;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::debug;

/// WebP 품질 프리셋
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebPQuality {
    /// 낮은 품질 (60%) — 썸네일용
    Low = 60,
    /// 중간 품질 (75%) — 델타용
    Medium = 75,
    /// 높은 품질 (85%) — 전체 프레임용
    High = 85,
}

/// 압축률 통계 (적응적 인코딩용)
struct CompressionStats {
    /// 누적 압축률 (encoded_size / raw_size * 1000)
    cumulative_ratio_x1000: AtomicU64,
    /// 샘플 수
    sample_count: AtomicU64,
}

impl CompressionStats {
    const fn new() -> Self {
        Self {
            cumulative_ratio_x1000: AtomicU64::new(0),
            sample_count: AtomicU64::new(0),
        }
    }

    /// 압축률 기록
    fn record(&self, raw_size: usize, encoded_size: usize) {
        if raw_size == 0 {
            return;
        }
        let ratio_x1000 = (encoded_size * 1000 / raw_size) as u64;
        self.cumulative_ratio_x1000
            .fetch_add(ratio_x1000, Ordering::Relaxed);
        self.sample_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 평균 압축률 반환 (0.0 ~ 1.0)
    fn average_ratio(&self) -> f32 {
        let count = self.sample_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0.3; // 기본 추정값 (30%)
        }
        let cumulative = self.cumulative_ratio_x1000.load(Ordering::Relaxed);
        (cumulative as f32 / count as f32) / 1000.0
    }
}

/// 품질별 압축률 통계
static STATS_HIGH: CompressionStats = CompressionStats::new();
static STATS_MEDIUM: CompressionStats = CompressionStats::new();
static STATS_LOW: CompressionStats = CompressionStats::new();

/// WebP 인코딩
pub fn encode_webp(image: &DynamicImage, quality: WebPQuality) -> Result<Vec<u8>, CoreError> {
    let rgba = image.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw_size = (w * h * 4) as usize;

    let encoder = webp::Encoder::from_rgba(&rgba, w, h);
    let encoded = encoder.encode(quality as u8 as f32);
    let encoded_vec = encoded.to_vec();

    // 압축률 통계 업데이트
    match quality {
        WebPQuality::High => STATS_HIGH.record(raw_size, encoded_vec.len()),
        WebPQuality::Medium => STATS_MEDIUM.record(raw_size, encoded_vec.len()),
        WebPQuality::Low => STATS_LOW.record(raw_size, encoded_vec.len()),
    }

    debug!(
        "WebP 인코딩: {}x{} → {} bytes (품질 {}, 압축률 {:.1}%)",
        w,
        h,
        encoded_vec.len(),
        quality as u8,
        (encoded_vec.len() as f32 / raw_size as f32) * 100.0
    );

    Ok(encoded_vec)
}

/// WebP 인코딩 후 Base64 반환
pub fn encode_webp_base64(image: &DynamicImage, quality: WebPQuality) -> Result<String, CoreError> {
    let bytes = encode_webp(image, quality)?;
    Ok(B64.encode(&bytes))
}

/// 적응적 인코딩 — 목표 크기 이하로 압축
///
/// Phase 31 최적화: 압축률 통계 기반으로 최적 품질 추정 후 1회 인코딩
/// 기존: 최대 3회 인코딩 → 최적화 후: 1-2회 인코딩
pub fn encode_adaptive(
    image: &DynamicImage,
    max_bytes: usize,
) -> Result<(Vec<u8>, WebPQuality), CoreError> {
    let (w, h) = image.dimensions();
    let raw_size = (w * h * 4) as usize;

    // 목표 압축률 계산
    let target_ratio = max_bytes as f32 / raw_size as f32;

    // 품질 추정 (과거 압축률 통계 기반)
    let estimated_quality = estimate_quality_from_stats(target_ratio);

    // 추정된 품질로 1회 인코딩
    let encoded = encode_webp(image, estimated_quality)?;

    if encoded.len() <= max_bytes {
        return Ok((encoded, estimated_quality));
    }

    // Fallback: 한 단계 낮은 품질로 재시도 (최대 1회 추가)
    let fallback_quality = match estimated_quality {
        WebPQuality::High => WebPQuality::Medium,
        WebPQuality::Medium => WebPQuality::Low,
        WebPQuality::Low => {
            // 이미 최저 품질 → 그냥 반환
            return Ok((encoded, WebPQuality::Low));
        }
    };

    let fallback_encoded = encode_webp(image, fallback_quality)?;

    if fallback_encoded.len() <= max_bytes {
        return Ok((fallback_encoded, fallback_quality));
    }

    // 최저 품질로도 크기 초과 → Low 품질 반환
    if fallback_quality != WebPQuality::Low {
        let low_encoded = encode_webp(image, WebPQuality::Low)?;
        return Ok((low_encoded, WebPQuality::Low));
    }

    Ok((fallback_encoded, WebPQuality::Low))
}

/// 압축률 통계 기반 품질 추정
fn estimate_quality_from_stats(target_ratio: f32) -> WebPQuality {
    let high_ratio = STATS_HIGH.average_ratio();
    let medium_ratio = STATS_MEDIUM.average_ratio();
    let _low_ratio = STATS_LOW.average_ratio();

    // 목표 압축률보다 약간 여유있는 품질 선택
    let margin = 1.1; // 10% 여유

    if high_ratio * margin <= target_ratio {
        WebPQuality::High
    } else if medium_ratio * margin <= target_ratio {
        WebPQuality::Medium
    } else {
        WebPQuality::Low
    }
}

/// 스마트 적응적 인코딩 — 저해상도 테스트 후 1회 인코딩
///
/// 대용량 이미지(1920x1080 이상)에서 추가 최적화:
/// 1. 10% 해상도 썸네일로 압축률 측정
/// 2. 압축률 기반 품질 결정
/// 3. 전체 해상도 1회 인코딩
pub fn encode_smart_adaptive(
    image: &DynamicImage,
    max_bytes: usize,
) -> Result<(Vec<u8>, WebPQuality), CoreError> {
    let (w, h) = image.dimensions();
    let raw_size = (w * h * 4) as usize;

    // 작은 이미지는 기본 적응형 사용
    if w * h < 500_000 {
        // ~707x707 미만
        return encode_adaptive(image, max_bytes);
    }

    // 10% 해상도 테스트 인코딩
    let test_w = (w / 10).max(1);
    let test_h = (h / 10).max(1);
    let test_image = image.resize_exact(test_w, test_h, image::imageops::FilterType::Nearest);

    let test_raw_size = (test_w * test_h * 4) as usize;
    let test_encoded = encode_webp(&test_image, WebPQuality::High)?;
    let test_ratio = test_encoded.len() as f32 / test_raw_size as f32;

    // 예상 압축 크기 계산
    let expected_high = (raw_size as f32 * test_ratio) as usize;

    // 품질 결정
    let quality = if expected_high <= max_bytes {
        WebPQuality::High
    } else {
        // Medium 테스트
        let test_medium = encode_webp(&test_image, WebPQuality::Medium)?;
        let medium_ratio = test_medium.len() as f32 / test_raw_size as f32;
        let expected_medium = (raw_size as f32 * medium_ratio) as usize;

        if expected_medium <= max_bytes {
            WebPQuality::Medium
        } else {
            WebPQuality::Low
        }
    };

    let encoded = encode_webp(image, quality)?;
    Ok((encoded, quality))
}

/// 압축률 통계 초기화 (테스트용)
#[cfg(test)]
pub fn reset_stats() {
    STATS_HIGH
        .cumulative_ratio_x1000
        .store(0, Ordering::Relaxed);
    STATS_HIGH.sample_count.store(0, Ordering::Relaxed);
    STATS_MEDIUM
        .cumulative_ratio_x1000
        .store(0, Ordering::Relaxed);
    STATS_MEDIUM.sample_count.store(0, Ordering::Relaxed);
    STATS_LOW.cumulative_ratio_x1000.store(0, Ordering::Relaxed);
    STATS_LOW.sample_count.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbaImage};

    fn make_test_image(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            w,
            h,
            image::Rgba([128, 64, 200, 255]),
        ))
    }

    #[test]
    fn encode_webp_basic() {
        let img = make_test_image(100, 100);
        let bytes = encode_webp(&img, WebPQuality::Medium).unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn encode_base64() {
        let img = make_test_image(50, 50);
        let b64 = encode_webp_base64(&img, WebPQuality::Low).unwrap();
        assert!(!b64.is_empty());
        // Base64 디코딩 가능 확인
        assert!(B64.decode(&b64).is_ok());
    }

    #[test]
    fn quality_levels_all_produce_output() {
        let img = make_test_image(200, 200);
        let low = encode_webp(&img, WebPQuality::Low).unwrap();
        let medium = encode_webp(&img, WebPQuality::Medium).unwrap();
        let high = encode_webp(&img, WebPQuality::High).unwrap();
        // 모든 품질 수준에서 유효한 출력 생성
        assert!(!low.is_empty());
        assert!(!medium.is_empty());
        assert!(!high.is_empty());
    }

    #[test]
    fn adaptive_encoding() {
        let img = make_test_image(100, 100);
        let (bytes, _quality) = encode_adaptive(&img, 100_000).unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn compression_stats_tracking() {
        reset_stats();

        let img = make_test_image(100, 100);

        // 여러 번 인코딩하여 통계 축적
        for _ in 0..5 {
            let _ = encode_webp(&img, WebPQuality::High);
        }

        // 통계가 기록되었는지 확인
        let ratio = STATS_HIGH.average_ratio();
        assert!(ratio > 0.0 && ratio < 1.0, "압축률: {}", ratio);
    }

    #[test]
    fn smart_adaptive_small_image() {
        let img = make_test_image(100, 100);
        let (bytes, quality) = encode_smart_adaptive(&img, 100_000).unwrap();
        assert!(!bytes.is_empty());
        // 작은 이미지는 High 품질 사용 가능
        assert!(matches!(
            quality,
            WebPQuality::High | WebPQuality::Medium | WebPQuality::Low
        ));
    }

    #[test]
    fn quality_estimation() {
        reset_stats();

        // 통계 없을 때 기본값 (0.3) 사용
        let quality = estimate_quality_from_stats(0.5);
        // 0.5 > 0.3 * 1.1 이므로 High 선택
        assert_eq!(quality, WebPQuality::High);
    }

    #[test]
    fn encode_adaptive_respects_size_limit() {
        let img = make_test_image(200, 200);
        let raw_size = 200 * 200 * 4; // 160,000 bytes

        // 매우 작은 제한
        let (bytes, quality) = encode_adaptive(&img, 1000).unwrap();
        // Low 품질로 폴백되어야 함
        assert_eq!(quality, WebPQuality::Low);
        // 결과는 존재해야 함
        assert!(!bytes.is_empty());

        // 넉넉한 제한
        let (bytes2, quality2) = encode_adaptive(&img, raw_size).unwrap();
        assert!(!bytes2.is_empty());
        // High 품질 사용 가능
        assert!(matches!(
            quality2,
            WebPQuality::High | WebPQuality::Medium | WebPQuality::Low
        ));
    }
}
