//! 썸네일 생성.
//!
//! fast_image_resize 기반 고속 리사이즈 + LRU 캐싱.
//! Phase 31 최적화: 동일 소스→크기 변환 캐싱으로 중복 연산 제거.

use fast_image_resize::{images::Image as FirImage, ResizeAlg, ResizeOptions, Resizer};
use image::{DynamicImage, RgbaImage};
use lru::LruCache;
use once_cell::sync::Lazy;
use oneshim_core::error::CoreError;
use parking_lot::Mutex;
use std::num::NonZeroUsize;
use tracing::debug;

/// 캐시 최대 크기 (100개 썸네일)
const CACHE_CAPACITY: usize = 100;

/// 캐시 키: (이미지 해시, 목표 너비, 목표 높이)
type CacheKey = (u64, u32, u32);

/// 전역 썸네일 캐시 (MSRV 1.75 호환을 위해 once_cell 사용)
static THUMBNAIL_CACHE: Lazy<Mutex<LruCache<CacheKey, Vec<u8>>>> = Lazy::new(|| {
    Mutex::new(LruCache::new(
        NonZeroUsize::new(CACHE_CAPACITY).expect("CACHE_CAPACITY must be > 0"),
    ))
});

/// 이미지 해시 계산 (FNV-1a)
///
/// 고속 해시를 위해 샘플링 방식 사용:
/// - 이미지 크기 (8바이트)
/// - 픽셀 샘플링 (최대 64개 픽셀)
#[inline]
fn compute_image_hash(image: &DynamicImage) -> u64 {
    let rgba = image.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.as_raw();

    // FNV-1a 초기값
    let mut hash: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    // 크기 해싱
    hash ^= w as u64;
    hash = hash.wrapping_mul(FNV_PRIME);
    hash ^= h as u64;
    hash = hash.wrapping_mul(FNV_PRIME);

    // 픽셀 샘플링 (8x8 그리드 = 64개 위치)
    let step_x = (w as usize).max(1) / 8;
    let step_y = (h as usize).max(1) / 8;
    let stride = w as usize * 4;

    for sy in 0..8 {
        let y = (sy * step_y).min((h as usize).saturating_sub(1));
        let row_offset = y * stride;

        for sx in 0..8 {
            let x = (sx * step_x).min((w as usize).saturating_sub(1));
            let pixel_offset = row_offset + x * 4;

            if pixel_offset + 3 < raw.len() {
                // RGBA 4바이트를 u32로
                let pixel = u32::from_le_bytes([
                    raw[pixel_offset],
                    raw[pixel_offset + 1],
                    raw[pixel_offset + 2],
                    raw[pixel_offset + 3],
                ]);
                hash ^= pixel as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
    }

    hash
}

/// 고속 리사이즈 (캐싱 적용)
///
/// Phase 31 최적화: 동일 소스→크기 변환을 캐싱하여 중복 연산 제거.
/// 캐시 히트율 50% 기준 약 2배 성능 향상 기대.
pub fn fast_resize(
    image: &DynamicImage,
    width: u32,
    height: u32,
) -> Result<DynamicImage, CoreError> {
    let (src_w, src_h) = (image.width(), image.height());

    // 동일 크기면 복제 반환 (캐시 불필요)
    if src_w == width && src_h == height {
        return Ok(image.clone());
    }

    if src_w == 0 || src_h == 0 {
        return Err(CoreError::Internal("소스 이미지 크기 0".to_string()));
    }
    if width == 0 || height == 0 {
        return Err(CoreError::Internal("목표 이미지 크기 0".to_string()));
    }

    // 캐시 키 생성
    let hash = compute_image_hash(image);
    let cache_key = (hash, width, height);

    // 캐시 조회
    {
        let mut cache = THUMBNAIL_CACHE.lock();
        if let Some(cached) = cache.get(&cache_key) {
            debug!(
                "썸네일 캐시 히트: {}x{} → {}x{} (hash={})",
                src_w, src_h, width, height, hash
            );

            let result = RgbaImage::from_raw(width, height, cached.clone())
                .ok_or_else(|| CoreError::Internal("캐시 이미지 복원 실패".to_string()))?;

            return Ok(DynamicImage::ImageRgba8(result));
        }
    }

    // 캐시 미스 → 리사이즈 실행
    let src_rgba = image.to_rgba8();

    let src_image = FirImage::from_vec_u8(
        src_w,
        src_h,
        src_rgba.into_raw(),
        fast_image_resize::PixelType::U8x4,
    )
    .map_err(|e| CoreError::Internal(format!("소스 이미지 생성 실패: {e}")))?;

    let mut dst_image = FirImage::new(width, height, fast_image_resize::PixelType::U8x4);

    let mut resizer = Resizer::new();
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(
        fast_image_resize::FilterType::Bilinear,
    ));

    resizer
        .resize(&src_image, &mut dst_image, &options)
        .map_err(|e| CoreError::Internal(format!("리사이즈 실패: {e}")))?;

    let raw_bytes = dst_image.into_vec();

    // 캐시에 저장
    {
        let mut cache = THUMBNAIL_CACHE.lock();
        cache.put(cache_key, raw_bytes.clone());
    }

    let result = RgbaImage::from_raw(width, height, raw_bytes)
        .ok_or_else(|| CoreError::Internal("결과 이미지 생성 실패".to_string()))?;

    debug!(
        "썸네일 생성: {}x{} → {}x{} (hash={}, 캐시 저장)",
        src_w, src_h, width, height, hash
    );

    Ok(DynamicImage::ImageRgba8(result))
}

/// 캐시 통계
pub struct CacheStats {
    /// 캐시 크기
    pub size: usize,
    /// 최대 용량
    pub capacity: usize,
}

/// 캐시 통계 조회
pub fn get_cache_stats() -> CacheStats {
    let cache = THUMBNAIL_CACHE.lock();
    CacheStats {
        size: cache.len(),
        capacity: CACHE_CAPACITY,
    }
}

/// 캐시 초기화 (테스트용)
#[cfg(test)]
pub fn clear_cache() {
    let mut cache = THUMBNAIL_CACHE.lock();
    cache.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, GenericImageView, RgbaImage};
    use std::sync::Mutex;

    /// 병렬 테스트 간 글로벌 LRU 캐시 경쟁 조건 방지용 락
    static CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn make_test_image(w: u32, h: u32, color: [u8; 4]) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, image::Rgba(color)))
    }

    #[test]
    fn resize_basic() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap();
        clear_cache();
        let img = make_test_image(1920, 1080, [100, 100, 100, 255]);
        let thumb = fast_resize(&img, 480, 270).unwrap();
        assert_eq!(thumb.dimensions(), (480, 270));
    }

    #[test]
    fn same_size_noop() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap();
        clear_cache();
        let img = make_test_image(480, 270, [100, 100, 100, 255]);
        let result = fast_resize(&img, 480, 270).unwrap();
        assert_eq!(result.dimensions(), (480, 270));
    }

    #[test]
    fn cache_hit() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap();
        clear_cache();
        let img = make_test_image(800, 600, [50, 100, 150, 255]);

        let size_before = get_cache_stats().size;

        // 첫 호출 (캐시 미스)
        let _thumb1 = fast_resize(&img, 200, 150).unwrap();
        let size_after_first = get_cache_stats().size;
        assert!(size_after_first >= size_before, "캐시 크기가 증가하지 않음");

        // 두 번째 호출 (캐시 히트 — 동일 이미지+크기)
        let _thumb2 = fast_resize(&img, 200, 150).unwrap();
        let size_after_second = get_cache_stats().size;
        assert_eq!(
            size_after_first, size_after_second,
            "캐시 히트 시 크기 증가 없어야 함"
        );
    }

    #[test]
    fn different_sizes_cached_separately() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap();
        clear_cache();
        let img = make_test_image(1000, 1000, [200, 100, 50, 255]);

        let size_before = get_cache_stats().size;

        let _t1 = fast_resize(&img, 100, 100).unwrap();
        let _t2 = fast_resize(&img, 200, 200).unwrap();
        let _t3 = fast_resize(&img, 300, 300).unwrap();

        let size_after = get_cache_stats().size;
        // 3개의 다른 크기 → 최소 3개 증가
        assert!(
            size_after >= size_before + 3,
            "다른 크기는 별도 캐싱되어야 함"
        );
    }

    #[test]
    fn different_images_cached_separately() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap();
        clear_cache();
        let img1 = make_test_image(500, 500, [255, 0, 0, 255]); // 빨강
        let img2 = make_test_image(500, 500, [0, 255, 0, 255]); // 초록

        let size_before = get_cache_stats().size;

        let _t1 = fast_resize(&img1, 100, 100).unwrap();
        let _t2 = fast_resize(&img2, 100, 100).unwrap();

        let size_after = get_cache_stats().size;
        // 2개의 다른 이미지 → 최소 2개 증가
        assert!(
            size_after >= size_before + 2,
            "다른 이미지는 별도 캐싱되어야 함"
        );
    }

    #[test]
    fn image_hash_deterministic() {
        let img = make_test_image(640, 480, [128, 128, 128, 255]);
        let hash1 = compute_image_hash(&img);
        let hash2 = compute_image_hash(&img);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn image_hash_different_for_different_images() {
        let img1 = make_test_image(640, 480, [0, 0, 0, 255]);
        let img2 = make_test_image(640, 480, [255, 255, 255, 255]);

        let hash1 = compute_image_hash(&img1);
        let hash2 = compute_image_hash(&img2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn zero_size_source_error() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(0, 0));
        let result = fast_resize(&img, 100, 100);
        assert!(result.is_err());
    }

    #[test]
    fn zero_size_target_error() {
        let img = make_test_image(100, 100, [100, 100, 100, 255]);
        let result = fast_resize(&img, 0, 100);
        assert!(result.is_err());
    }
}
