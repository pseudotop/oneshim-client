use chrono::DateTime;
use oneshim_core::error::CoreError;
use oneshim_core::models::work_session::{
    AppCategory, FocusMetrics, Interruption, LocalSuggestion, WorkSession,
};
use oneshim_storage::sqlite::SqliteStorage;

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
pub(crate) struct SuggestionCooldowns {
    pub(super) last_break: Option<DateTime<chrono::Utc>>,
    pub(super) last_focus_time: Option<DateTime<chrono::Utc>>,
    pub(super) last_restore_context: Option<DateTime<chrono::Utc>>,
    pub(super) last_excessive_comm: Option<DateTime<chrono::Utc>>,
    pub(super) last_pattern_detected: Option<DateTime<chrono::Utc>>,
}

#[derive(Debug, Default)]
pub(crate) struct SessionTracker {
    pub(super) active_session_id: Option<i64>,
    pub(super) current_app: Option<String>,
    pub(super) current_category: Option<AppCategory>,
    pub(super) current_app_start: Option<DateTime<chrono::Utc>>,
    pub(super) continuous_deep_work_secs: u64,
    pub(super) pending_interruption_id: Option<i64>,
}
