//! Extracted analysis pipeline helpers for the adaptive tiered-memory system.
//!
//! These functions were previously inlined in the monitor loop body inside
//! `loops.rs`. Extracting them keeps the loop orchestrator concise while the
//! heavy analysis logic lives here.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use oneshim_analysis::TriggerDecision;
use oneshim_core::models::event::InputActivityEvent;
use oneshim_core::models::gui_activity::GuiActivitySummary;
use oneshim_core::models::tiered_memory::{TriggerInput, TriggerReason};
use oneshim_core::models::work_session::AppCategory;
use oneshim_core::ports::storage::StorageService;
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::AdaptiveTriggerState;

/// Run a single tick of the adaptive tiered-memory pipeline.
///
/// This covers:
/// 1. TriggerInput classification
/// 2. Title-bar parsing & work-type classification
/// 3. Content tracking
/// 4. Regime classification & parameter cascade
/// 5. AdaptiveTrigger evaluation
/// 6. Calibration buffer flush
/// 7. Segment lifecycle (open / close / restart)
/// 8. Embedding pipeline (Phase 1 immediate + Phase 2 async spawn)
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_analysis_tick(
    ts: &mut AdaptiveTriggerState,
    app_name: &str,
    window_title: &str,
    prev_app: &Option<String>,
    app_changed: bool,
    input_snap: &InputActivityEvent,
    gui_summary: Option<&GuiActivitySummary>,
    storage: &Arc<dyn StorageService>,
) {
    let now = Utc::now();

    // 1. Classify event → TriggerInput
    let trigger_input = if app_changed {
        TriggerInput::AppSwitchNew {
            app_name: app_name.to_string(),
            prev_app: prev_app.clone().unwrap_or_default(),
            category: AppCategory::from_app_name(app_name),
        }
    } else {
        TriggerInput::AppPoll {
            app_name: app_name.to_string(),
        }
    };

    // 2. Parse title bar → content
    let parsed_content = ts.title_bar_parser.parse(app_name, window_title);

    // 3. Input activity snapshot (passed in by caller — shared with GUI pipeline)

    // 4. Classify work type
    let (work_type, engagement) = if let Some(ref content) = parsed_content {
        ts.work_type_classifier.classify(
            &input_snap.keyboard,
            &input_snap.mouse,
            &content.content_label,
            AppCategory::from_app_name(app_name),
        )
    } else {
        (
            oneshim_core::models::tiered_memory::WorkType::Unknown,
            oneshim_core::models::tiered_memory::EngagementMetrics::default(),
        )
    };

    // 4b. Refine work type using GUI signals (if available)
    let work_type = if let Some(ref gui) = gui_summary {
        ts.gui_work_type_refiner.refine(work_type, gui)
    } else {
        work_type
    };

    // 5. Update content tracker
    if let Some(ref content) = parsed_content {
        ts.content_tracker
            .update(oneshim_analysis::content_tracker::ContentUpdateInput {
                content_label: content.content_label.clone(),
                content_type: content.content_type,
                work_type,
                engagement: engagement.clone(),
                confidence: content.confidence,
                timestamp: now,
                gui_summary: gui_summary.cloned(),
            });
    }

    // 5b. Regime classification → parameter cascade
    let app_category = AppCategory::from_app_name(app_name);
    let current_regime =
        ts.regime_classifier
            .classify(&oneshim_core::models::tiered_memory::RegimeFeatures {
                category_coding: if app_category == AppCategory::Development {
                    1.0
                } else {
                    0.0
                },
                category_communication: if app_category == AppCategory::Communication {
                    1.0
                } else {
                    0.0
                },
                category_browser: if app_category == AppCategory::Browser {
                    1.0
                } else {
                    0.0
                },
                avg_event_rate: ts.trigger.current_density_signal(),
                avg_importance: ts.trigger.current_importance_signal(),
                context_activity_signal: ts.trigger.current_context_signal(),
                communication_ratio: if app_category == AppCategory::Communication {
                    1.0
                } else {
                    0.0
                },
            });

    // Detect regime transition
    let new_regime_id = current_regime.map(|r| r.regime_id.clone());
    if new_regime_id != ts.current_regime_id {
        let from_label = ts.current_regime_id.clone();
        if let Some(ref to_id) = new_regime_id {
            info!(
                from = ?from_label,
                to = %to_id,
                "regime transition detected"
            );
        }
        ts.current_regime_id = new_regime_id.clone();
    }

    // Mark regime as seen
    if let Some(ref regime_id) = new_regime_id {
        ts.regime_manager.mark_seen(regime_id, now);
    }

    // Resolve params via CSS cascade
    let resolved = ts
        .param_resolver
        .resolve(current_regime, &app_category, app_name);
    ts.params = resolved;

    // 5c. Auto-tuner: update EMA stats per tick
    let density = ts.trigger.current_density_signal();
    let importance = ts.trigger.current_importance_signal();
    let cat_str = format!("{:?}", app_category).to_lowercase();
    ts.ema_tracker
        .update(&cat_str, app_name, density, importance);

    ts.auto_tune_tick_count += 1;

    // Periodically (every 100 ticks): generate overrides → ParamResolver
    if ts.auto_tune_tick_count % 100 == 0 {
        let overrides = ts.ema_tracker.generate_overrides();
        for (cat_key, params) in &overrides {
            let category = AppCategory::from_category_str(cat_key);
            ts.param_resolver
                .set_category_override(category, params.clone());
        }
        if !overrides.is_empty() {
            debug!(count = overrides.len(), "auto-tune overrides applied");
        }

        // Check drift on importance signal
        if ts.drift_detector.observe(importance) {
            info!("drift detected — flagging for re-clustering");
            ts.recluster_requested
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    // 6. Feed to AdaptiveTrigger
    let (decision, cal_entry) = ts.trigger.process_event(&trigger_input, now, &ts.params);

    // 7. Buffer calibration entry
    if let Some(batch) = ts.calibration_buffer.push(cal_entry) {
        if let Err(e) = ts.calibration_writer.log_batch(&batch) {
            warn!("calibration log failure: {e}");
        }
    }

    // 8. Handle segment lifecycle
    handle_segment_lifecycle(ts, decision, trigger_input, now, storage).await;

    // --- Periodic regime detection (daily) ---
    run_periodic_regime_detection(ts, now).await;
}

/// Handle segment open / close / restart decisions and trigger embedding pipeline.
async fn handle_segment_lifecycle(
    ts: &mut AdaptiveTriggerState,
    decision: TriggerDecision,
    trigger_input: TriggerInput,
    now: DateTime<Utc>,
    storage: &Arc<dyn StorageService>,
) {
    match decision {
        TriggerDecision::OpenSegment => {
            ts.trigger.start_new_segment(now);
            ts.segment_buffer.start_segment(now);
            ts.segment_buffer.push(now, trigger_input);
        }
        TriggerDecision::RestartSegment
        | TriggerDecision::CloseSegment
        | TriggerDecision::ForceCloseSegment => {
            handle_segment_close(ts, decision, now, storage).await;

            // If restart, open new segment
            if matches!(decision, TriggerDecision::RestartSegment) {
                ts.trigger.start_new_segment(now);
                ts.segment_buffer.start_segment(now);
            }
        }
        TriggerDecision::Continue => {
            ts.segment_buffer.push(now, trigger_input);
        }
    }
}

/// Close a segment: summarize, run embedding Phase 1, spawn Phase 2 LLM summary.
async fn handle_segment_close(
    ts: &mut AdaptiveTriggerState,
    decision: TriggerDecision,
    now: DateTime<Utc>,
    storage: &Arc<dyn StorageService>,
) {
    let _seg_events = ts.segment_buffer.drain_all();
    let content_activities = ts.content_tracker.drain_all(now);

    let reason = match decision {
        TriggerDecision::RestartSegment => TriggerReason::ScoreHigh,
        TriggerDecision::CloseSegment => TriggerReason::ScoreLow,
        TriggerDecision::ForceCloseSegment => TriggerReason::ForcedMaxDuration,
        _ => TriggerReason::ScoreHigh,
    };

    if let Some(start) = ts.trigger.current_segment_start() {
        let summary = ts.segment_summarizer.summarize(
            uuid::Uuid::new_v4().to_string(),
            start,
            now,
            &[], // raw events from storage (Phase 1b)
            content_activities,
            None, // container detection (Phase 1b)
            reason,
            ts.current_regime_id.clone(),
        );

        info!(
            segment_id = %summary.segment_id,
            duration = summary.duration_secs,
            events = summary.event_count,
            "segment closed: {}",
            summary.dominant_category
        );

        // Phase 1: Embed content activities immediately
        if let Some(ref pipeline) = ts.embedding_pipeline {
            if let Err(e) = pipeline.process_content_activities(&summary).await {
                warn!("content embedding failure: {e}");
            }
        }

        // Phase 2: Async LLM summary + embed (non-blocking)
        if let Some(ref summarizer) = ts.llm_summarizer {
            let summarizer = summarizer.clone();
            let storage_clone = storage.clone();
            let pipeline = ts.embedding_pipeline.clone();
            let segment_id = summary.segment_id.clone();
            let end_time = summary.end_time;
            let summary_clone = summary.clone();

            tokio::spawn(async move {
                if let Some(text) = summarizer.summarize(&summary_clone).await {
                    if let Err(e) = storage_clone
                        .update_segment_llm_summary(&segment_id, &text)
                        .await
                    {
                        warn!("LLM summary storage failure: {e}");
                    }
                    if let Some(pipeline) = pipeline {
                        if let Err(e) = pipeline
                            .process_llm_summary(&segment_id, &text, end_time)
                            .await
                        {
                            warn!("LLM summary embedding failure: {e}");
                        }
                    }
                }
            });
        }
    }

    ts.trigger.close_segment();
}

/// Run regime detection at most once per day from calibration data,
/// or on demand when `recluster_requested` flag is set.
///
/// When a `ClusteringStrategy` is available, constrained re-clustering is used
/// (loading user overrides from `OverrideStore`). Otherwise falls back to the
/// legacy `RegimeDetector` (k-means).
async fn run_periodic_regime_detection(ts: &mut AdaptiveTriggerState, now: DateTime<Utc>) {
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
    use std::collections::HashMap;

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
    use std::collections::HashMap;

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
