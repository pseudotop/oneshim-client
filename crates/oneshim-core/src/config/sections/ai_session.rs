//! AI session configuration — concurrent session limits, timeouts, and audit retention.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSessionConfig {
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_sessions: u32,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    #[serde(default = "default_oneshot_timeout")]
    pub oneshot_timeout_secs: u64,
    #[serde(default = "default_session_timeout")]
    pub session_timeout_secs: u64,
    #[serde(default = "default_audit_retention")]
    pub audit_retention_days: u32,
    #[serde(default = "default_max_attachment")]
    pub max_attachment_bytes: u64,
    #[serde(default = "default_health_check")]
    pub health_check_interval_secs: u64,
    #[serde(default = "default_max_history")]
    pub max_history_turns: u32,
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
}

impl Default for AiSessionConfig {
    fn default() -> Self {
        Self {
            max_concurrent_sessions: default_max_concurrent(),
            idle_timeout_secs: default_idle_timeout(),
            oneshot_timeout_secs: default_oneshot_timeout(),
            session_timeout_secs: default_session_timeout(),
            audit_retention_days: default_audit_retention(),
            max_attachment_bytes: default_max_attachment(),
            health_check_interval_secs: default_health_check(),
            max_history_turns: default_max_history(),
            permission_mode: default_permission_mode(),
        }
    }
}

fn default_max_concurrent() -> u32 {
    3
}
fn default_idle_timeout() -> u64 {
    300
}
fn default_oneshot_timeout() -> u64 {
    60
}
fn default_session_timeout() -> u64 {
    600
}
fn default_audit_retention() -> u32 {
    30
}
fn default_max_attachment() -> u64 {
    10 * 1024 * 1024
}
fn default_health_check() -> u64 {
    30
}
fn default_max_history() -> u32 {
    100
}
fn default_permission_mode() -> String {
    "dontAsk".to_string()
}
