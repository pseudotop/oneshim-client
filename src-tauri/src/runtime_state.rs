use oneshim_automation::controller::AutomationController;
use oneshim_core::config::{AppConfig, CredentialBackendKind};
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::consent::ConsentManager;
use oneshim_core::ports::accessibility::AccessibilityExtractor;
use oneshim_core::ports::audio_capture::AudioCapturePort;
use oneshim_core::ports::coaching::CoachingPort;
use oneshim_core::ports::integration::{IntegrationAuthPort, IntegrationSessionPort};
use oneshim_core::ports::model_downloader::ModelDownloader;
use oneshim_core::ports::monitor::ActivityMonitor;
use oneshim_core::ports::oauth::OAuthPort;
use oneshim_core::ports::session_storage::SessionStoragePort;
use oneshim_core::ports::stt_provider::SttProvider;
use oneshim_core::ports::vision::FrameProcessor;
use oneshim_core::ports::work_classifier::WorkTypeClassifier;
use oneshim_storage::frame_storage::FrameFileStorage;
use oneshim_storage::sqlite::SqliteStorage;
use oneshim_web::update_control::{UpdateAction, UpdateControl};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16};
use std::sync::Arc;
use tauri::{App, Manager};

use crate::magic_overlay::MagicOverlayHandle;
use crate::session_manager::SessionManagerImpl;

#[cfg(feature = "server")]
pub(crate) type OAuthCoordinator =
    Option<Arc<oneshim_network::oauth::refresh_coordinator::TokenRefreshCoordinator>>;
#[cfg(not(feature = "server"))]
pub(crate) type OAuthCoordinator = Option<()>;

/// Groups frame processor, frame storage, activity monitor, accessibility
/// extractor, and consent manager used by IPC capture commands (A1, A2).
#[allow(dead_code)]
pub struct CaptureContext {
    /// Frame processor for on-demand capture (A1, A2).
    pub frame_processor: Option<Arc<dyn FrameProcessor>>,
    /// Frame file storage for persisting captured images.
    pub frame_storage: Option<Arc<FrameFileStorage>>,
    /// Activity monitor for current window context (A1, A2).
    pub activity_monitor: Option<Arc<dyn ActivityMonitor>>,
    /// Accessibility extractor for scene analysis (A2).
    pub accessibility_extractor: Option<Arc<dyn AccessibilityExtractor>>,
    /// Consent manager for PII level gating in accessibility extraction (A2).
    pub consent_manager: Option<Arc<ConsentManager>>,
    /// Work type classifier for scene analysis (A2).
    pub work_classifier: Option<Arc<dyn WorkTypeClassifier>>,
}

/// Audio capture, STT engine, and model management for voice input.
#[allow(dead_code)]
pub struct AudioContext {
    pub capture: Option<Arc<dyn AudioCapturePort>>,
    /// RwLock allows hot-reload after model download.
    pub stt_engine: Arc<tokio::sync::RwLock<Option<Arc<dyn SttProvider>>>>,
    pub model_downloader: Option<Arc<dyn ModelDownloader>>,
    pub model_dir: PathBuf,
    /// Prevents concurrent downloads.
    pub downloading: Arc<AtomicBool>,
    /// Cancel flag for active download — set to true to abort.
    pub download_cancel: Arc<AtomicBool>,
    /// VAD state: "idle", "listening", "speech", "transcribing".
    /// Tracked at IPC layer — not inside VadDetector.
    pub vad_state: Arc<parking_lot::Mutex<String>>,
}

/// Groups connectivity flags for server, LLM, and CLI connections.
#[allow(dead_code)]
pub struct ConnectionStatus {
    /// Server API connectivity (REST or gRPC).
    pub server_connected: Arc<AtomicBool>,
    /// Local LLM provider connectivity (Ollama, subprocess, etc.).
    pub llm_connected: Arc<AtomicBool>,
    /// CLI bridge / automation controller connectivity.
    pub cli_connected: Arc<AtomicBool>,
}

#[allow(dead_code)]
pub struct AppState {
    pub runtime_handle: tokio::runtime::Handle,
    pub background_runtime: Arc<crate::bootstrap_runtime::ManagedBackgroundRuntime>,
    pub config: AppConfig,
    pub web_port: Arc<AtomicU16>,
    pub storage: Arc<SqliteStorage>,
    pub config_manager: ConfigManager,
    pub update_control: Option<UpdateControl>,
    pub update_action_tx: tokio::sync::mpsc::UnboundedSender<UpdateAction>,
    pub automation_controller: Option<Arc<AutomationController>>,
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
    /// Shared flag for on-demand re-clustering requests from Tauri/REST.
    pub recluster_requested: Arc<std::sync::atomic::AtomicBool>,
    /// MagicOverlay handle for transparent coaching overlay window.
    pub magic_overlay: Option<MagicOverlayHandle>,
    /// Coaching engine for proactive coaching messages (Phase 2 IPC access).
    /// Typed as `dyn CoachingPort` so Tauri IPC commands call through the port trait,
    /// keeping the binary crate decoupled from the concrete `CoachingEngine` type.
    pub coaching_engine: Option<Arc<dyn CoachingPort>>,
    /// Whether capture/monitoring is paused (toggled via tray or IPC).
    pub capture_paused: Arc<AtomicBool>,
    /// Whether the tracking indicator border is visible.
    pub indicator_visible: Arc<AtomicBool>,
    /// Whether detection overlay mode is active (toggled via Cmd+Shift+D).
    pub detection_active: Arc<AtomicBool>,
    /// Connection status flags for server, LLM, and CLI.
    pub connection: ConnectionStatus,
    /// Focus mode state — transient, not persisted. Suppresses coaching + notifications.
    pub focus_mode: Arc<crate::focus_mode::FocusModeState>,
    /// Capture-related resources for IPC commands (A1, A2).
    pub capture: CaptureContext,
    /// Suggestion manager for overlay panel (A3). Shares queue with SuggestionReceiver.
    pub suggestion_manager: Option<Arc<crate::suggestion_manager::SuggestionManager>>,
    /// AI conversation session manager for Tauri IPC commands.
    pub session_manager: Option<Arc<SessionManagerImpl>>,
    /// Persisted session storage for AI chat history (fire-and-forget writes).
    pub session_storage: Option<Arc<dyn SessionStoragePort>>,
    /// Audio capture and STT engine for voice input (P1 Audio STT).
    pub audio: AudioContext,
}

pub struct OAuthState(pub Option<Arc<dyn OAuthPort>>);

#[allow(dead_code)]
pub struct OAuthCoordinatorState(pub OAuthCoordinator);

#[derive(Debug, Clone, Serialize)]
pub struct SecretBackendCapabilities {
    pub os_secret_store_available: bool,
    pub oauth_available: bool,
    pub oauth_provider_ids: Vec<String>,
    pub default_backend_kind: String,
    pub byok_backend_kind: String,
    pub fallback_backend_kind: String,
}

pub struct SecretBackendState(pub SecretBackendCapabilities);

#[allow(dead_code)]
pub struct IntegrationSessionState(pub Option<Arc<dyn IntegrationSessionPort>>);

#[allow(dead_code)]
pub struct IntegrationAuthState(pub Option<Arc<dyn IntegrationAuthPort>>);

#[derive(Clone)]
pub(crate) struct ManagedStateCapabilityProfile {
    pub(crate) oauth_provider_ids: Vec<String>,
    pub(crate) provider_backend_kind: CredentialBackendKind,
    pub(crate) fallback_backend_kind: CredentialBackendKind,
}

impl Default for ManagedStateCapabilityProfile {
    fn default() -> Self {
        Self {
            oauth_provider_ids: Vec::new(),
            provider_backend_kind: CredentialBackendKind::Unavailable,
            fallback_backend_kind: CredentialBackendKind::Unavailable,
        }
    }
}

pub(crate) struct ManagedStateRegistration {
    pub(crate) app_state: AppState,
    pub(crate) oauth_state: OAuthState,
    pub(crate) oauth_coordinator_state: OAuthCoordinatorState,
    pub(crate) secret_backend_state: SecretBackendState,
    pub(crate) feature_capability_state: crate::feature_capabilities::FeatureCapabilityState,
    pub(crate) integration_auth_state: IntegrationAuthState,
    pub(crate) integration_session_state: IntegrationSessionState,
}

pub(crate) struct ManagedStateBuilder {
    app_state: AppState,
    oauth_state: OAuthState,
    oauth_coordinator_state: OAuthCoordinatorState,
    capability_profile: ManagedStateCapabilityProfile,
    integration_auth_state: IntegrationAuthState,
    integration_session_state: IntegrationSessionState,
}

impl ManagedStateBuilder {
    pub(crate) fn new(app_state: AppState) -> Self {
        Self {
            app_state,
            oauth_state: OAuthState(None),
            oauth_coordinator_state: OAuthCoordinatorState(None),
            capability_profile: ManagedStateCapabilityProfile::default(),
            integration_auth_state: IntegrationAuthState(None),
            integration_session_state: IntegrationSessionState(None),
        }
    }

    #[cfg(feature = "server")]
    pub(crate) fn with_oauth(
        mut self,
        oauth_port: Option<Arc<dyn OAuthPort>>,
        oauth_coordinator: OAuthCoordinator,
    ) -> Self {
        self.oauth_state = OAuthState(oauth_port);
        self.oauth_coordinator_state = OAuthCoordinatorState(oauth_coordinator);
        self
    }

    #[cfg(feature = "server")]
    pub(crate) fn with_secret_backend_profile(
        mut self,
        capability_profile: ManagedStateCapabilityProfile,
    ) -> Self {
        self.capability_profile = capability_profile;
        self
    }

    #[cfg(feature = "server")]
    pub(crate) fn with_integration(
        mut self,
        integration_auth: Option<Arc<dyn IntegrationAuthPort>>,
        integration_session: Option<Arc<dyn IntegrationSessionPort>>,
    ) -> Self {
        self.integration_auth_state = IntegrationAuthState(integration_auth);
        self.integration_session_state = IntegrationSessionState(integration_session);
        self
    }

    pub(crate) fn build(self) -> ManagedStateRegistration {
        let oauth_available = self.oauth_state.0.is_some();
        let secret_backend_state = SecretBackendState(secret_backend_capabilities(
            oauth_available,
            self.capability_profile.oauth_provider_ids,
            self.capability_profile.provider_backend_kind,
            self.capability_profile.fallback_backend_kind,
        ));
        let feature_capability_state =
            crate::feature_capabilities::FeatureCapabilityState(secret_backend_state.0.clone());

        ManagedStateRegistration {
            app_state: self.app_state,
            oauth_state: self.oauth_state,
            oauth_coordinator_state: self.oauth_coordinator_state,
            secret_backend_state,
            feature_capability_state,
            integration_auth_state: self.integration_auth_state,
            integration_session_state: self.integration_session_state,
        }
    }
}

impl ManagedStateRegistration {
    pub(crate) fn register_on(self, app: &mut App) {
        app.manage(self.app_state);
        app.manage(self.oauth_state);
        app.manage(self.oauth_coordinator_state);
        app.manage(self.secret_backend_state);
        app.manage(self.feature_capability_state);
        app.manage(self.integration_auth_state);
        app.manage(self.integration_session_state);
    }
}

pub(crate) fn secret_backend_capabilities(
    oauth_available: bool,
    oauth_provider_ids: Vec<String>,
    provider_backend_kind: CredentialBackendKind,
    fallback_backend_kind: CredentialBackendKind,
) -> SecretBackendCapabilities {
    SecretBackendCapabilities {
        os_secret_store_available: oauth_available,
        oauth_available,
        oauth_provider_ids,
        default_backend_kind: credential_backend_kind_to_wire(provider_backend_kind).to_string(),
        byok_backend_kind: credential_backend_kind_to_wire(provider_backend_kind).to_string(),
        fallback_backend_kind: credential_backend_kind_to_wire(fallback_backend_kind).to_string(),
    }
}

fn credential_backend_kind_to_wire(value: CredentialBackendKind) -> &'static str {
    match value {
        CredentialBackendKind::OsSecretStore => "os_secret_store",
        CredentialBackendKind::FileSecretStore => "file_secret_store",
        CredentialBackendKind::Env => "env",
        CredentialBackendKind::BridgeManaged => "bridge_managed",
        CredentialBackendKind::Unavailable => "unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_web::update_control::UpdateAction;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    #[test]
    fn managed_state_builder_defaults_to_unavailable_secret_backend() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let handle = runtime.handle().clone();
        let temp_dir = TempDir::new().expect("temp dir");
        let storage = Arc::new(
            oneshim_storage::sqlite::SqliteStorage::open(&temp_dir.path().join("state.db"), 1)
                .expect("db"),
        );
        let config_manager =
            ConfigManager::with_path(temp_dir.path().join("config.json")).expect("config manager");
        let (update_action_tx, _update_action_rx) = mpsc::unbounded_channel::<UpdateAction>();
        let (shutdown_tx, _shutdown_rx) = tokio::sync::watch::channel(false);

        let registration = ManagedStateBuilder::new(AppState {
            runtime_handle: handle,
            background_runtime: crate::bootstrap_runtime::spawn_background_runtime()
                .expect("background runtime"),
            config: AppConfig::default_config(),
            web_port: Arc::new(AtomicU16::new(0)),
            storage,
            config_manager,
            update_control: None,
            update_action_tx,
            automation_controller: None,
            shutdown_tx,
            recluster_requested: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            magic_overlay: None,
            coaching_engine: None,
            capture_paused: Arc::new(AtomicBool::new(false)),
            indicator_visible: Arc::new(AtomicBool::new(true)),
            detection_active: Arc::new(AtomicBool::new(false)),
            connection: ConnectionStatus {
                server_connected: Arc::new(AtomicBool::new(false)),
                llm_connected: Arc::new(AtomicBool::new(false)),
                cli_connected: Arc::new(AtomicBool::new(false)),
            },
            focus_mode: Arc::new(crate::focus_mode::FocusModeState::new()),
            capture: CaptureContext {
                frame_processor: None,
                frame_storage: None,
                activity_monitor: None,
                accessibility_extractor: None,
                consent_manager: None,
                work_classifier: None,
            },
            suggestion_manager: None,
            session_manager: None,
            session_storage: None,
            audio: AudioContext {
                capture: None,
                stt_engine: Arc::new(tokio::sync::RwLock::new(None)),
                model_downloader: None,
                model_dir: PathBuf::from("/tmp/test-models"),
                downloading: Arc::new(AtomicBool::new(false)),
                download_cancel: Arc::new(AtomicBool::new(false)),
                vad_state: Arc::new(parking_lot::Mutex::new("idle".into())),
            },
        })
        .build();

        assert_eq!(
            registration.secret_backend_state.0.byok_backend_kind,
            "unavailable"
        );
        assert!(!registration.secret_backend_state.0.oauth_available);
    }
}
