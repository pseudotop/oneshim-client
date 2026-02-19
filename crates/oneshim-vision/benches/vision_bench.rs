//! oneshim-vision 성능 벤치마크
//!
//! 실행: cargo bench -p oneshim-vision
//!
//! 벤치마크 대상:
//! - 델타 인코딩 (compute_delta)
//! - 썸네일 생성 (fast_resize)
//! - WebP 인코딩 (encode_webp)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use image::{DynamicImage, Rgba, RgbaImage};
use oneshim_vision::{delta, encoder, encoder::WebPQuality, thumbnail};

/// 테스트용 랜덤 이미지 생성
fn create_test_image(width: u32, height: u32, seed: u8) -> DynamicImage {
    let mut img = RgbaImage::new(width, height);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let r = (x as u8).wrapping_add(seed).wrapping_mul(17);
        let g = (y as u8).wrapping_add(seed).wrapping_mul(31);
        let b = (x as u8).wrapping_add(y as u8).wrapping_add(seed);
        *pixel = Rgba([r, g, b, 255]);
    }
    DynamicImage::ImageRgba8(img)
}

/// 부분 변경된 이미지 생성 (델타 테스트용)
fn create_modified_image(base: &DynamicImage, change_ratio: f32) -> DynamicImage {
    let mut img = base.to_rgba8();
    let (w, h) = img.dimensions();
    let change_width = (w as f32 * change_ratio.sqrt()) as u32;
    let change_height = (h as f32 * change_ratio.sqrt()) as u32;

    // 좌상단 영역만 변경
    for y in 0..change_height.min(h) {
        for x in 0..change_width.min(w) {
            let pixel = img.get_pixel_mut(x, y);
            pixel[0] = pixel[0].wrapping_add(50);
            pixel[1] = pixel[1].wrapping_add(30);
        }
    }
    DynamicImage::ImageRgba8(img)
}

/// 델타 인코딩 벤치마크
fn bench_delta(c: &mut Criterion) {
    let mut group = c.benchmark_group("delta_encoding");

    // 다양한 해상도 테스트
    let resolutions = [(640, 480), (1280, 720), (1920, 1080)];

    for (width, height) in resolutions {
        let pixels = width * height;
        group.throughput(Throughput::Elements(pixels as u64));

        let prev = create_test_image(width, height, 42);
        let curr_10 = create_modified_image(&prev, 0.10); // 10% 변경
        let curr_50 = create_modified_image(&prev, 0.50); // 50% 변경

        // 10% 변경 시나리오
        group.bench_with_input(
            BenchmarkId::new("10%_change", format!("{}x{}", width, height)),
            &(&prev, &curr_10),
            |b, (prev, curr)| {
                b.iter(|| black_box(delta::compute_delta(prev, curr)));
            },
        );

        // 50% 변경 시나리오
        group.bench_with_input(
            BenchmarkId::new("50%_change", format!("{}x{}", width, height)),
            &(&prev, &curr_50),
            |b, (prev, curr)| {
                b.iter(|| black_box(delta::compute_delta(prev, curr)));
            },
        );
    }

    group.finish();
}

/// 썸네일 생성 벤치마크
fn bench_thumbnail(c: &mut Criterion) {
    let mut group = c.benchmark_group("thumbnail_resize");

    // 소스 해상도별 테스트
    let sources = [(1920, 1080), (2560, 1440), (3840, 2160)];
    let target = (480, 270); // 기본 썸네일 크기

    for (src_w, src_h) in sources {
        let pixels = src_w * src_h;
        group.throughput(Throughput::Elements(pixels as u64));

        let img = create_test_image(src_w, src_h, 123);

        // 새 이미지로 리사이즈 (캐시 미스 가능성 높음)
        group.bench_with_input(
            BenchmarkId::new("resize", format!("{}x{}", src_w, src_h)),
            &img,
            |b, img| {
                b.iter(|| black_box(thumbnail::fast_resize(img, target.0, target.1)));
            },
        );

        // 동일 이미지 반복 리사이즈 (캐시 히트 시나리오)
        let _ = thumbnail::fast_resize(&img, target.0, target.1); // 워밍업
        group.bench_with_input(
            BenchmarkId::new("cached", format!("{}x{}", src_w, src_h)),
            &img,
            |b, img| {
                b.iter(|| black_box(thumbnail::fast_resize(img, target.0, target.1)));
            },
        );
    }

    group.finish();
}

/// WebP 인코딩 벤치마크
fn bench_webp_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("webp_encoding");

    let resolutions = [(640, 480), (1280, 720), (1920, 1080)];
    let qualities = [
        ("low", WebPQuality::Low),
        ("medium", WebPQuality::Medium),
        ("high", WebPQuality::High),
    ];

    for (width, height) in resolutions {
        let pixels = width * height;
        let img = create_test_image(width, height, 77);

        for (quality_name, quality) in &qualities {
            group.throughput(Throughput::Elements(pixels as u64));

            group.bench_with_input(
                BenchmarkId::new(*quality_name, format!("{}x{}", width, height)),
                &(&img, *quality),
                |b, (img, quality)| {
                    b.iter(|| black_box(encoder::encode_webp(img, *quality)));
                },
            );
        }
    }

    group.finish();
}

/// 통합 파이프라인 벤치마크 (델타 → 썸네일 → WebP)
fn bench_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline");

    let (width, height) = (1920, 1080);
    let prev = create_test_image(width, height, 1);
    let curr = create_modified_image(&prev, 0.20);

    group.throughput(Throughput::Elements((width * height) as u64));

    group.bench_function("delta_thumbnail_webp", |b| {
        b.iter(|| {
            // 1. 델타 계산
            let _delta = delta::compute_delta(&prev, &curr);

            // 2. 썸네일 생성
            let thumb = thumbnail::fast_resize(&curr, 480, 270).unwrap();

            // 3. WebP 인코딩
            let _encoded = encoder::encode_webp(&thumb, WebPQuality::Medium);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_delta,
    bench_thumbnail,
    bench_webp_encode,
    bench_pipeline
);
criterion_main!(benches);
