//! Unified clustering interface for regime detection.
//!
//! Both `HdbscanDetector` and `KmeansDetector` implement this trait,
//! allowing the regime detection pipeline to be algorithm-agnostic.

use oneshim_core::error::CoreError;
use oneshim_core::models::recalibration::ClusterConstraint;
use oneshim_core::models::tiered_memory::RegimeFeatures;

/// Result of a clustering operation.
#[derive(Debug, Clone)]
pub struct ClusteringResult {
    /// Cluster label per input point. -1 = noise (HDBSCAN), non-negative = cluster ID.
    pub labels: Vec<i32>,
    /// Centroid (mean feature vector) per cluster, indexed by cluster ID.
    pub centroids: Vec<RegimeFeatures>,
    /// Number of distinct clusters discovered (excluding noise).
    pub cluster_count: usize,
    /// Number of points labeled as noise (-1).
    pub noise_count: usize,
    /// Soft membership probabilities (HDBSCAN only). `None` for hard-assignment algorithms.
    pub probabilities: Option<Vec<f32>>,
}

/// Assignment of a single point to a cluster.
#[derive(Debug, Clone)]
pub struct ClusterAssignment {
    /// The cluster this point belongs to.
    pub cluster_id: i32,
    /// Confidence of the assignment (1.0 for k-means, soft for HDBSCAN).
    pub probability: f32,
}

/// Unified interface for clustering algorithms used in regime detection.
///
/// This is NOT a port trait — it is a pure algorithm interface within
/// `oneshim-analysis`. Both `HdbscanDetector` and `KmeansDetector` implement it.
pub trait ClusteringStrategy: Send + Sync {
    /// Detect regimes from feature vectors.
    fn detect(&self, features: &[RegimeFeatures]) -> Result<ClusteringResult, CoreError>;

    /// Classify a single new point against existing clusters (nearest-centroid).
    fn classify(&self, point: &RegimeFeatures) -> Option<ClusterAssignment>;

    /// Re-detect with user override constraints applied.
    fn detect_with_constraints(
        &self,
        features: &[RegimeFeatures],
        constraints: &[ClusterConstraint],
    ) -> Result<ClusteringResult, CoreError>;

    /// Human-readable algorithm name for config/logging.
    fn algorithm_name(&self) -> &str;
}
