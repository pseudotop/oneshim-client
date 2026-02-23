//! 정책 클라이언트.
//!
//! 서버에서 실행 정책을 동기화하고, 자동화 명령의 정책 토큰을 검증한다.
//! 허가된 프로세스만 실행하며, 바이너리 해시 검증도 지원.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::controller::AutomationCommand;
use oneshim_core::config::SandboxProfile;
use oneshim_core::error::CoreError;

const POLICY_TOKEN_SIGNING_SECRET_ENV: &str = "ONESHIM_POLICY_TOKEN_SIGNING_SECRET";
const COMMAND_HASH_SEGMENT_PREFIX: char = 'h';

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
    /// 정책 토큰 서명 검증 강제 여부
    #[serde(default)]
    pub require_signed_token: bool,
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
    /// 재사용 방지용 검증 완료 토큰 캐시 (policy_token -> validated_at)
    validated_tokens: RwLock<HashMap<String, DateTime<Utc>>>,
}

impl PolicyClient {
    /// 새 정책 클라이언트 생성
    pub fn new() -> Self {
        Self {
            policy_cache: RwLock::new(PolicyCache::default()),
            allowed_processes: RwLock::new(HashSet::new()),
            validated_tokens: RwLock::new(HashMap::new()),
        }
    }

    /// 정책 목록 설정 (서버 동기화 후 호출)
    pub async fn update_policies(&self, policies: Vec<ExecutionPolicy>) {
        let mut cache = self.policy_cache.write().await;
        let mut allowed = self.allowed_processes.write().await;
        let mut validated = self.validated_tokens.write().await;

        allowed.clear();
        for policy in &policies {
            allowed.insert(policy.process_name.clone());
        }

        cache.policies = policies;
        cache.last_updated = Utc::now();
        validated.clear();
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
        let now = Utc::now();
        let token = cmd.policy_token.trim();
        if token.is_empty() {
            return Ok(false);
        }

        let Some(parsed_token) = parse_policy_token(token) else {
            tracing::warn!(policy_token = token, "정책 토큰 형식 오류");
            return Ok(false);
        };
        if !is_valid_nonce(parsed_token.nonce) {
            tracing::warn!(policy_token = token, "정책 토큰 nonce 형식 오류");
            return Ok(false);
        }

        let ttl_seconds = {
            let cache = self.policy_cache.read().await;
            let elapsed = now.signed_duration_since(cache.last_updated).num_seconds() as u64;
            if elapsed >= cache.ttl_seconds {
                0
            } else {
                cache.ttl_seconds
            }
        };

        if ttl_seconds == 0 {
            tracing::warn!("정책 캐시 만료 — 서버 동기화 필요");
            return Ok(false);
        }

        // 토큰의 policy_id가 현재 캐시에 존재해야 유효
        let Some(policy) = self.get_policy_for_token(token).await else {
            tracing::warn!(
                policy_id = parsed_token.policy_id,
                "정책 토큰에 매칭되는 정책이 없음"
            );
            return Ok(false);
        };

        if policy.require_signed_token {
            let Some(signature) = parsed_token.signature else {
                tracing::warn!(
                    policy_id = parsed_token.policy_id,
                    "서명 정책인데 정책 토큰 서명이 누락됨"
                );
                return Ok(false);
            };

            if !is_valid_signature(signature) {
                tracing::warn!(
                    policy_id = parsed_token.policy_id,
                    "정책 토큰 서명 형식 오류"
                );
                return Ok(false);
            }

            if !verify_policy_token_signature(
                parsed_token.policy_id,
                parsed_token.nonce,
                parsed_token.command_hash,
                signature,
            ) {
                tracing::warn!(
                    policy_id = parsed_token.policy_id,
                    "정책 토큰 서명 검증 실패"
                );
                return Ok(false);
            }
        }

        if let Some(token_command_hash) = parsed_token.command_hash {
            let expected_hash = compute_command_scope_hash(cmd)?;
            if !token_command_hash.eq_ignore_ascii_case(&expected_hash) {
                tracing::warn!(
                    policy_id = parsed_token.policy_id,
                    "정책 토큰 command hash 불일치"
                );
                return Ok(false);
            }
        }

        // nonce 재사용 방지 (캐시 TTL 범위 내에서 토큰 1회성 보장)
        let mut validated = self.validated_tokens.write().await;
        validated.retain(|_, validated_at| {
            now.signed_duration_since(*validated_at).num_seconds() < ttl_seconds as i64
        });
        if validated.contains_key(token) {
            tracing::warn!(policy_token = token, "정책 토큰 재사용 감지");
            return Ok(false);
        }
        validated.insert(token.to_string(), now);

        Ok(true)
    }

    /// 정책 ID로 실행용 정책 토큰 발급.
    ///
    /// 토큰 형식:
    /// - unsigned: `{policy_id}:{nonce}`
    /// - signed: `{policy_id}:{nonce}:{sha256(policy_id:nonce:secret)}`
    pub async fn issue_command_token(&self, policy_id: &str) -> Result<String, CoreError> {
        let policy = {
            let cache = self.policy_cache.read().await;
            cache
                .policies
                .iter()
                .find(|p| p.policy_id == policy_id)
                .cloned()
        }
        .ok_or_else(|| CoreError::PolicyDenied(format!("알 수 없는 정책 ID: {policy_id}")))?;

        let nonce = issue_policy_nonce();
        issue_command_token_for_policy(&policy, &nonce, None)
    }

    /// 정책 ID + 명령 컨텍스트 기반 실행용 정책 토큰 발급.
    ///
    /// 토큰에 command hash를 포함해 다른 명령으로의 재사용을 방지한다.
    pub async fn issue_command_token_for_command(
        &self,
        policy_id: &str,
        cmd: &AutomationCommand,
    ) -> Result<String, CoreError> {
        let policy = {
            let cache = self.policy_cache.read().await;
            cache
                .policies
                .iter()
                .find(|p| p.policy_id == policy_id)
                .cloned()
        }
        .ok_or_else(|| CoreError::PolicyDenied(format!("알 수 없는 정책 ID: {policy_id}")))?;

        let nonce = issue_policy_nonce();
        let command_hash = compute_command_scope_hash(cmd)?;
        issue_command_token_for_policy(&policy, &nonce, Some(command_hash.as_str()))
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
        // 토큰 형식: "{policy_id}:{nonce}[:signature]" — 정책 ID로 매칭
        let policy_id = parse_policy_token(policy_token)
            .map(|token| token.policy_id)
            .unwrap_or(policy_token);
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

struct ParsedPolicyToken<'a> {
    policy_id: &'a str,
    nonce: &'a str,
    command_hash: Option<&'a str>,
    signature: Option<&'a str>,
}

fn parse_policy_token(token: &str) -> Option<ParsedPolicyToken<'_>> {
    let parts: Vec<&str> = token.split(':').map(str::trim).collect();
    let (policy_id, nonce, command_hash, signature) = match parts.as_slice() {
        [policy_id, nonce] => (*policy_id, *nonce, None, None),
        [policy_id, nonce, third] => {
            if let Some(command_hash) = parse_command_hash_segment(third) {
                (*policy_id, *nonce, Some(command_hash), None)
            } else {
                (*policy_id, *nonce, None, Some(*third))
            }
        }
        [policy_id, nonce, third, fourth] => {
            let command_hash = parse_command_hash_segment(third)?;
            (*policy_id, *nonce, Some(command_hash), Some(*fourth))
        }
        _ => return None,
    };

    if policy_id.is_empty() || nonce.is_empty() {
        return None;
    }
    if command_hash.is_some_and(|hash| !is_valid_hash(hash)) {
        return None;
    }
    if signature.is_some_and(|sig| sig.is_empty()) {
        return None;
    }

    Some(ParsedPolicyToken {
        policy_id,
        nonce,
        command_hash,
        signature,
    })
}

fn is_valid_nonce(nonce: &str) -> bool {
    nonce.len() >= 8
        && nonce
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn is_valid_signature(signature: &str) -> bool {
    is_valid_hash(signature)
}

fn issue_policy_nonce() -> String {
    Uuid::new_v4().simple().to_string()
}

fn issue_command_token_for_policy(
    policy: &ExecutionPolicy,
    nonce: &str,
    command_hash: Option<&str>,
) -> Result<String, CoreError> {
    if !is_valid_nonce(nonce) {
        return Err(CoreError::InvalidArguments(
            "정책 토큰 nonce 형식이 유효하지 않습니다".to_string(),
        ));
    }
    if command_hash.is_some_and(|hash| !is_valid_hash(hash)) {
        return Err(CoreError::InvalidArguments(
            "정책 토큰 command hash 형식이 유효하지 않습니다".to_string(),
        ));
    }

    let mut token = format!("{}:{nonce}", policy.policy_id);
    if let Some(command_hash) = command_hash {
        token.push(':');
        token.push(COMMAND_HASH_SEGMENT_PREFIX);
        token.push_str(command_hash);
    }

    if policy.require_signed_token {
        let secret = load_signing_secret().ok_or_else(|| {
            CoreError::Config(format!(
                "서명 정책이 활성화되어 있지만 {} 환경 변수가 비어 있습니다.",
                POLICY_TOKEN_SIGNING_SECRET_ENV
            ))
        })?;
        let signature =
            compute_policy_token_signature(&policy.policy_id, nonce, command_hash, &secret);
        token.push(':');
        token.push_str(&signature);
    } else {
    }

    Ok(token)
}

fn parse_command_hash_segment(segment: &str) -> Option<&str> {
    let mut chars = segment.chars();
    if chars.next()? != COMMAND_HASH_SEGMENT_PREFIX {
        return None;
    }
    let hash = chars.as_str();
    if !is_valid_hash(hash) {
        return None;
    }
    Some(hash)
}

fn is_valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())
}

fn compute_command_scope_hash(cmd: &AutomationCommand) -> Result<String, CoreError> {
    #[derive(Serialize)]
    struct PolicyCommandScope<'a> {
        command_id: &'a str,
        session_id: &'a str,
        action: &'a crate::controller::AutomationAction,
        timeout_ms: Option<u64>,
    }

    let scope = PolicyCommandScope {
        command_id: cmd.command_id.as_str(),
        session_id: cmd.session_id.as_str(),
        action: &cmd.action,
        timeout_ms: cmd.timeout_ms,
    };
    let serialized = serde_json::to_vec(&scope)
        .map_err(|e| CoreError::Internal(format!("정책 토큰 command scope 직렬화 실패: {e}")))?;
    let digest = Sha256::digest(serialized);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn verify_policy_token_signature(
    policy_id: &str,
    nonce: &str,
    command_hash: Option<&str>,
    signature: &str,
) -> bool {
    let Some(secret) = load_signing_secret() else {
        tracing::warn!(
            env = POLICY_TOKEN_SIGNING_SECRET_ENV,
            "서명 정책 활성화됐지만 토큰 서명 시크릿이 설정되지 않음"
        );
        return false;
    };

    compute_policy_token_signature(policy_id, nonce, command_hash, &secret)
        .eq_ignore_ascii_case(signature)
}

fn load_signing_secret() -> Option<String> {
    std::env::var(POLICY_TOKEN_SIGNING_SECRET_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn compute_policy_token_signature(
    policy_id: &str,
    nonce: &str,
    command_hash: Option<&str>,
    secret: &str,
) -> String {
    let payload = if let Some(command_hash) = command_hash {
        format!("{policy_id}:{nonce}:{command_hash}:{secret}")
    } else {
        format!("{policy_id}:{nonce}:{secret}")
    };
    let digest = Sha256::digest(payload.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

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
            require_signed_token: false,
        };
        let json = serde_json::to_string(&policy).unwrap();
        let deser: ExecutionPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.policy_id, "pol-001");
        assert_eq!(deser.audit_level, AuditLevel::Basic);
        assert!(deser.sandbox_profile.is_none());
        assert!(deser.allowed_paths.is_empty());
        assert!(deser.allow_network.is_none());
        assert!(!deser.require_signed_token);
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
            require_signed_token: false,
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
            require_signed_token: false,
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
            require_signed_token: false,
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
            require_signed_token: false,
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

    fn make_policy(policy_id: &str) -> ExecutionPolicy {
        ExecutionPolicy {
            policy_id: policy_id.to_string(),
            process_name: "git".to_string(),
            process_hash: None,
            allowed_args: vec![],
            requires_sudo: false,
            max_execution_time_ms: 10000,
            audit_level: AuditLevel::Basic,
            sandbox_profile: None,
            allowed_paths: vec![],
            allow_network: None,
            require_signed_token: false,
        }
    }

    fn make_command(policy_token: &str) -> AutomationCommand {
        AutomationCommand {
            command_id: "cmd-1".to_string(),
            session_id: "sess-1".to_string(),
            action: crate::controller::AutomationAction::MouseMove { x: 0, y: 0 },
            timeout_ms: None,
            policy_token: policy_token.to_string(),
        }
    }

    #[tokio::test]
    async fn validate_command_requires_existing_policy_and_valid_nonce() {
        let client = PolicyClient::new();
        client.update_policies(vec![make_policy("pol-1")]).await;

        let invalid_format = make_command("pol-1");
        assert!(!client.validate_command(&invalid_format).await.unwrap());

        let invalid_nonce = make_command("pol-1:short");
        assert!(!client.validate_command(&invalid_nonce).await.unwrap());

        let unknown_policy = make_command("pol-x:nonce_1234");
        assert!(!client.validate_command(&unknown_policy).await.unwrap());

        let valid = make_command("pol-1:nonce_1234");
        assert!(client.validate_command(&valid).await.unwrap());
    }

    #[tokio::test]
    async fn validate_command_rejects_replayed_policy_token() {
        let client = PolicyClient::new();
        client.update_policies(vec![make_policy("pol-1")]).await;

        let cmd = make_command("pol-1:nonce_1234");
        assert!(client.validate_command(&cmd).await.unwrap());
        assert!(!client.validate_command(&cmd).await.unwrap());
    }

    #[test]
    fn parse_policy_token_supports_optional_signature() {
        let without_signature = parse_policy_token("pol-1:nonce_1234").unwrap();
        assert_eq!(without_signature.policy_id, "pol-1");
        assert_eq!(without_signature.nonce, "nonce_1234");
        assert!(without_signature.command_hash.is_none());
        assert!(without_signature.signature.is_none());

        let with_command_hash = parse_policy_token(
            "pol-1:nonce_1234:h14bf0b43befc58f56d4e4bcc9c8942d44f8d3af1321a96bea6f89fa44f4f5329",
        )
        .unwrap();
        assert_eq!(with_command_hash.policy_id, "pol-1");
        assert_eq!(with_command_hash.nonce, "nonce_1234");
        assert_eq!(
            with_command_hash.command_hash,
            Some("14bf0b43befc58f56d4e4bcc9c8942d44f8d3af1321a96bea6f89fa44f4f5329")
        );
        assert!(with_command_hash.signature.is_none());

        let with_signature = parse_policy_token(
            "pol-1:nonce_1234:14bf0b43befc58f56d4e4bcc9c8942d44f8d3af1321a96bea6f89fa44f4f5329",
        )
        .unwrap();
        assert_eq!(with_signature.policy_id, "pol-1");
        assert_eq!(with_signature.nonce, "nonce_1234");
        assert!(with_signature.command_hash.is_none());
        assert!(with_signature.signature.is_some());
    }

    #[test]
    fn parse_policy_token_rejects_too_many_segments() {
        assert!(parse_policy_token("pol-1:nonce:hdeadbeef:signature:extra").is_none());
    }

    #[test]
    fn compute_policy_signature_is_stable() {
        let signature = compute_policy_token_signature("pol-1", "nonce_1234", None, "secret");
        assert_eq!(
            signature,
            "14bf0b43befc58f56d4e4bcc9c8942d44f8d3af1321a96bea6f89fa44f4f5329"
        );
    }

    #[tokio::test]
    async fn validate_command_rejects_missing_signature_when_policy_requires_it() {
        let client = PolicyClient::new();
        let mut policy = make_policy("pol-1");
        policy.require_signed_token = true;
        client.update_policies(vec![policy]).await;

        let cmd = make_command("pol-1:nonce_1234");
        assert!(!client.validate_command(&cmd).await.unwrap());
    }

    #[tokio::test]
    async fn issue_command_token_rejects_unknown_policy() {
        let client = PolicyClient::new();
        let result = client.issue_command_token("unknown").await;
        assert!(matches!(result, Err(CoreError::PolicyDenied(_))));
    }

    #[tokio::test]
    async fn issue_command_token_for_unsigned_policy() {
        let client = PolicyClient::new();
        client.update_policies(vec![make_policy("pol-1")]).await;

        let token = client
            .issue_command_token("pol-1")
            .await
            .expect("토큰 발급 실패");
        let parsed = parse_policy_token(&token).expect("발급 토큰 파싱 실패");
        assert_eq!(parsed.policy_id, "pol-1");
        assert!(parsed.command_hash.is_none());
        assert!(parsed.signature.is_none());
        assert!(is_valid_nonce(parsed.nonce));
    }

    #[tokio::test]
    async fn issue_command_token_for_signed_policy_requires_secret() {
        let env_guard = env_lock().lock().unwrap();
        std::env::remove_var(POLICY_TOKEN_SIGNING_SECRET_ENV);

        let client = PolicyClient::new();
        let mut policy = make_policy("pol-1");
        policy.require_signed_token = true;
        client.update_policies(vec![policy]).await;

        let result = client.issue_command_token("pol-1").await;
        assert!(matches!(result, Err(CoreError::Config(_))));

        drop(env_guard);
    }

    #[tokio::test]
    async fn issue_command_token_for_signed_policy_generates_verifiable_token() {
        let env_guard = env_lock().lock().unwrap();
        std::env::set_var(POLICY_TOKEN_SIGNING_SECRET_ENV, "signing-secret");

        let client = PolicyClient::new();
        let mut policy = make_policy("pol-1");
        policy.require_signed_token = true;
        client.update_policies(vec![policy]).await;

        let token = client
            .issue_command_token("pol-1")
            .await
            .expect("토큰 발급 실패");

        let parsed = parse_policy_token(&token).expect("발급 토큰 파싱 실패");
        assert!(parsed.command_hash.is_none());
        let signature = parsed.signature.expect("서명 누락");
        assert!(is_valid_signature(signature));
        assert!(verify_policy_token_signature(
            parsed.policy_id,
            parsed.nonce,
            parsed.command_hash,
            signature
        ));

        let cmd = make_command(&token);
        assert!(client.validate_command(&cmd).await.unwrap());

        std::env::remove_var(POLICY_TOKEN_SIGNING_SECRET_ENV);
        drop(env_guard);
    }

    #[tokio::test]
    async fn issue_command_token_for_command_binds_command_scope() {
        let client = PolicyClient::new();
        client.update_policies(vec![make_policy("pol-1")]).await;

        let mut cmd = make_command("unused");
        cmd.command_id = "cmd-bound".to_string();
        cmd.session_id = "sess-bound".to_string();
        let token = client
            .issue_command_token_for_command("pol-1", &cmd)
            .await
            .expect("토큰 발급 실패");
        cmd.policy_token = token;
        assert!(client.validate_command(&cmd).await.unwrap());
    }

    #[tokio::test]
    async fn validate_command_rejects_when_bound_command_scope_mismatches() {
        let client = PolicyClient::new();
        client.update_policies(vec![make_policy("pol-1")]).await;

        let mut source_cmd = make_command("unused");
        source_cmd.command_id = "cmd-source".to_string();
        source_cmd.session_id = "sess-source".to_string();
        let token = client
            .issue_command_token_for_command("pol-1", &source_cmd)
            .await
            .expect("토큰 발급 실패");

        let mut different_cmd = make_command(&token);
        different_cmd.command_id = "cmd-other".to_string();
        assert!(!client.validate_command(&different_cmd).await.unwrap());
    }
}
