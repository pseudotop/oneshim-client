//! Extracted analysis pipeline helpers for the adaptive tiered-memory system.
//!
//! These functions were previously inlined in the monitor loop body inside
//! `loops.rs`. Extracting them keeps the loop orchestrator concise while the
//! heavy analysis logic lives here.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use oneshim_analysis::TriggerDecision;
use oneshim_core::models::tiered_memory::{TriggerInput, TriggerReason};
use oneshim_core::models::work_session::AppCategory;
use oneshim_core::ports::storage::StorageService;
use oneshim_monitor::input_activity::InputActivityCollector;
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
pub(super) async fn run_analysis_tick(
    ts: &mut AdaptiveTriggerState,
    app_name: &str,
    window_title: &str,
    prev_app: &Option<String>,
    app_changed: bool,
    input_collector: &Arc<InputActivityCollector>,
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

    // 3. Get input activity snapshot
    let input_snap = input_collector.take_snapshot();

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

    // 5. Update content tracker
    if let Some(ref content) = parsed_content {
        ts.content_tracker.update(
            &content.content_label,
            content.content_type,
            work_type,
            engagement.clone(),
            content.confidence,
            now,
        );
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

/// Run regime detection (k-means) at most once per day from calibration data.
async fn run_periodic_regime_detection(ts: &mut AdaptiveTriggerState, now: DateTime<Utc>) {
    let should_detect = ts
        .last_detection_time
        .map(|last| (now - last).num_hours() >= 24)
        .unwrap_or(true);

    if !should_detect {
        return;
    }

    ts.last_detection_time = Some(now);
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

            let detected = ts.regime_detector.detect(&features);
            if !detected.is_empty() {
                info!(count = detected.len(), "regime detection completed");
                ts.regime_manager.update_from_detection(detected);
                ts.regime_manager.run_maintenance(now);

                // Update classifier with active regimes
                let active: Vec<_> = ts
                    .regime_manager
                    .active_regimes()
                    .into_iter()
                    .cloned()
                    .collect();
                ts.regime_classifier.update_regimes(active);
            }
        }
        Ok(_) => {
            debug!("regime detection skipped: insufficient data");
        }
        Err(e) => {
            warn!("regime detection failure: {e}");
        }
    }
}
