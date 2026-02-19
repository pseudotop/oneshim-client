//! oneshim-network 압축 성능 벤치마크
//!
//! 실행: cargo bench -p oneshim-network
//!
//! 벤치마크 대상:
//! - 알고리즘별 압축 (gzip, zstd, lz4)
//! - 자동 알고리즘 선택 + 압축
//! - 라운드트립 (압축 → 압축 해제)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oneshim_core::ports::compressor::{CompressionAlgorithm, Compressor};
use oneshim_network::compression::AdaptiveCompressor;

/// 테스트용 데이터 생성 (반복 패턴 — 압축 가능)
fn create_compressible_data(size: usize) -> Vec<u8> {
    let pattern =
        b"ONESHIM event payload with repeated structure and timestamps 2026-01-31T12:00:00Z ";
    pattern.iter().cycle().take(size).copied().collect()
}

/// 테스트용 데이터 생성 (무작위 — 압축 어려움)
fn create_random_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| ((i * 17 + 31) % 256) as u8).collect()
}

/// 알고리즘별 압축 벤치마크
fn bench_compress_by_algorithm(c: &mut Criterion) {
    let mut group = c.benchmark_group("compress_algorithm");
    let compressor = AdaptiveCompressor::new();

    let sizes = [1024, 10_240, 102_400]; // 1KB, 10KB, 100KB
    let algorithms = [
        ("gzip", CompressionAlgorithm::Gzip),
        ("zstd", CompressionAlgorithm::Zstd),
        ("lz4", CompressionAlgorithm::Lz4),
    ];

    for size in sizes {
        let data = create_compressible_data(size);
        group.throughput(Throughput::Bytes(size as u64));

        for (name, algo) in &algorithms {
            group.bench_with_input(
                BenchmarkId::new(*name, format!("{}KB", size / 1024)),
                &(&data, *algo),
                |b, (data, algo)| {
                    b.iter(|| black_box(compressor.compress(data, *algo).unwrap()));
                },
            );
        }
    }

    group.finish();
}

/// 자동 선택 압축 벤치마크
fn bench_compress_auto(c: &mut Criterion) {
    let mut group = c.benchmark_group("compress_auto");
    let compressor = AdaptiveCompressor::new();

    let sizes = [512, 2048, 16_384, 65_536]; // 다양한 크기 → 다른 알고리즘 선택

    for size in sizes {
        let data = create_compressible_data(size);
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("compressible", format!("{}B", size)),
            &data,
            |b, data| {
                b.iter(|| black_box(compressor.compress_auto(data).unwrap()));
            },
        );
    }

    // 랜덤 데이터 (비압축성)
    let random_data = create_random_data(65_536);
    group.throughput(Throughput::Bytes(65_536));
    group.bench_function("random_64KB", |b| {
        b.iter(|| black_box(compressor.compress_auto(&random_data).unwrap()));
    });

    group.finish();
}

/// 라운드트립 (압축 → 압축 해제) 벤치마크
fn bench_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("compress_roundtrip");
    let compressor = AdaptiveCompressor::new();

    let data = create_compressible_data(65_536); // 64KB
    group.throughput(Throughput::Bytes(65_536));

    let algorithms = [
        ("gzip", CompressionAlgorithm::Gzip),
        ("zstd", CompressionAlgorithm::Zstd),
        ("lz4", CompressionAlgorithm::Lz4),
    ];

    for (name, algo) in &algorithms {
        let compressed = compressor.compress(&data, *algo).unwrap();

        group.bench_with_input(
            BenchmarkId::new(*name, "64KB"),
            &(&compressed, *algo),
            |b, (compressed, algo)| {
                b.iter(|| black_box(compressor.decompress(compressed, *algo).unwrap()));
            },
        );
    }

    group.finish();
}

/// 알고리즘 선택 벤치마크 (순수 분기 성능)
fn bench_algorithm_selection(c: &mut Criterion) {
    let mut group = c.benchmark_group("algorithm_selection");

    let sizes = [100, 1_000, 10_000, 100_000, 1_000_000];

    for size in sizes {
        group.bench_with_input(
            BenchmarkId::new("select", format!("{}B", size)),
            &size,
            |b, &size| {
                b.iter(|| black_box(AdaptiveCompressor::select_algorithm(size)));
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_compress_by_algorithm,
    bench_compress_auto,
    bench_roundtrip,
    bench_algorithm_selection
);
criterion_main!(benches);
