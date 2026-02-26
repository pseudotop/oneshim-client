use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use oneshim_core::error::CoreError;
use oneshim_core::models::context::WindowInfo;
use oneshim_core::models::gui::{ExecutionBinding, FocusSnapshot, FocusValidation};
use oneshim_core::ports::focus_probe::FocusProbe;
use oneshim_core::ports::monitor::ProcessMonitor;

pub struct ProcessMonitorFocusProbe {
    process_monitor: Arc<dyn ProcessMonitor>,
}

impl ProcessMonitorFocusProbe {
    pub fn new(process_monitor: Arc<dyn ProcessMonitor>) -> Self {
        Self { process_monitor }
    }
}

#[async_trait]
impl FocusProbe for ProcessMonitorFocusProbe {
    async fn current_focus(&self) -> Result<FocusSnapshot, CoreError> {
        let window = self
            .process_monitor
            .get_active_window()
            .await?
            .ok_or_else(|| {
                CoreError::ServiceUnavailable("No active window available".to_string())
            })?;

        Ok(window_to_focus_snapshot(window))
    }

    async fn validate_execution_binding(
        &self,
        binding: &ExecutionBinding,
    ) -> Result<FocusValidation, CoreError> {
        let window = self.process_monitor.get_active_window().await?;

        let Some(window) = window else {
            return Ok(FocusValidation {
                valid: false,
                reason: Some("No active window available".to_string()),
                current_focus: None,
            });
        };

        let snapshot = window_to_focus_snapshot(window);
        let focus_match = snapshot.focus_hash == binding.focus_hash;
        let app_match = binding
            .app_name
            .as_deref()
            .map(|value| value == snapshot.app_name)
            .unwrap_or(true);
        let pid_match = binding
            .pid
            .map(|value| value == snapshot.pid)
            .unwrap_or(true);
        let valid = focus_match && app_match && pid_match;

        let reason = if valid {
            None
        } else {
            Some(format!(
                "focus_match={} app_match={} pid_match={}",
                focus_match, app_match, pid_match
            ))
        };

        Ok(FocusValidation {
            valid,
            reason,
            current_focus: Some(snapshot),
        })
    }
}

fn window_to_focus_snapshot(window: WindowInfo) -> FocusSnapshot {
    let focus_hash = hash_window_info(&window);
    FocusSnapshot {
        app_name: window.app_name,
        window_title: window.title,
        pid: window.pid,
        bounds: window.bounds,
        captured_at: chrono::Utc::now(),
        focus_hash,
    }
}

fn hash_window_info(window: &WindowInfo) -> String {
    let mut hasher = Sha256::new();
    hasher.update(window.app_name.as_bytes());
    hasher.update(b"|");
    hasher.update(window.title.as_bytes());
    hasher.update(b"|");
    hasher.update(window.pid.to_le_bytes());

    if let Some(bounds) = window.bounds {
        hasher.update(bounds.x.to_le_bytes());
        hasher.update(bounds.y.to_le_bytes());
        hasher.update(bounds.width.to_le_bytes());
        hasher.update(bounds.height.to_le_bytes());
    }

    let digest = hasher.finalize();
    encode_hex(digest.as_slice())
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::models::context::WindowBounds;

    #[test]
    fn hash_changes_when_window_title_changes() {
        let base = WindowInfo {
            title: "editor".to_string(),
            app_name: "Code".to_string(),
            pid: 101,
            bounds: Some(WindowBounds {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            }),
        };

        let mut changed = base.clone();
        changed.title = "terminal".to_string();

        assert_ne!(hash_window_info(&base), hash_window_info(&changed));
    }
}
