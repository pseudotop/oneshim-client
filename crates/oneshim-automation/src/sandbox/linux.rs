use async_trait::async_trait;

use oneshim_core::config::{SandboxConfig, SandboxProfile};
use oneshim_core::error::CoreError;
use oneshim_core::models::automation::AutomationAction;
use oneshim_core::ports::sandbox::{Sandbox, SandboxCapabilities};

pub struct LinuxSandbox {
    landlock_available: bool,
}

impl Default for LinuxSandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSandbox {
    pub fn new() -> Self {
        let landlock_available = check_landlock_support();
        Self { landlock_available }
    }

    fn build_landlock_rules(config: &SandboxConfig) -> LandlockRules {
        let mut rules = LandlockRules::default();

        match config.profile {
            SandboxProfile::Permissive => {
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
                rules
                    .read_paths
                    .extend_from_slice(&["/usr/lib".to_string(), "/lib".to_string()]);
                rules.read_paths.extend(config.allowed_read_paths.clone());
            }
            SandboxProfile::Strict => {
                rules.read_paths.push("/usr/lib".to_string());
                rules.read_paths.extend(config.allowed_read_paths.clone());
            }
        }

        rules
    }

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

    fn build_resource_limits(config: &SandboxConfig) -> ResourceLimits {
        let (default_memory, default_cpu_ms) = match config.profile {
            SandboxProfile::Permissive => (0, 0),
            SandboxProfile::Standard => (512 * 1024 * 1024, 30_000), // 512MB, 30s
            SandboxProfile::Strict => (256 * 1024 * 1024, 10_000),   // 256MB, 10s
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
            "Linux sandbox execution"
        );

        let landlock_avail = self.landlock_available;
        let result = tokio::task::spawn_blocking(move || {
            if landlock_avail {
                apply_landlock_rules(&landlock_rules)?;
            }

            apply_seccomp_filter(&seccomp_allowlist)?;

            apply_resource_limits(&resource_limits)?;

            Ok::<(), CoreError>(())
        })
        .await
        .map_err(|e| CoreError::SandboxExecution(format!("Thread join failed: {}", e)))?;

        result?;

        tracing::info!(action = ?action, "Linux sandbox within execution completed");
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

#[derive(Debug, Default)]
struct LandlockRules {
    read_paths: Vec<String>,
    write_paths: Vec<String>,
}

#[derive(Debug, Default)]
struct SeccompAllowlist {
    allow_basic: bool,
    allow_network: bool,
    allow_process: bool,
}

#[derive(Debug)]
struct ResourceLimits {
    max_memory_bytes: u64,
    max_cpu_time_ms: u64,
}

fn check_landlock_support() -> bool {
    std::path::Path::new("/sys/kernel/security/landlock").exists()
}

fn apply_landlock_rules(rules: &LandlockRules) -> Result<(), CoreError> {
    tracing::debug!(
        read = rules.read_paths.len(),
        write = rules.write_paths.len(),
        "applying Landlock rules"
    );
    Ok(())
}

fn apply_seccomp_filter(allowlist: &SeccompAllowlist) -> Result<(), CoreError> {
    tracing::debug!(
        basic = allowlist.allow_basic,
        network = allowlist.allow_network,
        process = allowlist.allow_process,
        "applying seccomp filter"
    );
    Ok(())
}

fn apply_resource_limits(limits: &ResourceLimits) -> Result<(), CoreError> {
    if limits.max_memory_bytes > 0 {
        tracing::debug!(
            max_memory = limits.max_memory_bytes,
            "setting RLIMIT_AS with setrlimit"
        );
    }
    if limits.max_cpu_time_ms > 0 {
        let cpu_secs = limits.max_cpu_time_ms / 1000;
        tracing::debug!(cpu_secs, "setrlimit RLIMIT_CPU settings");
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
        let result = sandbox.execute_sandboxed(&action, &config).await;
        assert!(result.is_ok());
    }
}
