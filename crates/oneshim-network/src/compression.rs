//! 적응적 압축 어댑터.
//!
//! `Compressor` 포트 구현. gzip, zstd, lz4 알고리즘 지원.

use flate2::read::{GzDecoder, GzEncoder};
use flate2::Compression;
use oneshim_core::error::CoreError;
use oneshim_core::ports::compressor::{CompressionAlgorithm, Compressor};
use std::io::Read;

/// 적응적 압축기 — `Compressor` 포트 구현
pub struct AdaptiveCompressor;

impl AdaptiveCompressor {
    /// 새 적응적 압축기 생성
    pub fn new() -> Self {
        Self
    }

    /// 데이터 크기에 따라 최적 알고리즘 자동 선택
    ///
    /// - < 1KB: LZ4 (가장 빠름)
    /// - 1KB ~ 100KB: Zstd (균형)
    /// - > 100KB: Gzip (최고 압축)
    pub fn select_algorithm(data_size: usize) -> CompressionAlgorithm {
        if data_size < 1024 {
            CompressionAlgorithm::Lz4
        } else if data_size < 100 * 1024 {
            CompressionAlgorithm::Zstd
        } else {
            CompressionAlgorithm::Gzip
        }
    }

    /// 자동 알고리즘 선택 후 압축
    pub fn compress_auto(&self, data: &[u8]) -> Result<(Vec<u8>, CompressionAlgorithm), CoreError> {
        let algo = Self::select_algorithm(data.len());
        let compressed = self.compress(data, algo)?;
        Ok((compressed, algo))
    }
}

impl Default for AdaptiveCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl Compressor for AdaptiveCompressor {
    fn compress(&self, data: &[u8], algorithm: CompressionAlgorithm) -> Result<Vec<u8>, CoreError> {
        match algorithm {
            CompressionAlgorithm::Gzip => {
                let mut encoder = GzEncoder::new(data, Compression::default());
                let mut compressed = Vec::new();
                encoder
                    .read_to_end(&mut compressed)
                    .map_err(|e| CoreError::Internal(format!("gzip 압축 실패: {e}")))?;
                Ok(compressed)
            }
            CompressionAlgorithm::Zstd => zstd::encode_all(data, 3)
                .map_err(|e| CoreError::Internal(format!("zstd 압축 실패: {e}"))),
            CompressionAlgorithm::Lz4 => Ok(lz4_flex::compress_prepend_size(data)),
        }
    }

    fn decompress(
        &self,
        data: &[u8],
        algorithm: CompressionAlgorithm,
    ) -> Result<Vec<u8>, CoreError> {
        match algorithm {
            CompressionAlgorithm::Gzip => {
                let mut decoder = GzDecoder::new(data);
                let mut decompressed = Vec::new();
                decoder
                    .read_to_end(&mut decompressed)
                    .map_err(|e| CoreError::Internal(format!("gzip 해제 실패: {e}")))?;
                Ok(decompressed)
            }
            CompressionAlgorithm::Zstd => zstd::decode_all(data)
                .map_err(|e| CoreError::Internal(format!("zstd 해제 실패: {e}"))),
            CompressionAlgorithm::Lz4 => lz4_flex::decompress_size_prepended(data)
                .map_err(|e| CoreError::Internal(format!("lz4 해제 실패: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_roundtrip() {
        let compressor = AdaptiveCompressor::new();
        let data = b"Hello ONESHIM! This is test data for compression.";
        let compressed = compressor
            .compress(data, CompressionAlgorithm::Gzip)
            .unwrap();
        let decompressed = compressor
            .decompress(&compressed, CompressionAlgorithm::Gzip)
            .unwrap();
        assert_eq!(data.to_vec(), decompressed);
    }

    #[test]
    fn zstd_roundtrip() {
        let compressor = AdaptiveCompressor::new();
        let data = b"Zstandard compression test data for ONESHIM client.";
        let compressed = compressor
            .compress(data, CompressionAlgorithm::Zstd)
            .unwrap();
        let decompressed = compressor
            .decompress(&compressed, CompressionAlgorithm::Zstd)
            .unwrap();
        assert_eq!(data.to_vec(), decompressed);
    }

    #[test]
    fn lz4_roundtrip() {
        let compressor = AdaptiveCompressor::new();
        let data = b"LZ4 fast compression test.";
        let compressed = compressor
            .compress(data, CompressionAlgorithm::Lz4)
            .unwrap();
        let decompressed = compressor
            .decompress(&compressed, CompressionAlgorithm::Lz4)
            .unwrap();
        assert_eq!(data.to_vec(), decompressed);
    }

    #[test]
    fn algorithm_auto_selection() {
        // 작은 데이터 → LZ4
        assert_eq!(
            AdaptiveCompressor::select_algorithm(500),
            CompressionAlgorithm::Lz4
        );
        // 중간 데이터 → Zstd
        assert_eq!(
            AdaptiveCompressor::select_algorithm(50_000),
            CompressionAlgorithm::Zstd
        );
        // 큰 데이터 → Gzip
        assert_eq!(
            AdaptiveCompressor::select_algorithm(200_000),
            CompressionAlgorithm::Gzip
        );
    }

    #[test]
    fn compress_auto() {
        let compressor = AdaptiveCompressor::new();
        let data = vec![0u8; 50_000]; // 중간 크기 → Zstd
        let (compressed, algo) = compressor.compress_auto(&data).unwrap();
        assert_eq!(algo, CompressionAlgorithm::Zstd);
        let decompressed = compressor.decompress(&compressed, algo).unwrap();
        assert_eq!(data, decompressed);
    }

    #[test]
    fn empty_data() {
        let compressor = AdaptiveCompressor::new();
        let data = b"";
        for algo in [
            CompressionAlgorithm::Gzip,
            CompressionAlgorithm::Zstd,
            CompressionAlgorithm::Lz4,
        ] {
            let compressed = compressor.compress(data, algo).unwrap();
            let decompressed = compressor.decompress(&compressed, algo).unwrap();
            assert_eq!(data.to_vec(), decompressed);
        }
    }

    #[test]
    fn wrong_algorithm_gzip_as_zstd() {
        let compressor = AdaptiveCompressor::new();
        let data = b"test data for cross-algorithm check";
        let compressed = compressor
            .compress(data, CompressionAlgorithm::Gzip)
            .unwrap();
        // gzip 데이터를 zstd로 해제 시도 → 에러
        let result = compressor.decompress(&compressed, CompressionAlgorithm::Zstd);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_algorithm_lz4_as_gzip() {
        let compressor = AdaptiveCompressor::new();
        let data = b"cross algorithm test data lz4";
        let compressed = compressor
            .compress(data, CompressionAlgorithm::Lz4)
            .unwrap();
        // lz4 데이터를 gzip으로 해제 시도 → 에러
        let result = compressor.decompress(&compressed, CompressionAlgorithm::Gzip);
        assert!(result.is_err());
    }

    #[test]
    fn corrupted_data_gzip() {
        let compressor = AdaptiveCompressor::new();
        let corrupted = vec![0xFF, 0xFE, 0x00, 0x01, 0x02, 0x03];
        let result = compressor.decompress(&corrupted, CompressionAlgorithm::Gzip);
        assert!(result.is_err());
    }

    #[test]
    fn corrupted_data_zstd() {
        let compressor = AdaptiveCompressor::new();
        let corrupted = vec![0xAB, 0xCD, 0xEF, 0x00, 0x11, 0x22];
        let result = compressor.decompress(&corrupted, CompressionAlgorithm::Zstd);
        assert!(result.is_err());
    }

    #[test]
    fn corrupted_data_lz4() {
        let compressor = AdaptiveCompressor::new();
        // 유효한 데이터를 압축한 후 바이트를 뒤집어 손상시킴
        let data = b"valid test data for lz4 corruption test";
        let mut compressed = compressor
            .compress(data, CompressionAlgorithm::Lz4)
            .unwrap();
        // 압축 데이터 부분(크기 헤더 이후)을 손상
        for byte in compressed.iter_mut().skip(4) {
            *byte ^= 0xFF;
        }
        let result = compressor.decompress(&compressed, CompressionAlgorithm::Lz4);
        assert!(result.is_err());
    }
}
