use chrono::Utc;
use oneshim_core::models::activity::IdleState;
use oneshim_core::models::event::Event;
use oneshim_core::models::focused_element::AccessibilityElement;
use oneshim_core::models::frame::OcrRegion;
use oneshim_monitor::idle::IdleTracker;
use oneshim_monitor::input_activity::InputActivityCollector;
use oneshim_monitor::window_layout::WindowLayoutTracker;
use oneshim_vision::ring_buffer::{CaptureRingBuffer, RingFrame};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

use super::super::config::PlatformEgressPolicy;
use super::super::Scheduler;
use super::helpers::{
    build_personalization_prompt, build_segment_stats_snapshot, handle_event_analysis,
    handle_frame_capture, COACHING_SYSTEM_PROMPT,
};

impl Scheduler {
    #[tracing::instrument(skip_all)]
    #[allow(clippy::too_many_arguments)]
    pub(in crate::scheduler) fn spawn_monitor_loop(
        &self,
        poll: Duration,
        idle_threshold: u64,
        session_id: String,
        egress_policy: Arc<PlatformEgressPolicy>,
        input_collector: Arc<InputActivityCollector>,
        adaptive_trigger_state: Option<super::super::AdaptiveTriggerState>,
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
        let coaching_engine_ref = self.coaching_engine.clone();
        let overlay_ref = self.magic_overlay.clone();
        let coaching_storage_ref = self.coaching_storage.clone();
        let coaching_analysis_provider = self.analysis_provider.clone();

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

            // ── Coaching: track real regime dwell time ──
            let mut regime_entered_at: Option<std::time::Instant> = None;
            let mut prev_coaching_regime_id: Option<String> = None;

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
                                    // Update GUI detector + heatmap resolution from the latest layout event
                                    let (res_w, res_h) = layout_event.screen_resolution;
                                    if let Some(ref mut ts) = adaptive_trigger_state {
                                        if let Some(ref mut gui_state) = ts.gui_pipeline_state {
                                            gui_state.detector.update_resolution(res_w, res_h);
                                        }
                                        ts.heatmap_aggregator.update_resolution(res_w, res_h);
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

                                let event = oneshim_core::models::event::ContextEvent {
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
                                        accessibility_elements: last_focused_element
                                            .as_ref()
                                            .map(|f| {
                                                vec![AccessibilityElement {
                                                    role: f.role.clone(),
                                                    label: f.label.clone().unwrap_or_default(),
                                                    bounds: f.position,
                                                }]
                                            })
                                            .unwrap_or_default(),
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
                                                accessibility_elements: Vec::new(),
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
                                    super::super::analysis_pipeline::run_analysis_tick(
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
                                // Update ContextAnalyzer with current segment stats
                                // so that analyze() includes segment context in LLM prompts.
                                if let (Some(ref ts), Some(ref analyzer)) = (&adaptive_trigger_state, &context_analyzer1) {
                                    let stats = build_segment_stats_snapshot(ts);
                                    analyzer.set_segment_stats(stats).await;
                                }
                                // Update accessibility text for LLM context enrichment
                                if let Some(ref analyzer) = context_analyzer1 {
                                    let a11y_text = last_focused_element.as_ref()
                                        .and_then(|fe| fe.extracted_text.clone());
                                    analyzer.set_accessibility_text(a11y_text).await;
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

                                        let gui_summary = super::super::gui_pipeline::run_gui_tick(
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

                                // ── Heatmap aggregation ──
                                // Record click positions and periodically emit to overlay.
                                if let Some(ref mut ts) = adaptive_trigger_state {
                                    if let Some((x, y)) = input_snap.mouse.last_position {
                                        ts.heatmap_aggregator.record(x, y, input_snap.mouse.click_count);
                                    }
                                    if let Some(grid) = ts.heatmap_aggregator.take_snapshot() {
                                        if let Some(ref overlay) = overlay_ref {
                                            overlay.emit_heatmap(grid);
                                        }
                                    }
                                }

                                // ── Coaching evaluation (Phase 1) ──
                                // Uses placeholder values for regime data in Phase 1.
                                // Full integration with AdaptiveTriggerState will use
                                // regime_classifier and drift_detector outputs.
                                if let Some(ref coaching) = coaching_engine_ref {
                                    // Extract regime data from adaptive trigger state
                                    let regime_id_for_coaching: Option<&str> =
                                        adaptive_trigger_state.as_ref().and_then(|ts| {
                                            ts.current_regime_id.as_deref()
                                        });
                                    let regime_label_for_coaching =
                                        regime_id_for_coaching.unwrap_or("Unknown");
                                    let avg_regime_duration_secs: u64 = coaching
                                        .avg_regime_duration_secs(regime_label_for_coaching)
                                        .await;
                                    let drift_detected = adaptive_trigger_state
                                        .as_ref()
                                        .map(|ts| ts.last_drift_detected.swap(false, std::sync::atomic::Ordering::Relaxed))
                                        .unwrap_or(false);

                                    // Track real regime dwell time: reset timer on regime change
                                    let current_coaching_regime = regime_id_for_coaching.map(String::from);
                                    if current_coaching_regime != prev_coaching_regime_id {
                                        regime_entered_at = Some(std::time::Instant::now());
                                        prev_coaching_regime_id = current_coaching_regime;
                                    }
                                    let regime_duration_secs: u64 = regime_entered_at
                                        .map(|t| t.elapsed().as_secs())
                                        .unwrap_or(0);

                                    // Record elapsed minutes for goal tracking
                                    let elapsed_minutes =
                                        (poll.as_secs() as f32 / 60.0).max(0.0) as u32;
                                    if elapsed_minutes > 0 {
                                        coaching
                                            .record_minutes(
                                                regime_label_for_coaching,
                                                elapsed_minutes,
                                            )
                                            .await;
                                    }

                                    // Evaluate coaching triggers
                                    if let Some(message) = coaching
                                        .evaluate(
                                            regime_id_for_coaching,
                                            regime_label_for_coaching,
                                            regime_duration_secs,
                                            avg_regime_duration_secs,
                                            drift_detected,
                                            prev_app.as_deref().unwrap_or(""),
                                        )
                                        .await
                                    {
                                        // 1. Show on MagicOverlay (primary delivery)
                                        if let Some(ref overlay) = overlay_ref {
                                            overlay.show_coaching(&message).await;
                                        }

                                        // 2. Also send desktop notification (fallback)
                                        if let Some(ref notif) = notif1 {
                                            notif
                                                .notify_coaching(&message.template_text)
                                                .await;
                                        }

                                        // 3. Persist coaching event to storage
                                        if let Some(ref cs) = coaching_storage_ref {
                                            let event_row = oneshim_core::models::coaching::CoachingEventRow {
                                                event_id: message.message_id.clone(),
                                                trigger_type: oneshim_core::models::coaching::trigger_type_name(&message.trigger),
                                                profile_name: format!("{:?}", message.profile),
                                                regime_id: regime_id_for_coaching.map(String::from),
                                                message_template: message.template_text.clone(),
                                                personalized_message: None,
                                                shown_at: chrono::Utc::now().to_rfc3339(),
                                                dismissed_at: None,
                                                dismiss_action: None,
                                                feedback_type: None,
                                                feedback_score: None,
                                            };
                                            if let Err(e) = cs.insert_coaching_event(&event_row) {
                                                warn!("coaching event persist failure: {e}");
                                            }
                                        }

                                        // 4. Register for feedback tracking
                                        coaching
                                            .register_pending_feedback(
                                                &message.message_id,
                                                &format!("{:?}", message.profile),
                                                &oneshim_core::models::coaching::trigger_type_name(
                                                    &message.trigger,
                                                ),
                                                regime_id_for_coaching,
                                                prev_app.as_deref().unwrap_or(""),
                                            )
                                            .await;

                                        info!(
                                            profile = ?message.profile,
                                            trigger = ?message.trigger,
                                            "coaching message: {}",
                                            message.template_text,
                                        );

                                        // 5. Spawn background LLM personalization
                                        if let Some(ref provider) = coaching_analysis_provider {
                                            let msg_clone = message.clone();
                                            let provider_clone = provider.clone();
                                            let overlay_clone = overlay_ref.clone();
                                            let storage_clone = coaching_storage_ref.clone();
                                            let regime = regime_label_for_coaching.to_string();
                                            tokio::spawn(async move {
                                                let prompt = build_personalization_prompt(
                                                    &msg_clone.template_text,
                                                    &regime,
                                                );
                                                match provider_clone.analyze(&prompt, COACHING_SYSTEM_PROMPT).await {
                                                    Ok(suggestions) if !suggestions.is_empty() => {
                                                        let personalized = &suggestions[0].content;
                                                        // Upgrade overlay if still visible
                                                        if let Some(ref overlay) = overlay_clone {
                                                            overlay.upgrade_message(&msg_clone.message_id, personalized).await;
                                                        }
                                                        // Persist personalized text to storage
                                                        if let Some(ref cs) = storage_clone {
                                                            if let Err(e) = cs.update_coaching_event_personalized(
                                                                &msg_clone.message_id,
                                                                personalized,
                                                            ) {
                                                                debug!("coaching personalization persist: {e}");
                                                            }
                                                        }
                                                    }
                                                    Ok(_) => { /* No suggestions returned — template remains */ }
                                                    Err(e) => {
                                                        debug!("LLM coaching personalization failed: {e}");
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }

                                // Emit goal progress to overlay (every tick when coaching enabled)
                                if let Some(ref coaching) = coaching_engine_ref {
                                    if let Some(ref overlay) = overlay_ref {
                                        let goals = coaching.all_goal_progress().await;
                                        if !goals.is_empty() {
                                            overlay.update_goal_progress(goals).await;
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
}
