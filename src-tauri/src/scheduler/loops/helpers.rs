use std::sync::Arc;
use tracing::{debug, info, warn};

use chrono::Utc;
use oneshim_api_contracts::stream::{FrameUpdate, IdleUpdate, RealtimeEvent};
use oneshim_core::models::activity::IdleState;
use oneshim_core::models::event::InputActivityEvent;
use oneshim_core::models::frame::{ImagePayload, OcrRegion};
use oneshim_core::models::storage_records::SegmentSummaryRecord;
use oneshim_core::models::tiered_memory::{ContentActivity, SegmentSummary, TriggerReason};
use oneshim_core::ports::frame_storage::FrameStoragePort;
use oneshim_core::ports::storage::StorageService;
use oneshim_core::ports::vision::{CaptureRequest, FrameProcessor};
use oneshim_monitor::idle::IdleTracker;
use oneshim_monitor::input_activity::InputActivityCollector;
use tokio::sync::broadcast;

use super::super::config::{base64_decode, SchedulerStorage};
use crate::magic_overlay::MagicOverlayHandle;
use crate::notification_manager::NotificationManager;

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
///
/// Returns `(ocr_text_hint, ocr_regions, raw_rgba)` where `raw_rgba` contains
/// the frame's RGBA bytes + dimensions for ML classification.
type FrameCaptureResult = (Option<String>, Vec<OcrRegion>, Option<(Vec<u8>, u32, u32)>);

#[tracing::instrument(skip_all)]
pub(super) async fn handle_frame_capture(
    capture_req: &CaptureRequest,
    processor: &Arc<dyn FrameProcessor>,
    frame_storage: &Option<Arc<dyn FrameStoragePort>>,
    sqlite: &Arc<dyn SchedulerStorage>,
    session_id: &str,
    pii_filter_level: oneshim_core::config::PiiFilterLevel,
    event_tx: &Option<broadcast::Sender<RealtimeEvent>>,
) -> FrameCaptureResult {
    match processor.capture_and_process(capture_req).await {
        Ok(frame) => {
            debug!("frame completed: {:?}", frame.metadata.trigger_type);

            // Grab OCR regions and raw RGBA before consuming other fields
            let ocr_regions = frame.ocr_regions.clone();
            let raw_rgba = frame.raw_rgba.map(|rgba| {
                let (w, h) = frame.metadata.resolution;
                (rgba, w, h)
            });

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

            // D5 iter-3: sanitize OCR text before SQLite persist per PII contract.
            // Raw OCR output from external provider may contain user PII (email
            // addresses, phone numbers, card numbers) visible in the captured
            // screenshot. Sanitize at the write boundary before frames.ocr_text
            // persists.
            let sanitized_ocr = ocr_text.as_deref().map(|raw| {
                oneshim_vision::privacy::sanitize_title_with_level(raw, pii_filter_level)
            });
            match sqlite.save_frame_metadata_with_bounds(
                &frame.metadata,
                file_path.as_deref(),
                sanitized_ocr.as_deref(),
                capture_req.window_bounds.as_ref(),
            ) {
                Ok(frame_id) => {
                    // Emit FrameUpdate after successful DB insert. Fields sourced from
                    // in-memory frame.metadata — no DB round-trip needed (spec §B).
                    if let Some(tx) = event_tx.as_ref() {
                        let update = FrameUpdate {
                            id: frame_id,
                            timestamp: frame.metadata.timestamp.to_rfc3339(),
                            app_name: frame.metadata.app_name.clone(),
                            window_title: frame.metadata.window_title.clone(),
                            importance: frame.metadata.importance,
                            trigger_type: frame.metadata.trigger_type.clone(),
                        };
                        if let Err(e) = tx.send(RealtimeEvent::Frame(update)) {
                            debug!("frame event channel send failed: {e}");
                        }
                    }
                }
                Err(e) => warn!("frame data save failure: {e}"),
            }

            if let Err(e) = sqlite.increment_session_counters(session_id, 0, 1, 0).await {
                debug!("increment_session_counters failed: {e}");
            }

            (ocr_text, ocr_regions, raw_rgba)
        }
        Err(e) => {
            warn!("frame failure: {e}");
            (None, Vec::new(), None)
        }
    }
}

// ── Idle state tracking ───────────────────────────────────────────────

/// Process idle state transitions: start/end idle periods in storage,
/// reset notifications on resume, and check idle notification thresholds.
/// Returns the updated `prev_idle_secs` value for the caller to persist.
pub(super) async fn handle_idle_tick(
    idle_tracker: &mut IdleTracker,
    sqlite: &Arc<dyn SchedulerStorage>,
    notif: &Option<Arc<NotificationManager>>,
    input_collector: &InputActivityCollector,
    prev_idle_secs: u64,
    focus_mode_active: bool,
    event_tx: &Option<broadcast::Sender<RealtimeEvent>>,
) -> u64 {
    let idle_info = idle_tracker.check_idle().await;
    let prev_state = idle_tracker.previous_state();

    if prev_state == IdleState::Active && idle_info.state == IdleState::Idle {
        // Storage FIRST (spec §U2 I2 ordering). Log-and-continue on failure.
        match sqlite.start_idle_period(Utc::now()).await {
            Ok(id) => {
                idle_tracker.set_idle_period_id(Some(id));
                debug!("idle period started: id={}", id);
            }
            Err(e) => warn!("idle period started record failure: {e}"),
        }
        // Emit AFTER storage (success or failure — subscribers observe the edge).
        if let Some(tx) = event_tx.as_ref() {
            let ev = RealtimeEvent::Idle(IdleUpdate {
                is_idle: true,
                idle_secs: idle_info.idle_secs,
            });
            if let Err(e) = tx.send(ev) {
                debug!("idle event channel send failed (Active→Idle): {e}");
            }
        }
    } else if prev_state == IdleState::Idle && idle_info.state == IdleState::Active {
        if let Some(id) = idle_tracker.idle_period_id() {
            if let Err(e) = sqlite.end_idle_period(id, Utc::now()).await {
                warn!("idle period ended record failure: {e}");
            }
            idle_tracker.set_idle_period_id(None);
        }
        if let Some(ref notif) = notif {
            notif.reset_session().await;
        }
        // Emit AFTER storage + notif-reset.
        if let Some(tx) = event_tx.as_ref() {
            let ev = RealtimeEvent::Idle(IdleUpdate {
                is_idle: false,
                idle_secs: idle_info.idle_secs,
            });
            if let Err(e) = tx.send(ev) {
                debug!("idle event channel send failed (Idle→Active): {e}");
            }
        }
    }

    // A4: Suppress idle notification in focus mode (UNCHANGED)
    if !focus_mode_active {
        if let Some(ref notif) = notif {
            notif.check_idle(idle_info.idle_secs).await;
        }
    }

    input_collector.estimate_from_idle_change(prev_idle_secs, idle_info.idle_secs);
    idle_info.idle_secs
}

// ── Heatmap & goal-progress overlay emission ─────────────────────────

/// Record click positions into the heatmap aggregator and emit a snapshot
/// to the overlay when available.  Also emits goal progress.
pub(super) async fn emit_heatmap_and_goals(
    adaptive_trigger_state: &mut Option<super::super::AdaptiveTriggerState>,
    input_snap: &InputActivityEvent,
    overlay_ref: &Option<MagicOverlayHandle>,
    coaching_engine_ref: &Option<Arc<oneshim_analysis::CoachingEngine>>,
) {
    // Heatmap aggregation
    if let Some(ref mut ts) = adaptive_trigger_state {
        if let Some((x, y)) = input_snap.mouse.last_position {
            ts.heatmap_aggregator
                .record(x, y, input_snap.mouse.click_count);
        }
        if let Some(grid) = ts.heatmap_aggregator.take_snapshot() {
            if let Some(ref overlay) = overlay_ref {
                overlay.emit_heatmap(grid);
            }
        }
    }

    // Goal progress emission
    if let Some(ref coaching) = coaching_engine_ref {
        if let Some(ref overlay) = overlay_ref {
            let goals = coaching.all_goal_progress().await;
            if !goals.is_empty() {
                overlay.update_goal_progress(goals);
            }
        }
    }
}

// ── Audit: consent & PII level change logging ────────────────────────

/// Log audit events when full_text_extraction consent or PII extraction
/// level changes between ticks. Returns updated `(prev_consent, prev_pii_level)`.
pub(super) fn audit_consent_and_pii_changes(
    full_text_consent: bool,
    prev_full_text_consent: bool,
    pii_level: oneshim_core::config::PiiFilterLevel,
    prev_pii_level: oneshim_core::config::PiiFilterLevel,
) -> (bool, oneshim_core::config::PiiFilterLevel) {
    let mut new_consent = prev_full_text_consent;
    let mut new_pii = prev_pii_level;

    if full_text_consent != prev_full_text_consent {
        if full_text_consent {
            info!(
                event = "full_text_extraction_consent_granted",
                "User granted full_text_extraction consent — Off PII level now effective"
            );
        } else {
            warn!(
                event = "full_text_extraction_consent_revoked",
                "User revoked full_text_extraction consent — falling back to Standard PII level"
            );
        }
        new_consent = full_text_consent;
    }

    if pii_level != prev_pii_level {
        info!(
            event = "pii_extraction_level_changed",
            old = ?prev_pii_level,
            new = ?pii_level,
            "PII extraction level changed"
        );
        new_pii = pii_level;
    }

    (new_consent, new_pii)
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

/// Interval between automatic frame retention enforcement runs (100 seconds).
pub(super) const FRAME_RETENTION_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(100);

/// Enforce frame retention and storage limits. Called periodically from the
/// monitor loop to prevent unbounded disk usage.
pub(super) async fn enforce_frame_retention(frame_storage: &dyn FrameStoragePort) {
    if let Err(e) = frame_storage.enforce_retention().await {
        warn!("frame retention enforcement failed: {e}");
    }
    if let Err(e) = frame_storage.enforce_storage_limit().await {
        warn!("frame storage limit enforcement failed: {e}");
    }
}
