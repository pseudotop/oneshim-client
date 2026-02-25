//!

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

use crate::workflow_intelligence::{PlaybookSignal, WorkflowIntelligence};

///
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
    fn record_interruption_resume(
        &self,
        interruption_id: i64,
        resumed_to_app: &str,
    ) -> Result<(), CoreError>;
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

#[derive(Debug, Clone)]
pub struct FocusAnalyzerConfig {
    #[allow(dead_code)]
    pub deep_work_min_secs: u64,
    pub break_suggestion_mins: u32,
    pub excessive_communication_threshold: f32,
    pub suggestion_cooldown_secs: u64,
    pub focus_score_deep_work_weight: f32,
    pub focus_score_interruption_penalty: f32,
    pub workflow_split_idle_secs: u64,
    pub playbook_min_relevance: f32,
    pub playbook_stale_flush_secs: u64,
}

impl Default for FocusAnalyzerConfig {
    fn default() -> Self {
        Self {
            deep_work_min_secs: 300,                // 5 min
            break_suggestion_mins: 90,              // 90 min
            excessive_communication_threshold: 0.4, // 40%
            suggestion_cooldown_secs: 1800,         // 30 min
            focus_score_deep_work_weight: 0.7,
            focus_score_interruption_penalty: 0.1,
            workflow_split_idle_secs: 300, // 5 min
            playbook_min_relevance: 0.35,
            playbook_stale_flush_secs: 900, // 15 min
        }
    }
}

#[derive(Debug, Default)]
struct SuggestionCooldowns {
    last_break: Option<DateTime<Utc>>,
    last_focus_time: Option<DateTime<Utc>>,
    last_restore_context: Option<DateTime<Utc>>,
    last_excessive_comm: Option<DateTime<Utc>>,
    last_pattern_detected: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
struct SessionTracker {
    active_session_id: Option<i64>,
    current_app: Option<String>,
    current_category: Option<AppCategory>,
    current_app_start: Option<DateTime<Utc>>,
    continuous_deep_work_secs: u64,
    pending_interruption_id: Option<i64>,
}

pub struct FocusAnalyzer {
    config: FocusAnalyzerConfig,
    storage: Arc<dyn FocusStorage>,
    notifier: Arc<dyn DesktopNotifier>,
    tracker: RwLock<SessionTracker>,
    cooldowns: RwLock<SuggestionCooldowns>,
    workflow_intelligence: RwLock<WorkflowIntelligence>,
}

impl FocusAnalyzer {
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
            workflow_intelligence: RwLock::new(WorkflowIntelligence::default()),
        }
    }

    pub fn with_defaults(
        storage: Arc<dyn FocusStorage>,
        notifier: Arc<dyn DesktopNotifier>,
    ) -> Self {
        Self::new(FocusAnalyzerConfig::default(), storage, notifier)
    }

    ///
    #[allow(dead_code)]
    pub async fn on_app_switch(&self, new_app: &str) {
        self.on_app_switch_with_context(new_app, "", None).await;
    }

    ///
    pub async fn on_app_switch_with_context(
        &self,
        new_app: &str,
        window_title: &str,
        ocr_hint: Option<&str>,
    ) {
        let new_category = AppCategory::from_app_name(new_app);
        let now = Utc::now();
        let today = now.format("%Y-%m-%d").to_string();

        let mut previous_usage: Option<(String, AppCategory, u64)> = None;
        let mut should_suggest_restore = false;

        {
            let mut tracker = self.tracker.write().await;

            let prev_app = tracker.current_app.clone();
            let prev_category = tracker.current_category;
            let prev_start = tracker.current_app_start;

            if prev_app.as_deref() == Some(new_app) {
                return;
            }

            debug!(
                "앱 전환: {:?} ({:?}) → {} ({:?})",
                prev_app, prev_category, new_app, new_category
            );

            if let (Some(prev_app_name), Some(prev_cat), Some(start)) =
                (prev_app, prev_category, prev_start)
            {
                let duration_secs = (now - start).num_seconds().max(0) as u64;
                previous_usage = Some((prev_app_name.clone(), prev_cat, duration_secs));

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
                    warn!("in progress min failure: {e}");
                }

                if prev_cat.is_deep_work() {
                    tracker.continuous_deep_work_secs += duration_secs;

                    if let Some(session_id) = tracker.active_session_id {
                        if let Err(e) = self.storage.add_deep_work_secs(session_id, duration_secs) {
                            warn!("session deep_work_secs add failure: {e}");
                        }
                    }
                }

                if prev_cat.is_deep_work() && new_category.is_communication() {
                    let interruption = Interruption::new(
                        0, // id assigned on persist
                        prev_app_name,
                        new_app.to_string(),
                        None, // snapshot_frame_id (future linkage)
                    );

                    match self.storage.record_interruption(&interruption) {
                        Ok(id) => {
                            debug!("record: id={}", id);
                            tracker.pending_interruption_id = Some(id);

                            if let Some(session_id) = tracker.active_session_id {
                                let _ =
                                    self.storage.increment_work_session_interruption(session_id);
                            }

                            let _ = self.storage.increment_focus_metrics(&today, 0, 0, 0, 0, 1);
                        }
                        Err(e) => warn!("record failure: {e}"),
                    }
                }

                if prev_cat.is_communication() && new_category.is_deep_work() {
                    if let Some(int_id) = tracker.pending_interruption_id.take() {
                        let _ = self.storage.record_interruption_resume(int_id, new_app);
                        debug!(": id={}", int_id);
                        should_suggest_restore = true;
                    }
                }
            }

            if new_category.is_communication() {
                if let Some(session_id) = tracker.active_session_id.take() {
                    let _ = self.storage.end_work_session(session_id);
                    tracker.continuous_deep_work_secs = 0;
                    debug!("session ended ( switch): id={}", session_id);
                }
            } else if new_category.is_deep_work() && tracker.active_session_id.is_none() {
                match self.storage.start_work_session(new_app, new_category) {
                    Ok(session) => {
                        debug!("session started: id={}, app={}", session.id, new_app);
                        tracker.active_session_id = Some(session.id);
                    }
                    Err(e) => warn!("session started failure: {e}"),
                }
            }

            tracker.current_app = Some(new_app.to_string());
            tracker.current_category = Some(new_category);
            tracker.current_app_start = Some(now);
        }

        let playbook_signal = {
            let mut intelligence = self.workflow_intelligence.write().await;

            if let Some((prev_app, prev_cat, duration_secs)) = previous_usage {
                let score = intelligence.update_usage(&prev_app, prev_cat, duration_secs, now);
                debug!(
                    app = %prev_app,
                    category = ?prev_cat,
                    duration_secs,
                    relevance = score,
                    "앱 relevance update"
                );
            }

            let _ = intelligence.touch_app(new_app, new_category, now);
            intelligence.advance_workflow(
                new_app,
                new_category,
                window_title,
                ocr_hint,
                now,
                self.config.playbook_min_relevance,
                self.config.workflow_split_idle_secs,
            )
        };

        if let Some(signal) = playbook_signal {
            self.maybe_suggest_pattern_detected(signal).await;
        }

        if should_suggest_restore {
            self.maybe_suggest_restore_context(new_app, now).await;
        }
    }

    ///
    pub async fn analyze_periodic(&self) {
        let now = Utc::now();
        let today = now.format("%Y-%m-%d").to_string();

        let metrics = match self.storage.get_or_create_focus_metrics(&today) {
            Ok(m) => m,
            Err(e) => {
                warn!("in progress query failure: {e}");
                return;
            }
        };

        let focus_score = self.calculate_focus_score(&metrics);
        if (focus_score - metrics.focus_score).abs() > 0.01 {
            let mut updated = metrics.clone();
            updated.focus_score = focus_score;
            let _ = self.storage.update_focus_metrics(&today, &updated);
        }

        self.maybe_suggest_break().await;

        self.maybe_suggest_focus_time(&metrics).await;

        let playbook_signal = {
            let mut intelligence = self.workflow_intelligence.write().await;
            intelligence.flush_stale_segment(
                now,
                self.config.playbook_min_relevance,
                self.config.playbook_stale_flush_secs,
            )
        };
        if let Some(signal) = playbook_signal {
            self.maybe_suggest_pattern_detected(signal).await;
        }

        debug!(
            "집중도 분석: score={:.2}, deep_work={}초, comm={}초, interruptions={}",
            focus_score,
            metrics.deep_work_secs,
            metrics.communication_secs,
            metrics.interruption_count
        );
    }

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

    async fn maybe_suggest_break(&self) {
        let tracker = self.tracker.read().await;
        let continuous_mins = (tracker.continuous_deep_work_secs / 60) as u32;

        if continuous_mins < self.config.break_suggestion_mins {
            return;
        }

        if !self.check_cooldown("break").await {
            return;
        }

        let suggestion = LocalSuggestion::TakeBreak {
            continuous_work_mins: continuous_mins,
        };

        let suggestion_id = match self.storage.save_local_suggestion(&suggestion) {
            Ok(id) => id,
            Err(e) => {
                warn!("suggestion save failure: {e}");
                return;
            }
        };

        let title = "☕ 휴식 시간";
        let body = format!(
            "{}분 동안 집중하셨습니다. 잠시 휴식을 취해보세요!",
            continuous_mins
        );

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            warn!("notification failure: {e}");
        } else {
            let _ = self.storage.mark_suggestion_shown(suggestion_id);
            info!("suggestion sent: {}min consecutive", continuous_mins);
        }

        self.update_cooldown("break").await;
    }

    async fn maybe_suggest_focus_time(&self, metrics: &FocusMetrics) {
        let comm_ratio = metrics.communication_ratio();

        if comm_ratio < self.config.excessive_communication_threshold {
            return;
        }

        if !self.check_cooldown("focus_time").await {
            return;
        }

        let suggested_focus_mins = (metrics.communication_secs / 60).max(30) as u32;

        let suggestion = LocalSuggestion::NeedFocusTime {
            communication_ratio: comm_ratio,
            suggested_focus_mins,
        };

        let suggestion_id = match self.storage.save_local_suggestion(&suggestion) {
            Ok(id) => id,
            Err(e) => {
                warn!("in progress hour suggestion save failure: {e}");
                return;
            }
        };

        let title = "🎯 집중 시간 필요";
        let body = format!(
            "오늘 소통에 {:.0}%의 시간을 사용했습니다. {}분의 집중 시간을 확보해보세요.",
            comm_ratio * 100.0,
            suggested_focus_mins
        );

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            warn!("in progress hour notification failure: {e}");
        } else {
            let _ = self.storage.mark_suggestion_shown(suggestion_id);
            info!(
                "in progress hour suggestion sent: {:.1}%",
                comm_ratio * 100.0
            );
        }

        self.update_cooldown("focus_time").await;
    }

    async fn maybe_suggest_restore_context(&self, app: &str, now: DateTime<Utc>) {
        if !self.check_cooldown("restore_context").await {
            return;
        }

        let interruption = match self.storage.get_pending_interruption() {
            Ok(Some(int)) => int,
            _ => return,
        };

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
                warn!("context restore suggestion save failure: {e}");
                return;
            }
        };

        let title = "🔄 작업 context";
        let duration_mins = (now - interruption.interrupted_at).num_minutes();
        let body = format!(
            "{}에서 {}분 전 중단되었습니다. 이전 작업으로 돌아가시겠습니까?",
            app, duration_mins
        );

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            warn!("context restore notification failure: {e}");
        } else {
            let _ = self.storage.mark_suggestion_shown(suggestion_id);
            info!(
                "context 복원 suggestion 발송: {} ({}분 전 중단)",
                app, duration_mins
            );
        }

        self.update_cooldown("restore_context").await;
    }

    async fn maybe_suggest_pattern_detected(&self, signal: PlaybookSignal) {
        if !self.check_cooldown("pattern_detected").await {
            return;
        }

        let suggestion = LocalSuggestion::PatternDetected {
            pattern_description: signal.description.clone(),
            confidence: signal.confidence,
        };

        let suggestion_id = match self.storage.save_local_suggestion(&suggestion) {
            Ok(id) => id,
            Err(e) => {
                warn!("suggestion save failure: {e}");
                return;
            }
        };

        let title = "🧭 반복 플레이북";
        let confidence_percent = (signal.confidence * 100.0).round() as i32;
        let body = format!(
            "{} (confidence {}%)",
            signal.description, confidence_percent
        );

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            warn!("suggestion notification failure: {e}");
            return;
        }

        let _ = self.storage.mark_suggestion_shown(suggestion_id);
        info!(
            confidence = signal.confidence,
            description = %signal.description,
            "플레이북 패턴 suggestion 발송"
        );
        self.update_cooldown("pattern_detected").await;
    }

    async fn check_cooldown(&self, suggestion_type: &str) -> bool {
        let cooldowns = self.cooldowns.read().await;
        let now = Utc::now();
        let cooldown_duration = Duration::seconds(self.config.suggestion_cooldown_secs as i64);

        let last_time = match suggestion_type {
            "break" => cooldowns.last_break,
            "focus_time" => cooldowns.last_focus_time,
            "restore_context" => cooldowns.last_restore_context,
            "excessive_comm" => cooldowns.last_excessive_comm,
            "pattern_detected" => cooldowns.last_pattern_detected,
            _ => None,
        };

        match last_time {
            Some(last) => now - last > cooldown_duration,
            None => true,
        }
    }

    async fn update_cooldown(&self, suggestion_type: &str) {
        let mut cooldowns = self.cooldowns.write().await;
        let now = Utc::now();

        match suggestion_type {
            "break" => cooldowns.last_break = Some(now),
            "focus_time" => cooldowns.last_focus_time = Some(now),
            "restore_context" => cooldowns.last_restore_context = Some(now),
            "excessive_comm" => cooldowns.last_excessive_comm = Some(now),
            "pattern_detected" => cooldowns.last_pattern_detected = Some(now),
            _ => {}
        }
    }

    #[allow(dead_code)]
    pub async fn on_idle_resume(&self) {
        let now = Utc::now();
        let playbook_signal = {
            let mut intelligence = self.workflow_intelligence.write().await;
            intelligence.flush_stale_segment(now, self.config.playbook_min_relevance, 0)
        };

        let mut tracker = self.tracker.write().await;

        if let Some(session_id) = tracker.active_session_id.take() {
            let _ = self.storage.end_work_session(session_id);
        }

        tracker.continuous_deep_work_secs = 0;
        tracker.pending_interruption_id = None;
        tracker.current_app = None;
        tracker.current_category = None;
        tracker.current_app_start = None;

        debug!("session reset (idle )");

        if let Some(signal) = playbook_signal {
            self.maybe_suggest_pattern_detected(signal).await;
        }
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

        analyzer.on_app_switch("Visual Studio Code").await;

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

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
            total_active_secs: 3600,  // 1 hour
            deep_work_secs: 2400,     // 40 min
            communication_secs: 1200, // 20 min
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

        analyzer.on_app_switch("Visual Studio Code").await;

        analyzer.on_idle_resume().await;

        let tracker = analyzer.tracker.read().await;
        assert!(tracker.active_session_id.is_none());
        assert!(tracker.current_app.is_none());
        assert_eq!(tracker.continuous_deep_work_secs, 0);
    }

    #[tokio::test]
    async fn focus_score_zero_active_secs() {
        let (analyzer, _temp, _notifier) = create_test_analyzer().await;

        let now = Utc::now();
        let metrics = FocusMetrics {
            period_start: now,
            period_end: now + Duration::hours(8),
            total_active_secs: 0, // zero-safe path
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
            deep_work_secs: 3600, // 100%
            communication_secs: 0,
            context_switches: 100,
            interruption_count: 100,
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
        analyzer.on_app_switch("Visual Studio Code").await; // app
        let tracker = analyzer.tracker.read().await;
        assert_eq!(tracker.current_app, Some("Visual Studio Code".to_string()));
    }
}
