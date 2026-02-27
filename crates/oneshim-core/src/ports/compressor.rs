use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    /// gzip (flate2)
    Gzip,
    /// Zstandard
    Zstd,
    Lz4,
}

pub trait Compressor: Send + Sync {
    fn compress(&self, data: &[u8], algorithm: CompressionAlgorithm) -> Result<Vec<u8>, CoreError>;

    fn decompress(
        &self,
        data: &[u8],
        algorithm: CompressionAlgorithm,
    ) -> Result<Vec<u8>, CoreError>;
}
