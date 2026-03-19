use chrono::{Datelike, Duration as ChronoDuration, Timelike, Utc};
use oneshim_core::models::activity::{IdleState, ProcessSnapshot, ProcessSnapshotEntry};
use oneshim_core::models::event::{ContextEvent, Event, ProcessSnapshotEvent};
use oneshim_core::models::frame::{ImagePayload, OcrRegion};
use oneshim_monitor::idle::IdleTracker;
use oneshim_monitor::input_activity::InputActivityCollector;
use oneshim_monitor::window_layout::WindowLayoutTracker;
use oneshim_vision::ring_buffer::{CaptureRingBuffer, RingFrame};
use oneshim_web::{MetricsUpdate, RealtimeEvent};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

use oneshim_core::models::storage_records::SegmentSummaryRecord;
use oneshim_core::models::tiered_memory::{ContentActivity, SegmentSummary, TriggerReason};
use oneshim_core::ports::storage::StorageService;
use oneshim_core::ports::vision::{CaptureRequest, FrameProcessor};
use oneshim_storage::frame_storage::FrameFileStorage;

use super::config::{base64_decode, PlatformEgressPolicy, SchedulerStorage};
use super::Scheduler;

/// Run event-driven LLM analysis when the user switches to a new app.
/// Persists any resulting suggestions to storage.
async fn handle_event_analysis(
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
async fn handle_frame_capture(
    capture_req: &CaptureRequest,
    processor: &Arc<dyn FrameProcessor>,
    frame_storage: &Option<Arc<FrameFileStorage>>,
    sqlite: &Arc<dyn SchedulerStorage>,
    session_id: &str,
    window_bounds: Option<&oneshim_core::models::context::WindowBounds>,
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
                window_bounds,
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

impl Scheduler {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn spawn_monitor_loop(
        &self,
        poll: Duration,
        idle_threshold: u64,
        session_id: String,
        egress_policy: Arc<PlatformEgressPolicy>,
        input_collector: Arc<InputActivityCollector>,
        adaptive_trigger_state: Option<super::AdaptiveTriggerState>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let act_mon = self.activity_monitor.clone();
        let trigger = self.capture_trigger.clone();
        let processor = self.frame_processor.clone();
        let storage1 = self.storage.clone();
        let sqlite1 = self.sqlite_storage.clone();
        let frame_storage1 = self.frame_storage.clone();
        let uploader1 = self.batch_sink.clone();
        let egress1 = egress_policy;
        let session1 = session_id;
        let notif1 = self.notification_manager.clone();
        let focus1 = self.focus_analyzer.clone();
        let context_analyzer1 = self.context_analyzer.clone();
        let input_collector1 = input_collector;
        let accessibility_extractor1 = self.accessibility_extractor.clone();
        let config_manager1 = self.config_manager.clone();
        let consent_manager1 = self.consent_manager.clone();

        tokio::spawn(async move {
            let mut prev_app: Option<String> = None;
            let mut prev_idle_secs: u64 = 0;
            let mut interval = tokio::time::interval(poll);
            let mut idle_tracker = IdleTracker::new(Some(idle_threshold));
            let mut adaptive_trigger_state = adaptive_trigger_state;

            let window_tracker = WindowLayoutTracker::new();
            let input_collector = input_collector1;
            // Dashcam ring buffer: 6 slots (~18s at 3s poll), flush on importance >= 0.5,
            // capture 2 post-event frames after each flush.
            let ring_buffer = CaptureRingBuffer::new(6, 2, 0.5);

            // GUI Activity Intelligence state (carried across ticks)
            let mut last_gui_summary: Option<
                oneshim_core::models::gui_activity::GuiActivitySummary,
            > = None;
            // Focused element from accessibility API (Phase 2). Updated each
            // tick when accessibility extraction is enabled. Fed into the GUI
            // pipeline for supplementary context alongside OCR regions.
            let mut last_focused_element: Option<
                oneshim_core::models::focused_element::FocusedElementInfo,
            > = None;
            // OCR regions from the most recent frame capture. Updated each time
            // a high-importance frame is processed (importance >= 0.8). The GUI
            // pipeline uses these for click-to-element correlation via
            // `GuiElementDetector::correlate_click()`.
            let mut last_ocr_regions: Vec<OcrRegion> = Vec::new();

            // ── Audit tracking: consent and PII level changes (Task 7) ──
            let mut prev_full_text_consent = false;
            let mut prev_pii_level = config_manager1
                .as_ref()
                .map(|cm| cm.get().analysis.text_intelligence.pii_extraction_level)
                .unwrap_or_default();

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let idle_info = idle_tracker.check_idle().await;
                        let prev_state = idle_tracker.previous_state();

                        if prev_state == IdleState::Active && idle_info.state == IdleState::Idle {
                            match sqlite1.start_idle_period(Utc::now()).await {
                                Ok(id) => {
                                    idle_tracker.set_idle_period_id(Some(id));
                                    debug!("idle period started: id={}", id);
                                }
                                Err(e) => warn!("idle period started record failure: {e}"),
                            }
                        } else if prev_state == IdleState::Idle && idle_info.state == IdleState::Active {
                            if let Some(id) = idle_tracker.idle_period_id() {
                                if let Err(e) = sqlite1.end_idle_period(id, Utc::now()).await {
                                    warn!("idle period ended record failure: {e}");
                                }
                                idle_tracker.set_idle_period_id(None);
                            }
                            if let Some(ref notif) = notif1 {
                                notif.reset_session().await;
                            }
                        }

                        if let Some(ref notif) = notif1 {
                            notif.check_idle(idle_info.idle_secs).await;
                        }

                        input_collector.estimate_from_idle_change(prev_idle_secs, idle_info.idle_secs);
                        prev_idle_secs = idle_info.idle_secs;

                        match act_mon.collect_context().await {
                            Ok(ctx) => {
                                let app_name = ctx.active_window.as_ref()
                                    .map(|w| w.app_name.clone())
                                    .unwrap_or_default();
                                let window_title = ctx.active_window.as_ref()
                                    .map(|w| w.title.clone())
                                    .unwrap_or_default();
                                let focus_window_title = window_title.clone();
                                let window_bounds = ctx.active_window.as_ref()
                                    .and_then(|w| w.bounds);
                                let mut focus_ocr_hint: Option<String> = None;

                                input_collector.set_current_app(&app_name);

                                // ── Accessibility API extraction (Phase 2) ──
                                // Extract focused element info per tick when enabled.
                                // Result is stored for the GUI pipeline to consume.
                                if let Some(ref ax) = accessibility_extractor1 {
                                    let text_config = config_manager1
                                        .as_ref()
                                        .map(|cm| cm.get().analysis.text_intelligence.clone())
                                        .unwrap_or_default();
                                    let full_text_consent = consent_manager1
                                        .as_ref()
                                        .map(|cm| cm.is_permitted(|p| p.full_text_extraction))
                                        .unwrap_or(false);

                                    // Audit: log consent state changes
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
                                        prev_full_text_consent = full_text_consent;
                                    }

                                    // Audit: log PII extraction level config changes
                                    if text_config.pii_extraction_level != prev_pii_level {
                                        info!(
                                            event = "pii_extraction_level_changed",
                                            old = ?prev_pii_level,
                                            new = ?text_config.pii_extraction_level,
                                            "PII extraction level changed"
                                        );
                                        prev_pii_level = text_config.pii_extraction_level;
                                    }

                                    match ax
                                        .extract_focused_element(
                                            text_config.pii_extraction_level,
                                            full_text_consent,
                                        )
                                        .await
                                    {
                                        Ok(info) => {
                                            last_focused_element = info;
                                        }
                                        Err(e) => {
                                            debug!("accessibility extraction failed: {e}");
                                            last_focused_element = None;
                                        }
                                    }
                                }

                                if let Some(layout_event) = window_tracker.update(&app_name, &window_title, window_bounds) {
                                    // Update GUI detector resolution from the latest layout event
                                    let (res_w, res_h) = layout_event.screen_resolution;
                                    if let Some(ref mut ts) = adaptive_trigger_state {
                                        if let Some(ref mut gui_state) = ts.gui_pipeline_state {
                                            gui_state.detector.update_resolution(res_w, res_h);
                                        }
                                    }

                                    let win_event = Event::Window(layout_event);
                                    if let Err(e) = storage1.save_event(&win_event).await {
                                        warn!("window event save failure: {e}");
                                    }
                                    if let Some(ref sink) = uploader1 {
                                        if let Some(upload_event) = egress1.prepare_event_for_upload(win_event) {
                                            sink.enqueue(upload_event);
                                        }
                                    }
                                }

                                let event = ContextEvent {
                                    app_name: app_name.clone(),
                                    window_title,
                                    prev_app_name: prev_app.clone(),
                                    timestamp: Utc::now(),
                                    input_activity_level: input_collector.peek_activity_level(),
                                };

                                // --- Ring buffer: capture thumbnail every cycle ---
                                if let Ok(thumb_data) = processor.capture_thumbnail().await {
                                    ring_buffer.push(RingFrame {
                                        timestamp: Utc::now(),
                                        thumbnail_data: thumb_data,
                                        app_name: app_name.clone(),
                                        window_title: event.window_title.clone(),
                                    });
                                }

                                {
                                    let capture_req = trigger.should_capture(&event);

                                    // Force capture during post-event window (dashcam "after" frames)
                                    let force_post = ring_buffer.should_force_post_capture();

                                    if let Some(capture_req) = capture_req {
                                        // --- Ring buffer: flush pre-event frames on significant capture ---
                                        if let Some(ref fs) = frame_storage1 {
                                            let flush_frame = RingFrame {
                                                timestamp: Utc::now(),
                                                thumbnail_data: vec![],
                                                app_name: capture_req.app_name.clone(),
                                                window_title: capture_req.window_title.clone(),
                                            };
                                            if let Some(flush) = ring_buffer.check_and_flush(capture_req.importance, flush_frame) {
                                                let batch: Vec<_> = flush.pre_event_frames
                                                    .into_iter()
                                                    .filter(|f| !f.thumbnail_data.is_empty())
                                                    .map(|f| (f.timestamp, f.thumbnail_data))
                                                    .collect();
                                                if !batch.is_empty() {
                                                    debug!("ring buffer: saving {} pre-event frames", batch.len());
                                                    let _ = fs.save_frames_batch(batch).await;
                                                }
                                            }
                                        }

                                        let (ocr_hint, regions) = handle_frame_capture(
                                            &capture_req,
                                            &processor,
                                            &frame_storage1,
                                            &sqlite1,
                                            &session1,
                                            window_bounds.as_ref(),
                                        ).await;
                                        focus_ocr_hint = ocr_hint;
                                        if !regions.is_empty() {
                                            last_ocr_regions = regions;
                                        }
                                    } else if force_post {
                                        // Post-event forced capture (dashcam "after" frames)
                                        if let Some(ref fs) = frame_storage1 {
                                            if let Ok(thumb_data) = processor.capture_thumbnail().await {
                                                debug!("ring buffer: post-event forced capture");
                                                let _ = fs.save_frame(Utc::now(), &thumb_data).await;
                                            }
                                        }
                                    }
                                }

                                let ctx_event = Event::Context(event);
                                if let Err(e) = storage1.save_event(&ctx_event).await {
                                    warn!("event save failure: {e}");
                                }

                                let _ = sqlite1.increment_session_counters(&session1, 1, 0, 0).await;

                                if let Some(ref sink) = uploader1 {
                                    if let Some(upload_event) = egress1.prepare_event_for_upload(ctx_event) {
                                        sink.enqueue(upload_event);
                                    }
                                }

                                let app_changed = prev_app.as_ref() != Some(&app_name);
                                if app_changed {
                                    if let Some(ref focus) = focus1 {
                                        focus
                                            .on_app_switch_with_context(
                                                &app_name,
                                                &focus_window_title,
                                                focus_ocr_hint.as_deref(),
                                            )
                                            .await;
                                    }

                                    // Event-driven LLM analysis on significant app switches
                                    handle_event_analysis(
                                        &context_analyzer1,
                                        &storage1,
                                        &app_name,
                                        &focus_window_title,
                                        focus_ocr_hint.as_deref(),
                                    ).await;
                                }

                                // ── Take input snapshot once for both pipelines ──
                                let input_snap = input_collector.take_snapshot();

                                // ── Adaptive tiered-memory pipeline ──
                                // Feed GUI summary from the previous cycle (N-1) into
                                // the current analysis tick (N).
                                if let Some(ref mut ts) = adaptive_trigger_state {
                                    super::analysis_pipeline::run_analysis_tick(
                                        ts,
                                        &app_name,
                                        &focus_window_title,
                                        &prev_app,
                                        app_changed,
                                        &input_snap,
                                        last_gui_summary.as_ref(),
                                        last_focused_element.as_ref(),
                                        &storage1,
                                    ).await;
                                }
                                // Consume the GUI summary after feeding it to the analysis pipeline
                                last_gui_summary = None;

                                // ── GUI Activity Intelligence pipeline ──
                                if let Some(ref mut ts) = adaptive_trigger_state {
                                    if let Some(ref mut gui_state) = ts.gui_pipeline_state {
                                        let parsed_content_label = ts
                                            .title_bar_parser
                                            .parse(&app_name, &focus_window_title)
                                            .map(|c| c.content_label)
                                            .unwrap_or_default();

                                        let recent_shortcuts = input_collector.take_recent_shortcuts();

                                        let gui_summary = super::gui_pipeline::run_gui_tick(
                                            gui_state,
                                            &last_ocr_regions,
                                            &input_snap,
                                            &recent_shortcuts,
                                            &app_name,
                                            &focus_window_title,
                                            &parsed_content_label,
                                            last_focused_element.as_ref(),
                                        );

                                        if gui_summary.is_some() {
                                            last_gui_summary = gui_summary;
                                        }

                                        // Persist GUI interaction to SQLite (V13 table)
                                        if input_snap.mouse.click_count > 0 {
                                            let event_id = uuid::Uuid::new_v4().to_string();
                                            let timestamp_str = chrono::Utc::now().to_rfc3339();

                                            let input = oneshim_core::models::storage_records::NewGuiInteraction {
                                                event_id: &event_id,
                                                segment_id: None,
                                                timestamp: &timestamp_str,
                                                element_text: None,
                                                element_type: Some("Click"),
                                                interaction_type: "Click",
                                                bbox_json: None,
                                                app_name: &app_name,
                                            };

                                            if let Err(e) = sqlite1.save_gui_interaction(&input) {
                                                warn!("GUI interaction save failure: {e}");
                                            }
                                        }
                                    }
                                }

                                prev_app = Some(app_name);
                            }
                            Err(e) => {
                                warn!("context collect failure: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("monitoring ended");
                        break;
                    }
                }
            }
        })
    }

    pub(super) fn spawn_metrics_loop(
        &self,
        metrics_interval: Duration,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let sys_mon = self.system_monitor.clone();
        let sqlite2 = self.sqlite_storage.clone();
        let event_tx2 = self.event_tx.clone();
        let notif2 = self.notification_manager.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(metrics_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match sys_mon.collect_metrics().await {
                            Ok(metrics) => {
                                if let Err(e) = sqlite2.save_metrics(&metrics).await {
                                    warn!("system save failure: {e}");
                                }

                                let memory_percent = if metrics.memory_total > 0 {
                                    (metrics.memory_used as f32 / metrics.memory_total as f32) * 100.0
                                } else {
                                    0.0
                                };

                                if let Some(ref tx) = event_tx2 {
                                    let update = MetricsUpdate {
                                        timestamp: metrics.timestamp.to_rfc3339(),
                                        cpu_usage: metrics.cpu_usage,
                                        memory_percent,
                                        memory_used: metrics.memory_used,
                                        memory_total: metrics.memory_total,
                                    };
                                    let _ = tx.send(RealtimeEvent::Metrics(update));
                                }

                                if let Some(ref notif) = notif2 {
                                    notif.check_high_usage(metrics.cpu_usage, memory_percent).await;
                                }
                            }
                            Err(e) => {
                                warn!("system collect failure: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("ended");
                        break;
                    }
                }
            }
        })
    }

    pub(super) fn spawn_process_loop(
        &self,
        process_interval: Duration,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let proc_mon = self.process_monitor.clone();
        let sqlite3 = self.sqlite_storage.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(process_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match proc_mon.get_top_processes(10).await {
                            Ok(processes) => {
                                let snapshot = ProcessSnapshot {
                                    timestamp: Utc::now(),
                                    processes: processes.into_iter().map(|p| ProcessSnapshotEntry {
                                        pid: p.pid,
                                        name: p.name,
                                        cpu_usage: p.cpu_usage,
                                        memory_bytes: p.memory_bytes,
                                    }).collect(),
                                };
                                if let Err(e) = sqlite3.save_process_snapshot(&snapshot).await {
                                    warn!("save failure: {e}");
                                }
                            }
                            Err(e) => {
                                warn!("list collect failure: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("ended");
                        break;
                    }
                }
            }
        })
    }

    pub(super) fn spawn_sync_loop(
        &self,
        sync_interval: Duration,
        egress_policy: Arc<PlatformEgressPolicy>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let uploader4 = self.batch_sink.clone();
        let storage4 = self.storage.clone();
        let frame_storage4 = self.frame_storage.clone();
        let egress4 = egress_policy;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(sync_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Some(ref sink) = uploader4 {
                            if egress4.is_enabled() {
                                match sink.flush().await {
                                    Ok(count) => {
                                        if count > 0 {
                                            debug!("batch: {count}items sent");
                                            if let Err(e) = storage4.mark_unsent_as_sent_before(Utc::now()).await {
                                                warn!("mark sent failure: {e}");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("batch failure: {e}");
                                    }
                                }
                            }
                        }

                        if let Err(e) = storage4.enforce_retention().await {
                            warn!("event policy failure: {e}");
                        }

                        if let Some(ref fs) = frame_storage4 {
                            if let Err(e) = fs.enforce_retention().await {
                                warn!("frame policy failure: {e}");
                            }
                            if let Err(e) = fs.enforce_storage_limit().await {
                                warn!("frame failure: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("ended");
                        break;
                    }
                }
            }
        })
    }

    pub(super) fn spawn_heartbeat_loop(
        &self,
        heartbeat_interval: Duration,
        session_id: String,
        egress_policy: Arc<PlatformEgressPolicy>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let api = self.api_client.clone();
        let sid = session_id;

        tokio::spawn(async move {
            let api = match api {
                Some(a) => a,
                None => {
                    let _ = shutdown_rx.changed().await;
                    return;
                }
            };

            if !egress_policy.is_enabled() {
                let _ = shutdown_rx.changed().await;
                return;
            }

            let mut interval = tokio::time::interval(heartbeat_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = api.send_heartbeat(&sid).await {
                            warn!("heartbeat failure: {e}");
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("heartbeat ended");
                        break;
                    }
                }
            }
        })
    }

    pub(super) fn spawn_aggregation_loop(
        &self,
        aggregation_interval: Duration,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let sqlite6 = self.sqlite_storage.clone();
        let vector_store = self.vector_store.clone();
        let embedding_provider = self.embedding_provider.clone();
        let config_manager = self.config_manager.clone();
        let vector_index = self.vector_index.clone();
        let search_coordinator = self.search_coordinator.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(aggregation_interval);
            let mut last_reindex_check: Option<chrono::DateTime<Utc>> = None;
            let mut last_index_maintenance: Option<chrono::DateTime<Utc>> = None;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let now = Utc::now();

                        let prev_hour = now - ChronoDuration::hours(1);
                        if let Err(e) = sqlite6.aggregate_hourly_metrics(prev_hour).await {
                            warn!("hour failure: {e}");
                        }

                        let metrics_cutoff = now - ChronoDuration::hours(super::config::RAW_METRICS_RETENTION_HOURS);
                        if let Err(e) = sqlite6.cleanup_old_metrics(metrics_cutoff).await {
                            warn!("delete failure: {e}");
                        }

                        let process_cutoff = now - ChronoDuration::days(super::config::PROCESS_SNAPSHOT_RETENTION_DAYS);
                        if let Err(e) = sqlite6.cleanup_old_process_snapshots(process_cutoff).await {
                            warn!("delete failure: {e}");
                        }

                        let idle_cutoff = now - ChronoDuration::days(super::config::IDLE_PERIOD_RETENTION_DAYS);
                        if let Err(e) = sqlite6.cleanup_old_idle_periods(idle_cutoff).await {
                            warn!("idle period delete failure: {e}");
                        }

                        // --- Embedding re-indexing on model version change (daily) ---
                        if let (Some(ref vs), Some(ref ep)) = (&vector_store, &embedding_provider) {
                            let should_check = last_reindex_check
                                .map(|last| (now - last).num_hours() >= 24)
                                .unwrap_or(true);

                            if should_check {
                                last_reindex_check = Some(now);

                                let config_model = config_manager
                                    .as_ref()
                                    .map(|cm| cm.get().analysis.embedding.local_model.clone())
                                    .unwrap_or_default();

                                match vs.get_current_model_id().await {
                                    Ok(Some(stored_model)) if !config_model.is_empty() && stored_model != config_model => {
                                        info!(
                                            old_model = %stored_model,
                                            new_model = %config_model,
                                            "Embedding model changed — marking old vectors stale"
                                        );
                                        if let Err(e) = vs.mark_stale(&stored_model).await {
                                            warn!("mark stale failure: {e}");
                                        }
                                    }
                                    _ => {}
                                }

                                // Process stale vectors in batches of 100
                                loop {
                                    match vs.get_stale_vectors(100).await {
                                        Ok(batch) if !batch.is_empty() => {
                                            let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
                                            match ep.embed_batch(&texts).await {
                                                Ok(vectors) => {
                                                    let model_id = ep.model_id();
                                                    let mut updated = 0u64;
                                                    for ((id, _), vec) in batch.into_iter().zip(vectors) {
                                                        if let Err(e) = vs.update_vector(id, vec, model_id).await {
                                                            warn!("re-embed update failure: {e}");
                                                        } else {
                                                            updated += 1;
                                                        }
                                                    }
                                                    debug!("re-embedded {updated} stale vectors");
                                                }
                                                Err(e) => {
                                                    warn!("re-embed batch failure: {e}");
                                                    break;
                                                }
                                            }
                                        }
                                        Ok(_) => break, // no more stale vectors
                                        Err(e) => {
                                            warn!("get stale vectors failure: {e}");
                                            break;
                                        }
                                    }
                                }

                                // Enforce vector retention
                                let retention_days = config_manager
                                    .as_ref()
                                    .map(|cm| cm.get().analysis.embedding.retention_days)
                                    .unwrap_or(90);
                                if let Err(e) = vs.enforce_retention(retention_days).await {
                                    warn!("vector retention failure: {e}");
                                }
                            }
                        }

                        // --- Activity segment retention (default: 90 days, same as embedding) ---
                        {
                            let segment_retention_days = config_manager
                                .as_ref()
                                .map(|cm| cm.get().analysis.embedding.retention_days)
                                .unwrap_or(90);
                            if let Err(e) = sqlite6.enforce_segment_retention(segment_retention_days) {
                                warn!("segment retention failure: {e}");
                            }

                            // Weekly digests retention (keep 52 weeks = 1 year)
                            if let Err(e) = sqlite6.enforce_digest_retention(52) {
                                warn!("digest retention failure: {e}");
                            }
                        }

                        // --- Weekly digest auto-generation ---
                        {
                            let digest_day = config_manager
                                .as_ref()
                                .map(|cm| cm.get().analysis.embedding.digest_day)
                                .unwrap_or(oneshim_core::config::Weekday::Sun);

                            let local_now = chrono::Local::now();
                            let is_digest_day =
                                local_now.weekday().num_days_from_sunday() == digest_day.num_days_from_sunday();
                            let is_midnight_hour = local_now.hour() == 0;

                            if is_digest_day && is_midnight_hour {
                                // Calculate week boundaries (Monday-based ISO week aligned to digest_day)
                                let week_end = now;
                                let week_start = now - ChronoDuration::days(7);

                                // Check if digest already exists for this week
                                let existing = sqlite6
                                    .list_weekly_digests(1)
                                    .ok()
                                    .and_then(|d| d.into_iter().next());

                                let already_generated = existing
                                    .as_ref()
                                    .map(|d| (now - d.week_end).num_hours() < 24)
                                    .unwrap_or(false);

                                if !already_generated {
                                    // Load actual segments for this week from storage
                                    let week_segments = sqlite6
                                        .list_segments_between(week_start, week_end)
                                        .unwrap_or_default();
                                    let digest = oneshim_analysis::WeeklyDigestGenerator::generate(
                                        &week_segments,
                                        week_start,
                                        week_end,
                                        existing.as_ref(),
                                    );

                                    if let Err(e) = sqlite6.save_weekly_digest(&digest) {
                                        warn!("weekly digest save failure: {e}");
                                    } else {
                                        info!("Weekly digest generated for week ending {}", week_end);
                                    }
                                }
                            }
                        }

                        // --- Daily digest auto-generation (midnight) ---
                        {
                            let local_now = chrono::Local::now();
                            if local_now.hour() == 0 {
                                // Generate digest for yesterday
                                let yesterday = local_now.date_naive()
                                    .pred_opt()
                                    .unwrap_or(local_now.date_naive());
                                let date_str = yesterday.format("%Y-%m-%d").to_string();

                                // Check if daily digest already exists
                                let existing = sqlite6
                                    .get_daily_digest(&date_str)
                                    .ok()
                                    .flatten();

                                if existing.is_none() {
                                    // Load segments for yesterday
                                    let segment_records = sqlite6
                                        .get_segments_for_date(&date_str)
                                        .unwrap_or_default();

                                    if !segment_records.is_empty() {
                                        // Convert SegmentSummaryRecords to SegmentSummary for DailyDigestGenerator
                                        let segments: Vec<oneshim_core::models::tiered_memory::SegmentSummary> =
                                            segment_records
                                                .iter()
                                                .filter_map(record_to_segment_summary)
                                                .collect();

                                        // Load previous day for comparison
                                        let prev_date = yesterday
                                            .pred_opt()
                                            .unwrap_or(yesterday)
                                            .format("%Y-%m-%d")
                                            .to_string();
                                        let prev_digest = sqlite6
                                            .get_daily_digest(&prev_date)
                                            .ok()
                                            .flatten();

                                        let digest = oneshim_analysis::DailyDigestGenerator::generate(
                                            &segments,
                                            yesterday,
                                            prev_digest.as_ref(),
                                        );

                                        if let Err(e) = sqlite6.save_daily_digest(&digest) {
                                            warn!("daily digest save failure: {e}");
                                        } else {
                                            info!("Daily digest generated for {}", date_str);
                                        }
                                    }
                                }
                            }
                        }

                        // --- Vector index maintenance (every 5 minutes) ---
                        if let Some(ref vi) = vector_index {
                            let should_run = last_index_maintenance
                                .map(|last| (now - last).num_minutes() >= 5)
                                .unwrap_or(true);

                            if should_run {
                                last_index_maintenance = Some(now);

                                // Refresh cached vector count in the search coordinator
                                if let Some(ref coord) = search_coordinator {
                                    if let Err(e) = coord.refresh_count().await {
                                        warn!("search coordinator refresh_count failure: {e}");
                                    }
                                }

                                let embedding_config = config_manager
                                    .as_ref()
                                    .map(|cm| cm.get().analysis.embedding.clone())
                                    .unwrap_or_default();

                                if embedding_config.index_strategy != "brute_force" {
                                    match vi.get_index_meta().await {
                                        Ok(meta) => {
                                            let total = meta.total_vector_count;
                                            if total >= 10_000 {
                                                let needs_rebuild = meta.ivf_built_at.is_none()
                                                    || (meta.unindexed_count as f64 / total.max(1) as f64 > 0.10);

                                                if needs_rebuild {
                                                    let n_clusters = (total as f64).sqrt() as usize;
                                                    info!(
                                                        "Rebuilding IVF index: {} vectors, {} clusters",
                                                        total, n_clusters
                                                    );
                                                    if let Err(e) = vi.build_ivf_index(n_clusters, 10).await {
                                                        warn!("IVF index build failure: {e}");
                                                    }

                                                    if total > 100_000 {
                                                        info!("Building binary codes for {} vectors", total);
                                                        if let Err(e) = vi.build_binary_codes().await {
                                                            warn!("Binary code build failure: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            warn!("get_index_meta failure: {e}");
                                        }
                                    }
                                }
                            }
                        }

                        debug!("completed");
                    }
                    _ = shutdown_rx.changed() => {
                        info!("ended");
                        break;
                    }
                }
            }
        })
    }

    pub(super) fn spawn_notification_loop(
        &self,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let notif7 = self.notification_manager.clone();

        tokio::spawn(async move {
            let notif = match notif7 {
                Some(n) => n,
                None => {
                    let _ = shutdown_rx.changed().await;
                    return;
                }
            };

            let mut interval = tokio::time::interval(Duration::from_secs(60)); // 1min
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        notif.check_long_session().await;
                    }
                    _ = shutdown_rx.changed() => {
                        info!("notification ended");
                        break;
                    }
                }
            }
        })
    }

    pub(super) fn spawn_focus_loop(
        &self,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let focus8 = self.focus_analyzer.clone();

        tokio::spawn(async move {
            let focus = match focus8 {
                Some(f) => f,
                None => {
                    let _ = shutdown_rx.changed().await;
                    return;
                }
            };

            let mut interval = tokio::time::interval(Duration::from_secs(60)); // 1min
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        focus.analyze_periodic().await;
                    }
                    _ = shutdown_rx.changed() => {
                        info!("in progress min ended");
                        break;
                    }
                }
            }
        })
    }

    pub(super) fn spawn_event_snapshot_loop(
        &self,
        detailed_process_interval: Duration,
        input_activity_interval: Duration,
        egress_policy: Arc<PlatformEgressPolicy>,
        input_collector: Arc<InputActivityCollector>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let proc_mon9 = self.process_monitor.clone();
        let storage9 = self.storage.clone();
        let uploader9 = self.batch_sink.clone();
        let input_collector9 = input_collector;
        let egress9 = egress_policy;

        tokio::spawn(async move {
            let mut process_interval = tokio::time::interval(detailed_process_interval);
            let mut input_interval = tokio::time::interval(input_activity_interval);
            let mut foreground_pid: Option<u32> = None;

            loop {
                tokio::select! {
                    _ = process_interval.tick() => {
                        match proc_mon9.get_detailed_processes(foreground_pid, 10).await {
                            Ok(processes) => {
                                let total = processes.len() as u32;

                                foreground_pid = processes.iter()
                                    .find(|p| p.is_foreground)
                                    .map(|p| p.pid);

                                let snapshot_event = ProcessSnapshotEvent {
                                    timestamp: Utc::now(),
                                    processes,
                                    total_process_count: total,
                                };

                                let event = Event::Process(snapshot_event);
                                if let Err(e) = storage9.save_event(&event).await {
                                    warn!("event save failure: {e}");
                                }

                                if let Some(ref sink) = uploader9 {
                                    if let Some(upload_event) = egress9.prepare_event_for_upload(event) {
                                        sink.enqueue(upload_event);
                                    }
                                }

                                debug!(": {}items", total);
                            }
                            Err(e) => {
                                warn!("collect failure: {e}");
                            }
                        }
                    }
                    _ = input_interval.tick() => {
                        let input_event = input_collector9.take_snapshot();

                        if input_event.mouse.click_count > 0
                            || input_event.keyboard.total_keystrokes > 0
                            || input_event.mouse.scroll_count > 0
                        {
                            let event = Event::Input(input_event);
                            if let Err(e) = storage9.save_event(&event).await {
                                warn!("event save failure: {e}");
                            }

                            if let Some(ref sink) = uploader9 {
                                if let Some(upload_event) = egress9.prepare_event_for_upload(event) {
                                    sink.enqueue(upload_event);
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("server event collect ended");
                        break;
                    }
                }
            }
        })
    }

    /// Periodic LLM analysis loop — runs `analyze_if_changed()` on each tick
    /// and forces a full `analyze()` every `full_interval_secs`.
    /// Generated suggestions are persisted to SQLite for the web dashboard.
    pub(super) fn spawn_analysis_loop(
        &self,
        config: oneshim_core::config::AnalysisConfig,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let analyzer = self.context_analyzer.clone();
        let storage_ref = self.storage.clone();
        let sqlite_ref = self.sqlite_storage.clone();
        let config_manager = self.config_manager.clone();

        tokio::spawn(async move {
            let analyzer = match analyzer {
                Some(a) => a,
                None => {
                    let _ = shutdown_rx.changed().await;
                    return;
                }
            };

            // Use initial config for interval timing (changes require restart).
            // Other settings (enabled, min_confidence, max_suggestions, throttle_secs)
            // are read dynamically from ConfigManager on each tick so that
            // changes via the Tauri `update_analysis_config` command propagate
            // immediately without an agent restart.
            let mut interval = tokio::time::interval(Duration::from_secs(config.interval_secs));
            let full_interval = Duration::from_secs(config.full_interval_secs);
            let mut last_full = std::time::Instant::now();

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Read current config from ConfigManager (the single source
                        // of truth also written to by update_analysis_config).
                        let current_config = config_manager
                            .as_ref()
                            .map(|cm| cm.get().analysis)
                            .unwrap_or_else(|| config.clone());

                        if !current_config.enabled {
                            debug!("analysis loop: disabled via runtime config, skipping tick");
                            continue;
                        }

                        // Server coexistence: skip local LLM analysis when
                        // the server has recently sent suggestions via SSE.
                        match sqlite_ref.has_recent_server_suggestions(
                            current_config.server_coexistence_lookback_secs,
                        ) {
                            Ok(true) => {
                                debug!(
                                    "server suggestions active (last {}s) — skipping local analysis",
                                    current_config.server_coexistence_lookback_secs,
                                );
                                continue;
                            }
                            Ok(false) => { /* proceed with local analysis */ }
                            Err(e) => {
                                warn!("server coexistence check failed: {e}");
                                // Proceed anyway — fail-open
                            }
                        }

                        let force_full = last_full.elapsed() >= full_interval;

                        let result = if force_full {
                            last_full = std::time::Instant::now();
                            analyzer.analyze().await
                        } else {
                            analyzer.analyze_if_changed().await
                        };

                        match result {
                            Ok(suggestions) => {
                                if !suggestions.is_empty() {
                                    info!(
                                        count = suggestions.len(),
                                        "LLM analysis produced suggestions"
                                    );
                                }
                                for suggestion in &suggestions {
                                    info!(
                                        id = %suggestion.suggestion_id,
                                        priority = ?suggestion.priority,
                                        "suggestion: {}",
                                        suggestion.content
                                    );
                                    if let Err(e) = storage_ref.save_suggestion(suggestion).await {
                                        warn!("suggestion save failure: {e}");
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("analysis failure: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("analysis loop ended");
                        break;
                    }
                }
            }
        })
    }

    /// Periodically check and refresh OAuth tokens.
    #[cfg(feature = "server")]
    pub(super) fn spawn_oauth_refresh_loop(
        &self,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
        app_handle: Option<tauri::AppHandle>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        use super::config::OAUTH_REFRESH_INTERVAL_SECS;
        use oneshim_core::ports::oauth::TokenEvent;
        use std::time::Duration;
        use tauri::Emitter;

        let coordinator = self.oauth_coordinator.as_ref()?.clone();

        Some(tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(OAUTH_REFRESH_INTERVAL_SECS));
            let mut event_rx = coordinator.subscribe();
            let mut last_reauth_notify: Option<tokio::time::Instant> = None;
            let provider_id = "openai".to_string();

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let outcome = coordinator.check_and_refresh(&provider_id).await;
                        debug!(provider_id = %provider_id, ?outcome, "OAuth refresh tick");
                    }
                    event = event_rx.recv() => {
                        if let Ok(TokenEvent::ReauthRequired { ref provider_id }) = event {
                            let should_notify = last_reauth_notify
                                .map_or(true, |t| t.elapsed() > Duration::from_secs(300));
                            if should_notify {
                                warn!(
                                    provider_id = %provider_id,
                                    "OAuth re-authentication required — user must reconnect"
                                );
                                last_reauth_notify = Some(tokio::time::Instant::now());

                                // Emit Tauri event for frontend toast
                                if let Some(ref handle) = app_handle {
                                    let payload = serde_json::json!({
                                        "provider_id": provider_id,
                                    });
                                    if let Err(e) = handle.emit("oauth-reauth-required", &payload) {
                                        warn!("Failed to emit oauth-reauth-required event: {e}");
                                    }

                                    // Native OS notification for background/minimized state.
                                    // Body is English-only: i18n is frontend-side; Rust has no locale context.
                                    if let Err(e) = tauri_plugin_notification::NotificationExt::notification(handle)
                                        .builder()
                                        .title("ONESHIM")
                                        .body("OAuth re-authentication required — please reconnect in Settings")
                                        .show()
                                    {
                                        warn!("Failed to show native notification: {e}");
                                    }
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        debug!("OAuth refresh loop shutting down");
                        break;
                    }
                }
            }
        }))
    }

    /// 12. Cross-device sync loop (P3 Phase 3a-2).
    ///
    /// Runs the SyncEngine's pull/merge/push cycle at the configured interval.
    pub(super) fn spawn_cross_device_sync_loop(
        &self,
        sync_interval: Duration,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let sync_engine = self.sync_engine.clone();

        tokio::spawn(async move {
            let engine = match sync_engine {
                Some(e) => e,
                None => {
                    let _ = shutdown_rx.changed().await;
                    return;
                }
            };

            // Startup delay: wait 10 seconds before first sync
            tokio::time::sleep(Duration::from_secs(10)).await;

            let mut interval = tokio::time::interval(sync_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match engine.run_cycle().await {
                            Ok(Some(result)) => {
                                info!(
                                    applied = result.applied,
                                    skipped = result.skipped_lww + result.skipped_dup,
                                    "cross-device sync cycle completed"
                                );
                            }
                            Ok(None) => {
                                debug!("cross-device sync cycle: no changes or skipped");
                            }
                            Err(e) => {
                                warn!("cross-device sync cycle failed: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        // Push pending changes before shutdown
                        if let Err(e) = engine.run_cycle().await {
                            warn!("shutdown sync push failed: {e}");
                        }
                        info!("cross-device sync loop ended");
                        break;
                    }
                }
            }
        })
    }

    #[allow(unused_variables)]
    pub(super) async fn run_scheduler_loops(
        &self,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
        app_handle: Option<tauri::AppHandle>,
    ) {
        let poll = self.config.poll_interval;
        let metrics_interval = self.config.metrics_interval;
        let process_interval = self.config.process_interval;
        let detailed_process_interval = self.config.detailed_process_interval;
        let input_activity_interval = self.config.input_activity_interval;
        let sync = self.config.sync_interval;
        let heartbeat = self.config.heartbeat_interval;
        let aggregation = self.config.aggregation_interval;
        let session_id = self.config.session_id.clone();
        let idle_threshold = self.config.idle_threshold_secs;
        let egress_policy = Arc::new(PlatformEgressPolicy::new(&self.config));

        info!(
            platform_sync_enabled = egress_policy.is_enabled(),
            "플랫폼 egress policy 적용"
        );

        self.initialize_session(&session_id).await;

        let shared_input_collector = Arc::new(InputActivityCollector::new());

        // -- Phase 1.5: Platform key-category hook --
        // Spawns a passive OS keyboard observer that classifies key events
        // into KeyCategory and feeds them into InputActivityCollector.
        // Gated by text_intelligence.input_pattern_detail config flag.
        let _key_hook = {
            let text_intel_config = self
                .config_manager
                .as_ref()
                .map(|cm| cm.get().analysis.text_intelligence.clone())
                .unwrap_or_default();

            if text_intel_config.enabled && text_intel_config.input_pattern_detail {
                oneshim_monitor::key_hook::KeyHook::start(shared_input_collector.clone())
            } else {
                debug!(
                    "key-category hook disabled \
                     (text_intelligence.input_pattern_detail = false \
                     or text_intelligence.enabled = false)"
                );
                None
            }
        };

        // Take adaptive trigger state out of Mutex — it is consumed by the
        // monitor loop and cannot be shared.
        let mut adaptive_trigger_state = self
            .adaptive_trigger
            .lock()
            .expect("adaptive trigger lock")
            .take();

        // Construct GUI pipeline state if enabled + consented
        if let Some(ref mut ts) = adaptive_trigger_state {
            let gui_config = self
                .config_manager
                .as_ref()
                .map(|cm| cm.get().analysis.gui_intelligence.clone())
                .unwrap_or_default();

            // Consent is implicitly satisfied: AdaptiveTriggerState is only
            // constructed when the activity_pattern_learning consent has been
            // granted (agent_runtime.rs gates on that permission). The only
            // remaining gate is the gui_intelligence.enabled config flag.
            if gui_config.enabled {
                use oneshim_analysis::gui_aggregator::GuiActivityAggregator;
                use oneshim_vision::gui_detector::GuiElementDetector;

                use super::gui_pipeline::GuiPipelineState;

                let detector = GuiElementDetector::new(
                    (1920, 1080), // sensible default; updated per tick from WindowLayoutEvent
                    oneshim_core::config::PiiFilterLevel::Standard,
                );
                let aggregator = GuiActivityAggregator::new(&gui_config);
                ts.gui_pipeline_state = Some(GuiPipelineState {
                    detector,
                    aggregator,
                });
                info!("GUI Activity Intelligence pipeline enabled");
            }
        }

        let monitor_task = self.spawn_monitor_loop(
            poll,
            idle_threshold,
            session_id.clone(),
            egress_policy.clone(),
            shared_input_collector.clone(),
            adaptive_trigger_state,
            shutdown_rx.clone(),
        );

        let metrics_task = self.spawn_metrics_loop(metrics_interval, shutdown_rx.clone());

        let process_task = self.spawn_process_loop(process_interval, shutdown_rx.clone());

        let sync_task = self.spawn_sync_loop(sync, egress_policy.clone(), shutdown_rx.clone());

        let heartbeat_task = self.spawn_heartbeat_loop(
            heartbeat,
            session_id.clone(),
            egress_policy.clone(),
            shutdown_rx.clone(),
        );

        let aggregation_task = self.spawn_aggregation_loop(aggregation, shutdown_rx.clone());

        let notification_task = self.spawn_notification_loop(shutdown_rx.clone());

        let focus_task = self.spawn_focus_loop(shutdown_rx.clone());

        let event_snapshot_task = self.spawn_event_snapshot_loop(
            detailed_process_interval,
            input_activity_interval,
            egress_policy.clone(),
            shared_input_collector.clone(),
            shutdown_rx.clone(),
        );

        // 10. OAuth token refresh (conditional — returns None if no coordinator)
        #[cfg(feature = "server")]
        let oauth_task = self.spawn_oauth_refresh_loop(shutdown_rx.clone(), app_handle);

        // 11. LLM analysis loop (periodic + change-detection)
        let analysis_config = self.config.analysis_config.clone();
        let analysis_task = self.spawn_analysis_loop(analysis_config, shutdown_rx.clone());

        // 12. Cross-device sync loop (P3 Phase 3a-2)
        let cross_device_sync_task = self.spawn_cross_device_sync_loop(
            self.config.cross_device_sync_interval,
            shutdown_rx.clone(),
        );

        let _ = shutdown_rx.changed().await;
        info!("ended received");

        let sqlite_end = self.sqlite_storage.clone();
        if let Err(e) = sqlite_end.end_session(&session_id, Utc::now()).await {
            warn!("session ended record failure: {e}");
        }

        monitor_task.abort();
        metrics_task.abort();
        process_task.abort();
        sync_task.abort();
        heartbeat_task.abort();
        aggregation_task.abort();
        notification_task.abort();
        focus_task.abort();
        event_snapshot_task.abort();
        #[cfg(feature = "server")]
        if let Some(task) = oauth_task {
            task.abort();
        }
        analysis_task.abort();
        cross_device_sync_task.abort();
    }
}

/// Convert a SegmentSummaryRecord (storage row) to SegmentSummary (domain model)
/// for use with DailyDigestGenerator.
pub fn record_to_segment_summary(r: &SegmentSummaryRecord) -> Option<SegmentSummary> {
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
