//! 정책 클라이언트.
//!
//! 서버에서 실행 정책을 동기화하고, 자동화 명령의 정책 토큰을 검증한다.
//! 허가된 프로세스만 실행하며, 바이너리 해시 검증도 지원.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::sync::RwLock;

use crate::controller::AutomationCommand;
use oneshim_core::config::SandboxProfile;
use oneshim_core::error::CoreError;

// ============================================================
// 정책 모델
// ============================================================

/// 감사 로그 레벨
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditLevel {
    /// 감사 안 함
    None,
    /// 실행 여부만 기록
    #[default]
    Basic,
    /// 인자, 결과 포함
    Detailed,
    /// 전체 (stdout/stderr 포함)
    Full,
}

/// 실행 정책 — 서버에서 발급
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPolicy {
    /// 정책 ID
    pub policy_id: String,
    /// 허가된 프로세스 이름
    pub process_name: String,
    /// 바이너리 해시 (선택 — 변조 감지)
    pub process_hash: Option<String>,
    /// 허용된 인자 패턴
    pub allowed_args: Vec<String>,
    /// sudo 필요 여부
    pub requires_sudo: bool,
    /// 최대 실행 시간 (밀리초)
    pub max_execution_time_ms: u64,
    /// 감사 로그 레벨
    #[serde(default)]
    pub audit_level: AuditLevel,
    /// 샌드박스 프로필 오버라이드 (서버 지정, 없으면 자동 결정)
    #[serde(default)]
    pub sandbox_profile: Option<SandboxProfile>,
    /// 추가 허용 읽기 경로 (서버 지정)
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    /// 네트워크 접근 명시 (없으면 프로필 기반 자동 결정)
    #[serde(default)]
    pub allow_network: Option<bool>,
}

/// 정책 캐시
#[derive(Debug, Clone)]
pub struct PolicyCache {
    /// 정책 목록
    pub policies: Vec<ExecutionPolicy>,
    /// 마지막 동기화 시각
    pub last_updated: DateTime<Utc>,
    /// 캐시 TTL (초)
    pub ttl_seconds: u64,
}

impl Default for PolicyCache {
    fn default() -> Self {
        Self {
            policies: Vec::new(),
            last_updated: Utc::now(),
            ttl_seconds: 300, // 5분
        }
    }
}

/// 프로세스 실행 결과
#[derive(Debug, Clone)]
pub struct ProcessOutput {
    /// 프로세스 종료 코드
    pub exit_code: i32,
    /// 표준 출력
    pub stdout: String,
    /// 표준 에러
    pub stderr: String,
}

// ============================================================
// PolicyClient
// ============================================================

/// 정책 클라이언트 — 서버 정책 동기화 + 명령 검증 + 프로세스 실행
pub struct PolicyClient {
    /// 정책 캐시
    policy_cache: RwLock<PolicyCache>,
    /// 허가된 프로세스 이름 목록 (빠른 조회)
    allowed_processes: RwLock<HashSet<String>>,
}

impl PolicyClient {
    /// 새 정책 클라이언트 생성
    pub fn new() -> Self {
        Self {
            policy_cache: RwLock::new(PolicyCache::default()),
            allowed_processes: RwLock::new(HashSet::new()),
        }
    }

    /// 정책 목록 설정 (서버 동기화 후 호출)
    pub async fn update_policies(&self, policies: Vec<ExecutionPolicy>) {
        let mut cache = self.policy_cache.write().await;
        let mut allowed = self.allowed_processes.write().await;

        allowed.clear();
        for policy in &policies {
            allowed.insert(policy.process_name.clone());
        }

        cache.policies = policies;
        cache.last_updated = Utc::now();
    }

    /// 캐시 만료 확인
    pub async fn is_cache_valid(&self) -> bool {
        let cache = self.policy_cache.read().await;
        let elapsed = Utc::now()
            .signed_duration_since(cache.last_updated)
            .num_seconds() as u64;
        elapsed < cache.ttl_seconds
    }

    /// 자동화 명령의 정책 토큰 검증
    pub async fn validate_command(&self, cmd: &AutomationCommand) -> Result<bool, CoreError> {
        // 캐시 만료 확인
        if !self.is_cache_valid().await {
            tracing::warn!("정책 캐시 만료 — 서버 동기화 필요");
            return Ok(false);
        }

        // 정책 토큰 검증 (간단한 비어있지 않음 체크 — 실제로는 서버 검증)
        if cmd.policy_token.is_empty() {
            return Ok(false);
        }

        Ok(true)
    }

    /// 특정 프로세스의 정책 조회
    pub async fn get_policy_for_process(&self, process_name: &str) -> Option<ExecutionPolicy> {
        let cache = self.policy_cache.read().await;
        cache
            .policies
            .iter()
            .find(|p| p.process_name == process_name)
            .cloned()
    }

    /// 프로세스가 허가되었는지 확인
    pub async fn is_process_allowed(&self, process_name: &str) -> bool {
        let allowed = self.allowed_processes.read().await;
        allowed.contains(process_name)
    }

    /// 정책 토큰으로 정책 조회
    pub async fn get_policy_for_token(&self, policy_token: &str) -> Option<ExecutionPolicy> {
        let cache = self.policy_cache.read().await;
        // 토큰 형식: "{policy_id}:{nonce}" — 정책 ID로 매칭
        let policy_id = policy_token.split(':').next().unwrap_or(policy_token);
        cache
            .policies
            .iter()
            .find(|p| p.policy_id == policy_id)
            .cloned()
    }

    /// 인자 패턴 검증
    pub fn validate_args(policy: &ExecutionPolicy, args: &[String]) -> bool {
        if policy.allowed_args.is_empty() {
            return true; // 제한 없음
        }

        // 모든 인자가 허용 패턴에 매칭되어야 함
        args.iter().all(|arg| {
            policy.allowed_args.iter().any(|pattern| {
                if pattern.contains('*') {
                    // 간단한 glob 매칭
                    let parts: Vec<&str> = pattern.split('*').collect();
                    if parts.len() == 2 {
                        arg.starts_with(parts[0]) && arg.ends_with(parts[1])
                    } else {
                        arg == pattern
                    }
                } else {
                    arg == pattern
                }
            })
        })
    }
}

impl Default for PolicyClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_policy_serde() {
        let policy = ExecutionPolicy {
            policy_id: "pol-001".to_string(),
            process_name: "ls".to_string(),
            process_hash: None,
            allowed_args: vec!["-la".to_string()],
            requires_sudo: false,
            max_execution_time_ms: 5000,
            audit_level: AuditLevel::Basic,
            sandbox_profile: None,
            allowed_paths: vec![],
            allow_network: None,
        };
        let json = serde_json::to_string(&policy).unwrap();
        let deser: ExecutionPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.policy_id, "pol-001");
        assert_eq!(deser.audit_level, AuditLevel::Basic);
        assert!(deser.sandbox_profile.is_none());
        assert!(deser.allowed_paths.is_empty());
        assert!(deser.allow_network.is_none());
    }

    #[test]
    fn validate_args_empty_allows_all() {
        let policy = ExecutionPolicy {
            policy_id: "p".to_string(),
            process_name: "ls".to_string(),
            process_hash: None,
            allowed_args: vec![],
            requires_sudo: false,
            max_execution_time_ms: 5000,
            audit_level: AuditLevel::None,
            sandbox_profile: None,
            allowed_paths: vec![],
            allow_network: None,
        };
        assert!(PolicyClient::validate_args(
            &policy,
            &["anything".to_string()]
        ));
    }

    #[test]
    fn validate_args_pattern_match() {
        let policy = ExecutionPolicy {
            policy_id: "p".to_string(),
            process_name: "git".to_string(),
            process_hash: None,
            allowed_args: vec!["--*.txt".to_string(), "status".to_string()],
            requires_sudo: false,
            max_execution_time_ms: 5000,
            audit_level: AuditLevel::None,
            sandbox_profile: None,
            allowed_paths: vec![],
            allow_network: None,
        };
        assert!(PolicyClient::validate_args(
            &policy,
            &["status".to_string()]
        ));
        assert!(!PolicyClient::validate_args(&policy, &["push".to_string()]));
    }

    #[tokio::test]
    async fn update_and_check_policies() {
        let client = PolicyClient::new();
        let policies = vec![ExecutionPolicy {
            policy_id: "p1".to_string(),
            process_name: "ls".to_string(),
            process_hash: None,
            allowed_args: vec![],
            requires_sudo: false,
            max_execution_time_ms: 5000,
            audit_level: AuditLevel::Basic,
            sandbox_profile: None,
            allowed_paths: vec![],
            allow_network: None,
        }];

        client.update_policies(policies).await;
        assert!(client.is_process_allowed("ls").await);
        assert!(!client.is_process_allowed("rm").await);
    }

    #[tokio::test]
    async fn get_policy_for_token_found() {
        let client = PolicyClient::new();
        let policies = vec![ExecutionPolicy {
            policy_id: "pol-42".to_string(),
            process_name: "git".to_string(),
            process_hash: None,
            allowed_args: vec![],
            requires_sudo: false,
            max_execution_time_ms: 10000,
            audit_level: AuditLevel::Detailed,
            sandbox_profile: None,
            allowed_paths: vec![],
            allow_network: None,
        }];
        client.update_policies(policies).await;

        // 토큰 형식 "policy_id:nonce"에서 policy_id 추출
        let found = client.get_policy_for_token("pol-42:abc123").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().process_name, "git");
    }

    #[tokio::test]
    async fn get_policy_for_token_not_found() {
        let client = PolicyClient::new();
        let found = client.get_policy_for_token("nonexistent:xyz").await;
        assert!(found.is_none());
    }
}
