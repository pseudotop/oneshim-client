//! 정책 → 샌드박스 설정 리졸버.
//!
//! ExecutionPolicy 기반으로 동적 SandboxConfig를 결정한다.
//! Privacy Filter의 계단식 패턴을 적용하여 AuditLevel → SandboxProfile 매핑.
//! 모든 함수는 순수 함수 (compression `select_algorithm()` 패턴).

use crate::policy::{AuditLevel, ExecutionPolicy};
use oneshim_core::config::{SandboxConfig, SandboxProfile};

/// AuditLevel → SandboxProfile 계단식 매핑 (Privacy Filter 패턴)
///
/// 에스컬레이션 규칙:
/// - `policy.sandbox_profile`이 Some이면 우선 적용
/// - `requires_sudo = true`이면 Permissive → Standard로 승격
pub fn resolve_sandbox_profile(policy: &ExecutionPolicy) -> SandboxProfile {
    // 서버 오버라이드 우선
    if let Some(profile) = policy.sandbox_profile {
        return profile;
    }

    // AuditLevel → SandboxProfile 계단식 매핑
    let base_profile = match policy.audit_level {
        AuditLevel::None => SandboxProfile::Permissive,
        AuditLevel::Basic => SandboxProfile::Standard,
        AuditLevel::Detailed => SandboxProfile::Strict,
        AuditLevel::Full => SandboxProfile::Strict,
    };

    // sudo 에스컬레이션: Permissive → Standard
    if policy.requires_sudo && matches!(base_profile, SandboxProfile::Permissive) {
        return SandboxProfile::Standard;
    }

    base_profile
}

/// 정책 기반 동적 SandboxConfig 결정
///
/// - `resolve_sandbox_profile()` 호출하여 프로필 결정
/// - `policy.allowed_paths` → `base_config.allowed_read_paths`에 병합
/// - `policy.allow_network` → 우선 적용, 없으면 프로필 기반
/// - `policy.max_execution_time_ms` → `max_cpu_time_ms` 설정
pub fn resolve_sandbox_config(
    policy: &ExecutionPolicy,
    base_config: &SandboxConfig,
) -> SandboxConfig {
    let profile = resolve_sandbox_profile(policy);

    // 네트워크: 정책 오버라이드 > 프로필 기반 기본값
    let allow_network = policy
        .allow_network
        .unwrap_or(matches!(profile, SandboxProfile::Permissive));

    // 허용 읽기 경로: base + 정책 추가 경로 병합
    let mut allowed_read_paths = base_config.allowed_read_paths.clone();
    for path in &policy.allowed_paths {
        if !allowed_read_paths.contains(path) {
            allowed_read_paths.push(path.clone());
        }
    }

    // CPU 시간: 정책 값 사용 (0이면 base 유지)
    let max_cpu_time_ms = if policy.max_execution_time_ms > 0 {
        policy.max_execution_time_ms
    } else {
        base_config.max_cpu_time_ms
    };

    SandboxConfig {
        enabled: base_config.enabled,
        profile,
        allowed_read_paths,
        allowed_write_paths: base_config.allowed_write_paths.clone(),
        allow_network,
        max_memory_bytes: base_config.max_memory_bytes,
        max_cpu_time_ms,
    }
}

/// 정책 없는 명령용 — Strict 프로필, 쓰기 불가, 네트워크 차단
pub fn default_strict_config(base_config: &SandboxConfig) -> SandboxConfig {
    SandboxConfig {
        enabled: base_config.enabled,
        profile: SandboxProfile::Strict,
        allowed_read_paths: base_config.allowed_read_paths.clone(),
        allowed_write_paths: Vec::new(),
        allow_network: false,
        max_memory_bytes: base_config.max_memory_bytes,
        max_cpu_time_ms: base_config.max_cpu_time_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_policy(audit: AuditLevel, sudo: bool) -> ExecutionPolicy {
        ExecutionPolicy {
            policy_id: "test".to_string(),
            process_name: "test".to_string(),
            process_hash: None,
            allowed_args: vec![],
            requires_sudo: sudo,
            max_execution_time_ms: 0,
            audit_level: audit,
            sandbox_profile: None,
            allowed_paths: vec![],
            allow_network: None,
        }
    }

    // --- resolve_sandbox_profile 매핑 테스트 (4개) ---

    #[test]
    fn audit_none_maps_to_permissive() {
        let policy = make_policy(AuditLevel::None, false);
        assert!(matches!(
            resolve_sandbox_profile(&policy),
            SandboxProfile::Permissive
        ));
    }

    #[test]
    fn audit_basic_maps_to_standard() {
        let policy = make_policy(AuditLevel::Basic, false);
        assert!(matches!(
            resolve_sandbox_profile(&policy),
            SandboxProfile::Standard
        ));
    }

    #[test]
    fn audit_detailed_maps_to_strict() {
        let policy = make_policy(AuditLevel::Detailed, false);
        assert!(matches!(
            resolve_sandbox_profile(&policy),
            SandboxProfile::Strict
        ));
    }

    #[test]
    fn audit_full_maps_to_strict() {
        let policy = make_policy(AuditLevel::Full, false);
        assert!(matches!(
            resolve_sandbox_profile(&policy),
            SandboxProfile::Strict
        ));
    }

    // --- sudo 에스컬레이션 테스트 (2개) ---

    #[test]
    fn sudo_escalates_permissive_to_standard() {
        let policy = make_policy(AuditLevel::None, true);
        // None → Permissive지만 sudo → Standard로 승격
        assert!(matches!(
            resolve_sandbox_profile(&policy),
            SandboxProfile::Standard
        ));
    }

    #[test]
    fn sudo_does_not_escalate_strict() {
        let policy = make_policy(AuditLevel::Detailed, true);
        // Detailed → Strict, sudo여도 Strict 유지 (이미 최고 레벨)
        assert!(matches!(
            resolve_sandbox_profile(&policy),
            SandboxProfile::Strict
        ));
    }

    // --- 서버 오버라이드 테스트 (1개) ---

    #[test]
    fn server_override_takes_priority() {
        let mut policy = make_policy(AuditLevel::Full, true);
        policy.sandbox_profile = Some(SandboxProfile::Permissive);
        // 서버가 Permissive 지정하면 audit_level/sudo 무시
        assert!(matches!(
            resolve_sandbox_profile(&policy),
            SandboxProfile::Permissive
        ));
    }

    // --- resolve_sandbox_config 병합 테스트 (2개) ---

    #[test]
    fn config_merges_allowed_paths() {
        let mut policy = make_policy(AuditLevel::Basic, false);
        policy.allowed_paths = vec!["/tmp/extra".to_string()];

        let base = SandboxConfig {
            allowed_read_paths: vec!["/usr/lib".to_string()],
            ..Default::default()
        };

        let resolved = resolve_sandbox_config(&policy, &base);
        assert_eq!(resolved.allowed_read_paths.len(), 2);
        assert!(resolved
            .allowed_read_paths
            .contains(&"/usr/lib".to_string()));
        assert!(resolved
            .allowed_read_paths
            .contains(&"/tmp/extra".to_string()));
    }

    #[test]
    fn config_network_override() {
        let mut policy = make_policy(AuditLevel::Detailed, false);
        // Detailed → Strict → 네트워크 차단이 기본
        policy.allow_network = Some(true);

        let resolved = resolve_sandbox_config(&policy, &SandboxConfig::default());
        assert!(resolved.allow_network);
    }

    #[test]
    fn config_max_cpu_time_from_policy() {
        let mut policy = make_policy(AuditLevel::Basic, false);
        policy.max_execution_time_ms = 3000;

        let resolved = resolve_sandbox_config(&policy, &SandboxConfig::default());
        assert_eq!(resolved.max_cpu_time_ms, 3000);
    }

    // --- default_strict_config 테스트 (1개) ---

    #[test]
    fn default_strict_blocks_write_and_network() {
        let base = SandboxConfig {
            allowed_write_paths: vec!["/tmp".to_string()],
            allow_network: true,
            ..Default::default()
        };

        let strict = default_strict_config(&base);
        assert!(matches!(strict.profile, SandboxProfile::Strict));
        assert!(strict.allowed_write_paths.is_empty());
        assert!(!strict.allow_network);
    }
}
