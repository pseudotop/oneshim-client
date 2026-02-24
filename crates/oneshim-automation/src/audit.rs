//!

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::policy::AuditLevel;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditStatus {
    Started,
    Completed,
    Failed,
    Denied,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub entry_id: String,
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub command_id: String,
    pub action_type: String,
    pub status: AuditStatus,
    pub details: Option<String>,
    pub execution_time_ms: Option<u64>,
}

pub struct AuditLogger {
    buffer: VecDeque<AuditEntry>,
    max_buffer_size: usize,
    batch_size: usize,
}

impl AuditLogger {
    pub fn new(max_buffer_size: usize, batch_size: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(max_buffer_size),
            max_buffer_size,
            batch_size,
        }
    }

    pub fn log_start(&mut self, command_id: &str, session_id: &str, action_type: &str) {
        self.push_entry(
            command_id,
            session_id,
            action_type,
            AuditStatus::Started,
            None,
        );
    }

    pub fn log_complete(&mut self, command_id: &str, session_id: &str, details: &str) {
        self.push_entry(
            command_id,
            session_id,
            "complete",
            AuditStatus::Completed,
            Some(details.to_string()),
        );
    }

    pub fn log_denied(&mut self, command_id: &str, session_id: &str, action_type: &str) {
        self.push_entry(
            command_id,
            session_id,
            action_type,
            AuditStatus::Denied,
            None,
        );
    }

    pub fn log_failed(&mut self, command_id: &str, session_id: &str, error: &str) {
        self.push_entry(
            command_id,
            session_id,
            "failed",
            AuditStatus::Failed,
            Some(error.to_string()),
        );
    }

    ///
    pub fn log_event(&mut self, action_type: &str, session_id: &str, details: &str) {
        self.push_entry(
            &format!("event-{}", uuid::Uuid::new_v4()),
            session_id,
            action_type,
            AuditStatus::Completed,
            Some(details.to_string()),
        );
    }

    pub fn log_start_if(
        &mut self,
        level: AuditLevel,
        command_id: &str,
        session_id: &str,
        action_type: &str,
    ) {
        if matches!(level, AuditLevel::None) {
            return;
        }
        self.push_entry(
            command_id,
            session_id,
            action_type,
            AuditStatus::Started,
            None,
        );
    }

    pub fn log_complete_with_time(
        &mut self,
        level: AuditLevel,
        command_id: &str,
        session_id: &str,
        details: &str,
        execution_time_ms: u64,
    ) {
        if matches!(level, AuditLevel::None) {
            return;
        }
        self.push_entry_with_time(
            command_id,
            session_id,
            "complete",
            AuditStatus::Completed,
            Some(details.to_string()),
            Some(execution_time_ms),
        );
    }

    pub fn log_timeout(&mut self, command_id: &str, session_id: &str, timeout_ms: u64) {
        self.push_entry_with_time(
            command_id,
            session_id,
            "timeout",
            AuditStatus::Timeout,
            Some(format!("{}ms 초과", timeout_ms)),
            Some(timeout_ms),
        );
    }

    pub fn has_pending_batch(&self) -> bool {
        self.buffer.len() >= self.batch_size
    }

    pub fn pending_count(&self) -> usize {
        self.buffer.len()
    }

    pub fn drain_batch(&mut self) -> Vec<AuditEntry> {
        let count = self.buffer.len().min(self.batch_size);
        self.buffer.drain(..count).collect()
    }

    pub fn drain_all(&mut self) -> Vec<AuditEntry> {
        self.buffer.drain(..).collect()
    }

    pub fn recent_entries(&self, limit: usize) -> Vec<AuditEntry> {
        self.buffer.iter().rev().take(limit).cloned().collect()
    }

    pub fn entries_by_status(&self, status: &AuditStatus, limit: usize) -> Vec<AuditEntry> {
        self.buffer
            .iter()
            .rev()
            .filter(|e| &e.status == status)
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn stats(&self) -> (usize, usize, usize, usize, usize) {
        let mut success = 0;
        let mut failed = 0;
        let mut denied = 0;
        let mut timeout = 0;
        for entry in &self.buffer {
            match entry.status {
                AuditStatus::Completed => success += 1,
                AuditStatus::Failed => failed += 1,
                AuditStatus::Denied => denied += 1,
                AuditStatus::Timeout => timeout += 1,
                AuditStatus::Started => {}
            }
        }
        let total = success + failed + denied + timeout;
        (total, success, failed, denied, timeout)
    }

    fn push_entry(
        &mut self,
        command_id: &str,
        session_id: &str,
        action_type: &str,
        status: AuditStatus,
        details: Option<String>,
    ) {
        if self.buffer.len() >= self.max_buffer_size {
            self.buffer.pop_front();
            tracing::warn!("— delete");
        }

        let entry = AuditEntry {
            entry_id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            session_id: session_id.to_string(),
            command_id: command_id.to_string(),
            action_type: action_type.to_string(),
            status,
            details,
            execution_time_ms: None,
        };

        self.buffer.push_back(entry);
    }

    fn push_entry_with_time(
        &mut self,
        command_id: &str,
        session_id: &str,
        action_type: &str,
        status: AuditStatus,
        details: Option<String>,
        execution_time_ms: Option<u64>,
    ) {
        if self.buffer.len() >= self.max_buffer_size {
            self.buffer.pop_front();
            tracing::warn!("— delete");
        }

        let entry = AuditEntry {
            entry_id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            session_id: session_id.to_string(),
            command_id: command_id.to_string(),
            action_type: action_type.to_string(),
            status,
            details,
            execution_time_ms,
        };

        self.buffer.push_back(entry);
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new(1000, 50)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_and_drain() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_start("cmd-001", "sess-001", "MouseClick");
        logger.log_complete("cmd-001", "sess-001", "Success");

        assert_eq!(logger.pending_count(), 2);
        assert!(!logger.has_pending_batch()); // 2 < 10

        let entries = logger.drain_all();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].status, AuditStatus::Started);
        assert_eq!(entries[1].status, AuditStatus::Completed);
    }

    #[test]
    fn buffer_overflow_evicts_oldest() {
        let mut logger = AuditLogger::new(3, 2);
        logger.log_start("cmd-1", "s", "a");
        logger.log_start("cmd-2", "s", "b");
        logger.log_start("cmd-3", "s", "c");
        logger.log_start("cmd-4", "s", "d");

        assert_eq!(logger.pending_count(), 3);
        let entries = logger.drain_all();
        assert_eq!(entries[0].command_id, "cmd-2");
    }

    #[test]
    fn drain_batch_partial() {
        let mut logger = AuditLogger::new(100, 2);
        logger.log_start("cmd-1", "s", "a");
        logger.log_start("cmd-2", "s", "b");
        logger.log_start("cmd-3", "s", "c");

        assert!(logger.has_pending_batch());
        let batch = logger.drain_batch();
        assert_eq!(batch.len(), 2);
        assert_eq!(logger.pending_count(), 1);
    }

    #[test]
    fn audit_entry_serde() {
        let entry = AuditEntry {
            entry_id: "e-001".to_string(),
            timestamp: Utc::now(),
            session_id: "sess-001".to_string(),
            command_id: "cmd-001".to_string(),
            action_type: "MouseClick".to_string(),
            status: AuditStatus::Completed,
            details: Some("Success".to_string()),
            execution_time_ms: Some(150),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deser: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.entry_id, "e-001");
        assert_eq!(deser.status, AuditStatus::Completed);
    }

    #[test]
    fn log_start_if_skips_on_none() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_start_if(AuditLevel::None, "cmd-1", "sess-1", "KeyPress");
        assert_eq!(logger.pending_count(), 0);
    }

    #[test]
    fn log_start_if_records_on_basic() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_start_if(AuditLevel::Basic, "cmd-1", "sess-1", "KeyPress");
        assert_eq!(logger.pending_count(), 1);
        let entries = logger.drain_all();
        assert_eq!(entries[0].status, AuditStatus::Started);
    }

    #[test]
    fn log_complete_with_time_records_execution_ms() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_complete_with_time(AuditLevel::Detailed, "cmd-1", "sess-1", "OK", 150);
        let entries = logger.drain_all();
        assert_eq!(entries[0].execution_time_ms, Some(150));
        assert_eq!(entries[0].status, AuditStatus::Completed);
    }

    #[test]
    fn log_timeout_records_timeout_entry() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_timeout("cmd-1", "sess-1", 5000);
        let entries = logger.drain_all();
        assert_eq!(entries[0].status, AuditStatus::Timeout);
        assert_eq!(entries[0].execution_time_ms, Some(5000));
        assert!(entries[0].details.as_ref().unwrap().contains("5000ms"));
    }

    #[test]
    fn recent_entries_nondestructive() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_start("cmd-1", "s", "a");
        logger.log_complete("cmd-2", "s", "ok");
        logger.log_failed("cmd-3", "s", "err");

        let recent = logger.recent_entries(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].command_id, "cmd-3");
        assert_eq!(recent[1].command_id, "cmd-2");
        assert_eq!(logger.pending_count(), 3);
    }

    #[test]
    fn entries_by_status_filter() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_start("cmd-1", "s", "a");
        logger.log_complete("cmd-2", "s", "ok");
        logger.log_denied("cmd-3", "s", "x");
        logger.log_complete("cmd-4", "s", "ok2");

        let completed = logger.entries_by_status(&AuditStatus::Completed, 10);
        assert_eq!(completed.len(), 2);
        let denied = logger.entries_by_status(&AuditStatus::Denied, 10);
        assert_eq!(denied.len(), 1);
    }

    #[test]
    fn stats_aggregation() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_start("cmd-1", "s", "a");
        logger.log_complete("cmd-2", "s", "ok");
        logger.log_failed("cmd-3", "s", "err");
        logger.log_denied("cmd-4", "s", "x");
        logger.log_timeout("cmd-5", "s", 5000);
        logger.log_complete("cmd-6", "s", "ok2");

        let (total, success, failed, denied, timeout) = logger.stats();
        assert_eq!(total, 5); // Started is excluded
        assert_eq!(success, 2);
        assert_eq!(failed, 1);
        assert_eq!(denied, 1);
        assert_eq!(timeout, 1);
    }


    #[test]
    fn log_complete_with_time_skips_on_none_level() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_complete_with_time(AuditLevel::None, "cmd-1", "sess-1", "OK", 100);
        assert_eq!(logger.pending_count(), 0);
    }

    #[test]
    fn log_denied_has_correct_status() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_denied("cmd-1", "sess-1", "MouseClick");
        let entries = logger.drain_all();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, AuditStatus::Denied);
        assert_eq!(entries[0].action_type, "MouseClick");
    }

    #[test]
    fn log_failed_includes_error_details() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_failed("cmd-1", "sess-1", "connection failure: timeout");
        let entries = logger.drain_all();
        assert_eq!(entries[0].status, AuditStatus::Failed);
        assert_eq!(entries[0].details.as_ref().unwrap(), "connection failure: timeout");
    }

    #[test]
    fn log_event_records_policy_event() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_event(
            "policy.scene_action_override.applied",
            "settings",
            "override=true reason=calibration",
        );

        let entries = logger.drain_all();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, AuditStatus::Completed);
        assert_eq!(
            entries[0].action_type,
            "policy.scene_action_override.applied"
        );
        assert_eq!(entries[0].session_id, "settings");
    }

    #[test]
    fn default_constructor_values() {
        let logger = AuditLogger::default();
        assert_eq!(logger.pending_count(), 0);
        assert!(!logger.has_pending_batch());
    }

    #[test]
    fn recent_entries_with_zero_limit() {
        let mut logger = AuditLogger::new(100, 10);
        logger.log_start("cmd-1", "s", "a");
        logger.log_start("cmd-2", "s", "b");
        let recent = logger.recent_entries(0);
        assert!(recent.is_empty());
    }

    #[test]
    fn entries_by_status_empty_buffer() {
        let logger = AuditLogger::new(100, 10);
        let results = logger.entries_by_status(&AuditStatus::Completed, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn stats_on_empty_logger() {
        let logger = AuditLogger::new(100, 10);
        let (total, success, failed, denied, timeout) = logger.stats();
        assert_eq!(total, 0);
        assert_eq!(success, 0);
        assert_eq!(failed, 0);
        assert_eq!(denied, 0);
        assert_eq!(timeout, 0);
    }

    #[test]
    fn drain_batch_on_empty_logger() {
        let mut logger = AuditLogger::new(100, 10);
        let batch = logger.drain_batch();
        assert!(batch.is_empty());
    }
}
