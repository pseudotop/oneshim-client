//! Unified clustering interface for regime detection.
//!
//! Both `HdbscanDetector` and `KmeansDetector` implement this trait,
//! allowing the regime detection pipeline to be algorithm-agnostic.
//!
//! Shared constraint preprocessing helpers are provided so that each
//! detector does not duplicate the NoiseLabel / ForceCluster logic.

use std::collections::{HashMap, HashSet};

use oneshim_core::error::CoreError;
use oneshim_core::models::recalibration::ClusterConstraint;
use oneshim_core::models::tiered_memory::RegimeFeatures;
use tracing::warn;

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

// ---------------------------------------------------------------------------
// Shared constraint preprocessing helpers
// ---------------------------------------------------------------------------

/// Parsed constraint directives ready for the clustering pipeline.
///
/// Produced by [`parse_constraints`], consumed by [`filter_features`] and
/// [`reconstruct_labels`].
pub struct ParsedConstraints {
    /// Point indices that should be excluded from clustering and labeled as noise.
    pub noise_indices: HashSet<usize>,
    /// Point indices that should be force-assigned to a specific cluster ID.
    pub force_clusters: HashMap<usize, i32>,
}

/// Parse a slice of [`ClusterConstraint`] into noise exclusions and force-cluster
/// assignments.  Unsupported constraint types (MustLink, CannotLink) are logged
/// and skipped.
///
/// `algorithm` is included in the warning message for diagnostics (e.g. "hdbscan",
/// "k-means").
pub fn parse_constraints(constraints: &[ClusterConstraint], algorithm: &str) -> ParsedConstraints {
    let mut noise_indices = HashSet::new();
    let mut force_clusters: HashMap<usize, i32> = HashMap::new();

    for constraint in constraints {
        match constraint {
            ClusterConstraint::NoiseLabel(idx) => {
                noise_indices.insert(*idx);
            }
            ClusterConstraint::ForceCluster(idx, cluster_id) => {
                force_clusters.insert(*idx, *cluster_id);
            }
            ClusterConstraint::MustLink(a, b) => {
                warn!(
                    "MustLink({a}, {b}) constraint ignored by {algorithm} — not supported in Phase 1"
                );
            }
            ClusterConstraint::CannotLink(a, b) => {
                warn!(
                    "CannotLink({a}, {b}) constraint ignored by {algorithm} — not supported in Phase 1"
                );
            }
        }
    }

    ParsedConstraints {
        noise_indices,
        force_clusters,
    }
}

/// Filter out noise-labeled points and return the surviving features together
/// with a mapping back to their original indices.
pub fn filter_features(
    features: &[RegimeFeatures],
    noise_indices: &HashSet<usize>,
) -> (Vec<RegimeFeatures>, Vec<usize>) {
    let mut filtered = Vec::new();
    let mut original_indices = Vec::new();
    for (i, feat) in features.iter().enumerate() {
        if !noise_indices.contains(&i) {
            filtered.push(feat.clone());
            original_indices.push(i);
        }
    }
    (filtered, original_indices)
}

/// Reconstruct a full-length label vector from the sub-result produced by
/// clustering on filtered data, then apply ForceCluster overrides.
///
/// Points that were excluded (noise) retain label -1.
pub fn reconstruct_labels(
    total_len: usize,
    sub_labels: &[i32],
    original_indices: &[usize],
    force_clusters: &HashMap<usize, i32>,
) -> Vec<i32> {
    let mut full_labels = vec![-1i32; total_len];
    for (sub_idx, &orig_idx) in original_indices.iter().enumerate() {
        if sub_idx < sub_labels.len() {
            full_labels[orig_idx] = sub_labels[sub_idx];
        }
    }
    for (&idx, &cluster_id) in force_clusters {
        if idx < full_labels.len() {
            full_labels[idx] = cluster_id;
        }
    }
    full_labels
}
