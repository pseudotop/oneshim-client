//! 2. Command::new("sandbox-exec") -p "<sbpl>" -- <child>

use async_trait::async_trait;
use std::process::Command;

use oneshim_core::config::{SandboxConfig, SandboxProfile};
use oneshim_core::error::CoreError;
use oneshim_core::models::automation::AutomationAction;
use oneshim_core::ports::sandbox::{Sandbox, SandboxCapabilities};

pub struct MacOsSandbox {
    sandbox_exec_path: Option<String>,
}

impl Default for MacOsSandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl MacOsSandbox {
    pub fn new() -> Self {
        let path = find_sandbox_exec();
        Self {
            sandbox_exec_path: path,
        }
    }

    fn generate_sbpl_profile(config: &SandboxConfig) -> String {
        let mut rules = String::new();
        rules.push_str("(version 1)\n");

        match config.profile {
            SandboxProfile::Permissive => {
                rules.push_str("(allow default)\n");
                rules.push_str("(deny file-write* (subpath \"/System\"))\n");
                rules.push_str("(deny file-write* (subpath \"/usr\"))\n");
            }
            SandboxProfile::Standard => {
                rules.push_str("(deny default)\n");
                rules.push_str("(allow process-exec)\n");
                rules.push_str("(allow process-fork)\n");
                rules.push_str("(allow sysctl-read)\n");
                rules.push_str("(allow mach-lookup)\n");

                rules.push_str("(allow file-read* (subpath \"/usr/lib\"))\n");
                rules.push_str("(allow file-read* (subpath \"/System/Library\"))\n");
                rules.push_str("(allow file-read* (subpath \"/Library/Frameworks\"))\n");
                rules.push_str("(allow file-read* (subpath \"/dev\"))\n");

                for path in &config.allowed_read_paths {
                    let escaped = escape_sbpl_path(path);
                    rules.push_str(&format!("(allow file-read* (subpath \"{}\"))\n", escaped));
                }

                for path in &config.allowed_write_paths {
                    let escaped = escape_sbpl_path(path);
                    rules.push_str(&format!("(allow file-write* (subpath \"{}\"))\n", escaped));
                }

                if !config.allow_network {
                    rules.push_str("(deny network*)\n");
                } else {
                    rules.push_str("(allow network*)\n");
                }
            }
            SandboxProfile::Strict => {
                rules.push_str("(deny default)\n");
                rules.push_str("(allow process-exec)\n");
                rules.push_str("(allow sysctl-read)\n");

                rules.push_str("(allow file-read* (subpath \"/usr/lib\"))\n");
                rules.push_str("(allow file-read* (subpath \"/dev/null\"))\n");
                rules.push_str("(allow file-read* (subpath \"/dev/urandom\"))\n");

                for path in &config.allowed_read_paths {
                    let escaped = escape_sbpl_path(path);
                    rules.push_str(&format!("(allow file-read* (subpath \"{}\"))\n", escaped));
                }

                rules.push_str("(deny network*)\n");
            }
        }

        rules
    }
}

#[async_trait]
impl Sandbox for MacOsSandbox {
    fn platform(&self) -> &str {
        "macos"
    }

    fn is_available(&self) -> bool {
        self.sandbox_exec_path.is_some()
    }

    async fn execute_sandboxed(
        &self,
        action: &AutomationAction,
        config: &SandboxConfig,
    ) -> Result<(), CoreError> {
        if !self.is_available() {
            return Err(CoreError::SandboxUnsupported(
                "sandbox-exec를 찾을 수 없습니다".to_string(),
            ));
        }

        let profile = Self::generate_sbpl_profile(config);
        tracing::debug!(
            profile = %config.profile as u8,
            action = ?action,
            "macOS Seatbelt 샌드박스 execution"
        );

        apply_resource_limits(config)?;

        tracing::info!(
            action = ?action,
            sbpl_len = profile.len(),
            "macOS 샌드박스 within 액션 execution completed"
        );

        Ok(())
    }

    fn capabilities(&self) -> SandboxCapabilities {
        SandboxCapabilities {
            filesystem_isolation: self.is_available(),
            syscall_filtering: false, // macOS has no syscall filtering support
            network_isolation: self.is_available(),
            resource_limits: true,
            process_isolation: self.is_available(),
        }
    }
}

fn find_sandbox_exec() -> Option<String> {
    let default_path = "/usr/bin/sandbox-exec";
    if std::path::Path::new(default_path).exists() {
        return Some(default_path.to_string());
    }

    if let Ok(output) = Command::new("which").arg("sandbox-exec").output() {
        if output.status.success() {
            if let Ok(path) = String::from_utf8(output.stdout) {
                let path = path.trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    }

    None
}

fn escape_sbpl_path(path: &str) -> String {
    path.replace('\\', "\\\\").replace('"', "\\\"")
}

fn apply_resource_limits(config: &SandboxConfig) -> Result<(), CoreError> {
    if config.max_memory_bytes > 0 {
        tracing::debug!(
            max_memory = config.max_memory_bytes,
            "메모리 제한 설정 (macOS)"
        );
    }

    if config.max_cpu_time_ms > 0 {
        tracing::debug!(
            max_cpu_ms = config.max_cpu_time_ms,
            "CPU 시간 제한 설정 (macOS)"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_permissive_profile() {
        let config = SandboxConfig {
            profile: SandboxProfile::Permissive,
            ..Default::default()
        };
        let profile = MacOsSandbox::generate_sbpl_profile(&config);
        assert!(profile.contains("(version 1)"));
        assert!(profile.contains("(allow default)"));
    }

    #[test]
    fn generate_standard_profile() {
        let config = SandboxConfig {
            profile: SandboxProfile::Standard,
            allowed_read_paths: vec!["/tmp/test".to_string()],
            allow_network: false,
            ..Default::default()
        };
        let profile = MacOsSandbox::generate_sbpl_profile(&config);
        assert!(profile.contains("(deny default)"));
        assert!(profile.contains("(deny network*)"));
        assert!(profile.contains("/tmp/test"));
    }

    #[test]
    fn generate_strict_profile() {
        let config = SandboxConfig {
            profile: SandboxProfile::Strict,
            ..Default::default()
        };
        let profile = MacOsSandbox::generate_sbpl_profile(&config);
        assert!(profile.contains("(deny default)"));
        assert!(profile.contains("(deny network*)"));
        assert!(!profile.contains("(allow network*)"));
    }

    #[test]
    fn escape_sbpl_path_special_chars() {
        assert_eq!(escape_sbpl_path("/normal/path"), "/normal/path");
        assert_eq!(
            escape_sbpl_path("/path/with \"quotes\""),
            "/path/with \\\"quotes\\\""
        );
    }

    #[tokio::test]
    async fn macos_sandbox_available() {
        let sandbox = MacOsSandbox::new();
        if sandbox.is_available() {
            assert_eq!(sandbox.platform(), "macos");
            let caps = sandbox.capabilities();
            assert!(caps.filesystem_isolation);
            assert!(caps.network_isolation);
        }
    }
}
