//! 집중도 분석 및 제안 생성기.
//!
//! 앱 전환 패턴을 분석하여:
//! - 작업 세션 감지/종료
//! - 중단(인터럽션) 추적
//! - 집중도 메트릭 계산
//! - 로컬 제안 생성 + OS 알림 전달

use chrono::{DateTime, Duration, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::work_session::{
    AppCategory, FocusMetrics, Interruption, LocalSuggestion, WorkSession,
};
use oneshim_core::ports::notifier::DesktopNotifier;
use oneshim_storage::sqlite::SqliteStorage;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 집중도 분석 저장소 포트.
///
/// FocusAnalyzer는 구체 저장소 구현 대신 이 포트에 의존한다.
pub trait FocusStorage: Send + Sync {
    fn increment_focus_metrics(
        &self,
        date: &str,
        active_secs: u64,
        deep_work_secs: u64,
        communication_secs: u64,
        context_switches: u32,
        interruption_count: u32,
    ) -> Result<(), CoreError>;

    fn add_deep_work_secs(&self, session_id: i64, secs: u64) -> Result<(), CoreError>;
    fn record_interruption(&self, interruption: &Interruption) -> Result<i64, CoreError>;
    fn increment_work_session_interruption(&self, session_id: i64) -> Result<(), CoreError>;
    fn record_interruption_resume(&self, interruption_id: i64, resumed_to_app: &str)
        -> Result<(), CoreError>;
    fn end_work_session(&self, session_id: i64) -> Result<(), CoreError>;
    fn start_work_session(
        &self,
        primary_app: &str,
        category: AppCategory,
    ) -> Result<WorkSession, CoreError>;
    fn get_or_create_focus_metrics(&self, date: &str) -> Result<FocusMetrics, CoreError>;
    fn update_focus_metrics(&self, date: &str, metrics: &FocusMetrics) -> Result<(), CoreError>;
    fn save_local_suggestion(&self, suggestion: &LocalSuggestion) -> Result<i64, CoreError>;
    fn mark_suggestion_shown(&self, suggestion_id: i64) -> Result<(), CoreError>;
    fn get_pending_interruption(&self) -> Result<Option<Interruption>, CoreError>;
}

impl FocusStorage for SqliteStorage {
    fn increment_focus_metrics(
        &self,
        date: &str,
        active_secs: u64,
        deep_work_secs: u64,
        communication_secs: u64,
        context_switches: u32,
        interruption_count: u32,
    ) -> Result<(), CoreError> {
        SqliteStorage::increment_focus_metrics(
            self,
            date,
            active_secs,
            deep_work_secs,
            communication_secs,
            context_switches,
            interruption_count,
        )
    }

    fn add_deep_work_secs(&self, session_id: i64, secs: u64) -> Result<(), CoreError> {
        SqliteStorage::add_deep_work_secs(self, session_id, secs)
    }

    fn record_interruption(&self, interruption: &Interruption) -> Result<i64, CoreError> {
        SqliteStorage::record_interruption(self, interruption)
    }

    fn increment_work_session_interruption(&self, session_id: i64) -> Result<(), CoreError> {
        SqliteStorage::increment_work_session_interruption(self, session_id)
    }

    fn record_interruption_resume(
        &self,
        interruption_id: i64,
        resumed_to_app: &str,
    ) -> Result<(), CoreError> {
        SqliteStorage::record_interruption_resume(self, interruption_id, resumed_to_app)
    }

    fn end_work_session(&self, session_id: i64) -> Result<(), CoreError> {
        SqliteStorage::end_work_session(self, session_id)
    }

    fn start_work_session(
        &self,
        primary_app: &str,
        category: AppCategory,
    ) -> Result<WorkSession, CoreError> {
        SqliteStorage::start_work_session(self, primary_app, category)
    }

    fn get_or_create_focus_metrics(&self, date: &str) -> Result<FocusMetrics, CoreError> {
        SqliteStorage::get_or_create_focus_metrics(self, date)
    }

    fn update_focus_metrics(&self, date: &str, metrics: &FocusMetrics) -> Result<(), CoreError> {
        SqliteStorage::update_focus_metrics(self, date, metrics)
    }

    fn save_local_suggestion(&self, suggestion: &LocalSuggestion) -> Result<i64, CoreError> {
        SqliteStorage::save_local_suggestion(self, suggestion)
    }

    fn mark_suggestion_shown(&self, suggestion_id: i64) -> Result<(), CoreError> {
        SqliteStorage::mark_suggestion_shown(self, suggestion_id)
    }

    fn get_pending_interruption(&self) -> Result<Option<Interruption>, CoreError> {
        SqliteStorage::get_pending_interruption(self)
    }
}

/// 집중도 분석기 설정
#[derive(Debug, Clone)]
pub struct FocusAnalyzerConfig {
    /// 깊은 작업 최소 지속 시간 (초) - 5분 이상 연속 작업 시 깊은 작업으로 인정
    #[allow(dead_code)]
    pub deep_work_min_secs: u64,
    /// 휴식 권장 연속 작업 시간 (분) - 기본 90분
    pub break_suggestion_mins: u32,
    /// 소통 과다 임계값 (%) - 오늘 소통 비율이 이 값 이상이면 알림
    pub excessive_communication_threshold: f32,
    /// 제안 쿨다운 (초) - 동일 유형 제안 재전송 방지
    pub suggestion_cooldown_secs: u64,
    /// 집중 점수 계산 가중치
    pub focus_score_deep_work_weight: f32,
    pub focus_score_interruption_penalty: f32,
}

impl Default for FocusAnalyzerConfig {
    fn default() -> Self {
        Self {
            deep_work_min_secs: 300,                // 5분
            break_suggestion_mins: 90,              // 90분
            excessive_communication_threshold: 0.4, // 40%
            suggestion_cooldown_secs: 1800,         // 30분
            focus_score_deep_work_weight: 0.7,
            focus_score_interruption_penalty: 0.1,
        }
    }
}

/// 제안 쿨다운 상태
#[derive(Debug, Default)]
struct SuggestionCooldowns {
    last_break: Option<DateTime<Utc>>,
    last_focus_time: Option<DateTime<Utc>>,
    last_restore_context: Option<DateTime<Utc>>,
    last_excessive_comm: Option<DateTime<Utc>>,
}

/// 세션 추적 상태
#[derive(Debug, Default)]
struct SessionTracker {
    /// 현재 활성 작업 세션 ID
    active_session_id: Option<i64>,
    /// 현재 앱
    current_app: Option<String>,
    /// 현재 앱 카테고리
    current_category: Option<AppCategory>,
    /// 현재 앱 시작 시간
    current_app_start: Option<DateTime<Utc>>,
    /// 연속 깊은 작업 시간 (초)
    continuous_deep_work_secs: u64,
    /// 마지막 미복귀 인터럽션 ID
    pending_interruption_id: Option<i64>,
}

/// 집중도 분석기
pub struct FocusAnalyzer {
    config: FocusAnalyzerConfig,
    storage: Arc<dyn FocusStorage>,
    notifier: Arc<dyn DesktopNotifier>,
    /// 세션 추적 상태
    tracker: RwLock<SessionTracker>,
    /// 쿨다운 상태
    cooldowns: RwLock<SuggestionCooldowns>,
}

impl FocusAnalyzer {
    /// 새 분석기 생성
    pub fn new(
        config: FocusAnalyzerConfig,
        storage: Arc<dyn FocusStorage>,
        notifier: Arc<dyn DesktopNotifier>,
    ) -> Self {
        Self {
            config,
            storage,
            notifier,
            tracker: RwLock::new(SessionTracker::default()),
            cooldowns: RwLock::new(SuggestionCooldowns::default()),
        }
    }

    /// 기본 설정으로 생성
    pub fn with_defaults(storage: Arc<dyn FocusStorage>, notifier: Arc<dyn DesktopNotifier>) -> Self {
        Self::new(FocusAnalyzerConfig::default(), storage, notifier)
    }

    /// 앱 전환 이벤트 처리
    ///
    /// 새 앱으로 전환될 때 호출됨. 작업 세션과 인터럽션을 추적.
    pub async fn on_app_switch(&self, new_app: &str) {
        let new_category = AppCategory::from_app_name(new_app);
        let now = Utc::now();
        let today = now.format("%Y-%m-%d").to_string();

        let mut tracker = self.tracker.write().await;

        // 이전 앱 정보
        let prev_app = tracker.current_app.clone();
        let prev_category = tracker.current_category;
        let prev_start = tracker.current_app_start;

        // 동일 앱이면 무시
        if prev_app.as_deref() == Some(new_app) {
            return;
        }

        debug!(
            "앱 전환: {:?} ({:?}) → {} ({:?})",
            prev_app, prev_category, new_app, new_category
        );

        // 1. 이전 앱 시간 누적
        if let (Some(prev_cat), Some(start)) = (prev_category, prev_start) {
            let duration_secs = (now - start).num_seconds().max(0) as u64;

            // 집중도 메트릭 증분 업데이트
            let (deep_work, comm) = if prev_cat.is_deep_work() {
                (duration_secs, 0)
            } else if prev_cat.is_communication() {
                (0, duration_secs)
            } else {
                (0, 0)
            };

            if let Err(e) = self.storage.increment_focus_metrics(
                &today,
                duration_secs, // total_active
                deep_work,
                comm,
                1, // context_switch
                0, // interruption
            ) {
                warn!("집중도 메트릭 증분 실패: {e}");
            }

            // 깊은 작업 시간 누적
            if prev_cat.is_deep_work() {
                tracker.continuous_deep_work_secs += duration_secs;

                // 활성 세션에 deep_work_secs 추가
                if let Some(session_id) = tracker.active_session_id {
                    if let Err(e) = self.storage.add_deep_work_secs(session_id, duration_secs) {
                        warn!("세션 deep_work_secs 추가 실패: {e}");
                    }
                }
            }
        }

        // 2. 인터럽션 감지 (깊은 작업 → 소통)
        if let Some(prev_cat) = prev_category {
            if prev_cat.is_deep_work() && new_category.is_communication() {
                // 인터럽션 기록
                let interruption = Interruption::new(
                    0, // ID는 저장 시 생성
                    prev_app.clone().unwrap_or_default(),
                    new_app.to_string(),
                    None, // snapshot_frame_id (향후 연결)
                );

                match self.storage.record_interruption(&interruption) {
                    Ok(id) => {
                        debug!("인터럽션 기록: id={}", id);
                        tracker.pending_interruption_id = Some(id);

                        // 세션 인터럽션 카운트 증가
                        if let Some(session_id) = tracker.active_session_id {
                            let _ = self.storage.increment_work_session_interruption(session_id);
                        }

                        // 집중도 메트릭 인터럽션 카운트 증가
                        let _ = self.storage.increment_focus_metrics(&today, 0, 0, 0, 0, 1);
                    }
                    Err(e) => warn!("인터럽션 기록 실패: {e}"),
                }
            }
        }

        // 3. 인터럽션 복귀 감지 (소통 → 깊은 작업)
        if let Some(prev_cat) = prev_category {
            if prev_cat.is_communication() && new_category.is_deep_work() {
                if let Some(int_id) = tracker.pending_interruption_id.take() {
                    let _ = self.storage.record_interruption_resume(int_id, new_app);
                    debug!("인터럽션 복귀: id={}", int_id);

                    // 컨텍스트 복원 제안 생성
                    self.maybe_suggest_restore_context(new_app, now).await;
                }
            }
        }

        // 4. 작업 세션 관리
        // 소통 앱으로 전환 시 기존 세션 종료
        if new_category.is_communication() {
            if let Some(session_id) = tracker.active_session_id.take() {
                let _ = self.storage.end_work_session(session_id);
                tracker.continuous_deep_work_secs = 0;
                debug!("작업 세션 종료 (소통 전환): id={}", session_id);
            }
        }
        // 깊은 작업 앱으로 전환 시 새 세션 시작 (없으면)
        else if new_category.is_deep_work() && tracker.active_session_id.is_none() {
            match self.storage.start_work_session(new_app, new_category) {
                Ok(session) => {
                    debug!("작업 세션 시작: id={}, app={}", session.id, new_app);
                    tracker.active_session_id = Some(session.id);
                }
                Err(e) => warn!("작업 세션 시작 실패: {e}"),
            }
        }

        // 5. 현재 앱 업데이트
        tracker.current_app = Some(new_app.to_string());
        tracker.current_category = Some(new_category);
        tracker.current_app_start = Some(now);
    }

    /// 주기적 분석 (1분마다 호출)
    ///
    /// - 집중 점수 계산
    /// - 휴식 제안 확인
    /// - 소통 과다 확인
    pub async fn analyze_periodic(&self) {
        let now = Utc::now();
        let today = now.format("%Y-%m-%d").to_string();

        // 오늘 메트릭 조회
        let metrics = match self.storage.get_or_create_focus_metrics(&today) {
            Ok(m) => m,
            Err(e) => {
                warn!("집중도 메트릭 조회 실패: {e}");
                return;
            }
        };

        // 1. 집중 점수 계산 및 업데이트
        let focus_score = self.calculate_focus_score(&metrics);
        if (focus_score - metrics.focus_score).abs() > 0.01 {
            let mut updated = metrics.clone();
            updated.focus_score = focus_score;
            let _ = self.storage.update_focus_metrics(&today, &updated);
        }

        // 2. 휴식 제안 확인
        self.maybe_suggest_break().await;

        // 3. 소통 과다 확인
        self.maybe_suggest_focus_time(&metrics).await;

        debug!(
            "집중도 분석: score={:.2}, deep_work={}초, comm={}초, interruptions={}",
            focus_score,
            metrics.deep_work_secs,
            metrics.communication_secs,
            metrics.interruption_count
        );
    }

    /// 집중 점수 계산 (0.0 ~ 1.0)
    fn calculate_focus_score(&self, metrics: &FocusMetrics) -> f32 {
        if metrics.total_active_secs == 0 {
            return 0.0;
        }

        let deep_work_ratio = metrics.deep_work_secs as f32 / metrics.total_active_secs as f32;
        let interruption_penalty = (metrics.interruption_count as f32
            * self.config.focus_score_interruption_penalty)
            .min(0.5);

        ((deep_work_ratio * self.config.focus_score_deep_work_weight) - interruption_penalty)
            .clamp(0.0, 1.0)
    }

    /// 휴식 제안 확인 및 발송
    async fn maybe_suggest_break(&self) {
        let tracker = self.tracker.read().await;
        let continuous_mins = (tracker.continuous_deep_work_secs / 60) as u32;

        if continuous_mins < self.config.break_suggestion_mins {
            return;
        }

        // 쿨다운 확인
        if !self.check_cooldown("break").await {
            return;
        }

        // 제안 생성 및 저장
        let suggestion = LocalSuggestion::TakeBreak {
            continuous_work_mins: continuous_mins,
        };

        let suggestion_id = match self.storage.save_local_suggestion(&suggestion) {
            Ok(id) => id,
            Err(e) => {
                warn!("휴식 제안 저장 실패: {e}");
                return;
            }
        };

        // OS 알림 발송
        let title = "☕ 휴식 시간";
        let body = format!(
            "{}분 동안 집중하셨습니다. 잠시 휴식을 취해보세요!",
            continuous_mins
        );

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            warn!("휴식 알림 실패: {e}");
        } else {
            let _ = self.storage.mark_suggestion_shown(suggestion_id);
            info!("휴식 제안 발송: {}분 연속 작업", continuous_mins);
        }

        // 쿨다운 업데이트
        self.update_cooldown("break").await;
    }

    /// 소통 과다 시 집중 시간 제안
    async fn maybe_suggest_focus_time(&self, metrics: &FocusMetrics) {
        let comm_ratio = metrics.communication_ratio();

        if comm_ratio < self.config.excessive_communication_threshold {
            return;
        }

        // 쿨다운 확인
        if !self.check_cooldown("focus_time").await {
            return;
        }

        // 권장 집중 시간 계산 (소통 시간만큼 깊은 작업 추천)
        let suggested_focus_mins = (metrics.communication_secs / 60).max(30) as u32;

        let suggestion = LocalSuggestion::NeedFocusTime {
            communication_ratio: comm_ratio,
            suggested_focus_mins,
        };

        let suggestion_id = match self.storage.save_local_suggestion(&suggestion) {
            Ok(id) => id,
            Err(e) => {
                warn!("집중 시간 제안 저장 실패: {e}");
                return;
            }
        };

        // OS 알림 발송
        let title = "🎯 집중 시간 필요";
        let body = format!(
            "오늘 소통에 {:.0}%의 시간을 사용했습니다. {}분의 집중 시간을 확보해보세요.",
            comm_ratio * 100.0,
            suggested_focus_mins
        );

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            warn!("집중 시간 알림 실패: {e}");
        } else {
            let _ = self.storage.mark_suggestion_shown(suggestion_id);
            info!("집중 시간 제안 발송: 소통 비율 {:.1}%", comm_ratio * 100.0);
        }

        self.update_cooldown("focus_time").await;
    }

    /// 컨텍스트 복원 제안 (인터럽션 복귀 시)
    async fn maybe_suggest_restore_context(&self, app: &str, now: DateTime<Utc>) {
        // 쿨다운 확인
        if !self.check_cooldown("restore_context").await {
            return;
        }

        // 가장 최근 미복귀 인터럽션 조회
        let interruption = match self.storage.get_pending_interruption() {
            Ok(Some(int)) => int,
            _ => return,
        };

        // 30분 이상 지난 인터럽션은 무시
        if (now - interruption.interrupted_at).num_minutes() > 30 {
            return;
        }

        let suggestion = LocalSuggestion::RestoreContext {
            interrupted_app: app.to_string(),
            interrupted_at: interruption.interrupted_at,
            snapshot_frame_id: interruption.snapshot_frame_id.unwrap_or(0),
        };

        let suggestion_id = match self.storage.save_local_suggestion(&suggestion) {
            Ok(id) => id,
            Err(e) => {
                warn!("컨텍스트 복원 제안 저장 실패: {e}");
                return;
            }
        };

        // OS 알림 발송
        let title = "🔄 작업 컨텍스트";
        let duration_mins = (now - interruption.interrupted_at).num_minutes();
        let body = format!(
            "{}에서 {}분 전 중단되었습니다. 이전 작업으로 돌아가시겠습니까?",
            app, duration_mins
        );

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            warn!("컨텍스트 복원 알림 실패: {e}");
        } else {
            let _ = self.storage.mark_suggestion_shown(suggestion_id);
            info!(
                "컨텍스트 복원 제안 발송: {} ({}분 전 중단)",
                app, duration_mins
            );
        }

        self.update_cooldown("restore_context").await;
    }

    /// 쿨다운 확인
    async fn check_cooldown(&self, suggestion_type: &str) -> bool {
        let cooldowns = self.cooldowns.read().await;
        let now = Utc::now();
        let cooldown_duration = Duration::seconds(self.config.suggestion_cooldown_secs as i64);

        let last_time = match suggestion_type {
            "break" => cooldowns.last_break,
            "focus_time" => cooldowns.last_focus_time,
            "restore_context" => cooldowns.last_restore_context,
            "excessive_comm" => cooldowns.last_excessive_comm,
            _ => None,
        };

        match last_time {
            Some(last) => now - last > cooldown_duration,
            None => true,
        }
    }

    /// 쿨다운 업데이트
    async fn update_cooldown(&self, suggestion_type: &str) {
        let mut cooldowns = self.cooldowns.write().await;
        let now = Utc::now();

        match suggestion_type {
            "break" => cooldowns.last_break = Some(now),
            "focus_time" => cooldowns.last_focus_time = Some(now),
            "restore_context" => cooldowns.last_restore_context = Some(now),
            "excessive_comm" => cooldowns.last_excessive_comm = Some(now),
            _ => {}
        }
    }

    /// 유휴 복귀 시 세션 리셋
    #[allow(dead_code)]
    pub async fn on_idle_resume(&self) {
        let mut tracker = self.tracker.write().await;

        // 기존 세션 종료
        if let Some(session_id) = tracker.active_session_id.take() {
            let _ = self.storage.end_work_session(session_id);
        }

        // 상태 리셋
        tracker.continuous_deep_work_secs = 0;
        tracker.pending_interruption_id = None;
        tracker.current_app = None;
        tracker.current_category = None;
        tracker.current_app_start = None;

        debug!("세션 리셋 (유휴 복귀)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oneshim_core::error::CoreError;
    use oneshim_core::models::suggestion::Suggestion;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tempfile::TempDir;

    struct MockNotifier {
        call_count: AtomicU32,
    }

    impl MockNotifier {
        fn new() -> Self {
            Self {
                call_count: AtomicU32::new(0),
            }
        }

        #[allow(dead_code)]
        fn calls(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl DesktopNotifier for MockNotifier {
        async fn show_suggestion(&self, _: &Suggestion) -> Result<(), CoreError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn show_notification(&self, _: &str, _: &str) -> Result<(), CoreError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn show_error(&self, _: &str) -> Result<(), CoreError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    async fn create_test_analyzer() -> (FocusAnalyzer, TempDir, Arc<MockNotifier>) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(
            SqliteStorage::open(&temp_dir.path().join("test.db"), 30)
                .expect("storage creation failed"),
        );
        let notifier = Arc::new(MockNotifier::new());

        let analyzer = FocusAnalyzer::with_defaults(storage, notifier.clone());
        (analyzer, temp_dir, notifier)
    }

    #[tokio::test]
    async fn app_switch_updates_tracker() {
        let (analyzer, _temp, _notifier) = create_test_analyzer().await;

        analyzer.on_app_switch("Visual Studio Code").await;

        let tracker = analyzer.tracker.read().await;
        assert_eq!(tracker.current_app, Some("Visual Studio Code".to_string()));
        assert_eq!(tracker.current_category, Some(AppCategory::Development));
    }

    #[tokio::test]
    async fn deep_work_to_communication_creates_interruption() {
        let (analyzer, _temp, _notifier) = create_test_analyzer().await;

        // 개발 앱 시작
        analyzer.on_app_switch("Visual Studio Code").await;

        // 잠시 대기 (시간 경과 시뮬레이션)
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // 소통 앱으로 전환
        analyzer.on_app_switch("Slack").await;

        let tracker = analyzer.tracker.read().await;
        assert!(tracker.pending_interruption_id.is_some());
    }

    #[tokio::test]
    async fn focus_score_calculation() {
        let (analyzer, _temp, _notifier) = create_test_analyzer().await;

        let now = Utc::now();
        let metrics = FocusMetrics {
            period_start: now,
            period_end: now + Duration::hours(8),
            total_active_secs: 3600,  // 1시간
            deep_work_secs: 2400,     // 40분
            communication_secs: 1200, // 20분
            context_switches: 10,
            interruption_count: 3,
            avg_focus_duration_secs: 600,
            max_focus_duration_secs: 1200,
            focus_score: 0.0,
        };

        let score = analyzer.calculate_focus_score(&metrics);
        // deep_work_ratio = 2400/3600 = 0.667
        // weighted = 0.667 * 0.7 = 0.467
        // penalty = 3 * 0.1 = 0.3
        // score = 0.467 - 0.3 = 0.167
        assert!(score > 0.1 && score < 0.3, "score was {}", score);
    }

    #[tokio::test]
    async fn idle_resume_resets_session() {
        let (analyzer, _temp, _notifier) = create_test_analyzer().await;

        // 세션 시작
        analyzer.on_app_switch("Visual Studio Code").await;

        // 유휴 복귀
        analyzer.on_idle_resume().await;

        let tracker = analyzer.tracker.read().await;
        assert!(tracker.active_session_id.is_none());
        assert!(tracker.current_app.is_none());
        assert_eq!(tracker.continuous_deep_work_secs, 0);
    }

    // --- 추가 테스트 ---

    #[tokio::test]
    async fn focus_score_zero_active_secs() {
        let (analyzer, _temp, _notifier) = create_test_analyzer().await;

        let now = Utc::now();
        let metrics = FocusMetrics {
            period_start: now,
            period_end: now + Duration::hours(8),
            total_active_secs: 0, // 0으로 나누기 방지 확인
            deep_work_secs: 0,
            communication_secs: 0,
            context_switches: 0,
            interruption_count: 0,
            avg_focus_duration_secs: 0,
            max_focus_duration_secs: 0,
            focus_score: 0.0,
        };

        let score = analyzer.calculate_focus_score(&metrics);
        assert_eq!(score, 0.0);
    }

    #[tokio::test]
    async fn focus_score_max_interruptions_clamped() {
        let (analyzer, _temp, _notifier) = create_test_analyzer().await;

        let now = Utc::now();
        let metrics = FocusMetrics {
            period_start: now,
            period_end: now + Duration::hours(8),
            total_active_secs: 3600,
            deep_work_secs: 3600, // 100% 깊은 작업
            communication_secs: 0,
            context_switches: 100,
            interruption_count: 100, // 매우 높은 인터럽션
            avg_focus_duration_secs: 36,
            max_focus_duration_secs: 36,
            focus_score: 0.0,
        };

        let score = analyzer.calculate_focus_score(&metrics);
        // deep_work_ratio = 1.0, weighted = 0.7
        // penalty = min(100 * 0.1, 0.5) = 0.5
        // score = 0.7 - 0.5 = 0.2
        assert!((0.0..=1.0).contains(&score), "score was {}", score);
        assert!((score - 0.2).abs() < 0.01, "score was {}", score);
    }

    #[tokio::test]
    async fn multiple_app_switches_tracking() {
        let (analyzer, _temp, _notifier) = create_test_analyzer().await;

        analyzer.on_app_switch("Visual Studio Code").await;
        {
            let tracker = analyzer.tracker.read().await;
            assert_eq!(tracker.current_app, Some("Visual Studio Code".to_string()));
            assert_eq!(tracker.current_category, Some(AppCategory::Development));
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        analyzer.on_app_switch("Google Chrome").await;
        {
            let tracker = analyzer.tracker.read().await;
            assert_eq!(tracker.current_app, Some("Google Chrome".to_string()));
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        analyzer.on_app_switch("Terminal").await;
        {
            let tracker = analyzer.tracker.read().await;
            assert_eq!(tracker.current_app, Some("Terminal".to_string()));
            assert_eq!(tracker.current_category, Some(AppCategory::Development));
        }
    }

    #[tokio::test]
    async fn same_app_switch_no_change() {
        let (analyzer, _temp, _notifier) = create_test_analyzer().await;

        analyzer.on_app_switch("Visual Studio Code").await;
        analyzer.on_app_switch("Visual Studio Code").await; // 같은 앱

        let tracker = analyzer.tracker.read().await;
        assert_eq!(tracker.current_app, Some("Visual Studio Code".to_string()));
    }
}
