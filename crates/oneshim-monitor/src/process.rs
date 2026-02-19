//! 프로세스 및 활성 창 모니터링.
//!
//! `ProcessMonitor` 포트 구현.

use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::models::context::{ProcessInfo, WindowInfo};
use oneshim_core::models::event::ProcessDetail;
use oneshim_core::ports::monitor::ProcessMonitor;
use std::collections::HashSet;
use std::sync::Mutex;
use sysinfo::System;
use tracing::debug;

/// 프로세스 추적기 — `ProcessMonitor` 포트 구현
pub struct ProcessTracker {
    sys: Mutex<System>,
}

impl ProcessTracker {
    /// 새 프로세스 추적기 생성
    pub fn new() -> Self {
        Self {
            sys: Mutex::new(System::new_all()),
        }
    }
}

impl Default for ProcessTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProcessMonitor for ProcessTracker {
    async fn get_active_window(&self) -> Result<Option<WindowInfo>, CoreError> {
        // 플랫폼별 구현 호출
        #[cfg(target_os = "macos")]
        {
            crate::macos::get_active_window_macos()
        }
        #[cfg(target_os = "windows")]
        {
            crate::windows::get_active_window_windows()
        }
        #[cfg(target_os = "linux")]
        {
            crate::linux::get_active_window_linux()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            // 기타 플랫폼: 미구현
            Ok(None)
        }
    }

    async fn get_top_processes(&self, limit: usize) -> Result<Vec<ProcessInfo>, CoreError> {
        let mut sys = self
            .sys
            .lock()
            .map_err(|e| CoreError::Internal(format!("시스템 잠금 실패: {e}")))?;
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        let mut processes: Vec<ProcessInfo> = sys
            .processes()
            .values()
            .map(|p| ProcessInfo {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu_usage: p.cpu_usage(),
                memory_bytes: p.memory(),
            })
            .collect();

        // CPU 사용률 기준 내림차순 정렬
        processes.sort_by(|a, b| {
            b.cpu_usage
                .partial_cmp(&a.cpu_usage)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        processes.truncate(limit);

        debug!("상위 프로세스 {}개 수집", processes.len());
        Ok(processes)
    }

    async fn get_detailed_processes(
        &self,
        foreground_pid: Option<u32>,
        top_n: usize,
    ) -> Result<Vec<ProcessDetail>, CoreError> {
        let mut sys = self
            .sys
            .lock()
            .map_err(|e| CoreError::Internal(format!("시스템 잠금 실패: {e}")))?;
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // 모든 프로세스를 ProcessDetail로 변환
        let mut all_details: Vec<ProcessDetail> = sys
            .processes()
            .values()
            .map(|p| {
                let pid = p.pid().as_u32();
                let start_time = p.start_time();
                let running_secs = if start_time > 0 && now > start_time {
                    now - start_time
                } else {
                    0
                };

                // 실행 경로 익명화 (홈 디렉토리 마스킹)
                let exe_path = p.exe().map(|path| {
                    let path_str = path.to_string_lossy().to_string();
                    // 홈 디렉토리 패턴 마스킹
                    if path_str.contains("/Users/") {
                        path_str
                            .split("/Users/")
                            .last()
                            .and_then(|s| s.split('/').nth(1))
                            .map(|rest| format!("~/{}", rest))
                            .unwrap_or_else(|| "~/...".to_string())
                    } else if path_str.contains("\\Users\\") {
                        path_str
                            .split("\\Users\\")
                            .last()
                            .and_then(|s| s.split('\\').nth(1))
                            .map(|rest| format!("~\\{}", rest))
                            .unwrap_or_else(|| "~\\...".to_string())
                    } else {
                        path_str
                    }
                });

                ProcessDetail {
                    name: p.name().to_string_lossy().to_string(),
                    pid,
                    cpu_percent: p.cpu_usage(),
                    memory_mb: p.memory() as f64 / (1024.0 * 1024.0),
                    window_count: 0, // 플랫폼별 API로 채워야 함
                    is_foreground: foreground_pid == Some(pid),
                    running_secs,
                    executable_path: exe_path,
                }
            })
            .collect();

        // CPU 사용률 기준 정렬
        all_details.sort_by(|a, b| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Foreground + Top N 선택 (중복 제거)
        let mut result: Vec<ProcessDetail> = Vec::with_capacity(top_n + 1);
        let mut seen_pids: HashSet<u32> = HashSet::new();

        // Foreground 프로세스 우선 추가
        if let Some(fg_pid) = foreground_pid {
            if let Some(fg_detail) = all_details.iter().find(|d| d.pid == fg_pid) {
                result.push(fg_detail.clone());
                seen_pids.insert(fg_pid);
            }
        }

        // Top N 추가 (중복 제외)
        for detail in all_details {
            if result.len() > top_n {
                break;
            }
            if !seen_pids.contains(&detail.pid) {
                seen_pids.insert(detail.pid);
                result.push(detail);
            }
        }

        debug!(
            "상세 프로세스 {}개 수집 (foreground={:?})",
            result.len(),
            foreground_pid
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_top_processes() {
        let tracker = ProcessTracker::new();
        let procs = tracker.get_top_processes(5).await.unwrap();
        assert!(procs.len() <= 5);
        // 적어도 하나의 프로세스는 존재
        assert!(!procs.is_empty());
    }
}
