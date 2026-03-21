use chrono::Utc;
use oneshim_core::models::event::{Event, ProcessSnapshotEvent};
use oneshim_monitor::input_activity::InputActivityCollector;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

use super::super::config::PlatformEgressPolicy;
use super::super::Scheduler;

impl Scheduler {
    pub(in crate::scheduler) fn spawn_event_snapshot_loop(
        &self,
        detailed_process_interval: Duration,
        input_activity_interval: Duration,
        egress_policy: Arc<PlatformEgressPolicy>,
        input_collector: Arc<InputActivityCollector>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let proc_mon9 = self.process_monitor.clone();
        let storage9 = self.storage.clone();
        let uploader9 = self.batch_sink.clone();
        let input_collector9 = input_collector;
        let egress9 = egress_policy;

        // Clipboard monitor — polls system clipboard for changes each input tick.
        let clipboard_pii_level = self
            .config_manager
            .as_ref()
            .map(|cm| cm.get().privacy.pii_filter_level)
            .unwrap_or(oneshim_core::config::PiiFilterLevel::Standard);
        let clipboard_monitor = Arc::new(oneshim_monitor::clipboard::ClipboardMonitor::new(
            clipboard_pii_level,
        ));

        // File access watcher — polls monitored directories for changes each input tick.
        let file_access_config = self
            .config_manager
            .as_ref()
            .map(|cm| cm.get().file_access.clone())
            .unwrap_or_default();
        let file_watcher = Arc::new(oneshim_monitor::file_access::FileAccessWatcher::new(
            file_access_config,
        ));

        tokio::spawn(async move {
            let mut process_interval = tokio::time::interval(detailed_process_interval);
            let mut input_interval = tokio::time::interval(input_activity_interval);
            let mut foreground_pid: Option<u32> = None;

            loop {
                tokio::select! {
                    _ = process_interval.tick() => {
                        match proc_mon9.get_detailed_processes(foreground_pid, 10).await {
                            Ok(processes) => {
                                let total = processes.len() as u32;

                                foreground_pid = processes.iter()
                                    .find(|p| p.is_foreground)
                                    .map(|p| p.pid);

                                let snapshot_event = ProcessSnapshotEvent {
                                    timestamp: Utc::now(),
                                    processes,
                                    total_process_count: total,
                                };

                                let event = Event::Process(snapshot_event);
                                if let Err(e) = storage9.save_event(&event).await {
                                    warn!("event save failure: {e}");
                                }

                                if let Some(ref sink) = uploader9 {
                                    if let Some(upload_event) = egress9.prepare_event_for_upload(event) {
                                        sink.enqueue(upload_event);
                                    }
                                }

                                debug!(": {}items", total);
                            }
                            Err(e) => {
                                warn!("collect failure: {e}");
                            }
                        }
                    }
                    _ = input_interval.tick() => {
                        let input_event = input_collector9.take_snapshot();

                        if input_event.mouse.click_count > 0
                            || input_event.keyboard.total_keystrokes > 0
                            || input_event.mouse.scroll_count > 0
                        {
                            let event = Event::Input(input_event);
                            if let Err(e) = storage9.save_event(&event).await {
                                warn!("event save failure: {e}");
                            }

                            if let Some(ref sink) = uploader9 {
                                if let Some(upload_event) = egress9.prepare_event_for_upload(event) {
                                    sink.enqueue(upload_event);
                                }
                            }
                        }

                        // Poll clipboard for changes (non-blocking on macOS/Linux/Windows).
                        // Runs on the same cadence as input activity collection.
                        let cb = clipboard_monitor.clone();
                        if let Some(clip_event) = tokio::task::spawn_blocking(move || {
                            cb.poll_system_clipboard()
                        }).await.unwrap_or(None) {
                            debug!(
                                content_type = ?clip_event.content_type,
                                chars = clip_event.char_count,
                                "clipboard change detected"
                            );
                            let event = Event::Clipboard(clip_event);
                            if let Err(e) = storage9.save_event(&event).await {
                                warn!("clipboard event save failure: {e}");
                            }
                        }

                        // Poll monitored directories for file changes.
                        let fw = file_watcher.clone();
                        let file_events = tokio::task::spawn_blocking(move || {
                            fw.poll_changes()
                        }).await.unwrap_or_default();
                        for file_event in file_events {
                            debug!(
                                event_type = ?file_event.event_type,
                                path = %file_event.relative_path.display(),
                                "file change detected"
                            );
                            let event = Event::FileAccess(file_event);
                            if let Err(e) = storage9.save_event(&event).await {
                                warn!("file access event save failure: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("server event collect ended");
                        break;
                    }
                }
            }
        })
    }

    pub(in crate::scheduler) fn spawn_notification_loop(
        &self,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let notif7 = self.notification_manager.clone();

        tokio::spawn(async move {
            let notif = match notif7 {
                Some(n) => n,
                None => {
                    let _ = shutdown_rx.changed().await;
                    return;
                }
            };

            let mut interval = tokio::time::interval(Duration::from_secs(60)); // 1min
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        notif.check_long_session().await;
                    }
                    _ = shutdown_rx.changed() => {
                        info!("notification ended");
                        break;
                    }
                }
            }
        })
    }
}
