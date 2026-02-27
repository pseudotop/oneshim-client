use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use oneshim_core::config::SandboxProfile;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditLevel {
    None,
    #[default]
    Basic,
    Detailed,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPolicy {
    pub policy_id: String,
    pub process_name: String,
    pub process_hash: Option<String>,
    pub allowed_args: Vec<String>,
    pub requires_sudo: bool,
    pub max_execution_time_ms: u64,
    #[serde(default)]
    pub audit_level: AuditLevel,
    #[serde(default)]
    pub sandbox_profile: Option<SandboxProfile>,
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub allow_network: Option<bool>,
    #[serde(default)]
    pub require_signed_token: bool,
}

#[derive(Debug, Clone)]
pub struct PolicyCache {
    pub policies: Vec<ExecutionPolicy>,
    pub last_updated: DateTime<Utc>,
    pub ttl_seconds: u64,
}

impl Default for PolicyCache {
    fn default() -> Self {
        Self {
            policies: Vec::new(),
            last_updated: Utc::now(),
            ttl_seconds: 300, // 5 min
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}
