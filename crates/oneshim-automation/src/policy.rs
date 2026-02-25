//!

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

// PolicyClient

pub struct PolicyClient {
    policy_cache: RwLock<PolicyCache>,
    allowed_processes: RwLock<HashSet<String>>,
    validated_tokens: RwLock<HashMap<String, DateTime<Utc>>>,
}

impl PolicyClient {
    pub fn new() -> Self {
        Self {
            policy_cache: RwLock::new(PolicyCache::default()),
            allowed_processes: RwLock::new(HashSet::new()),
            validated_tokens: RwLock::new(HashMap::new()),
        }
    }

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

    pub async fn is_cache_valid(&self) -> bool {
        let cache = self.policy_cache.read().await;
        let elapsed = Utc::now()
            .signed_duration_since(cache.last_updated)
            .num_seconds() as u64;
        elapsed < cache.ttl_seconds
    }

    pub async fn validate_command(&self, cmd: &AutomationCommand) -> Result<bool, CoreError> {
        let now = Utc::now();
        let token = cmd.policy_token.trim();
        if token.is_empty() {
            return Ok(false);
        }

        let Some(parsed_token) = parse_policy_token(token) else {
            tracing::warn!(policy_token = token, "policy token error");
            return Ok(false);
        };
        if !is_valid_nonce(parsed_token.nonce) {
            tracing::warn!(policy_token = token, "policy token nonce error");
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
            tracing::warn!("policy cache expired: server refresh required");
            return Ok(false);
        }

        let Some(policy) = self.get_policy_for_token(token).await else {
            tracing::warn!(
                policy_id = parsed_token.policy_id,
                "no matching policy found for policy token"
            );
            return Ok(false);
        };

        if policy.require_signed_token {
            let Some(signature) = parsed_token.signature else {
                tracing::warn!(
                    policy_id = parsed_token.policy_id,
                    "policy requires signature but policy token has no signature"
                );
                return Ok(false);
            };

            if !is_valid_signature(signature) {
                tracing::warn!(
                    policy_id = parsed_token.policy_id,
                    "invalid policy token signature format"
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
                    "policy token signature validation failed"
                );
                return Ok(false);
            }
        }

        if let Some(token_command_hash) = parsed_token.command_hash {
            let expected_hash = compute_command_scope_hash(cmd)?;
            if !token_command_hash.eq_ignore_ascii_case(&expected_hash) {
                tracing::warn!(
                    policy_id = parsed_token.policy_id,
                    "policy token command hash mismatch"
                );
                return Ok(false);
            }
        }

        let mut validated = self.validated_tokens.write().await;
        validated.retain(|_, validated_at| {
            now.signed_duration_since(*validated_at).num_seconds() < ttl_seconds as i64
        });
        if validated.contains_key(token) {
            tracing::warn!(policy_token = token, "policy token detection");
            return Ok(false);
        }
        validated.insert(token.to_string(), now);

        Ok(true)
    }

    ///
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
        .ok_or_else(|| CoreError::PolicyDenied(format!("Unknown policy ID: {policy_id}")))?;

        let nonce = issue_policy_nonce();
        issue_command_token_for_policy(&policy, &nonce, None)
    }

    ///
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
        .ok_or_else(|| CoreError::PolicyDenied(format!("Unknown policy ID: {policy_id}")))?;

        let nonce = issue_policy_nonce();
        let command_hash = compute_command_scope_hash(cmd)?;
        issue_command_token_for_policy(&policy, &nonce, Some(command_hash.as_str()))
    }

    pub async fn get_policy_for_process(&self, process_name: &str) -> Option<ExecutionPolicy> {
        let cache = self.policy_cache.read().await;
        cache
            .policies
            .iter()
            .find(|p| p.process_name == process_name)
            .cloned()
    }

    pub async fn is_process_allowed(&self, process_name: &str) -> bool {
        let allowed = self.allowed_processes.read().await;
        allowed.contains(process_name)
    }

    pub async fn get_policy_for_token(&self, policy_token: &str) -> Option<ExecutionPolicy> {
        let cache = self.policy_cache.read().await;
        let policy_id = parse_policy_token(policy_token)
            .map(|token| token.policy_id)
            .unwrap_or(policy_token);
        cache
            .policies
            .iter()
            .find(|p| p.policy_id == policy_id)
            .cloned()
    }

    pub fn validate_args(policy: &ExecutionPolicy, args: &[String]) -> bool {
        if policy.allowed_args.is_empty() {
            return true; // unrestricted
        }

        args.iter().all(|arg| {
            policy.allowed_args.iter().any(|pattern| {
                if pattern.contains('*') {
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
            "policy token nonce 형식이 유효하지 않습니다".to_string(),
        ));
    }
    if command_hash.is_some_and(|hash| !is_valid_hash(hash)) {
        return Err(CoreError::InvalidArguments(
            "policy token command hash 형식이 유효하지 않습니다".to_string(),
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
                "서명 policy이 active화되어 있지만 {} 환경 변수가 비어 있습니다.",
                POLICY_TOKEN_SIGNING_SECRET_ENV
            ))
        })?;
        let signature =
            compute_policy_token_signature(&policy.policy_id, nonce, command_hash, &secret);
        token.push(':');
        token.push_str(&signature);
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
    let serialized = serde_json::to_vec(&scope).map_err(|e| {
        CoreError::Internal(format!(
            "Failed to serialize policy token command scope: {e}"
        ))
    })?;
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
            "서명 policy active화됐지만 token 서명 시크릿이 설정되지 않음"
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
    use std::sync::OnceLock;
    use tokio::sync::Mutex;

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
            .expect("Failed to issue token");
        let parsed = parse_policy_token(&token).expect("Failed to parse issued token");
        assert_eq!(parsed.policy_id, "pol-1");
        assert!(parsed.command_hash.is_none());
        assert!(parsed.signature.is_none());
        assert!(is_valid_nonce(parsed.nonce));
    }

    #[tokio::test]
    async fn issue_command_token_for_signed_policy_requires_secret() {
        let env_guard = env_lock().lock().await;
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
        let env_guard = env_lock().lock().await;
        std::env::set_var(POLICY_TOKEN_SIGNING_SECRET_ENV, "signing-secret");

        let client = PolicyClient::new();
        let mut policy = make_policy("pol-1");
        policy.require_signed_token = true;
        client.update_policies(vec![policy]).await;

        let token = client
            .issue_command_token("pol-1")
            .await
            .expect("Failed to issue token");

        let parsed = parse_policy_token(&token).expect("Failed to parse issued token");
        assert!(parsed.command_hash.is_none());
        let signature = parsed.signature.expect("Missing signature");
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
            .expect("Failed to issue token");
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
            .expect("Failed to issue token");

        let mut different_cmd = make_command(&token);
        different_cmd.command_id = "cmd-other".to_string();
        assert!(!client.validate_command(&different_cmd).await.unwrap());
    }
}
