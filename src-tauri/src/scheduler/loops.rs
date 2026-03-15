use chrono::{Duration as ChronoDuration, Utc};
use oneshim_core::models::activity::{IdleState, ProcessSnapshot, ProcessSnapshotEntry};
use oneshim_core::models::event::{ContextEvent, Event, ProcessSnapshotEvent};
use oneshim_core::models::frame::ImagePayload;
use oneshim_monitor::idle::IdleTracker;
use oneshim_monitor::input_activity::InputActivityCollector;
use oneshim_monitor::window_layout::WindowLayoutTracker;
use oneshim_vision::ring_buffer::{CaptureRingBuffer, RingFrame};
use oneshim_web::{MetricsUpdate, RealtimeEvent};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

use super::config::{base64_decode, PlatformEgressPolicy};
use super::Scheduler;

impl Scheduler {
    pub(super) fn spawn_monitor_loop(
        &self,
        poll: Duration,
        idle_threshold: u64,
        session_id: String,
        egress_policy: Arc<PlatformEgressPolicy>,
        input_collector: Arc<InputActivityCollector>,
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
        let input_collector1 = input_collector;

        tokio::spawn(async move {
            let mut prev_app: Option<String> = None;
            let mut prev_idle_secs: u64 = 0;
            let mut interval = tokio::time::interval(poll);
            let mut idle_tracker = IdleTracker::new(Some(idle_threshold));

            let window_tracker = WindowLayoutTracker::new();
            let input_collector = input_collector1;
            // Dashcam ring buffer: 6 slots (~18s at 3s poll), flush on importance >= 0.5,
            // capture 2 post-event frames after each flush.
            let ring_buffer = CaptureRingBuffer::new(6, 2, 0.5);

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

                                if let Some(layout_event) = window_tracker.update(&app_name, &window_title, window_bounds) {
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

                                        match processor.capture_and_process(&capture_req).await {
                                            Ok(frame) => {
                                                debug!("frame completed: {:?}", frame.metadata.trigger_type);

                                                let (file_path, ocr_text) = if let Some(ref payload) = frame.image_payload {
                                                    let (data_str, ocr) = match payload {
                                                        ImagePayload::Full { data, ocr_text, .. } => (data.as_str(), ocr_text.clone()),
                                                        ImagePayload::Delta { data, .. } => (data.as_str(), None),
                                                        ImagePayload::Thumbnail { data, .. } => (data.as_str(), None),
                                                    };

                                                    let saved_path = if let Some(ref fs) = frame_storage1 {
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
                                                focus_ocr_hint = ocr_text.clone();

                                                if let Err(e) = sqlite1.save_frame_metadata_with_bounds(
                                                    &frame.metadata,
                                                    file_path.as_deref(),
                                                    ocr_text.as_deref(),
                                                    window_bounds.as_ref(),
                                                ) {
                                                    warn!("frame data save failure: {e}");
                                                }

                                                let _ = sqlite1.increment_session_counters(&session1, 0, 1, 0).await;
                                            }
                                            Err(e) => {
                                                warn!("frame failure: {e}");
                                            }
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

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(aggregation_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let now = Utc::now();

                        let prev_hour = now - ChronoDuration::hours(1);
                        if let Err(e) = sqlite6.aggregate_hourly_metrics(prev_hour).await {
                            warn!("hour failure: {e}");
                        }

                        let metrics_cutoff = now - ChronoDuration::hours(24);
                        if let Err(e) = sqlite6.cleanup_old_metrics(metrics_cutoff).await {
                            warn!("delete failure: {e}");
                        }

                        let process_cutoff = now - ChronoDuration::days(7);
                        if let Err(e) = sqlite6.cleanup_old_process_snapshots(process_cutoff).await {
                            warn!("delete failure: {e}");
                        }

                        let idle_cutoff = now - ChronoDuration::days(30);
                        if let Err(e) = sqlite6.cleanup_old_idle_periods(idle_cutoff).await {
                            warn!("idle period delete failure: {e}");
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
            access_mode = ?self.config.ai_access_mode,
            platform_sync_enabled = egress_policy.is_enabled(),
            "플랫폼 egress policy 적용"
        );

        self.initialize_session(&session_id).await;

        let shared_input_collector = Arc::new(InputActivityCollector::new());

        let monitor_task = self.spawn_monitor_loop(
            poll,
            idle_threshold,
            session_id.clone(),
            egress_policy.clone(),
            shared_input_collector.clone(),
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
    }
}
