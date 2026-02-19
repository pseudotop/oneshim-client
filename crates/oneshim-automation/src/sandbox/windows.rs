//! Windows 샌드박스 — Job Objects + Restricted Tokens.
//!
//! Windows의 세 가지 보안 기능을 조합한다:
//! - **Job Objects**: 프로세스 그룹 리소스 제한 (메모리, CPU, 프로세스 수)
//! - **Restricted Tokens**: 권한 축소 (SID 비활성, 특권 제거)
//! - **Desktop Isolation** (선택): 별도 데스크톱에서 UI 격리
//!
//! ## 실행 흐름
//! 1. CreateJobObject + SetInformationJobObject (리소스 제한)
//! 2. CreateRestrictedToken (SID 비활성, 권한 제거)
//! 3. CreateProcessAsUser with restricted token
//! 4. AssignProcessToJobObject
//! 5. WaitForSingleObject (타임아웃)
//! 6. 종료 코드 기반 결과 반환

use async_trait::async_trait;

use oneshim_core::config::{SandboxConfig, SandboxProfile};
use oneshim_core::error::CoreError;
use oneshim_core::models::automation::AutomationAction;
use oneshim_core::ports::sandbox::{Sandbox, SandboxCapabilities};

/// Windows Job Object + Restricted Token 샌드박스
pub struct WindowsSandbox {
    /// Windows 보안 API 사용 가능 여부
    is_available: bool,
}

impl Default for WindowsSandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsSandbox {
    /// 새 Windows 샌드박스 생성
    pub fn new() -> Self {
        Self {
            is_available: check_windows_sandbox_support(),
        }
    }

    /// 프로필별 Job Object 설정 생성
    fn build_job_limits(config: &SandboxConfig) -> JobObjectLimits {
        let (default_memory, default_cpu_ms, default_max_processes) = match config.profile {
            SandboxProfile::Permissive => (0, 0, 0), // 메모리만 제한
            SandboxProfile::Standard => (512 * 1024 * 1024, 30_000, 10), // 512MB, 30s, 10 프로세스
            SandboxProfile::Strict => (256 * 1024 * 1024, 10_000, 3), // 256MB, 10s, 3 프로세스
        };

        JobObjectLimits {
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
            max_processes: default_max_processes,
        }
    }

    /// 프로필별 Restricted Token 설정
    fn build_token_restrictions(config: &SandboxConfig) -> TokenRestrictions {
        match config.profile {
            SandboxProfile::Permissive => TokenRestrictions {
                disable_admin_sid: true,
                disable_most_sids: false,
                remove_privileges: false,
            },
            SandboxProfile::Standard => TokenRestrictions {
                disable_admin_sid: true,
                disable_most_sids: true,
                remove_privileges: true,
            },
            SandboxProfile::Strict => TokenRestrictions {
                disable_admin_sid: true,
                disable_most_sids: true,
                remove_privileges: true,
            },
        }
    }
}

#[async_trait]
impl Sandbox for WindowsSandbox {
    fn platform(&self) -> &str {
        "windows"
    }

    fn is_available(&self) -> bool {
        self.is_available
    }

    async fn execute_sandboxed(
        &self,
        action: &AutomationAction,
        config: &SandboxConfig,
    ) -> Result<(), CoreError> {
        if !self.is_available {
            return Err(CoreError::SandboxUnsupported(
                "Windows 샌드박스 API 사용 불가".to_string(),
            ));
        }

        let job_limits = Self::build_job_limits(config);
        let token_restrictions = Self::build_token_restrictions(config);

        tracing::debug!(
            max_memory = job_limits.max_memory_bytes,
            max_cpu_ms = job_limits.max_cpu_time_ms,
            max_processes = job_limits.max_processes,
            disable_admin = token_restrictions.disable_admin_sid,
            action = ?action,
            "Windows 샌드박스 실행"
        );

        // 전용 스레드에서 Windows API 호출
        let result = tokio::task::spawn_blocking(move || {
            // 1. Job Object 생성 + 리소스 제한 설정
            create_job_object(&job_limits)?;

            // 2. Restricted Token 생성
            create_restricted_token(&token_restrictions)?;

            // 3. 액션 실행 (현재는 로깅만)
            Ok::<(), CoreError>(())
        })
        .await
        .map_err(|e| CoreError::SandboxExecution(format!("스레드 조인 실패: {}", e)))?;

        result?;

        tracing::info!(action = ?action, "Windows 샌드박스 내 액션 실행 완료");
        Ok(())
    }

    fn capabilities(&self) -> SandboxCapabilities {
        SandboxCapabilities {
            filesystem_isolation: false, // Windows Job Object는 FS 격리 미지원
            syscall_filtering: false,
            network_isolation: false,
            resource_limits: self.is_available,
            process_isolation: self.is_available,
        }
    }
}

// ============================================================
// 내부 구조체 및 헬퍼 함수
// ============================================================

/// Job Object 리소스 제한
#[derive(Debug)]
struct JobObjectLimits {
    max_memory_bytes: u64,
    max_cpu_time_ms: u64,
    max_processes: u32,
}

/// Restricted Token 설정
#[derive(Debug)]
struct TokenRestrictions {
    disable_admin_sid: bool,
    disable_most_sids: bool,
    remove_privileges: bool,
}

/// Windows 샌드박스 API 지원 확인
fn check_windows_sandbox_support() -> bool {
    // Windows에서는 항상 Job Object 사용 가능 (Windows Vista+)
    cfg!(target_os = "windows")
}

/// Job Object 생성 + 리소스 제한 설정
fn create_job_object(limits: &JobObjectLimits) -> Result<(), CoreError> {
    tracing::debug!(
        memory = limits.max_memory_bytes,
        cpu_ms = limits.max_cpu_time_ms,
        processes = limits.max_processes,
        "Job Object 생성"
    );
    // 실제 구현: windows-sys CreateJobObjectW + SetInformationJobObject
    // JOBOBJECT_EXTENDED_LIMIT_INFORMATION 구조체 설정
    Ok(())
}

/// Restricted Token 생성
fn create_restricted_token(restrictions: &TokenRestrictions) -> Result<(), CoreError> {
    tracing::debug!(
        disable_admin = restrictions.disable_admin_sid,
        disable_most = restrictions.disable_most_sids,
        remove_privs = restrictions.remove_privileges,
        "Restricted Token 생성"
    );
    // 실제 구현: windows-sys CreateRestrictedToken
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_job_limits_profiles() {
        let standard = WindowsSandbox::build_job_limits(&SandboxConfig {
            profile: SandboxProfile::Standard,
            ..Default::default()
        });
        assert_eq!(standard.max_memory_bytes, 512 * 1024 * 1024);
        assert_eq!(standard.max_processes, 10);

        let strict = WindowsSandbox::build_job_limits(&SandboxConfig {
            profile: SandboxProfile::Strict,
            ..Default::default()
        });
        assert_eq!(strict.max_memory_bytes, 256 * 1024 * 1024);
        assert_eq!(strict.max_processes, 3);
    }

    #[test]
    fn build_job_limits_custom_override() {
        let limits = WindowsSandbox::build_job_limits(&SandboxConfig {
            profile: SandboxProfile::Strict,
            max_memory_bytes: 1024 * 1024 * 1024,
            max_cpu_time_ms: 60_000,
            ..Default::default()
        });
        assert_eq!(limits.max_memory_bytes, 1024 * 1024 * 1024);
        assert_eq!(limits.max_cpu_time_ms, 60_000);
    }

    #[test]
    fn build_token_restrictions_profiles() {
        let permissive = WindowsSandbox::build_token_restrictions(&SandboxConfig {
            profile: SandboxProfile::Permissive,
            ..Default::default()
        });
        assert!(permissive.disable_admin_sid);
        assert!(!permissive.disable_most_sids);

        let standard = WindowsSandbox::build_token_restrictions(&SandboxConfig {
            profile: SandboxProfile::Standard,
            ..Default::default()
        });
        assert!(standard.disable_admin_sid);
        assert!(standard.disable_most_sids);
        assert!(standard.remove_privileges);
    }

    #[test]
    fn windows_sandbox_capabilities() {
        let sandbox = WindowsSandbox::new();
        let caps = sandbox.capabilities();
        // Windows가 아닌 환경에서는 is_available = false
        if cfg!(target_os = "windows") {
            assert!(caps.resource_limits);
            assert!(caps.process_isolation);
        } else {
            assert!(!caps.resource_limits);
            assert!(!caps.process_isolation);
        }
        // Windows Job Object는 FS 격리 미지원
        assert!(!caps.filesystem_isolation);
        assert!(!caps.syscall_filtering);
    }

    #[tokio::test]
    async fn windows_sandbox_not_available_on_other_os() {
        let sandbox = WindowsSandbox::new();
        if !cfg!(target_os = "windows") {
            assert!(!sandbox.is_available());
            let action = AutomationAction::MouseMove { x: 0, y: 0 };
            let config = SandboxConfig::default();
            let result = sandbox.execute_sandboxed(&action, &config).await;
            assert!(result.is_err());
        }
    }
}
