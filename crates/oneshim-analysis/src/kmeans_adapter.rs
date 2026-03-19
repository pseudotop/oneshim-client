//! K-means adapter implementing `ClusteringStrategy`.
//!
//! Wraps the existing hand-rolled `RegimeDetector` to provide a unified
//! clustering interface alongside `HdbscanDetector`.

use std::collections::HashMap;
use std::sync::Mutex;

use oneshim_core::error::CoreError;
use oneshim_core::models::recalibration::ClusterConstraint;
use oneshim_core::models::tiered_memory::{euclidean_distance, RegimeFeatures};
use tracing::warn;

use crate::clustering_strategy::{ClusterAssignment, ClusteringResult, ClusteringStrategy};
use crate::regime_detector::RegimeDetector;

/// K-means clustering adapter for `ClusteringStrategy`.
///
/// Delegates to the existing `RegimeDetector` (hand-rolled k-means) and
/// converts its output to the unified `ClusteringResult` format.
pub struct KmeansDetector {
    max_k: usize,
    #[allow(dead_code)]
    max_iterations: usize,
    min_cluster_samples: u64,
    /// Stored centroids from the last `detect()` call, for `classify()`.
    centroids: Mutex<Vec<RegimeFeatures>>,
}

impl KmeansDetector {
    /// Create a new k-means detector with default settings.
    pub fn new() -> Self {
        Self {
            max_k: 7,
            max_iterations: 50,
            min_cluster_samples: 50,
            centroids: Mutex::new(Vec::new()),
        }
    }

    /// Override the maximum number of clusters.
    pub fn with_max_k(mut self, max_k: usize) -> Self {
        self.max_k = max_k;
        self
    }

    /// Override the minimum sample count.
    pub fn with_min_samples(mut self, min: u64) -> Self {
        self.min_cluster_samples = min;
        self
    }

    /// Build the internal RegimeDetector with current settings.
    fn build_detector(&self) -> RegimeDetector {
        RegimeDetector::new()
            .with_max_k(self.max_k)
            .with_min_samples(self.min_cluster_samples as usize)
    }

    /// Convert RegimeDetector output (Vec<Regime>) to ClusteringResult.
    fn regimes_to_result(
        &self,
        features: &[RegimeFeatures],
        detector: &RegimeDetector,
    ) -> ClusteringResult {
        let regimes = detector.detect(features);

        if regimes.is_empty() {
            return ClusteringResult {
                labels: vec![0; features.len()],
                centroids: vec![],
                cluster_count: 0,
                noise_count: 0,
                probabilities: None,
            };
        }

        let centroids: Vec<RegimeFeatures> = regimes.iter().map(|r| r.centroid.clone()).collect();

        // Assign each point to the nearest centroid
        let labels: Vec<i32> = features
            .iter()
            .map(|f| {
                let mut best_id = 0i32;
                let mut best_dist = f32::INFINITY;
                for (i, c) in centroids.iter().enumerate() {
                    let d = euclidean_distance(f, c);
                    if d < best_dist {
                        best_dist = d;
                        best_id = i as i32;
                    }
                }
                best_id
            })
            .collect();

        // Store centroids for classify()
        if let Ok(mut stored) = self.centroids.lock() {
            *stored = centroids.clone();
        }

        ClusteringResult {
            labels,
            centroids,
            cluster_count: regimes.len(),
            noise_count: 0,      // k-means has no noise concept
            probabilities: None, // k-means is hard assignment
        }
    }
}

impl Default for KmeansDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ClusteringStrategy for KmeansDetector {
    fn detect(&self, features: &[RegimeFeatures]) -> Result<ClusteringResult, CoreError> {
        let detector = self.build_detector();
        Ok(self.regimes_to_result(features, &detector))
    }

    fn classify(&self, point: &RegimeFeatures) -> Option<ClusterAssignment> {
        let centroids = self.centroids.lock().ok()?;
        if centroids.is_empty() {
            return None;
        }

        let mut best_id = -1i32;
        let mut best_dist = f32::INFINITY;

        for (i, centroid) in centroids.iter().enumerate() {
            let d = euclidean_distance(point, centroid);
            if d < best_dist {
                best_dist = d;
                best_id = i as i32;
            }
        }

        if best_id >= 0 {
            Some(ClusterAssignment {
                cluster_id: best_id,
                probability: 1.0,
            })
        } else {
            None
        }
    }

    fn detect_with_constraints(
        &self,
        features: &[RegimeFeatures],
        constraints: &[ClusterConstraint],
    ) -> Result<ClusteringResult, CoreError> {
        if features.is_empty() {
            return Ok(ClusteringResult {
                labels: vec![],
                centroids: vec![],
                cluster_count: 0,
                noise_count: 0,
                probabilities: None,
            });
        }

        // Collect noise indices and force-cluster assignments
        let mut noise_indices = std::collections::HashSet::new();
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
                    warn!("MustLink({a}, {b}) constraint ignored by k-means — not supported");
                }
                ClusterConstraint::CannotLink(a, b) => {
                    warn!("CannotLink({a}, {b}) constraint ignored by k-means — not supported");
                }
            }
        }

        // Build filtered data (exclude noise indices)
        let mut filtered_features = Vec::new();
        let mut original_indices = Vec::new();
        for (i, feat) in features.iter().enumerate() {
            if !noise_indices.contains(&i) {
                filtered_features.push(feat.clone());
                original_indices.push(i);
            }
        }

        // Run k-means on filtered data
        let sub_result = self.detect(&filtered_features)?;

        // Reconstruct full-size labels
        let mut full_labels = vec![-1i32; features.len()];
        for (sub_idx, &orig_idx) in original_indices.iter().enumerate() {
            if sub_idx < sub_result.labels.len() {
                full_labels[orig_idx] = sub_result.labels[sub_idx];
            }
        }

        // Apply ForceCluster overrides
        for (&idx, &cluster_id) in &force_clusters {
            if idx < full_labels.len() {
                full_labels[idx] = cluster_id;
            }
        }

        // Recompute noise count and centroids
        let noise_count = full_labels.iter().filter(|&&l| l < 0).count();

        // Store centroids from the sub-result (they're valid for the filtered data)
        if let Ok(mut stored) = self.centroids.lock() {
            *stored = sub_result.centroids.clone();
        }

        Ok(ClusteringResult {
            labels: full_labels,
            centroids: sub_result.centroids,
            cluster_count: sub_result.cluster_count,
            noise_count,
            probabilities: None,
        })
    }

    fn algorithm_name(&self) -> &str {
        "kmeans"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coding_point(rate: f32, importance: f32) -> RegimeFeatures {
        RegimeFeatures {
            category_coding: 1.0,
            category_communication: 0.0,
            category_browser: 0.0,
            avg_event_rate: rate,
            avg_importance: importance,
            context_activity_signal: 0.1,
            communication_ratio: 0.05,
        }
    }

    fn comm_point(rate: f32, importance: f32) -> RegimeFeatures {
        RegimeFeatures {
            category_coding: 0.0,
            category_communication: 1.0,
            category_browser: 0.0,
            avg_event_rate: rate,
            avg_importance: importance,
            context_activity_signal: 0.4,
            communication_ratio: 0.8,
        }
    }

    #[test]
    fn detect_matches_existing_regime_detector_behavior() {
        let detector = KmeansDetector::new().with_min_samples(5).with_max_k(5);

        let mut features = Vec::new();
        for i in 0..30 {
            features.push(coding_point(0.3 + (i as f32) * 0.005, 0.8));
        }
        for i in 0..30 {
            features.push(comm_point(0.7 + (i as f32) * 0.005, 0.4));
        }

        let result = detector.detect(&features).unwrap();

        // Should find 2 clusters (same as RegimeDetector)
        assert_eq!(result.cluster_count, 2);
        assert_eq!(result.noise_count, 0);
        assert_eq!(result.labels.len(), 60);
        assert!(result.probabilities.is_none());

        // All labels should be non-negative (k-means has no noise)
        assert!(result.labels.iter().all(|&l| l >= 0));
    }

    #[test]
    fn classify_works_after_detect() {
        let detector = KmeansDetector::new().with_min_samples(5).with_max_k(5);

        let mut features = Vec::new();
        for i in 0..30 {
            features.push(coding_point(0.3 + (i as f32) * 0.005, 0.8));
        }
        for i in 0..30 {
            features.push(comm_point(0.7 + (i as f32) * 0.005, 0.4));
        }

        let _result = detector.detect(&features).unwrap();

        // Classify a coding point
        let assignment = detector.classify(&coding_point(0.35, 0.85));
        assert!(assignment.is_some());
        let a = assignment.unwrap();
        assert!(a.cluster_id >= 0);
        assert!((a.probability - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn too_few_samples_returns_empty_clusters() {
        let detector = KmeansDetector::new().with_min_samples(50);

        let features: Vec<RegimeFeatures> = (0..10).map(|_| coding_point(0.5, 0.5)).collect();
        let result = detector.detect(&features).unwrap();

        assert_eq!(result.cluster_count, 0);
    }

    #[test]
    fn constraints_applied() {
        let detector = KmeansDetector::new().with_min_samples(5).with_max_k(5);

        let mut features = Vec::new();
        for i in 0..30 {
            features.push(coding_point(0.3 + (i as f32) * 0.005, 0.8));
        }
        for i in 0..30 {
            features.push(comm_point(0.7 + (i as f32) * 0.005, 0.4));
        }

        let constraints = vec![
            ClusterConstraint::NoiseLabel(0),
            ClusterConstraint::ForceCluster(1, 42),
        ];

        let result = detector
            .detect_with_constraints(&features, &constraints)
            .unwrap();

        assert_eq!(result.labels[0], -1); // noise
        assert_eq!(result.labels[1], 42); // forced
    }

    #[test]
    fn algorithm_name_returns_kmeans() {
        let detector = KmeansDetector::new();
        assert_eq!(detector.algorithm_name(), "kmeans");
    }

    #[test]
    fn default_trait() {
        let detector = KmeansDetector::default();
        assert_eq!(detector.max_k, 7);
        assert_eq!(detector.min_cluster_samples, 50);
    }
}
