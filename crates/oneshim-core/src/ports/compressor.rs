//! 압축 포트.
//!
//! 구현: `oneshim-network` crate (flate2, zstd, lz4_flex)

use crate::error::CoreError;

/// 압축 알고리즘 유형
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    /// gzip (flate2)
    Gzip,
    /// Zstandard
    Zstd,
    /// LZ4 (가장 빠름)
    Lz4,
}

/// 데이터 압축/해제 인터페이스
pub trait Compressor: Send + Sync {
    /// 데이터 압축
    fn compress(&self, data: &[u8], algorithm: CompressionAlgorithm) -> Result<Vec<u8>, CoreError>;

    /// 데이터 해제
    fn decompress(
        &self,
        data: &[u8],
        algorithm: CompressionAlgorithm,
    ) -> Result<Vec<u8>, CoreError>;
}
