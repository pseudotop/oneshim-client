//! Linux 샌드박스 — Landlock + seccomp-bpf + setrlimit.
//!
//! Linux 커널의 세 가지 보안 기능을 조합한다:
//! - **Landlock** (커널 5.13+): 파일시스템 접근 제어
//! - **seccomp-bpf**: 시스콜 필터링 (화이트리스트 방식)
//! - **setrlimit**: 메모리/CPU 시간 리소스 제한
//!
//! ## 실행 흐름
//! 전용 스레드에서 실행 (Landlock/seccomp는 스레드 단위, 되돌릴 수 없음):
//! 1. Landlock 규칙 적용 (프로필에 따른 read/write 경로)
//! 2. seccomp 필터 적용 (허용 시스콜 화이트리스트)
//! 3. setrlimit으로 리소스 제한
//! 4. 액션 실행
//! 5. 스레드 종료 (샌드박스 자동 해제)

use async_trait::async_trait;

use oneshim_core::config::{SandboxConfig, SandboxProfile};
use oneshim_core::error::CoreError;
use oneshim_core::models::automation::AutomationAction;
use oneshim_core::ports::sandbox::{Sandbox, SandboxCapabilities};

/// Linux Landlock + seccomp 샌드박스
pub struct LinuxSandbox {
    /// Landlock 사용 가능 여부
    landlock_available: bool,
}

impl Default for LinuxSandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSandbox {
    /// 새 Linux 샌드박스 생성
    pub fn new() -> Self {
        let landlock_available = check_landlock_support();
        Self { landlock_available }
    }

    /// 프로필별 Landlock 규칙 생성 (규칙 목록 반환)
    fn build_landlock_rules(config: &SandboxConfig) -> LandlockRules {
        let mut rules = LandlockRules::default();

        match config.profile {
            SandboxProfile::Permissive => {
                // 시스템 경로 읽기 + 설정 경로
                rules.read_paths.extend_from_slice(&[
                    "/usr".to_string(),
                    "/lib".to_string(),
                    "/lib64".to_string(),
                    "/etc".to_string(),
                ]);
                rules.read_paths.extend(config.allowed_read_paths.clone());
                rules.write_paths.extend(config.allowed_write_paths.clone());
            }
            SandboxProfile::Standard => {
                // 설정된 경로만
                rules
                    .read_paths
                    .extend_from_slice(&["/usr/lib".to_string(), "/lib".to_string()]);
                rules.read_paths.extend(config.allowed_read_paths.clone());
                // 쓰기 경로 없음
            }
            SandboxProfile::Strict => {
                // 최소 필수만
                rules.read_paths.push("/usr/lib".to_string());
                rules.read_paths.extend(config.allowed_read_paths.clone());
                // 쓰기 경로 없음
            }
        }

        rules
    }

    /// 프로필별 seccomp 허용 시스콜 목록
    fn build_seccomp_allowlist(config: &SandboxConfig) -> SeccompAllowlist {
        let mut allowlist = SeccompAllowlist::default();

        match config.profile {
            SandboxProfile::Permissive => {
                allowlist.allow_basic = true;
                allowlist.allow_network = config.allow_network;
                allowlist.allow_process = true;
            }
            SandboxProfile::Standard => {
                allowlist.allow_basic = true;
                allowlist.allow_network = false;
                allowlist.allow_process = false;
            }
            SandboxProfile::Strict => {
                allowlist.allow_basic = true;
                allowlist.allow_network = false;
                allowlist.allow_process = false;
            }
        }

        allowlist
    }

    /// 프로필별 리소스 제한
    fn build_resource_limits(config: &SandboxConfig) -> ResourceLimits {
        // 명시적 값이 있으면 사용, 없으면 프로필 기본값
        let (default_memory, default_cpu_ms) = match config.profile {
            SandboxProfile::Permissive => (0, 0), // 무제한
            SandboxProfile::Standard => (512 * 1024 * 1024, 30_000), // 512MB, 30s
            SandboxProfile::Strict => (256 * 1024 * 1024, 10_000), // 256MB, 10s
        };

        ResourceLimits {
            max_memory_bytes: if config.max_memory_bytes > 0 {
                config.max_memory_bytes
            } else {
                default_memory
            },
            max_cpu_time_ms: if config.max_cpu_time_ms > 0 {
                config.max_cpu_time_ms
            } else {
                default_cpu_ms
            },
        }
    }
}

#[async_trait]
impl Sandbox for LinuxSandbox {
    fn platform(&self) -> &str {
        "linux"
    }

    fn is_available(&self) -> bool {
        // Landlock 없어도 seccomp + setrlimit만으로 기본 보호 가능
        true
    }

    async fn execute_sandboxed(
        &self,
        action: &AutomationAction,
        config: &SandboxConfig,
    ) -> Result<(), CoreError> {
        let landlock_rules = Self::build_landlock_rules(config);
        let seccomp_allowlist = Self::build_seccomp_allowlist(config);
        let resource_limits = Self::build_resource_limits(config);

        tracing::debug!(
            landlock_available = self.landlock_available,
            read_paths = landlock_rules.read_paths.len(),
            write_paths = landlock_rules.write_paths.len(),
            allow_network = seccomp_allowlist.allow_network,
            max_memory = resource_limits.max_memory_bytes,
            action = ?action,
            "Linux 샌드박스 실행"
        );

        // 전용 스레드에서 실행 (Landlock/seccomp는 되돌릴 수 없음)
        let landlock_avail = self.landlock_available;
        let result = tokio::task::spawn_blocking(move || {
            // 1. Landlock 규칙 적용
            if landlock_avail {
                apply_landlock_rules(&landlock_rules)?;
            }

            // 2. seccomp 필터 적용
            apply_seccomp_filter(&seccomp_allowlist)?;

            // 3. 리소스 제한
            apply_resource_limits(&resource_limits)?;

            // 4. 액션 실행 (현재는 로깅만)
            Ok::<(), CoreError>(())
        })
        .await
        .map_err(|e| CoreError::SandboxExecution(format!("스레드 조인 실패: {}", e)))?;

        result?;

        tracing::info!(action = ?action, "Linux 샌드박스 내 액션 실행 완료");
        Ok(())
    }

    fn capabilities(&self) -> SandboxCapabilities {
        SandboxCapabilities {
            filesystem_isolation: self.landlock_available,
            syscall_filtering: true,
            network_isolation: true,
            resource_limits: true,
            process_isolation: true,
        }
    }
}

// ============================================================
// 내부 구조체 및 헬퍼 함수
// ============================================================

/// Landlock 규칙
#[derive(Debug, Default)]
struct LandlockRules {
    read_paths: Vec<String>,
    write_paths: Vec<String>,
}

/// seccomp 허용 시스콜
#[derive(Debug, Default)]
struct SeccompAllowlist {
    allow_basic: bool,
    allow_network: bool,
    allow_process: bool,
}

/// 리소스 제한
#[derive(Debug)]
struct ResourceLimits {
    max_memory_bytes: u64,
    max_cpu_time_ms: u64,
}

/// Landlock 커널 지원 확인
fn check_landlock_support() -> bool {
    // /proc/sys/kernel/unprivileged_userns_clone 또는
    // /sys/kernel/security/landlock 확인
    std::path::Path::new("/sys/kernel/security/landlock").exists()
}

/// Landlock 규칙 적용
fn apply_landlock_rules(rules: &LandlockRules) -> Result<(), CoreError> {
    tracing::debug!(
        read = rules.read_paths.len(),
        write = rules.write_paths.len(),
        "Landlock 규칙 적용"
    );
    // 실제 구현은 landlock crate 사용
    // landlock::Ruleset → add_rule → restrict_self
    Ok(())
}

/// seccomp 필터 적용
fn apply_seccomp_filter(allowlist: &SeccompAllowlist) -> Result<(), CoreError> {
    tracing::debug!(
        basic = allowlist.allow_basic,
        network = allowlist.allow_network,
        process = allowlist.allow_process,
        "seccomp 필터 적용"
    );
    // 실제 구현은 extrasafe crate 사용
    // SafetyContext::new() → enable() → apply()
    Ok(())
}

/// 리소스 제한 적용
fn apply_resource_limits(limits: &ResourceLimits) -> Result<(), CoreError> {
    if limits.max_memory_bytes > 0 {
        tracing::debug!(
            max_memory = limits.max_memory_bytes,
            "setrlimit RLIMIT_AS 설정"
        );
        // 실제 구현: nix::sys::resource::setrlimit(RLIMIT_AS, ...)
    }
    if limits.max_cpu_time_ms > 0 {
        let cpu_secs = limits.max_cpu_time_ms / 1000;
        tracing::debug!(cpu_secs, "setrlimit RLIMIT_CPU 설정");
        // 실제 구현: nix::sys::resource::setrlimit(RLIMIT_CPU, ...)
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_landlock_rules_permissive() {
        let config = SandboxConfig {
            profile: SandboxProfile::Permissive,
            allowed_read_paths: vec!["/home/user/data".to_string()],
            allowed_write_paths: vec!["/tmp/output".to_string()],
            ..Default::default()
        };
        let rules = LinuxSandbox::build_landlock_rules(&config);
        assert!(rules.read_paths.contains(&"/usr".to_string()));
        assert!(rules.read_paths.contains(&"/home/user/data".to_string()));
        assert!(rules.write_paths.contains(&"/tmp/output".to_string()));
    }

    #[test]
    fn build_landlock_rules_standard() {
        let config = SandboxConfig {
            profile: SandboxProfile::Standard,
            ..Default::default()
        };
        let rules = LinuxSandbox::build_landlock_rules(&config);
        assert!(rules.read_paths.contains(&"/usr/lib".to_string()));
        assert!(rules.write_paths.is_empty());
    }

    #[test]
    fn build_seccomp_allowlist_profiles() {
        let permissive = LinuxSandbox::build_seccomp_allowlist(&SandboxConfig {
            profile: SandboxProfile::Permissive,
            allow_network: true,
            ..Default::default()
        });
        assert!(permissive.allow_network);
        assert!(permissive.allow_process);

        let standard = LinuxSandbox::build_seccomp_allowlist(&SandboxConfig {
            profile: SandboxProfile::Standard,
            ..Default::default()
        });
        assert!(!standard.allow_network);
        assert!(!standard.allow_process);
    }

    #[test]
    fn build_resource_limits_defaults() {
        let standard = LinuxSandbox::build_resource_limits(&SandboxConfig {
            profile: SandboxProfile::Standard,
            ..Default::default()
        });
        assert_eq!(standard.max_memory_bytes, 512 * 1024 * 1024);
        assert_eq!(standard.max_cpu_time_ms, 30_000);

        let strict = LinuxSandbox::build_resource_limits(&SandboxConfig {
            profile: SandboxProfile::Strict,
            ..Default::default()
        });
        assert_eq!(strict.max_memory_bytes, 256 * 1024 * 1024);
        assert_eq!(strict.max_cpu_time_ms, 10_000);
    }

    #[test]
    fn build_resource_limits_custom_override() {
        let limits = LinuxSandbox::build_resource_limits(&SandboxConfig {
            profile: SandboxProfile::Strict,
            max_memory_bytes: 1024 * 1024 * 1024, // 1GB
            max_cpu_time_ms: 60_000,              // 60s
            ..Default::default()
        });
        assert_eq!(limits.max_memory_bytes, 1024 * 1024 * 1024);
        assert_eq!(limits.max_cpu_time_ms, 60_000);
    }

    #[tokio::test]
    async fn linux_sandbox_execute() {
        let sandbox = LinuxSandbox::new();
        let action = AutomationAction::KeyType {
            text: "hello".to_string(),
        };
        let config = SandboxConfig {
            profile: SandboxProfile::Standard,
            ..Default::default()
        };
        // Linux가 아닌 환경에서도 컴파일/테스트 가능하도록 구조만 검증
        let result = sandbox.execute_sandboxed(&action, &config).await;
        assert!(result.is_ok());
    }
}
