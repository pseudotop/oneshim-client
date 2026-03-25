use anyhow::Result;
use oneshim_core::config::AppConfig;
#[cfg(feature = "analysis")]
use oneshim_network::analysis_client::AnalysisClient;
#[cfg(feature = "server")]
use oneshim_network::auth::TokenManager;
#[cfg(feature = "server")]
use oneshim_network::batch_uploader::BatchUploader;
#[cfg(feature = "server")]
use oneshim_network::http_client::HttpApiClient;
#[cfg(feature = "server")]
use oneshim_network::sse_client::SseStreamClient;
use oneshim_storage::frame_storage::FrameFileStorage;
use oneshim_vision::processor::EdgeFrameProcessor;
use oneshim_vision::trigger::SmartCaptureTrigger;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::focus_analyzer::{FocusAnalyzer, FocusStorage};
use crate::notification_manager::NotificationManager;
use crate::scheduler::SchedulerConfig;

#[allow(dead_code)] // suggestion_receiver is read only with feature = "server"
pub(crate) struct AgentSupportContext {
    pub(crate) frame_storage: Arc<FrameFileStorage>,
    pub(crate) system_monitor: Arc<oneshim_monitor::system::SysInfoMonitor>,
    pub(crate) process_monitor: Arc<dyn oneshim_core::ports::monitor::ProcessMonitor>,
    pub(crate) activity_monitor: Arc<oneshim_monitor::activity::ActivityTracker>,
    pub(crate) capture_trigger: Arc<dyn oneshim_core::ports::vision::CaptureTrigger>,
    pub(crate) frame_processor: Arc<dyn oneshim_core::ports::vision::FrameProcessor>,
    pub(crate) scheduler_config: SchedulerConfig,
    pub(crate) batch_sink_opt: Option<Arc<dyn oneshim_core::ports::batch_sink::BatchSink>>,
    pub(crate) api_client_opt: Option<Arc<dyn oneshim_core::ports::api_client::ApiClient>>,
    pub(crate) notification_manager: Arc<NotificationManager>,
    pub(crate) focus_analyzer: Arc<FocusAnalyzer>,
    pub(crate) context_analyzer: Option<Arc<oneshim_analysis::ContextAnalyzer>>,
    pub(crate) suggestion_receiver: Option<Arc<oneshim_suggestion::receiver::SuggestionReceiver>>,
}

type BatchSinkPort = Arc<dyn oneshim_core::ports::batch_sink::BatchSink>;
type ApiClientPort = Arc<dyn oneshim_core::ports::api_client::ApiClient>;
#[cfg(feature = "server")]
type SseClientPort = Arc<SseStreamClient>;
#[cfg(feature = "server")]
type ServerTransportPorts = (
    Option<BatchSinkPort>,
    Option<ApiClientPort>,
    Option<SseClientPort>,
);
#[cfg(not(feature = "server"))]
type ServerTransportPorts = (Option<BatchSinkPort>, Option<ApiClientPort>);

pub(crate) struct AgentSupportContextBuilder<'a> {
    data_dir: &'a Path,
    config: &'a AppConfig,
    focus_storage: Arc<dyn FocusStorage>,
    storage: Option<Arc<dyn oneshim_core::ports::storage::StorageService>>,
    app_handle: Option<tauri::AppHandle>,
    /// Pre-created shared suggestion queue from SuggestionManager.
    /// When set, the SuggestionReceiver will use this queue instead of creating its own.
    shared_suggestion_queue:
        Option<Arc<tokio::sync::Mutex<oneshim_suggestion::queue::SuggestionQueue>>>,
}

impl<'a> AgentSupportContextBuilder<'a> {
    pub(crate) fn new(
        data_dir: &'a Path,
        config: &'a AppConfig,
        focus_storage: Arc<dyn FocusStorage>,
    ) -> Self {
        Self {
            data_dir,
            config,
            focus_storage,
            storage: None,
            app_handle: None,
            shared_suggestion_queue: None,
        }
    }

    pub(crate) fn with_storage(
        mut self,
        storage: Arc<dyn oneshim_core::ports::storage::StorageService>,
    ) -> Self {
        self.storage = Some(storage);
        self
    }

    pub(crate) fn with_app_handle(mut self, handle: tauri::AppHandle) -> Self {
        self.app_handle = Some(handle);
        self
    }

    pub(crate) fn with_shared_suggestion_queue(
        mut self,
        queue: Arc<tokio::sync::Mutex<oneshim_suggestion::queue::SuggestionQueue>>,
    ) -> Self {
        self.shared_suggestion_queue = Some(queue);
        self
    }

    #[cfg(feature = "analysis")]
    fn build_context_analyzer(&self) -> Option<Arc<oneshim_analysis::ContextAnalyzer>> {
        if !self.config.analysis.enabled {
            return None;
        }

        let storage = match self.storage.as_ref() {
            Some(s) => s.clone(),
            None => {
                tracing::warn!("analysis enabled but no storage available");
                return None;
            }
        };

        let analysis_provider: Arc<dyn oneshim_core::ports::analysis_provider::AnalysisProvider> =
            if let Some(ref llm_api) = self.config.ai_provider.llm_api {
                Arc::new(AnalysisClient::new(llm_api))
            } else {
                tracing::warn!("analysis enabled but no LLM provider configured");
                return None;
            };

        let pattern_miner = oneshim_analysis::PatternMiner::new();
        let pii_level = self.config.privacy.pii_filter_level;
        let context_assembler = oneshim_analysis::ContextAssembler::new(Box::new(move |text| {
            oneshim_vision::privacy::sanitize_title_with_level(text, pii_level)
        }));

        Some(Arc::new(oneshim_analysis::ContextAnalyzer::new(
            storage,
            analysis_provider,
            pattern_miner,
            context_assembler,
            self.config.analysis.clone(),
        )))
    }

    #[cfg(not(feature = "analysis"))]
    fn build_context_analyzer(&self) -> Option<Arc<oneshim_analysis::ContextAnalyzer>> {
        None
    }

    pub(crate) async fn build(self) -> Result<AgentSupportContext> {
        let frame_storage = Arc::new(
            FrameFileStorage::new(
                self.data_dir.to_path_buf(),
                self.config.storage.max_storage_mb,
                self.config.storage.retention_days,
            )
            .await?,
        );

        let system_monitor = Arc::new(oneshim_monitor::system::SysInfoMonitor::new());
        let process_monitor: Arc<dyn oneshim_core::ports::monitor::ProcessMonitor> =
            Arc::new(oneshim_monitor::process::ProcessTracker::new());
        let activity_monitor = Arc::new(oneshim_monitor::activity::ActivityTracker::new(
            process_monitor.clone(),
        ));

        let capture_trigger: Arc<dyn oneshim_core::ports::vision::CaptureTrigger> = Arc::new(
            SmartCaptureTrigger::new(self.config.vision.capture_throttle_ms),
        );
        let ocr_tessdata = std::env::var("ONESHIM_TESSDATA").ok().map(PathBuf::from);
        let frame_processor: Arc<dyn oneshim_core::ports::vision::FrameProcessor> =
            Arc::new(EdgeFrameProcessor::new(
                self.config.vision.thumbnail_width,
                self.config.vision.thumbnail_height,
                ocr_tessdata,
            ));

        let session_id = generate_session_id();
        #[cfg(feature = "server")]
        let (batch_sink_opt, api_client_opt, sse_client_opt) =
            build_server_transports(self.config, &session_id)?;
        #[cfg(not(feature = "server"))]
        let (batch_sink_opt, api_client_opt) = build_server_transports(self.config, &session_id)?;

        let notifier: Arc<dyn oneshim_core::ports::notifier::DesktopNotifier> =
            if let Some(handle) = self.app_handle.clone() {
                Arc::new(TauriNotifier::new(handle))
            } else {
                Arc::new(LogOnlyNotifier)
            };
        let notification_manager = Arc::new(NotificationManager::new(
            self.config.notification.clone(),
            notifier.clone(),
        ));
        let focus_analyzer = Arc::new(FocusAnalyzer::with_defaults(
            self.focus_storage.clone(),
            notifier.clone(),
        ));

        let context_analyzer = self.build_context_analyzer();

        // Build SuggestionReceiver when SSE client is available and suggestions enabled.
        // When a shared_suggestion_queue is provided (from SuggestionManager), the receiver
        // uses it so SSE-received suggestions are visible in IPC queries.
        #[cfg(feature = "server")]
        let suggestion_receiver = if let Some(sse_client) = sse_client_opt {
            if self.config.suggestions.enabled {
                let queue = self.shared_suggestion_queue.unwrap_or_else(|| {
                    Arc::new(tokio::sync::Mutex::new(
                        oneshim_suggestion::queue::SuggestionQueue::new(
                            self.config.analysis.max_suggestions,
                        ),
                    ))
                });
                Some(Arc::new(
                    oneshim_suggestion::receiver::SuggestionReceiver::new(
                        sse_client,
                        Some(notifier),
                        queue,
                    ),
                ))
            } else {
                None
            }
        } else {
            None
        };
        #[cfg(not(feature = "server"))]
        let suggestion_receiver = None;

        let scheduler_config = SchedulerConfig {
            poll_interval: Duration::from_millis(self.config.monitor.poll_interval_ms),
            metrics_interval: Duration::from_secs(5),
            process_interval: Duration::from_secs(10),
            detailed_process_interval: Duration::from_secs(30),
            input_activity_interval: Duration::from_secs(30),
            sync_interval: Duration::from_millis(self.config.monitor.sync_interval_ms),
            heartbeat_interval: Duration::from_millis(self.config.monitor.heartbeat_interval_ms),
            aggregation_interval: Duration::from_secs(3600),
            session_id,
            external_data_policy: self.config.ai_provider.external_data_policy,
            privacy_config: self.config.privacy.clone(),
            idle_threshold_secs: 300,
            upload_enabled: self.config.monitor.upload_enabled,
            analysis_config: self.config.analysis.clone(),
            cross_device_sync_interval: Duration::from_secs(300), // 5 min default
        };

        Ok(AgentSupportContext {
            frame_storage,
            system_monitor,
            process_monitor,
            activity_monitor,
            capture_trigger,
            frame_processor,
            scheduler_config,
            batch_sink_opt,
            api_client_opt,
            notification_manager,
            focus_analyzer,
            context_analyzer,
            suggestion_receiver,
        })
    }
}

#[cfg(feature = "server")]
fn build_server_transports(config: &AppConfig, session_id: &str) -> Result<ServerTransportPorts> {
    let token_manager = Arc::new(
        TokenManager::new_with_tls(
            &config.server.base_url,
            &config.tls,
            Some(config.request_timeout()),
        )
        .map_err(|e| anyhow::anyhow!("failed to build TLS-aware TokenManager: {e}"))?,
    );
    let api_client = Arc::new(HttpApiClient::new_with_tls(
        &config.server.base_url,
        token_manager.clone(),
        config.request_timeout(),
        &config.tls,
    )?);
    let batch_uploader = Arc::new(BatchUploader::new(
        api_client.clone(),
        session_id.to_string(),
        100,
        3,
    ));
    let sse_client = Arc::new(
        SseStreamClient::new_with_tls(
            &config.server.base_url,
            token_manager,
            config.server.sse_max_retry_secs,
            &config.tls,
        )
        .map_err(|e| anyhow::anyhow!("failed to build SSE client: {e}"))?,
    );

    Ok((Some(batch_uploader), Some(api_client), Some(sse_client)))
}

#[cfg(not(feature = "server"))]
fn build_server_transports(_config: &AppConfig, _session_id: &str) -> Result<ServerTransportPorts> {
    Ok((None, None))
}

pub(crate) fn generate_session_id() -> String {
    use std::hash::{Hash, Hasher};

    let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    let rand_part = hasher.finish() as u32;
    format!("sess_{ts}_{rand_part:08x}")
}

/// Notifier that bridges to Tauri's native notification plugin.
pub(crate) struct TauriNotifier {
    app_handle: tauri::AppHandle,
}

impl TauriNotifier {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

#[async_trait::async_trait]
impl oneshim_core::ports::notifier::DesktopNotifier for TauriNotifier {
    async fn show_suggestion(
        &self,
        suggestion: &oneshim_core::models::suggestion::Suggestion,
    ) -> Result<(), oneshim_core::error::CoreError> {
        let title = match suggestion.priority {
            oneshim_core::models::suggestion::Priority::Critical => "ONESHIM \u{2014} Urgent",
            oneshim_core::models::suggestion::Priority::High => "ONESHIM \u{2014} Important",
            oneshim_core::models::suggestion::Priority::Medium => "ONESHIM",
            oneshim_core::models::suggestion::Priority::Low => "ONESHIM \u{2014} Info",
        };
        let body = suggestion.content.chars().take(200).collect::<String>();
        if let Err(e) = tauri_plugin_notification::NotificationExt::notification(&self.app_handle)
            .builder()
            .title(title)
            .body(&body)
            .show()
        {
            tracing::warn!("native notification failed, suppressing: {e}");
        }
        Ok(())
    }

    async fn show_notification(
        &self,
        title: &str,
        body: &str,
    ) -> Result<(), oneshim_core::error::CoreError> {
        if let Err(e) = tauri_plugin_notification::NotificationExt::notification(&self.app_handle)
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            tracing::warn!("native notification failed, suppressing: {e}");
        }
        Ok(())
    }

    async fn show_error(&self, message: &str) -> Result<(), oneshim_core::error::CoreError> {
        if let Err(e) = tauri_plugin_notification::NotificationExt::notification(&self.app_handle)
            .builder()
            .title("ONESHIM \u{2014} Error")
            .body(message)
            .show()
        {
            tracing::warn!("native error notification failed, suppressing: {e}");
        }
        Ok(())
    }
}

/// Fallback notifier for headless/test mode.
struct LogOnlyNotifier;

#[async_trait::async_trait]
impl oneshim_core::ports::notifier::DesktopNotifier for LogOnlyNotifier {
    async fn show_suggestion(
        &self,
        suggestion: &oneshim_core::models::suggestion::Suggestion,
    ) -> Result<(), oneshim_core::error::CoreError> {
        tracing::debug!(id = %suggestion.suggestion_id, "suggestion notification (headless mode)");
        Ok(())
    }

    async fn show_notification(
        &self,
        title: &str,
        body: &str,
    ) -> Result<(), oneshim_core::error::CoreError> {
        tracing::debug!(title, body, "notification (headless mode)");
        Ok(())
    }

    async fn show_error(&self, message: &str) -> Result<(), oneshim_core::error::CoreError> {
        tracing::debug!(message, "error notification (headless mode)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_session_id_format() {
        let id = generate_session_id();
        assert!(id.starts_with("sess_"));
        assert!(id.len() > 20);
    }

    #[test]
    fn tauri_notifier_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TauriNotifier>();
        assert_send_sync::<LogOnlyNotifier>();
    }
}
