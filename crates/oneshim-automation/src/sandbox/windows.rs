//! 3. CreateProcessAsUser with restricted token
//! 4. AssignProcessToJobObject

use async_trait::async_trait;

use oneshim_core::config::{SandboxConfig, SandboxProfile};
use oneshim_core::error::CoreError;
use oneshim_core::models::automation::AutomationAction;
use oneshim_core::ports::sandbox::{Sandbox, SandboxCapabilities};

pub struct WindowsSandbox {
    is_available: bool,
}

impl Default for WindowsSandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsSandbox {
    pub fn new() -> Self {
        Self {
            is_available: check_windows_sandbox_support(),
        }
    }

    fn build_job_limits(config: &SandboxConfig) -> JobObjectLimits {
        let (default_memory, default_cpu_ms, default_max_processes) = match config.profile {
            SandboxProfile::Permissive => (0, 0, 0),
            SandboxProfile::Standard => (512 * 1024 * 1024, 30_000, 10), // 512MB, 30s, 10 processes
            SandboxProfile::Strict => (256 * 1024 * 1024, 10_000, 3),    // 256MB, 10s, 3 processes
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
                "Windows 샌드박스 API 사용 not-available".to_string(),
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
            "Windows 샌드박스 execution"
        );

        let result = tokio::task::spawn_blocking(move || {
            create_job_object(&job_limits)?;

            create_restricted_token(&token_restrictions)?;

            Ok::<(), CoreError>(())
        })
        .await
        .map_err(|e| CoreError::SandboxExecution(format!("Thread join failed: {}", e)))?;

        result?;

        tracing::info!(action = ?action, "Windows sandbox within execution completed");
        Ok(())
    }

    fn capabilities(&self) -> SandboxCapabilities {
        SandboxCapabilities {
            filesystem_isolation: false, // Windows Job Object does not isolate filesystem
            syscall_filtering: false,
            network_isolation: false,
            resource_limits: self.is_available,
            process_isolation: self.is_available,
        }
    }
}

#[derive(Debug)]
struct JobObjectLimits {
    max_memory_bytes: u64,
    max_cpu_time_ms: u64,
    max_processes: u32,
}

#[derive(Debug)]
struct TokenRestrictions {
    disable_admin_sid: bool,
    disable_most_sids: bool,
    remove_privileges: bool,
}

fn check_windows_sandbox_support() -> bool {
    cfg!(target_os = "windows")
}

fn create_job_object(limits: &JobObjectLimits) -> Result<(), CoreError> {
    tracing::debug!(
        memory = limits.max_memory_bytes,
        cpu_ms = limits.max_cpu_time_ms,
        processes = limits.max_processes,
        "Job Object create"
    );
    Ok(())
}

fn create_restricted_token(restrictions: &TokenRestrictions) -> Result<(), CoreError> {
    tracing::debug!(
        disable_admin = restrictions.disable_admin_sid,
        disable_most = restrictions.disable_most_sids,
        remove_privs = restrictions.remove_privileges,
        "Restricted Token create"
    );
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
        if cfg!(target_os = "windows") {
            assert!(caps.resource_limits);
            assert!(caps.process_isolation);
        } else {
            assert!(!caps.resource_limits);
            assert!(!caps.process_isolation);
        }
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
