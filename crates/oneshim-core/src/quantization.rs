//! Scalar INT8 quantization for embedding vectors.
//!
//! Converts f32 embedding vectors to i8 using per-vector min-max scaling.
//! ~4x storage reduction with ~99% recall preservation for 384-dim embeddings.
//!
//! See: docs/superpowers/specs/2026-03-19-p3-vector-compression-embedding-optimization-design.md

use crate::error::CoreError;
use serde::{Deserialize, Serialize};

/// A quantized embedding vector stored as INT8 with scale/offset for dequantization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedVector {
    /// INT8-quantized embedding data (384 bytes for MiniLM-L6-v2).
    pub data: Vec<i8>,
    /// Scale factor: (max - min) / 255.0
    pub scale: f32,
    /// Offset: min value of original vector.
    pub offset: f32,
}

/// Stateless scalar quantizer for f32 → INT8 conversion.
pub struct ScalarQuantizer;

impl ScalarQuantizer {
    /// Quantize a f32 embedding vector to INT8.
    ///
    /// Edge cases:
    /// - Zero-length vector: returns `CoreError::Internal`
    /// - Constant vector (all same value): scale=1.0, offset=min, all INT8=0
    /// - NaN/Inf: rejected via `f32::is_finite()` pre-scan
    pub fn quantize(vector: &[f32]) -> Result<QuantizedVector, CoreError> {
        if vector.is_empty() {
            return Err(CoreError::Internal(
                "cannot quantize zero-length vector".to_string(),
            ));
        }

        // Reject NaN/Inf
        if !vector.iter().all(|v| v.is_finite()) {
            return Err(CoreError::Internal(
                "vector contains NaN or Inf values".to_string(),
            ));
        }

        let min = vector.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = vector.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        let range = max - min;
        let scale = if range < f32::EPSILON {
            1.0
        } else {
            range / 255.0
        };

        let data: Vec<i8> = vector
            .iter()
            .map(|&v| {
                let normalized = if range < f32::EPSILON {
                    0.0
                } else {
                    (v - min) / range * 255.0
                };
                (normalized.round() as i16 - 128).clamp(-128, 127) as i8
            })
            .collect();

        Ok(QuantizedVector {
            data,
            scale,
            offset: min,
        })
    }

    /// Dequantize an INT8 vector back to f32 (approximate reconstruction).
    pub fn dequantize(qv: &QuantizedVector) -> Vec<f32> {
        qv.data
            .iter()
            .map(|&v| (v as f32 + 128.0) * qv.scale + qv.offset)
            .collect()
    }

    /// Compute approximate cosine similarity between two quantized vectors
    /// using INT8 dot product (avoids full dequantization).
    pub fn cosine_similarity_int8(a: &QuantizedVector, b: &QuantizedVector) -> f32 {
        if a.data.len() != b.data.len() || a.data.is_empty() {
            return 0.0;
        }

        let mut dot: i64 = 0;
        let mut norm_a: i64 = 0;
        let mut norm_b: i64 = 0;

        for (va, vb) in a.data.iter().zip(b.data.iter()) {
            let a_val = *va as i64;
            let b_val = *vb as i64;
            dot += a_val * b_val;
            norm_a += a_val * a_val;
            norm_b += b_val * b_val;
        }

        let denom = ((norm_a as f64).sqrt() * (norm_b as f64).sqrt()) as f32;
        if denom < f32::EPSILON {
            0.0
        } else {
            dot as f32 / denom
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_basic() {
        let v = vec![0.0, 0.5, 1.0, -0.5, -1.0];
        let qv = ScalarQuantizer::quantize(&v).unwrap();
        assert_eq!(qv.data.len(), 5);

        // Dequantize should be approximately equal
        let reconstructed = ScalarQuantizer::dequantize(&qv);
        for (orig, recon) in v.iter().zip(reconstructed.iter()) {
            assert!((orig - recon).abs() < 0.02, "{orig} vs {recon}");
        }
    }

    #[test]
    fn quantize_empty_vector() {
        let result = ScalarQuantizer::quantize(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn quantize_constant_vector() {
        let v = vec![0.5; 384];
        let qv = ScalarQuantizer::quantize(&v).unwrap();
        // All values should be the same after dequant
        let reconstructed = ScalarQuantizer::dequantize(&qv);
        for val in &reconstructed {
            assert!((val - 0.5).abs() < 0.01);
        }
    }

    #[test]
    fn quantize_nan_rejected() {
        let v = vec![1.0, f32::NAN, 0.5];
        let result = ScalarQuantizer::quantize(&v);
        assert!(result.is_err());
    }

    #[test]
    fn quantize_inf_rejected() {
        let v = vec![1.0, f32::INFINITY, 0.5];
        let result = ScalarQuantizer::quantize(&v);
        assert!(result.is_err());
    }

    #[test]
    fn cosine_similarity_identical() {
        let v = vec![0.1, 0.5, 0.9, -0.3, 0.7];
        let qv = ScalarQuantizer::quantize(&v).unwrap();
        let sim = ScalarQuantizer::cosine_similarity_int8(&qv, &qv);
        assert!((sim - 1.0).abs() < 0.01);
    }

    #[test]
    fn cosine_similarity_different() {
        // Use higher-dimensional vectors to reduce quantization noise
        let mut a = vec![0.0; 64];
        let mut b = vec![0.0; 64];
        // Make them point in different directions
        for i in 0..32 {
            a[i] = 1.0;
        }
        for i in 32..64 {
            b[i] = 1.0;
        }
        let qa = ScalarQuantizer::quantize(&a).unwrap();
        let qb = ScalarQuantizer::quantize(&b).unwrap();
        let sim = ScalarQuantizer::cosine_similarity_int8(&qa, &qb);
        // Quantized orthogonal vectors — similarity should be low
        assert!(sim < 0.3, "expected low similarity, got {sim}");
    }
}
