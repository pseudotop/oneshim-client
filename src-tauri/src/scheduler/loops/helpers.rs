use std::sync::Arc;
use tracing::{debug, info, warn};

use oneshim_core::models::frame::{ImagePayload, OcrRegion};
use oneshim_core::models::storage_records::SegmentSummaryRecord;
use oneshim_core::models::tiered_memory::{ContentActivity, SegmentSummary, TriggerReason};
use oneshim_core::ports::storage::StorageService;
use oneshim_core::ports::vision::{CaptureRequest, FrameProcessor};
use oneshim_storage::frame_storage::FrameFileStorage;

use super::super::config::{base64_decode, SchedulerStorage};

// ── Coaching LLM personalization ──────────────────────────────────────

pub(super) const COACHING_SYSTEM_PROMPT: &str =
    "You are a concise productivity coach. Rewrite the given message \
     to be more personalized and contextual. Keep the same intent. \
     Respond with ONLY the rewritten message, no preamble.";

pub(super) fn build_personalization_prompt(template_text: &str, regime_label: &str) -> String {
    format!(
        "Rewrite this productivity coaching message to be more personalized \
         and contextual. Keep the same intent and information, but make it \
         feel natural.\n\n\
         Original: {template_text}\n\
         Current regime: {regime_label}\n\
         Respond with ONLY the rewritten message, no preamble.",
    )
}

/// Build a `SegmentStats` snapshot from the current `AdaptiveTriggerState`.
/// Returns `None` if the content tracker has no active content.
pub(super) fn build_segment_stats_snapshot(
    ts: &super::super::AdaptiveTriggerState,
) -> Option<oneshim_analysis::SegmentStats> {
    let entries = oneshim_analysis::to_content_summary_entries(&ts.content_tracker.peek());
    if entries.is_empty() {
        return None;
    }

    let duration_mins = ts
        .trigger
        .current_segment_start()
        .map(|start| {
            let elapsed = (chrono::Utc::now() - start).num_seconds().max(0) as u32;
            elapsed / 60
        })
        .unwrap_or(0);

    let gui_patterns: Vec<String> = entries
        .iter()
        .flat_map(|e| e.gui_patterns.iter().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    Some(oneshim_analysis::SegmentStats {
        duration_mins,
        regime_label: ts.current_regime_id.clone(),
        event_count: 0, // not tracked per-tick; segment summarizer computes on close
        context_switches: 0,
        dominant_category: entries
            .first()
            .map(|e| e.content_type.clone())
            .unwrap_or_default(),
        content_summary: entries,
        gui_patterns,
    })
}

/// Run event-driven LLM analysis when the user switches to a new app.
/// Persists any resulting suggestions to storage.
#[tracing::instrument(skip_all)]
pub(super) async fn handle_event_analysis(
    analyzer: &Option<Arc<oneshim_analysis::ContextAnalyzer>>,
    storage: &Arc<dyn StorageService>,
    app_name: &str,
    window_title: &str,
    ocr_hint: Option<&str>,
) {
    if let Some(ref analyzer) = analyzer {
        match analyzer
            .on_significant_event(app_name, window_title, ocr_hint)
            .await
        {
            Ok(suggestions) => {
                for s in &suggestions {
                    info!(
                        id = %s.suggestion_id,
                        priority = ?s.priority,
                        "event-driven suggestion: {}",
                        s.content
                    );
                    if let Err(e) = storage.save_suggestion(s).await {
                        warn!("suggestion save failure: {e}");
                    }
                }
            }
            Err(e) => {
                debug!("event analysis skipped: {e}");
            }
        }
    }
}

/// Capture a frame, process it (full/delta/thumbnail), save image data and
/// metadata.  Returns the OCR text extracted from the frame (if any) and
/// any OCR regions with bounding boxes for GUI element correlation.
#[tracing::instrument(skip_all)]
pub(super) async fn handle_frame_capture(
    capture_req: &CaptureRequest,
    processor: &Arc<dyn FrameProcessor>,
    frame_storage: &Option<Arc<FrameFileStorage>>,
    sqlite: &Arc<dyn SchedulerStorage>,
    session_id: &str,
) -> (Option<String>, Vec<OcrRegion>) {
    match processor.capture_and_process(capture_req).await {
        Ok(frame) => {
            debug!("frame completed: {:?}", frame.metadata.trigger_type);

            // Grab OCR regions from the processed frame before consuming payload
            let ocr_regions = frame.ocr_regions.clone();

            let (file_path, ocr_text) = if let Some(ref payload) = frame.image_payload {
                let (data_str, ocr) = match payload {
                    ImagePayload::Full { data, ocr_text, .. } => (data.as_str(), ocr_text.clone()),
                    ImagePayload::Delta { data, .. } => (data.as_str(), None),
                    ImagePayload::Thumbnail { data, .. } => (data.as_str(), None),
                };

                let saved_path = if let Some(ref fs) = frame_storage {
                    match base64_decode(data_str) {
                        Ok(webp_bytes) => {
                            match fs.save_frame(frame.metadata.timestamp, &webp_bytes).await {
                                Ok(path) => Some(path.to_string_lossy().to_string()),
                                Err(e) => {
                                    warn!("frame file save failure: {e}");
                                    None
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Base64 decoding failure: {e}");
                            None
                        }
                    }
                } else {
                    None
                };

                (saved_path, ocr)
            } else {
                (None, None)
            };

            if let Err(e) = sqlite.save_frame_metadata_with_bounds(
                &frame.metadata,
                file_path.as_deref(),
                ocr_text.as_deref(),
                capture_req.window_bounds.as_ref(),
            ) {
                warn!("frame data save failure: {e}");
            }

            let _ = sqlite.increment_session_counters(session_id, 0, 1, 0).await;

            (ocr_text, ocr_regions)
        }
        Err(e) => {
            warn!("frame failure: {e}");
            (None, Vec::new())
        }
    }
}

/// Convert a SegmentSummaryRecord (storage row) to SegmentSummary (domain model)
/// for use with DailyDigestGenerator.
pub(crate) fn record_to_segment_summary(r: &SegmentSummaryRecord) -> Option<SegmentSummary> {
    let start_time = r.start_time.parse().ok()?;
    let end_time = r.end_time.parse().ok()?;

    let app_breakdown: std::collections::HashMap<String, u64> =
        serde_json::from_str(&r.app_breakdown).unwrap_or_default();

    let content_activities: Vec<ContentActivity> =
        serde_json::from_str(&r.content_activities_json).unwrap_or_default();

    Some(SegmentSummary {
        segment_id: r.segment_id.clone(),
        start_time,
        end_time,
        duration_secs: r.duration_secs,
        regime_id: r.regime_id.clone(),
        trigger_reason: TriggerReason::RegimeChange,
        event_count: 0,
        app_breakdown,
        category_breakdown: std::collections::HashMap::new(),
        context_switch_count: r.context_switch_count,
        dominant_category: r.dominant_category.clone(),
        avg_importance: 0.5,
        patterns_detected: vec![],
        content_activities,
        container: None,
        llm_summary: r.llm_summary.clone(),
    })
}
