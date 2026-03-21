//! Periodic regime detection and constrained re-clustering.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use oneshim_core::models::work_session::AppCategory;
use std::collections::HashMap;
use tracing::{debug, info, warn};

use super::super::AdaptiveTriggerState;

/// Run regime detection at most once per day from calibration data,
/// or on demand when `recluster_requested` flag is set.
///
/// When a `ClusteringStrategy` is available, constrained re-clustering is used
/// (loading user overrides from `OverrideStore`). Otherwise falls back to the
/// legacy `RegimeDetector` (k-means).
pub(in crate::scheduler) async fn run_periodic_regime_detection(
    ts: &mut AdaptiveTriggerState,
    now: DateTime<Utc>,
) {
    let on_demand = ts
        .recluster_requested
        .swap(false, std::sync::atomic::Ordering::Relaxed);

    let should_detect = on_demand
        || ts
            .last_detection_time
            .map(|last| (now - last).num_hours() >= 24)
            .unwrap_or(true);

    if !should_detect {
        return;
    }

    ts.last_detection_time = Some(now);

    if on_demand {
        info!("on-demand re-clustering triggered");
    }

    let reader = ts.calibration_reader.clone();
    let lookback = now - ChronoDuration::days(7);

    match reader.get_entries(lookback, now, true).await {
        Ok(entries) if !entries.is_empty() => {
            // Build feature vectors from calibration entries
            let features: Vec<oneshim_core::models::tiered_memory::RegimeFeatures> = entries
                .iter()
                .map(|e| {
                    let cat = &e.app_category;
                    oneshim_core::models::tiered_memory::RegimeFeatures {
                        category_coding: if *cat == AppCategory::Development {
                            1.0
                        } else {
                            0.0
                        },
                        category_communication: if *cat == AppCategory::Communication {
                            1.0
                        } else {
                            0.0
                        },
                        category_browser: if *cat == AppCategory::Browser {
                            1.0
                        } else {
                            0.0
                        },
                        avg_event_rate: e.density_signal,
                        avg_importance: e.importance_signal,
                        context_activity_signal: e.context_signal,
                        communication_ratio: if *cat == AppCategory::Communication {
                            1.0
                        } else {
                            0.0
                        },
                    }
                })
                .collect();

            // Try constrained re-clustering via ClusteringStrategy if available
            let has_strategy = ts.clustering_strategy.is_some();
            if has_strategy {
                run_constrained_clustering(ts, &features, now).await;
            } else {
                // Fallback: legacy k-means regime detection
                let detected = ts.regime_detector.detect(&features);
                if !detected.is_empty() {
                    info!(
                        count = detected.len(),
                        "regime detection completed (legacy)"
                    );
                    ts.regime_manager.update_from_detection(detected);
                }
            }

            ts.regime_manager.run_maintenance(now);

            // Update classifier with active regimes
            let active: Vec<_> = ts
                .regime_manager
                .active_regimes()
                .into_iter()
                .cloned()
                .collect();
            ts.regime_classifier.update_regimes(active);

            // Reset drift detector after successful re-clustering
            ts.drift_detector.reset();
        }
        Ok(_) => {
            debug!("regime detection skipped: insufficient data");
        }
        Err(e) => {
            warn!("regime detection failure: {e}");
        }
    }
}

/// Run constrained re-clustering using `ClusteringStrategy` and user overrides.
///
/// Assumes `ts.clustering_strategy.is_some()` — caller must verify.
async fn run_constrained_clustering(
    ts: &mut AdaptiveTriggerState,
    features: &[oneshim_core::models::tiered_memory::RegimeFeatures],
    now: DateTime<Utc>,
) {
    use oneshim_analysis::constraint_builder;

    // Temporarily take the strategy out to avoid borrow conflict
    let strategy = ts.clustering_strategy.take().unwrap();

    // Load user overrides if OverrideStore is available
    let overrides = if let Some(ref store) = ts.override_store {
        let lookback = now - ChronoDuration::days(7);
        match store.list_overrides(lookback, now).await {
            Ok(ovrs) => ovrs,
            Err(e) => {
                warn!("failed to load overrides for re-clustering: {e}");
                vec![]
            }
        }
    } else {
        vec![]
    };

    let result = if overrides.is_empty() {
        // No overrides — standard detection
        strategy.as_ref().detect(features)
    } else {
        // Build feature_indices: map segment_id → feature vector index.
        // Query activity_segments for the lookback window, then for each
        // segment find the first calibration entry whose timestamp falls
        // within [segment.start, segment.end]. That entry's index in the
        // feature vector array becomes the segment's feature index.
        let lookback = now - ChronoDuration::days(7);
        let segment_ranges = match ts
            .calibration_reader
            .list_segment_time_ranges(lookback, now)
            .await
        {
            Ok(ranges) => ranges,
            Err(e) => {
                warn!("failed to load segment ranges for feature mapping: {e}");
                vec![]
            }
        };

        // Also need the calibration entries' timestamps to correlate indices.
        // Re-fetch entries with timestamps (they were already fetched by the
        // caller but we only received the derived feature vectors, not
        // timestamps). We query them again; this is once-per-detection.
        let entries_with_ts = match ts.calibration_reader.get_entries(lookback, now, true).await {
            Ok(entries) => entries,
            Err(e) => {
                warn!("failed to re-fetch calibration entries for index mapping: {e}");
                vec![]
            }
        };

        let feature_indices: HashMap<String, usize> = segment_ranges
            .iter()
            .filter_map(|(seg_id, seg_start, seg_end)| {
                // Find the first calibration entry whose timestamp falls within
                // this segment's time range. Its position = feature vector index.
                entries_with_ts
                    .iter()
                    .position(|e| e.timestamp >= *seg_start && e.timestamp <= *seg_end)
                    .map(|idx| (seg_id.clone(), idx))
            })
            .collect();

        let regime_cluster_map: HashMap<String, i32> = ts
            .regime_manager
            .active_regimes()
            .iter()
            .enumerate()
            .map(|(i, r)| (r.regime_id.clone(), i as i32))
            .collect();

        let constraints = constraint_builder::build_constraints(
            &overrides,
            &feature_indices,
            &regime_cluster_map,
        );

        if constraints.is_empty() {
            strategy.as_ref().detect(features)
        } else {
            info!(
                count = constraints.len(),
                "applying constraints to re-clustering"
            );
            strategy
                .as_ref()
                .detect_with_constraints(features, &constraints)
        }
    };

    let algo_name = strategy.algorithm_name().to_string();

    // Put strategy back before mutating ts
    ts.clustering_strategy = Some(strategy);

    match result {
        Ok(clustering_result) if clustering_result.cluster_count > 0 => {
            // Convert ClusteringResult to Regime vec for RegimeManager
            let detected = build_regimes_from_clustering(&clustering_result, features, now);
            if !detected.is_empty() {
                info!(
                    count = detected.len(),
                    noise = clustering_result.noise_count,
                    algorithm = algo_name,
                    "constrained regime detection completed"
                );
                ts.regime_manager.update_from_detection(detected);
            }
        }
        Ok(_) => {
            debug!(
                algorithm = algo_name,
                "clustering produced 0 clusters — skipping update"
            );
        }
        Err(e) => {
            warn!(
                algorithm = algo_name,
                "constrained clustering failure: {e} — falling back to legacy"
            );
            // Fallback to legacy k-means
            let detected = ts.regime_detector.detect(features);
            if !detected.is_empty() {
                info!(
                    count = detected.len(),
                    "regime detection completed (fallback)"
                );
                ts.regime_manager.update_from_detection(detected);
            }
        }
    }
}

/// Build `Regime` entries from a `ClusteringResult`.
fn build_regimes_from_clustering(
    result: &oneshim_analysis::clustering_strategy::ClusteringResult,
    features: &[oneshim_core::models::tiered_memory::RegimeFeatures],
    now: DateTime<Utc>,
) -> Vec<oneshim_core::models::tiered_memory::Regime> {
    use oneshim_core::models::tiered_memory::{Regime, RegimeStatus, TriggerParams};

    let mut cluster_points: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, &label) in result.labels.iter().enumerate() {
        if label >= 0 {
            cluster_points.entry(label).or_default().push(i);
        }
    }

    cluster_points
        .iter()
        .map(|(&cluster_id, indices)| {
            let centroid = if (cluster_id as usize) < result.centroids.len() {
                result.centroids[cluster_id as usize].clone()
            } else {
                // Compute centroid from member points
                let mut sum = oneshim_core::models::tiered_memory::RegimeFeatures::default();
                for &idx in indices {
                    if idx < features.len() {
                        sum.category_coding += features[idx].category_coding;
                        sum.category_communication += features[idx].category_communication;
                        sum.category_browser += features[idx].category_browser;
                        sum.avg_event_rate += features[idx].avg_event_rate;
                        sum.avg_importance += features[idx].avg_importance;
                        sum.context_activity_signal += features[idx].context_activity_signal;
                        sum.communication_ratio += features[idx].communication_ratio;
                    }
                }
                let n = indices.len() as f32;
                if n > 0.0 {
                    sum.category_coding /= n;
                    sum.category_communication /= n;
                    sum.category_browser /= n;
                    sum.avg_event_rate /= n;
                    sum.avg_importance /= n;
                    sum.context_activity_signal /= n;
                    sum.communication_ratio /= n;
                }
                sum
            };

            // Generate auto-label from dominant feature
            let auto_label = if centroid.category_coding > 0.5 {
                "Deep Work".to_string()
            } else if centroid.category_communication > 0.5 {
                "Communication".to_string()
            } else if centroid.category_browser > 0.5 {
                "Browsing".to_string()
            } else {
                format!("Regime-{}", cluster_id)
            };

            Regime {
                regime_id: format!("cluster-{}", cluster_id),
                name: None,
                auto_label,
                centroid,
                optimal_params: TriggerParams::default(),
                sample_count: indices.len() as u64,
                first_seen: now,
                last_seen: now,
                status: RegimeStatus::Active,
            }
        })
        .collect()
}
