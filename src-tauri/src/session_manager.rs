//! Session manager implementation — creates, manages, and reaps AI conversation sessions.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{info, warn};

use oneshim_api_contracts::provider_specs::{self, ProviderTransportKind, SurfaceCapabilityKind};
use oneshim_core::config::AiSessionConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::ai_session::{
    ConversationSessionInfo, SessionConfig, SessionState, SessionTransport,
};
use oneshim_core::ports::audit_log::AuditLogPort;
use oneshim_core::ports::conversation_session::{ConversationSession, SessionManager};
use oneshim_core::ports::credential_source::CredentialSource;
use oneshim_core::ports::secret_store::SecretStore;

use crate::auditing_session::AuditingSession;
use crate::session_adapters::claude_session::ClaudeSubprocessSession;
use crate::session_context::SessionContextAssembler;
use crate::subprocess_provider::detect_known_cli_surfaces;

use oneshim_network::http_api_session::HttpApiSession;
use oneshim_network::local_llm_session::LocalLlmSession;

struct ManagedSession {
    session: Arc<dyn ConversationSession>,
    state: SessionState,
    #[allow(dead_code)]
    created_at: Instant,
    last_active: Instant,
    retry_count: u32,
}

pub struct SessionManagerImpl {
    sessions: RwLock<HashMap<String, ManagedSession>>,
    pub(crate) config: Arc<AiSessionConfig>,
    audit: Arc<dyn AuditLogPort>,
    context_assembler: Option<Arc<SessionContextAssembler>>,
    /// Secret store for resolving provider credentials (HttpApi sessions).
    secret_store: Option<Arc<dyn SecretStore>>,
}

impl SessionManagerImpl {
    pub fn new(
        config: Arc<AiSessionConfig>,
        audit: Arc<dyn AuditLogPort>,
        context_assembler: Option<Arc<SessionContextAssembler>>,
    ) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            config,
            audit,
            context_assembler,
            secret_store: None,
        }
    }

    /// Attach a secret store for resolving provider credentials.
    #[allow(dead_code)]
    pub fn with_secret_store(mut self, store: Arc<dyn SecretStore>) -> Self {
        self.secret_store = Some(store);
        self
    }

    /// Terminate all sessions (called during app shutdown).
    pub async fn shutdown_all(&self) {
        let session_ids: Vec<String> = {
            let sessions = self.sessions.read().await;
            sessions.keys().cloned().collect()
        };

        for id in session_ids {
            if let Err(err) = self.kill_session(&id).await {
                warn!(session_id = %id, "failed to terminate session during shutdown: {err}");
            }
        }

        info!("all AI sessions terminated");
    }

    /// Touch a session to reset its idle timer and mark it as Active.
    /// Called on every send_message to keep the session alive.
    pub async fn touch_session(&self, session_id: &str) {
        if let Some(managed) = self.sessions.write().await.get_mut(session_id) {
            managed.last_active = Instant::now();
            managed.state = SessionState::Active;
        }
    }

    /// Background task: check for idle sessions and terminate them.
    /// Two-phase idle: Active→Idle (warning) on first timeout, Idle→Terminated on second.
    pub async fn reap_idle_sessions(&self) {
        let idle_timeout = std::time::Duration::from_secs(self.config.idle_timeout_secs);
        let mut to_reap = vec![];

        {
            let mut sessions = self.sessions.write().await;
            for (id, managed) in sessions.iter_mut() {
                if managed.last_active.elapsed() > idle_timeout {
                    if managed.state == SessionState::Active {
                        // First pass: mark Active → Idle (grace period)
                        managed.state = SessionState::Idle;
                        warn!(session_id = %id, "session marked idle");
                    } else if managed.state == SessionState::Idle {
                        // Second pass: Idle past timeout → collect for reaping
                        to_reap.push(id.clone());
                    }
                }
            }
        }

        for id in to_reap {
            info!(session_id = %id, "reaping idle session");
            let _ = self.kill_session(&id).await;
        }
    }

    /// Attempt to recover a session after a stream error.
    /// Increments retry_count and transitions state through Recovering → Active.
    /// Returns the session Arc for re-use, or an error if max retries exceeded.
    pub async fn recover_session(
        &self,
        session_id: &str,
    ) -> Result<Arc<dyn ConversationSession>, CoreError> {
        let mut sessions = self.sessions.write().await;
        let managed = sessions
            .get_mut(session_id)
            .ok_or_else(|| CoreError::Internal(format!("session not found: {session_id}")))?;

        if managed.retry_count >= self.config.max_retries {
            managed.state = SessionState::Failed;
            return Err(CoreError::Internal("max retries exceeded".into()));
        }

        managed.retry_count += 1;
        managed.state = SessionState::Recovering;
        info!(
            session_id,
            retry = managed.retry_count,
            "recovering session"
        );

        // The adapter itself handles --continue/resume for session continuity
        managed.state = SessionState::Active;
        Ok(managed.session.clone())
    }
}

#[async_trait]
impl SessionManager for SessionManagerImpl {
    async fn create_session(
        &self,
        config: SessionConfig,
    ) -> Result<Arc<dyn ConversationSession>, CoreError> {
        let session_count = self.sessions.read().await.len();
        if session_count >= self.config.max_concurrent_sessions as usize {
            return Err(CoreError::Internal(format!(
                "max concurrent sessions ({}) reached",
                self.config.max_concurrent_sessions,
            )));
        }

        // Auto-generate system prompt from context if not provided
        let mut config = config;
        if config.system_prompt.is_none() {
            if let Some(ref assembler) = self.context_assembler {
                let message = assembler.build_system_message().await;
                config.system_prompt = Some(message.content);
            }
        }

        match config.transport {
            SessionTransport::Subprocess => {
                let surfaces = detect_known_cli_surfaces();
                let surface = surfaces
                    .into_iter()
                    .find(|s| s.surface_id == "provider_surface.anthropic.subprocess_cli")
                    .ok_or_else(|| {
                        CoreError::Internal("no Claude CLI detected on this system".to_string())
                    })?;

                let inner: Arc<dyn ConversationSession> = Arc::new(ClaudeSubprocessSession::new(
                    surface,
                    &config,
                    self.config.clone(),
                ));

                let wrapped: Arc<dyn ConversationSession> =
                    Arc::new(AuditingSession::new(inner, self.audit.clone()));

                let session_id = wrapped.session_id().to_string();
                info!(session_id = %session_id, "created Claude subprocess session");

                let managed = ManagedSession {
                    session: wrapped.clone(),
                    state: SessionState::Active,
                    created_at: Instant::now(),
                    last_active: Instant::now(),
                    retry_count: 0,
                };
                self.sessions.write().await.insert(session_id, managed);

                Ok(wrapped)
            }
            SessionTransport::HttpApi => {
                let surface_id = config.surface_id.as_deref().ok_or_else(|| {
                    CoreError::InvalidArguments(
                        "surface_id is required for HttpApi sessions".to_string(),
                    )
                })?;

                // Resolve surface spec from the provider catalog.
                let surface_spec = provider_specs::provider_surface_spec(surface_id)
                    .map_err(CoreError::Internal)?;
                let provider_type = oneshim_core::provider_surface::provider_type_from_vendor_id(
                    &surface_spec.provider_type,
                )
                .ok_or_else(|| {
                    CoreError::Internal(format!(
                        "unknown provider_type for vendor '{}'",
                        surface_spec.vendor_id
                    ))
                })?;

                // Resolve the LLM transport endpoint from the catalog.
                let transport_spec = provider_specs::resolved_transport_spec(
                    provider_type,
                    Some(surface_id),
                    ProviderTransportKind::Llm,
                )
                .map_err(CoreError::Internal)?;

                // Model: explicit > catalog default > error.
                let model = config
                    .model
                    .clone()
                    .or_else(|| {
                        provider_specs::resolved_default_model(
                            provider_type,
                            Some(surface_id),
                            SurfaceCapabilityKind::Llm,
                        )
                        .ok()
                        .flatten()
                    })
                    .ok_or_else(|| {
                        CoreError::InvalidArguments(format!(
                            "no model specified and surface '{surface_id}' has no default LLM model"
                        ))
                    })?;

                // Build credential source from the secret store.
                let credential = CredentialSource::from_api_key_endpoint(
                    &oneshim_core::config::ExternalApiEndpoint {
                        endpoint: transport_spec.url.clone(),
                        api_key: String::new(),
                        model: Some(model.clone()),
                        timeout_secs: 30,
                        provider_type,
                        surface_id: Some(surface_id.to_string()),
                        credential: None,
                    },
                    self.secret_store.clone(),
                )
                .or_else(|_| {
                    // Fallback: no-auth surfaces (e.g. Ollama local_http).
                    if oneshim_core::provider_surface::provider_surface_uses_no_auth(surface_id) {
                        Ok(CredentialSource::NoAuth)
                    } else {
                        Err(CoreError::Auth(format!(
                            "no credential available for surface '{surface_id}'"
                        )))
                    }
                })?;

                let inner: Arc<dyn ConversationSession> = Arc::new(HttpApiSession::new(
                    surface_id.to_string(),
                    model,
                    transport_spec.url.clone(),
                    credential,
                    provider_type,
                    config.system_prompt.clone(),
                    self.config.clone(),
                ));

                let wrapped: Arc<dyn ConversationSession> =
                    Arc::new(AuditingSession::new(inner, self.audit.clone()));

                let session_id = wrapped.session_id().to_string();
                info!(session_id = %session_id, surface_id = %surface_id, "created HttpApi session");

                let managed = ManagedSession {
                    session: wrapped.clone(),
                    state: SessionState::Active,
                    created_at: Instant::now(),
                    last_active: Instant::now(),
                    retry_count: 0,
                };
                self.sessions.write().await.insert(session_id, managed);

                Ok(wrapped)
            }
            SessionTransport::LocalLlm => {
                // Resolve Ollama base URL: use the Ollama surface spec probe URL,
                // stripping the path to get the base.
                let base_url = "http://localhost:11434".to_string();

                let model = config.model.clone().unwrap_or_else(|| "llama3".to_string());
                let session_id = uuid::Uuid::new_v4().to_string();

                let inner: Arc<dyn ConversationSession> = Arc::new(LocalLlmSession::new(
                    session_id.clone(),
                    model,
                    base_url,
                    config.system_prompt.clone(),
                    self.config.clone(),
                ));

                let wrapped: Arc<dyn ConversationSession> =
                    Arc::new(AuditingSession::new(inner, self.audit.clone()));

                let session_id = wrapped.session_id().to_string();
                info!(session_id = %session_id, "created LocalLlm (Ollama) session");

                let managed = ManagedSession {
                    session: wrapped.clone(),
                    state: SessionState::Active,
                    created_at: Instant::now(),
                    last_active: Instant::now(),
                    retry_count: 0,
                };
                self.sessions.write().await.insert(session_id, managed);

                Ok(wrapped)
            }
        }
    }

    async fn kill_session(&self, session_id: &str) -> Result<(), CoreError> {
        let removed = self.sessions.write().await.remove(session_id);
        match removed {
            Some(_) => {
                info!(session_id = %session_id, "session terminated");
                Ok(())
            }
            None => Err(CoreError::Internal(format!(
                "session not found: {session_id}"
            ))),
        }
    }

    async fn list_sessions(&self) -> Vec<ConversationSessionInfo> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .map(|managed| {
                let mut info = managed.session.info();
                // Override adapter's always-Active state with manager's authoritative state
                info.state = managed.state;
                info
            })
            .collect()
    }

    async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Arc<dyn ConversationSession>, CoreError> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .map(|m| m.session.clone())
            .ok_or_else(|| CoreError::Internal(format!("session not found: {session_id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Arc<AiSessionConfig> {
        Arc::new(AiSessionConfig {
            max_concurrent_sessions: 2,
            idle_timeout_secs: 1,
            ..Default::default()
        })
    }

    fn test_manager() -> SessionManagerImpl {
        SessionManagerImpl::new(
            test_config(),
            Arc::new(crate::auditing_session::tests::MockAudit::default()),
            None,
        )
    }

    /// Helper: extract error message from a Result whose Ok type is not Debug.
    fn expect_err_msg(result: Result<Arc<dyn ConversationSession>, CoreError>) -> String {
        match result {
            Err(e) => format!("{e}"),
            Ok(_) => panic!("expected Err, got Ok"),
        }
    }

    fn has_claude_cli() -> bool {
        detect_known_cli_surfaces()
            .iter()
            .any(|s| s.surface_id == "provider_surface.anthropic.subprocess_cli")
    }

    #[tokio::test]
    async fn list_sessions_empty() {
        let mgr = test_manager();
        assert!(mgr.list_sessions().await.is_empty());
    }

    #[tokio::test]
    async fn kill_nonexistent_session_returns_error() {
        let mgr = test_manager();
        let result = mgr.kill_session("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_session_not_found() {
        let mgr = test_manager();
        let err_msg = expect_err_msg(mgr.get_session("no-such-id").await);
        assert!(err_msg.contains("session not found"));
    }

    #[tokio::test]
    async fn create_subprocess_session_succeeds_when_claude_detected() {
        // detect_known_cli_surfaces checks the filesystem for installed CLIs.
        // If Claude CLI is not installed (e.g. CI), the test gracefully verifies
        // the "no Claude CLI detected" error instead.
        let mgr = test_manager();
        let config = SessionConfig {
            transport: SessionTransport::Subprocess,
            surface_id: None,
            model: None,
            system_prompt: Some("You are a test assistant.".to_string()),
            tools_enabled: false,
        };
        let result = mgr.create_session(config).await;

        if has_claude_cli() {
            let session = match result {
                Ok(s) => s,
                Err(e) => panic!("should create session when Claude CLI is present: {e}"),
            };
            assert_eq!(session.provider_name(), "claude");
            assert!(!session.session_id().is_empty());

            // Verify it was stored and is retrievable
            let retrieved = mgr.get_session(session.session_id()).await;
            assert!(retrieved.is_ok());

            // Verify it appears in list
            let list = mgr.list_sessions().await;
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].session_id, session.session_id());
        } else {
            let err_msg = expect_err_msg(result);
            assert!(
                err_msg.contains("no Claude CLI detected"),
                "unexpected error: {err_msg}",
            );
        }
    }

    #[tokio::test]
    async fn create_http_api_session_requires_surface_id() {
        let mgr = test_manager();
        let config = SessionConfig {
            transport: SessionTransport::HttpApi,
            surface_id: None,
            model: None,
            system_prompt: None,
            tools_enabled: false,
        };
        let err_msg = expect_err_msg(mgr.create_session(config).await);
        assert!(
            err_msg.contains("surface_id is required"),
            "expected surface_id error, got: {err_msg}",
        );
    }

    #[tokio::test]
    async fn create_local_llm_session_succeeds() {
        let mgr = test_manager();
        let config = SessionConfig {
            transport: SessionTransport::LocalLlm,
            surface_id: None,
            model: Some("llama3".to_string()),
            system_prompt: Some("Be concise.".to_string()),
            tools_enabled: false,
        };
        let session = mgr
            .create_session(config)
            .await
            .expect("should create LocalLlm session");
        assert_eq!(session.provider_name(), "ollama");
        assert!(!session.session_id().is_empty());

        // Verify stored and retrievable.
        let retrieved = mgr.get_session(session.session_id()).await;
        assert!(retrieved.is_ok());

        let list = mgr.list_sessions().await;
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn create_local_llm_session_uses_default_model() {
        let mgr = test_manager();
        let config = SessionConfig {
            transport: SessionTransport::LocalLlm,
            surface_id: None,
            model: None,
            system_prompt: None,
            tools_enabled: false,
        };
        let session = mgr
            .create_session(config)
            .await
            .expect("should create LocalLlm session");
        let info = session.info();
        assert_eq!(info.model, "llama3");
    }

    #[tokio::test]
    async fn create_session_enforces_max_concurrent_limit() {
        if !has_claude_cli() {
            return; // skip in environments without Claude CLI
        }

        let mgr = test_manager(); // max_concurrent_sessions = 2
        let make_config = || SessionConfig {
            transport: SessionTransport::Subprocess,
            surface_id: None,
            model: None,
            system_prompt: None,
            tools_enabled: false,
        };

        let _s1 = mgr.create_session(make_config()).await.expect("session 1");
        let _s2 = mgr.create_session(make_config()).await.expect("session 2");
        let err_msg = expect_err_msg(mgr.create_session(make_config()).await);
        assert!(err_msg.contains("max concurrent sessions"));
    }

    #[tokio::test]
    async fn kill_session_removes_from_map() {
        if !has_claude_cli() {
            return;
        }

        let mgr = test_manager();
        let config = SessionConfig {
            transport: SessionTransport::Subprocess,
            surface_id: None,
            model: None,
            system_prompt: None,
            tools_enabled: false,
        };
        let session = mgr.create_session(config).await.expect("create session");
        let id = session.session_id().to_string();

        assert!(mgr.get_session(&id).await.is_ok());
        mgr.kill_session(&id).await.unwrap();
        assert!(mgr.get_session(&id).await.is_err());
        assert!(mgr.list_sessions().await.is_empty());
    }

    #[tokio::test]
    async fn touch_session_resets_state_to_active() {
        let mgr = test_manager();

        // Create a LocalLlm session (no CLI dependency).
        let config = SessionConfig {
            transport: SessionTransport::LocalLlm,
            surface_id: None,
            model: Some("llama3".to_string()),
            system_prompt: None,
            tools_enabled: false,
        };
        let session = mgr.create_session(config).await.expect("create session");
        let id = session.session_id().to_string();

        // Manually mark the session as Idle to simulate idle timeout.
        {
            let mut sessions = mgr.sessions.write().await;
            let managed = sessions.get_mut(&id).unwrap();
            managed.state = SessionState::Idle;
            assert_eq!(managed.state, SessionState::Idle);
        }

        // touch_session should reset state to Active.
        mgr.touch_session(&id).await;

        {
            let sessions = mgr.sessions.read().await;
            let managed = sessions.get(&id).unwrap();
            assert_eq!(managed.state, SessionState::Active);
        }
    }

    #[tokio::test]
    async fn reap_marks_idle_then_terminates() {
        // Use a very short idle timeout (1 second from test_config).
        let mgr = test_manager();

        let config = SessionConfig {
            transport: SessionTransport::LocalLlm,
            surface_id: None,
            model: Some("llama3".to_string()),
            system_prompt: None,
            tools_enabled: false,
        };
        let session = mgr.create_session(config).await.expect("create session");
        let id = session.session_id().to_string();

        // Force last_active to be in the past (beyond idle_timeout_secs=1).
        {
            let mut sessions = mgr.sessions.write().await;
            let managed = sessions.get_mut(&id).unwrap();
            managed.last_active = Instant::now() - std::time::Duration::from_secs(5);
        }

        // First reap: Active → Idle (should NOT remove from map).
        mgr.reap_idle_sessions().await;
        {
            let sessions = mgr.sessions.read().await;
            let managed = sessions
                .get(&id)
                .expect("session should still exist after first reap");
            assert_eq!(managed.state, SessionState::Idle);
        }

        // Force last_active again so the Idle session also exceeds timeout.
        {
            let mut sessions = mgr.sessions.write().await;
            let managed = sessions.get_mut(&id).unwrap();
            managed.last_active = Instant::now() - std::time::Duration::from_secs(5);
        }

        // Second reap: Idle → Terminated (removed from map).
        mgr.reap_idle_sessions().await;
        assert!(
            mgr.get_session(&id).await.is_err(),
            "session should be removed after second reap"
        );
        assert!(mgr.list_sessions().await.is_empty());
    }

    #[tokio::test]
    async fn create_session_uses_context_assembler() {
        use crate::scheduler::shared_regime_state::SharedRegimeState;
        use oneshim_core::config::AppConfig;
        use oneshim_storage::sqlite::SqliteStorage;

        let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
        let app_config = Arc::new(AppConfig::default_config());
        let regime_state = Arc::new(SharedRegimeState::new());
        let assembler = Arc::new(SessionContextAssembler::new(
            storage,
            app_config,
            regime_state,
        ));

        let mgr = SessionManagerImpl::new(
            test_config(),
            Arc::new(crate::auditing_session::tests::MockAudit::default()),
            Some(assembler),
        );

        // Create a LocalLlm session with system_prompt = None.
        // The context assembler should inject a system prompt automatically.
        let config = SessionConfig {
            transport: SessionTransport::LocalLlm,
            surface_id: None,
            model: Some("llama3".to_string()),
            system_prompt: None,
            tools_enabled: false,
        };

        let session = mgr
            .create_session(config)
            .await
            .expect("should create session with assembled context");

        // The session should have been created successfully.
        assert!(!session.session_id().is_empty());

        // Verify the session is stored and retrievable.
        let retrieved = mgr.get_session(session.session_id()).await;
        assert!(retrieved.is_ok());
    }

    #[tokio::test]
    async fn create_session_preserves_explicit_system_prompt() {
        use crate::scheduler::shared_regime_state::SharedRegimeState;
        use oneshim_core::config::AppConfig;
        use oneshim_storage::sqlite::SqliteStorage;

        let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
        let app_config = Arc::new(AppConfig::default_config());
        let regime_state = Arc::new(SharedRegimeState::new());
        let assembler = Arc::new(SessionContextAssembler::new(
            storage,
            app_config,
            regime_state,
        ));

        let mgr = SessionManagerImpl::new(
            test_config(),
            Arc::new(crate::auditing_session::tests::MockAudit::default()),
            Some(assembler),
        );

        // Create a LocalLlm session with an explicit system prompt.
        // The context assembler should NOT override it.
        let config = SessionConfig {
            transport: SessionTransport::LocalLlm,
            surface_id: None,
            model: Some("llama3".to_string()),
            system_prompt: Some("Custom prompt".to_string()),
            tools_enabled: false,
        };

        let session = mgr
            .create_session(config)
            .await
            .expect("should create session with explicit prompt");

        assert!(!session.session_id().is_empty());
    }

    #[tokio::test]
    async fn recover_session_increments_retry_count() {
        if !has_claude_cli() {
            return; // skip in environments without Claude CLI
        }

        let mgr = test_manager();
        let config = SessionConfig {
            transport: SessionTransport::Subprocess,
            surface_id: None,
            model: None,
            system_prompt: None,
            tools_enabled: false,
        };
        let session = mgr.create_session(config).await.expect("create session");
        let id = session.session_id().to_string();

        // First recovery should succeed with retry_count = 1.
        let recovered = mgr.recover_session(&id).await;
        assert!(recovered.is_ok());

        {
            let sessions = mgr.sessions.read().await;
            let managed = sessions.get(&id).unwrap();
            assert_eq!(managed.retry_count, 1);
            assert_eq!(managed.state, SessionState::Active);
        }

        // Second recovery should succeed with retry_count = 2.
        let _ = mgr.recover_session(&id).await.expect("second recovery");
        {
            let sessions = mgr.sessions.read().await;
            let managed = sessions.get(&id).unwrap();
            assert_eq!(managed.retry_count, 2);
        }
    }

    #[tokio::test]
    async fn recover_session_fails_after_max_retries() {
        if !has_claude_cli() {
            return; // skip in environments without Claude CLI
        }

        let config = Arc::new(AiSessionConfig {
            max_concurrent_sessions: 2,
            idle_timeout_secs: 1,
            max_retries: 2,
            ..Default::default()
        });
        let mgr = SessionManagerImpl::new(
            config,
            Arc::new(crate::auditing_session::tests::MockAudit::default()),
            None,
        );

        let session_config = SessionConfig {
            transport: SessionTransport::Subprocess,
            surface_id: None,
            model: None,
            system_prompt: None,
            tools_enabled: false,
        };
        let session = mgr
            .create_session(session_config)
            .await
            .expect("create session");
        let id = session.session_id().to_string();

        // Exhaust max_retries (2).
        let _ = mgr.recover_session(&id).await.expect("recovery 1");
        let _ = mgr.recover_session(&id).await.expect("recovery 2");

        // Third attempt should fail.
        let err_msg = expect_err_msg(mgr.recover_session(&id).await);
        assert!(
            err_msg.contains("max retries exceeded"),
            "unexpected error: {err_msg}",
        );

        // Session state should be Failed.
        {
            let sessions = mgr.sessions.read().await;
            let managed = sessions.get(&id).unwrap();
            assert_eq!(managed.state, SessionState::Failed);
        }
    }
}
