use oneshim_core::error::CoreError;
use oneshim_core::models::suggestion::Suggestion;
use oneshim_core::models::work_session::{AppCategory, FocusMetrics, Interruption, WorkSession};
use oneshim_core::ports::focus_storage::FocusStorage;

use super::SqliteStorage;

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

    fn save_rule_suggestion(&self, suggestion: &Suggestion) -> Result<String, CoreError> {
        SqliteStorage::save_rule_suggestion_sync(self, suggestion)
    }

    fn mark_suggestion_shown_by_id(&self, suggestion_id: &str) -> Result<(), CoreError> {
        SqliteStorage::mark_unified_suggestion_shown(self, suggestion_id)
    }

    fn get_pending_interruption(&self) -> Result<Option<Interruption>, CoreError> {
        SqliteStorage::get_pending_interruption(self)
    }
}
