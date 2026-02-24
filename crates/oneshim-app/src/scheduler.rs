//!

use base64::Engine;
use chrono::{Datelike, Duration as ChronoDuration, Timelike, Utc};
use oneshim_core::config::{AiAccessMode, AppConfig, ExternalDataPolicy, PrivacyConfig, Weekday};
use oneshim_core::error::CoreError;
use oneshim_core::models::activity::{
    IdleState, ProcessSnapshot, ProcessSnapshotEntry, SessionStats,
};
use oneshim_core::models::context::WindowBounds;
use oneshim_core::models::event::{ContextEvent, Event, ProcessSnapshotEvent};
use oneshim_core::models::frame::{FrameMetadata, ImagePayload};
use oneshim_core::ports::api_client::ApiClient;
use oneshim_core::ports::monitor::{ActivityMonitor, ProcessMonitor, SystemMonitor};
use oneshim_core::ports::storage::{MetricsStorage, StorageService};
use oneshim_core::ports::vision::{CaptureTrigger, FrameProcessor};
use oneshim_monitor::idle::IdleTracker;
use oneshim_monitor::input_activity::InputActivityCollector;
use oneshim_monitor::window_layout::WindowLayoutTracker;
use oneshim_network::batch_uploader::BatchUploader;
use oneshim_storage::frame_storage::FrameFileStorage;
use oneshim_storage::sqlite::SqliteStorage;
use oneshim_vision::privacy::{sanitize_title_with_level, should_exclude};
use oneshim_web::{MetricsUpdate, RealtimeEvent};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, info, warn};

use crate::focus_analyzer::FocusAnalyzer;
use crate::notification_manager::NotificationManager;

///
pub trait SchedulerStorage: MetricsStorage + Send + Sync {
    fn save_frame_metadata_with_bounds(
        &self,
        metadata: &FrameMetadata,
        file_path: Option<&str>,
        ocr_text: Option<&str>,
        bounds: Option<&WindowBounds>,
    ) -> Result<i64, CoreError>;
}

impl SchedulerStorage for SqliteStorage {
    fn save_frame_metadata_with_bounds(
        &self,
        metadata: &FrameMetadata,
        file_path: Option<&str>,
        ocr_text: Option<&str>,
        bounds: Option<&WindowBounds>,
    ) -> Result<i64, CoreError> {
        SqliteStorage::save_frame_metadata_with_bounds(self, metadata, file_path, ocr_text, bounds)
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| e.to_string())
}

const REDACTED_WINDOW_TITLE: &str = "[REDACTED_WINDOW_TITLE]";

#[derive(Clone)]
struct PlatformEgressPolicy {
    enabled: bool,
    external_data_policy: ExternalDataPolicy,
    privacy_config: PrivacyConfig,
}

impl PlatformEgressPolicy {
    fn new(config: &SchedulerConfig) -> Self {
        Self {
            enabled: !config.offline_mode
                && config.ai_access_mode == AiAccessMode::PlatformConnected,
            external_data_policy: config.external_data_policy,
            privacy_config: config.privacy_config.clone(),
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn prepare_event_for_upload(&self, mut event: Event) -> Option<Event> {
        if !self.enabled {
            return None;
        }

        match &mut event {
            Event::Context(ctx) => {
                let app_name = ctx.app_name.clone();
                let title = ctx.window_title.clone();
                if self.should_skip(&app_name, &title) {
                    return None;
                }
                ctx.window_title = self.sanitize_title(&title);
            }
            Event::Window(layout) => {
                let app_name = layout.window.app_name.clone();
                let title = layout.window.window_title.clone();
                if self.should_skip(&app_name, &title) {
                    return None;
                }
                layout.window.window_title = self.sanitize_title(&title);
            }
            Event::User(user) => {
                let app_name = user.app_name.clone();
                let title = user.window_title.clone();
                if self.should_skip(&app_name, &title) {
                    return None;
                }
                user.window_title = self.sanitize_title(&title);
            }
            Event::System(_) | Event::Input(_) | Event::Process(_) => {}
        }

        Some(event)
    }

    fn sanitize_title(&self, title: &str) -> String {
        match self.external_data_policy {
            ExternalDataPolicy::AllowFiltered => {
                sanitize_title_with_level(title, self.privacy_config.pii_filter_level)
            }
            ExternalDataPolicy::PiiFilterStrict | ExternalDataPolicy::PiiFilterStandard => {
                REDACTED_WINDOW_TITLE.to_string()
            }
        }
    }

    fn should_skip(&self, app_name: &str, window_title: &str) -> bool {
        should_exclude(
            app_name,
            window_title,
            &self.privacy_config.excluded_apps,
            &self.privacy_config.excluded_app_patterns,
            &self.privacy_config.excluded_title_patterns,
            self.privacy_config.auto_exclude_sensitive,
        )
    }
}

pub struct SchedulerConfig {
    pub poll_interval: Duration,
    pub metrics_interval: Duration,
    pub process_interval: Duration,
    pub detailed_process_interval: Duration,
    pub input_activity_interval: Duration,
    pub sync_interval: Duration,
    pub heartbeat_interval: Duration,
    pub aggregation_interval: Duration,
    pub session_id: String,
    pub offline_mode: bool,
    pub ai_access_mode: AiAccessMode,
    pub external_data_policy: ExternalDataPolicy,
    pub privacy_config: PrivacyConfig,
    pub idle_threshold_secs: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(1),
            metrics_interval: Duration::from_secs(5),
            process_interval: Duration::from_secs(10),
            detailed_process_interval: Duration::from_secs(30), // 30 s
            input_activity_interval: Duration::from_secs(30),   // 30 s
            sync_interval: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            aggregation_interval: Duration::from_secs(3600), // 1 hour
            session_id: String::new(),                       // set by caller
            offline_mode: false,
            ai_access_mode: AiAccessMode::default(),
            external_data_policy: ExternalDataPolicy::default(),
            privacy_config: PrivacyConfig::default(),
            idle_threshold_secs: 300, // 5 min
        }
    }
}

pub struct Scheduler {
    config: SchedulerConfig,
    #[allow(dead_code)]
    app_config: Arc<tokio::sync::RwLock<AppConfig>>,
    system_monitor: Arc<dyn SystemMonitor>,
    activity_monitor: Arc<dyn ActivityMonitor>,
    process_monitor: Arc<dyn ProcessMonitor>,
    capture_trigger: Arc<Mutex<Box<dyn CaptureTrigger>>>,
    frame_processor: Arc<Mutex<Box<dyn FrameProcessor>>>,
    storage: Arc<dyn StorageService>,
    sqlite_storage: Arc<dyn SchedulerStorage>,
    frame_storage: Option<Arc<FrameFileStorage>>,
    batch_uploader: Arc<BatchUploader>,
    api_client: Arc<dyn ApiClient>,
    event_tx: Option<broadcast::Sender<RealtimeEvent>>,
    notification_manager: Option<Arc<NotificationManager>>,
    focus_analyzer: Option<Arc<FocusAnalyzer>>,
}

impl Scheduler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: SchedulerConfig,
        app_config: Arc<tokio::sync::RwLock<AppConfig>>,
        system_monitor: Arc<dyn SystemMonitor>,
        activity_monitor: Arc<dyn ActivityMonitor>,
        process_monitor: Arc<dyn ProcessMonitor>,
        capture_trigger: Box<dyn CaptureTrigger>,
        frame_processor: Box<dyn FrameProcessor>,
        storage: Arc<dyn StorageService>,
        sqlite_storage: Arc<dyn SchedulerStorage>,
        frame_storage: Option<Arc<FrameFileStorage>>,
        batch_uploader: Arc<BatchUploader>,
        api_client: Arc<dyn ApiClient>,
    ) -> Self {
        Self {
            config,
            app_config,
            system_monitor,
            activity_monitor,
            process_monitor,
            capture_trigger: Arc::new(Mutex::new(capture_trigger)),
            frame_processor: Arc::new(Mutex::new(frame_processor)),
            storage,
            sqlite_storage,
            frame_storage,
            batch_uploader,
            api_client,
            event_tx: None,
            notification_manager: None,
            focus_analyzer: None,
        }
    }

    pub fn with_event_tx(mut self, event_tx: broadcast::Sender<RealtimeEvent>) -> Self {
        self.event_tx = Some(event_tx);
        self
    }

    pub fn with_notification_manager(mut self, manager: Arc<NotificationManager>) -> Self {
        self.notification_manager = Some(manager);
        self
    }

    pub fn with_focus_analyzer(mut self, analyzer: Arc<FocusAnalyzer>) -> Self {
        self.focus_analyzer = Some(analyzer);
        self
    }

    #[allow(dead_code)]
    pub fn app_config(&self) -> Arc<tokio::sync::RwLock<AppConfig>> {
        self.app_config.clone()
    }

    async fn initialize_session(&self, session_id: &str) {
        let sqlite_init = self.sqlite_storage.clone();
        let session_stats = SessionStats::new(session_id.to_string());
        if let Err(e) = sqlite_init.upsert_session(&session_stats).await {
            warn!("session initialize failure: {e}");
        }
    }

    pub async fn run(&self, shutdown_rx: tokio::sync::watch::Receiver<bool>) {
        info!(
            "스케줄러 started: 모니터링={}ms, 메트릭={}ms, 프로세스={}ms, 동기화={}ms, heartbeat={}ms, 집계={}ms",
            self.config.poll_interval.as_millis(),
            self.config.metrics_interval.as_millis(),
            self.config.process_interval.as_millis(),
            self.config.sync_interval.as_millis(),
            self.config.heartbeat_interval.as_millis(),
            self.config.aggregation_interval.as_millis(),
        );
        self.run_scheduler_loops(shutdown_rx).await;
    }

    fn spawn_monitor_loop(
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
        let uploader1 = self.batch_uploader.clone();
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

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let idle_info = idle_tracker.check_idle();
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
                                    if let Some(upload_event) = egress1.prepare_event_for_upload(win_event) {
                                        uploader1.enqueue(upload_event);
                                    }
                                }

                                let event = ContextEvent {
                                    app_name: app_name.clone(),
                                    window_title,
                                    prev_app_name: prev_app.clone(),
                                    timestamp: Utc::now(),
                                };

                                {
                                    let mut trig = trigger.lock().await;
                                    if let Some(capture_req) = trig.should_capture(&event) {
                                        let mut proc = processor.lock().await;
                                        match proc.capture_and_process(&capture_req).await {
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
                                    }
                                }

                                let ctx_event = Event::Context(event);
                                if let Err(e) = storage1.save_event(&ctx_event).await {
                                    warn!("event save failure: {e}");
                                }

                                let _ = sqlite1.increment_session_counters(&session1, 1, 0, 0).await;

                                if let Some(upload_event) = egress1.prepare_event_for_upload(ctx_event) {
                                    uploader1.enqueue(upload_event);
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

    fn spawn_metrics_loop(
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

    fn spawn_process_loop(
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

    fn spawn_sync_loop(
        &self,
        sync_interval: Duration,
        egress_policy: Arc<PlatformEgressPolicy>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let uploader4 = self.batch_uploader.clone();
        let storage4 = self.storage.clone();
        let frame_storage4 = self.frame_storage.clone();
        let egress4 = egress_policy;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(sync_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if egress4.is_enabled() {
                            match uploader4.flush().await {
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

    fn spawn_heartbeat_loop(
        &self,
        heartbeat_interval: Duration,
        session_id: String,
        egress_policy: Arc<PlatformEgressPolicy>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let api = self.api_client.clone();
        let sid = session_id;

        tokio::spawn(async move {
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

    fn spawn_aggregation_loop(
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

    fn spawn_notification_loop(
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

    fn spawn_focus_loop(
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

    fn spawn_event_snapshot_loop(
        &self,
        detailed_process_interval: Duration,
        input_activity_interval: Duration,
        egress_policy: Arc<PlatformEgressPolicy>,
        input_collector: Arc<InputActivityCollector>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let proc_mon9 = self.process_monitor.clone();
        let storage9 = self.storage.clone();
        let uploader9 = self.batch_uploader.clone();
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

                                if let Some(upload_event) = egress9.prepare_event_for_upload(event) {
                                    uploader9.enqueue(upload_event);
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

                            if let Some(upload_event) = egress9.prepare_event_for_upload(event) {
                                uploader9.enqueue(upload_event);
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

    async fn run_scheduler_loops(&self, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
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
    }
}


///
#[allow(dead_code)]
pub fn should_run_now(config: &AppConfig) -> bool {
    let schedule = &config.schedule;
    if !schedule.active_hours_enabled {
        return true;
    }

    let now = chrono::Local::now();
    let hour = now.hour() as u8;
    let weekday = match now.weekday() {
        chrono::Weekday::Mon => Weekday::Mon,
        chrono::Weekday::Tue => Weekday::Tue,
        chrono::Weekday::Wed => Weekday::Wed,
        chrono::Weekday::Thu => Weekday::Thu,
        chrono::Weekday::Fri => Weekday::Fri,
        chrono::Weekday::Sat => Weekday::Sat,
        chrono::Weekday::Sun => Weekday::Sun,
    };

    if !schedule.active_days.contains(&weekday) {
        return false;
    }

    hour >= schedule.active_start_hour && hour < schedule.active_end_hour
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::config::PiiFilterLevel;

    #[test]
    fn should_run_when_disabled() {
        let config = AppConfig::default_config();
        assert!(should_run_now(&config));
    }

    #[test]
    fn scheduler_config_default() {
        let config = SchedulerConfig::default();
        assert_eq!(config.poll_interval, Duration::from_secs(1));
        assert_eq!(config.metrics_interval, Duration::from_secs(5));
        assert_eq!(config.ai_access_mode, AiAccessMode::ProviderApiKey);
        assert_eq!(config.idle_threshold_secs, 300);
    }

    #[test]
    fn platform_sync_enabled_only_for_platform_connected_mode() {
        let mut config = SchedulerConfig {
            offline_mode: false,
            ai_access_mode: AiAccessMode::ProviderApiKey,
            ..SchedulerConfig::default()
        };

        let policy = PlatformEgressPolicy::new(&config);
        assert!(!policy.is_enabled());

        config.ai_access_mode = AiAccessMode::PlatformConnected;
        let policy = PlatformEgressPolicy::new(&config);
        assert!(policy.is_enabled());
    }

    #[test]
    fn strict_policy_redacts_window_title() {
        let config = SchedulerConfig {
            offline_mode: false,
            ai_access_mode: AiAccessMode::PlatformConnected,
            external_data_policy: ExternalDataPolicy::PiiFilterStrict,
            ..SchedulerConfig::default()
        };
        let policy = PlatformEgressPolicy::new(&config);
        let event = Event::Context(ContextEvent {
            app_name: "Chrome".to_string(),
            window_title: "Inbox user@example.com".to_string(),
            prev_app_name: None,
            timestamp: Utc::now(),
        });

        let uploaded = policy.prepare_event_for_upload(event);
        let Some(Event::Context(ctx)) = uploaded else {
            panic!("context event should be uploadable");
        };
        assert_eq!(ctx.window_title, REDACTED_WINDOW_TITLE);
    }

    #[test]
    fn allow_filtered_policy_uses_pii_filter() {
        let privacy = PrivacyConfig {
            pii_filter_level: PiiFilterLevel::Basic,
            ..PrivacyConfig::default()
        };
        let config = SchedulerConfig {
            offline_mode: false,
            ai_access_mode: AiAccessMode::PlatformConnected,
            external_data_policy: ExternalDataPolicy::AllowFiltered,
            privacy_config: privacy,
            ..SchedulerConfig::default()
        };
        let policy = PlatformEgressPolicy::new(&config);
        let event = Event::Context(ContextEvent {
            app_name: "Chrome".to_string(),
            window_title: "Inbox user@example.com".to_string(),
            prev_app_name: None,
            timestamp: Utc::now(),
        });

        let uploaded = policy.prepare_event_for_upload(event);
        let Some(Event::Context(ctx)) = uploaded else {
            panic!("context event should be uploadable");
        };
        assert!(ctx.window_title.contains("[EMAIL]"));
        assert!(!ctx.window_title.contains('@'));
    }

    #[test]
    fn sensitive_apps_are_skipped_from_upload() {
        let config = SchedulerConfig {
            offline_mode: false,
            ai_access_mode: AiAccessMode::PlatformConnected,
            ..SchedulerConfig::default()
        };
        let policy = PlatformEgressPolicy::new(&config);
        let event = Event::Context(ContextEvent {
            app_name: "Bitwarden".to_string(),
            window_title: "Vault".to_string(),
            prev_app_name: None,
            timestamp: Utc::now(),
        });

        assert!(policy.prepare_event_for_upload(event).is_none());
    }
}
