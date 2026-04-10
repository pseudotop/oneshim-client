//! Linux sandbox — Landlock, seccomp, and resource limits enforcement.
//!
//! **Enforcement status:**
//! - **Resource limits**: Enforced via `setrlimit(2)` — always available.
//! - **Landlock**: Enforced when `linux-sandbox` feature is enabled and kernel >= 5.13.
//!   Uses ABI v3 with graceful fallback if unsupported.
//! - **seccomp-BPF**: Deferred — requires `seccompiler` crate + arch-specific
//!   syscall tables (x86_64/aarch64).

use async_trait::async_trait;

use crate::error::AutomationError;
use oneshim_core::config::{SandboxConfig, SandboxProfile};
use oneshim_core::error::CoreError;
use oneshim_core::models::automation::AutomationAction;
use oneshim_core::ports::sandbox::{Sandbox, SandboxCapabilities};

/// Linux sandbox adapter.
///
/// Detects Landlock availability at construction time by probing
/// `/sys/kernel/security/landlock`. Even when Landlock is unavailable,
/// seccomp and resource-limit rules are still constructed (but not yet
/// enforced -- see module-level docs).
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

    /// Returns `true` when resource limits are enforceable (always on Linux)
    /// or when Landlock is available.
    fn is_available(&self) -> bool {
        true // Resource limits (setrlimit) are always available on Linux
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

            Ok::<(), AutomationError>(())
        })
        .await
        .map_err(|e| CoreError::SandboxExecution(format!("Thread join failed: {}", e)))?;

        result.map_err(CoreError::from)?;

        tracing::info!(action = ?action, "Linux sandbox within execution completed");
        Ok(())
    }

    /// Report capabilities based on actual enforcement availability.
    fn capabilities(&self) -> SandboxCapabilities {
        SandboxCapabilities {
            #[cfg(feature = "linux-sandbox")]
            filesystem_isolation: self.landlock_available,
            #[cfg(not(feature = "linux-sandbox"))]
            filesystem_isolation: false,
            syscall_filtering: false, // seccomp deferred
            network_isolation: false, // requires seccomp
            resource_limits: true,    // setrlimit always available
            process_isolation: false, // requires subprocess model
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

/// Apply Landlock filesystem isolation rules.
///
/// When the `linux-sandbox` feature is enabled and kernel supports Landlock,
/// restricts filesystem access to the configured read/write paths.
/// Otherwise logs the rules and returns `Ok(())`.
fn apply_landlock_rules(rules: &LandlockRules) -> Result<(), AutomationError> {
    #[cfg(feature = "linux-sandbox")]
    {
        use landlock::{
            path_beneath_rules, Access, AccessFs, Ruleset, RulesetAttr, RulesetCreatedAttr,
            RulesetStatus, ABI,
        };

        let abi = ABI::V3;
        let read_access = AccessFs::from_read(abi);
        let write_access = AccessFs::from_all(abi);

        let mut ruleset = Ruleset::default()
            .handle_access(write_access)
            .map_err(|e| AutomationError::SandboxEnforcement(format!("Landlock ruleset: {e}")))?
            .create()
            .map_err(|e| AutomationError::SandboxEnforcement(format!("Landlock create: {e}")))?;

        // Add read-only path rules
        let read_rules = path_beneath_rules(
            rules
                .read_paths
                .iter()
                .filter(|p| std::path::Path::new(p).exists()),
            read_access,
        );
        for rule in read_rules {
            if let Ok(r) = rule {
                let _ = ruleset.add_rule(r);
            }
        }

        // Add read-write path rules
        let write_rules = path_beneath_rules(
            rules
                .write_paths
                .iter()
                .filter(|p| std::path::Path::new(p).exists()),
            write_access,
        );
        for rule in write_rules {
            if let Ok(r) = rule {
                let _ = ruleset.add_rule(r);
            }
        }

        match ruleset.restrict_self() {
            Ok(status) => {
                let enforced = status.ruleset != RulesetStatus::NotSupported;
                tracing::info!(
                    enforced,
                    read = rules.read_paths.len(),
                    write = rules.write_paths.len(),
                    "Landlock filesystem isolation applied"
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Landlock restrict_self failed: {e} — continuing without FS isolation"
                );
            }
        }

        return Ok(());
    }

    #[cfg(not(feature = "linux-sandbox"))]
    {
        tracing::debug!(
            read = rules.read_paths.len(),
            write = rules.write_paths.len(),
            "Landlock rules built (enforcement requires linux-sandbox feature)"
        );
        Ok(())
    }
}

/// Apply seccomp-BPF syscall filtering.
///
/// **Deferred** — requires `seccompiler` crate with arch-specific syscall tables.
/// Logs the allowlist and returns `Ok(())`.
fn apply_seccomp_filter(allowlist: &SeccompAllowlist) -> Result<(), AutomationError> {
    tracing::debug!(
        basic = allowlist.allow_basic,
        network = allowlist.allow_network,
        process = allowlist.allow_process,
        "seccomp filter (enforcement deferred — requires seccompiler crate)"
    );
    Ok(())
}

/// Apply resource limits via `setrlimit(2)`.
///
/// Sets `RLIMIT_AS` (virtual memory) and `RLIMIT_CPU` (CPU seconds) on the
/// current process. Only effective when applied before exec in a child process.
fn apply_resource_limits(limits: &ResourceLimits) -> Result<(), AutomationError> {
    if limits.max_memory_bytes > 0 {
        let rlim = libc::rlimit {
            rlim_cur: limits.max_memory_bytes,
            rlim_max: limits.max_memory_bytes,
        };
        let ret = unsafe { libc::setrlimit(libc::RLIMIT_AS, &rlim) };
        if ret != 0 {
            let errno = std::io::Error::last_os_error();
            tracing::warn!("setrlimit RLIMIT_AS failed: {errno}");
        } else {
            tracing::debug!(max_memory = limits.max_memory_bytes, "RLIMIT_AS set");
        }
    }
    if limits.max_cpu_time_ms > 0 {
        let cpu_secs = limits.max_cpu_time_ms / 1000;
        if cpu_secs > 0 {
            let rlim = libc::rlimit {
                rlim_cur: cpu_secs,
                rlim_max: cpu_secs,
            };
            let ret = unsafe { libc::setrlimit(libc::RLIMIT_CPU, &rlim) };
            if ret != 0 {
                let errno = std::io::Error::last_os_error();
                tracing::warn!("setrlimit RLIMIT_CPU failed: {errno}");
            } else {
                tracing::debug!(cpu_secs, "RLIMIT_CPU set");
            }
        }
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
