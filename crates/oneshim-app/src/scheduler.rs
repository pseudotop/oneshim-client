//! 9-루프 스케줄러.
//!
//! 모니터링(1초), 메트릭(5초), 프로세스(10초), 상세 프로세스(30초), 동기화(10초), 하트비트(30초), 집계(1시간), 알림(1분), 집중도 분석(1분) 오케스트레이션.

use base64::Engine;
use chrono::{Datelike, Duration as ChronoDuration, Timelike, Utc};
use oneshim_core::config::{AppConfig, Weekday};
use oneshim_core::models::activity::{
    IdleState, ProcessSnapshot, ProcessSnapshotEntry, SessionStats,
};
use oneshim_core::models::event::{ContextEvent, Event, ProcessSnapshotEvent};
use oneshim_core::models::frame::ImagePayload;
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
use oneshim_web::{MetricsUpdate, RealtimeEvent};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, info, warn};

use crate::focus_analyzer::FocusAnalyzer;
use crate::notification_manager::NotificationManager;

/// Base64 문자열을 바이트로 디코딩
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| e.to_string())
}

/// 스케줄러 설정
pub struct SchedulerConfig {
    /// 모니터링 폴링 간격
    pub poll_interval: Duration,
    /// 시스템 메트릭 수집 간격
    pub metrics_interval: Duration,
    /// 프로세스 스냅샷 간격 (로컬 저장용)
    pub process_interval: Duration,
    /// 상세 프로세스 이벤트 간격 (서버 전송용)
    pub detailed_process_interval: Duration,
    /// 입력 활동 집계 간격
    pub input_activity_interval: Duration,
    /// 서버 동기화 간격
    pub sync_interval: Duration,
    /// 하트비트 간격
    pub heartbeat_interval: Duration,
    /// 집계 간격 (시간별 메트릭 집계)
    pub aggregation_interval: Duration,
    /// 세션 ID
    pub session_id: String,
    /// 오프라인 모드 (서버 연결 없이 로컬 기능만 사용)
    pub offline_mode: bool,
    /// 유휴 감지 임계값 (초)
    pub idle_threshold_secs: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(1),
            metrics_interval: Duration::from_secs(5),
            process_interval: Duration::from_secs(10),
            detailed_process_interval: Duration::from_secs(30), // 30초
            input_activity_interval: Duration::from_secs(30),   // 30초
            sync_interval: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            aggregation_interval: Duration::from_secs(3600), // 1시간
            session_id: String::new(),                       // 호출자가 설정
            offline_mode: false,
            idle_threshold_secs: 300, // 5분
        }
    }
}

/// 9-루프 스케줄러
pub struct Scheduler {
    config: SchedulerConfig,
    /// 앱 설정 (스케줄/프라이버시/텔레메트리 조건부 실행용)
    #[allow(dead_code)]
    app_config: Arc<tokio::sync::RwLock<AppConfig>>,
    system_monitor: Arc<dyn SystemMonitor>,
    activity_monitor: Arc<dyn ActivityMonitor>,
    process_monitor: Arc<dyn ProcessMonitor>,
    capture_trigger: Arc<Mutex<Box<dyn CaptureTrigger>>>,
    frame_processor: Arc<Mutex<Box<dyn FrameProcessor>>>,
    storage: Arc<dyn StorageService>,
    /// SQLite 저장소 (프레임 메타데이터 + 메트릭 저장용)
    sqlite_storage: Arc<SqliteStorage>,
    /// 프레임 파일 저장소 (옵션)
    frame_storage: Option<Arc<FrameFileStorage>>,
    batch_uploader: Arc<BatchUploader>,
    api_client: Arc<dyn ApiClient>,
    /// 실시간 이벤트 브로드캐스트 채널 (웹 대시보드용)
    event_tx: Option<broadcast::Sender<RealtimeEvent>>,
    /// 알림 관리자 (옵션)
    notification_manager: Option<Arc<NotificationManager>>,
    /// 집중도 분석기 (옵션)
    focus_analyzer: Option<Arc<FocusAnalyzer>>,
}

impl Scheduler {
    /// 새 스케줄러 생성
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
        sqlite_storage: Arc<SqliteStorage>,
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

    /// 실시간 이벤트 브로드캐스트 채널 설정
    pub fn with_event_tx(mut self, event_tx: broadcast::Sender<RealtimeEvent>) -> Self {
        self.event_tx = Some(event_tx);
        self
    }

    /// 알림 관리자 설정
    pub fn with_notification_manager(mut self, manager: Arc<NotificationManager>) -> Self {
        self.notification_manager = Some(manager);
        self
    }

    /// 집중도 분석기 설정
    pub fn with_focus_analyzer(mut self, analyzer: Arc<FocusAnalyzer>) -> Self {
        self.focus_analyzer = Some(analyzer);
        self
    }

    /// 앱 설정 참조 반환 (외부 설정 변경용)
    #[allow(dead_code)]
    pub fn app_config(&self) -> Arc<tokio::sync::RwLock<AppConfig>> {
        self.app_config.clone()
    }

    /// 모든 루프 시작
    pub async fn run(&self, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
        info!(
            "스케줄러 시작: 모니터링={}ms, 메트릭={}ms, 프로세스={}ms, 동기화={}ms, 하트비트={}ms, 집계={}ms",
            self.config.poll_interval.as_millis(),
            self.config.metrics_interval.as_millis(),
            self.config.process_interval.as_millis(),
            self.config.sync_interval.as_millis(),
            self.config.heartbeat_interval.as_millis(),
            self.config.aggregation_interval.as_millis(),
        );

        let poll = self.config.poll_interval;
        let metrics_interval = self.config.metrics_interval;
        let process_interval = self.config.process_interval;
        let detailed_process_interval = self.config.detailed_process_interval;
        let input_activity_interval = self.config.input_activity_interval;
        let sync = self.config.sync_interval;
        let heartbeat = self.config.heartbeat_interval;
        let aggregation = self.config.aggregation_interval;
        let session_id = self.config.session_id.clone();
        let offline_mode = self.config.offline_mode;
        let idle_threshold = self.config.idle_threshold_secs;

        // 세션 초기화
        let sqlite_init = self.sqlite_storage.clone();
        let session_init = session_id.clone();
        let session_stats = SessionStats::new(session_init.clone());
        if let Err(e) = sqlite_init.upsert_session(&session_stats).await {
            warn!("세션 초기화 실패: {e}");
        }

        // 공유 입력 활동 수집기 (루프 1, 9에서 사용)
        let shared_input_collector = Arc::new(InputActivityCollector::new());

        // ============================================================
        // 1. 모니터링 루프 (1초)
        // ============================================================
        let act_mon = self.activity_monitor.clone();
        let trigger = self.capture_trigger.clone();
        let processor = self.frame_processor.clone();
        let storage1 = self.storage.clone();
        let sqlite1 = self.sqlite_storage.clone();
        let frame_storage1 = self.frame_storage.clone();
        let uploader1 = self.batch_uploader.clone();
        let mut shutdown1 = shutdown_rx.clone();
        let offline1 = offline_mode;
        let session1 = session_id.clone();
        let notif1 = self.notification_manager.clone();
        let focus1 = self.focus_analyzer.clone();
        let input_collector1 = shared_input_collector.clone();

        let monitor_task = tokio::spawn(async move {
            let mut prev_app: Option<String> = None;
            let mut prev_idle_secs: u64 = 0;
            let mut interval = tokio::time::interval(poll);
            let mut idle_tracker = IdleTracker::new(Some(idle_threshold));

            // 창 레이아웃 추적기
            let window_tracker = WindowLayoutTracker::new();
            let input_collector = input_collector1;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // 유휴 상태 확인
                        let idle_info = idle_tracker.check_idle();
                        let prev_state = idle_tracker.previous_state();

                        // 유휴 상태 전환 처리
                        if prev_state == IdleState::Active && idle_info.state == IdleState::Idle {
                            // 유휴 시작
                            match sqlite1.start_idle_period(Utc::now()).await {
                                Ok(id) => {
                                    idle_tracker.set_idle_period_id(Some(id));
                                    debug!("유휴 기간 시작: id={}", id);
                                }
                                Err(e) => warn!("유휴 기간 시작 기록 실패: {e}"),
                            }
                        } else if prev_state == IdleState::Idle && idle_info.state == IdleState::Active {
                            // 유휴 종료
                            if let Some(id) = idle_tracker.idle_period_id() {
                                if let Err(e) = sqlite1.end_idle_period(id, Utc::now()).await {
                                    warn!("유휴 기간 종료 기록 실패: {e}");
                                }
                                idle_tracker.set_idle_period_id(None);
                            }
                            // 세션 리셋 (유휴 복귀 시)
                            if let Some(ref notif) = notif1 {
                                notif.reset_session().await;
                            }
                        }

                        // 유휴 알림 체크
                        if let Some(ref notif) = notif1 {
                            notif.check_idle(idle_info.idle_secs).await;
                        }

                        // 입력 활동 추정 (유휴 시간 변화 기반)
                        input_collector.estimate_from_idle_change(prev_idle_secs, idle_info.idle_secs);
                        prev_idle_secs = idle_info.idle_secs;

                        // 컨텍스트 수집
                        match act_mon.collect_context().await {
                            Ok(ctx) => {
                                let app_name = ctx.active_window.as_ref()
                                    .map(|w| w.app_name.clone())
                                    .unwrap_or_default();
                                let window_title = ctx.active_window.as_ref()
                                    .map(|w| w.title.clone())
                                    .unwrap_or_default();
                                let window_bounds = ctx.active_window.as_ref()
                                    .and_then(|w| w.bounds);

                                // 입력 활동 수집기에 현재 앱 설정
                                input_collector.set_current_app(&app_name);

                                // 창 레이아웃 변경 감지 및 이벤트 저장
                                if let Some(layout_event) = window_tracker.update(&app_name, &window_title, window_bounds) {
                                    let win_event = Event::Window(layout_event);
                                    if let Err(e) = storage1.save_event(&win_event).await {
                                        warn!("창 레이아웃 이벤트 저장 실패: {e}");
                                    }
                                    if !offline1 {
                                        uploader1.enqueue(win_event);
                                    }
                                }

                                // 컨텍스트 이벤트 생성
                                let event = ContextEvent {
                                    app_name: app_name.clone(),
                                    window_title,
                                    prev_app_name: prev_app.clone(),
                                    timestamp: Utc::now(),
                                };

                                // 캡처 트리거 확인
                                {
                                    let mut trig = trigger.lock().await;
                                    if let Some(capture_req) = trig.should_capture(&event) {
                                        let mut proc = processor.lock().await;
                                        match proc.capture_and_process(&capture_req).await {
                                            Ok(frame) => {
                                                debug!("프레임 처리 완료: {:?}", frame.metadata.trigger_type);

                                                // 프레임 파일 저장 (이미지 페이로드가 있는 경우)
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
                                                                        warn!("프레임 파일 저장 실패: {e}");
                                                                        None
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                warn!("Base64 디코딩 실패: {e}");
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

                                                // 프레임 메타데이터 저장 (창 위치 포함)
                                                if let Err(e) = sqlite1.save_frame_metadata_with_bounds(
                                                    &frame.metadata,
                                                    file_path.as_deref(),
                                                    ocr_text.as_deref(),
                                                    window_bounds.as_ref(),
                                                ) {
                                                    warn!("프레임 메타데이터 저장 실패: {e}");
                                                }

                                                // 세션 프레임 카운터 증가
                                                let _ = sqlite1.increment_session_counters(&session1, 0, 1, 0).await;
                                            }
                                            Err(e) => {
                                                warn!("프레임 처리 실패: {e}");
                                            }
                                        }
                                    }
                                }

                                // 이벤트 저장
                                let ctx_event = Event::Context(event);
                                if let Err(e) = storage1.save_event(&ctx_event).await {
                                    warn!("이벤트 저장 실패: {e}");
                                }

                                // 세션 이벤트 카운터 증가
                                let _ = sqlite1.increment_session_counters(&session1, 1, 0, 0).await;

                                // 배치 업로더에 추가 (온라인 모드에서만)
                                if !offline1 {
                                    uploader1.enqueue(ctx_event);
                                }

                                // 앱 전환 시 집중도 분석기에 알림
                                let app_changed = prev_app.as_ref() != Some(&app_name);
                                if app_changed {
                                    if let Some(ref focus) = focus1 {
                                        focus.on_app_switch(&app_name).await;
                                    }
                                }

                                prev_app = Some(app_name);
                            }
                            Err(e) => {
                                warn!("컨텍스트 수집 실패: {e}");
                            }
                        }
                    }
                    _ = shutdown1.changed() => {
                        info!("모니터링 루프 종료");
                        break;
                    }
                }
            }
        });

        // ============================================================
        // 2. 메트릭 루프 (5초)
        // ============================================================
        let sys_mon = self.system_monitor.clone();
        let sqlite2 = self.sqlite_storage.clone();
        let event_tx2 = self.event_tx.clone();
        let mut shutdown2 = shutdown_rx.clone();
        let notif2 = self.notification_manager.clone();

        let metrics_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(metrics_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match sys_mon.collect_metrics().await {
                            Ok(metrics) => {
                                // SQLite 저장
                                if let Err(e) = sqlite2.save_metrics(&metrics).await {
                                    warn!("시스템 메트릭 저장 실패: {e}");
                                }

                                // 메모리 사용률 계산
                                let memory_percent = if metrics.memory_total > 0 {
                                    (metrics.memory_used as f32 / metrics.memory_total as f32) * 100.0
                                } else {
                                    0.0
                                };

                                // 실시간 브로드캐스트 (웹 대시보드용)
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

                                // 고사용량 알림 체크
                                if let Some(ref notif) = notif2 {
                                    notif.check_high_usage(metrics.cpu_usage, memory_percent).await;
                                }
                            }
                            Err(e) => {
                                warn!("시스템 메트릭 수집 실패: {e}");
                            }
                        }
                    }
                    _ = shutdown2.changed() => {
                        info!("메트릭 루프 종료");
                        break;
                    }
                }
            }
        });

        // ============================================================
        // 3. 프로세스 스냅샷 루프 (10초)
        // ============================================================
        let proc_mon = self.process_monitor.clone();
        let sqlite3 = self.sqlite_storage.clone();
        let mut shutdown3 = shutdown_rx.clone();

        let process_task = tokio::spawn(async move {
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
                                    warn!("프로세스 스냅샷 저장 실패: {e}");
                                }
                            }
                            Err(e) => {
                                warn!("프로세스 목록 수집 실패: {e}");
                            }
                        }
                    }
                    _ = shutdown3.changed() => {
                        info!("프로세스 루프 종료");
                        break;
                    }
                }
            }
        });

        // ============================================================
        // 4. 동기화 루프 (10초)
        // ============================================================
        let uploader4 = self.batch_uploader.clone();
        let storage4 = self.storage.clone();
        let frame_storage4 = self.frame_storage.clone();
        let mut shutdown4 = shutdown_rx.clone();
        let offline4 = offline_mode;

        let sync_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(sync);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // 서버 동기화 (온라인 모드에서만)
                        if !offline4 {
                            match uploader4.flush().await {
                                Ok(count) => {
                                    if count > 0 {
                                        debug!("배치 동기화: {count}개 전송");
                                    }
                                }
                                Err(e) => {
                                    warn!("배치 동기화 실패: {e}");
                                }
                            }
                        }

                        // 이벤트 보존 정책 적용
                        if let Err(e) = storage4.enforce_retention().await {
                            warn!("이벤트 보존 정책 적용 실패: {e}");
                        }

                        // 프레임 파일 보존 정책 적용
                        if let Some(ref fs) = frame_storage4 {
                            if let Err(e) = fs.enforce_retention().await {
                                warn!("프레임 보존 정책 적용 실패: {e}");
                            }
                            if let Err(e) = fs.enforce_storage_limit().await {
                                warn!("프레임 용량 제한 적용 실패: {e}");
                            }
                        }
                    }
                    _ = shutdown4.changed() => {
                        info!("동기화 루프 종료");
                        break;
                    }
                }
            }
        });

        // ============================================================
        // 5. 하트비트 루프 (30초, 온라인 모드에서만)
        // ============================================================
        let api = self.api_client.clone();
        let sid = session_id.clone();
        let mut shutdown5 = shutdown_rx.clone();

        let heartbeat_task = tokio::spawn(async move {
            if offline_mode {
                let _ = shutdown5.changed().await;
                return;
            }

            let mut interval = tokio::time::interval(heartbeat);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = api.send_heartbeat(&sid).await {
                            warn!("하트비트 실패: {e}");
                        }
                    }
                    _ = shutdown5.changed() => {
                        info!("하트비트 루프 종료");
                        break;
                    }
                }
            }
        });

        // ============================================================
        // 6. 집계 루프 (1시간)
        // ============================================================
        let sqlite6 = self.sqlite_storage.clone();
        let mut shutdown6 = shutdown_rx.clone();

        let aggregation_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(aggregation);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let now = Utc::now();

                        // 이전 시간의 메트릭 집계
                        let prev_hour = now - ChronoDuration::hours(1);
                        if let Err(e) = sqlite6.aggregate_hourly_metrics(prev_hour).await {
                            warn!("시간별 메트릭 집계 실패: {e}");
                        }

                        // 오래된 상세 메트릭 삭제 (24시간 이전)
                        let metrics_cutoff = now - ChronoDuration::hours(24);
                        if let Err(e) = sqlite6.cleanup_old_metrics(metrics_cutoff).await {
                            warn!("오래된 메트릭 삭제 실패: {e}");
                        }

                        // 오래된 프로세스 스냅샷 삭제 (7일 이전)
                        let process_cutoff = now - ChronoDuration::days(7);
                        if let Err(e) = sqlite6.cleanup_old_process_snapshots(process_cutoff).await {
                            warn!("오래된 프로세스 스냅샷 삭제 실패: {e}");
                        }

                        // 오래된 유휴 기간 삭제 (30일 이전)
                        let idle_cutoff = now - ChronoDuration::days(30);
                        if let Err(e) = sqlite6.cleanup_old_idle_periods(idle_cutoff).await {
                            warn!("오래된 유휴 기간 삭제 실패: {e}");
                        }

                        debug!("집계 및 정리 완료");
                    }
                    _ = shutdown6.changed() => {
                        info!("집계 루프 종료");
                        break;
                    }
                }
            }
        });

        // ============================================================
        // 7. 알림 루프 (1분) - 장시간 작업 체크
        // ============================================================
        let notif7 = self.notification_manager.clone();
        let mut shutdown7 = shutdown_rx.clone();

        let notification_task = tokio::spawn(async move {
            // 알림 관리자가 없으면 바로 종료
            let notif = match notif7 {
                Some(n) => n,
                None => {
                    let _ = shutdown7.changed().await;
                    return;
                }
            };

            let mut interval = tokio::time::interval(Duration::from_secs(60)); // 1분

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // 장시간 작업 알림 체크
                        notif.check_long_session().await;
                    }
                    _ = shutdown7.changed() => {
                        info!("알림 루프 종료");
                        break;
                    }
                }
            }
        });

        // ============================================================
        // 8. 집중도 분석 루프 (1분) - Edge Intelligence
        // ============================================================
        let focus8 = self.focus_analyzer.clone();
        let mut shutdown8 = shutdown_rx.clone();

        let focus_task = tokio::spawn(async move {
            // 집중도 분석기가 없으면 바로 종료
            let focus = match focus8 {
                Some(f) => f,
                None => {
                    let _ = shutdown8.changed().await;
                    return;
                }
            };

            let mut interval = tokio::time::interval(Duration::from_secs(60)); // 1분

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // 주기적 집중도 분석 (제안 생성 포함)
                        focus.analyze_periodic().await;
                    }
                    _ = shutdown8.changed() => {
                        info!("집중도 분석 루프 종료");
                        break;
                    }
                }
            }
        });

        // ============================================================
        // 9. 서버 이벤트 수집 루프 (30초) - ProcessSnapshot + InputActivity
        // ============================================================
        let proc_mon9 = self.process_monitor.clone();
        let storage9 = self.storage.clone();
        let uploader9 = self.batch_uploader.clone();
        let input_collector9 = shared_input_collector.clone();
        let mut shutdown9 = shutdown_rx.clone();
        let offline9 = offline_mode;

        let event_snapshot_task = tokio::spawn(async move {
            // 상세 프로세스 및 입력 활동 수집 간격
            let mut process_interval = tokio::time::interval(detailed_process_interval);
            let mut input_interval = tokio::time::interval(input_activity_interval);
            let mut foreground_pid: Option<u32> = None;

            loop {
                tokio::select! {
                    // 상세 프로세스 스냅샷 (30초)
                    _ = process_interval.tick() => {
                        match proc_mon9.get_detailed_processes(foreground_pid, 10).await {
                            Ok(processes) => {
                                let total = processes.len() as u32;

                                // Foreground PID 업데이트
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
                                    warn!("프로세스 스냅샷 이벤트 저장 실패: {e}");
                                }

                                if !offline9 {
                                    uploader9.enqueue(event);
                                }

                                debug!("상세 프로세스 스냅샷: {}개", total);
                            }
                            Err(e) => {
                                warn!("상세 프로세스 수집 실패: {e}");
                            }
                        }
                    }
                    // 입력 활동 스냅샷 (30초)
                    _ = input_interval.tick() => {
                        let input_event = input_collector9.take_snapshot();

                        // 활동이 있을 때만 저장 (빈 이벤트 방지)
                        if input_event.mouse.click_count > 0
                            || input_event.keyboard.total_keystrokes > 0
                            || input_event.mouse.scroll_count > 0
                        {
                            let event = Event::Input(input_event);
                            if let Err(e) = storage9.save_event(&event).await {
                                warn!("입력 활동 이벤트 저장 실패: {e}");
                            }

                            if !offline9 {
                                uploader9.enqueue(event);
                            }
                        }
                    }
                    _ = shutdown9.changed() => {
                        info!("서버 이벤트 수집 루프 종료");
                        break;
                    }
                }
            }
        });

        // ============================================================
        // 종료 대기
        // ============================================================
        let _ = shutdown_rx.changed().await;
        info!("스케줄러 종료 신호 수신");

        // 세션 종료 기록
        let sqlite_end = self.sqlite_storage.clone();
        if let Err(e) = sqlite_end.end_session(&session_id, Utc::now()).await {
            warn!("세션 종료 기록 실패: {e}");
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

// ============================================================
// 스케줄 기반 조건부 실행 유틸리티
// ============================================================

/// 현재 시각이 활동 시간대에 해당하는지 확인
///
/// `ScheduleConfig.active_hours_enabled`가 false이면 항상 true 반환.
/// true이면 현재 요일/시간이 설정된 범위 내인지 확인.
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

    // 요일 확인
    if !schedule.active_days.contains(&weekday) {
        return false;
    }

    // 시간 확인
    hour >= schedule.active_start_hour && hour < schedule.active_end_hour
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_run_when_disabled() {
        let config = AppConfig::default_config();
        // active_hours_enabled = false → 항상 true
        assert!(should_run_now(&config));
    }

    #[test]
    fn scheduler_config_default() {
        let config = SchedulerConfig::default();
        assert_eq!(config.poll_interval, Duration::from_secs(1));
        assert_eq!(config.metrics_interval, Duration::from_secs(5));
        assert_eq!(config.idle_threshold_secs, 300);
    }
}
