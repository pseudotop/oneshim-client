//! Session manager implementation — creates, manages, and reaps AI conversation sessions.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{info, warn};

use oneshim_core::config::AiSessionConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::ai_session::{
    ConversationSessionInfo, SessionConfig, SessionState, SessionTransport,
};
use oneshim_core::ports::audit_log::AuditLogPort;
use oneshim_core::ports::conversation_session::{ConversationSession, SessionManager};

use crate::auditing_session::AuditingSession;
use crate::session_adapters::claude_session::ClaudeSubprocessSession;
use crate::session_context::SessionContextAssembler;
use crate::subprocess_provider::detect_known_cli_surfaces;

// Phase 2b: state/created_at/last_active/retry_count used by idle reaper + crash recovery
#[allow(dead_code)]
struct ManagedSession {
    session: Arc<dyn ConversationSession>,
    state: SessionState,
    created_at: Instant,
    last_active: Instant,
    retry_count: u32,
}

pub struct SessionManagerImpl {
    sessions: RwLock<HashMap<String, ManagedSession>>,
    config: Arc<AiSessionConfig>,
    audit: Arc<dyn AuditLogPort>,
    // Phase 2b: used by session adapters for system prompt generation
    #[allow(dead_code)]
    context_assembler: Option<Arc<SessionContextAssembler>>,
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
        }
    }

    /// Retrieve a session by ID.
    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Arc<dyn ConversationSession>, CoreError> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .map(|m| m.session.clone())
            .ok_or_else(|| CoreError::Internal(format!("session not found: {session_id}")))
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

    /// Background task: check for idle sessions and terminate them.
    /// Phase 2b: wired into scheduler loop for periodic idle reaping.
    #[allow(dead_code)]
    pub async fn reap_idle_sessions(&self) {
        let idle_timeout = std::time::Duration::from_secs(self.config.idle_timeout_secs);
        let mut to_reap = vec![];

        {
            let sessions = self.sessions.read().await;
            for (id, managed) in sessions.iter() {
                if managed.state == SessionState::Idle
                    && managed.last_active.elapsed() > idle_timeout
                {
                    to_reap.push(id.clone());
                }
            }
        }

        for id in to_reap {
            info!(session_id = %id, "reaping idle session");
            let _ = self.kill_session(&id).await;
        }
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
            _ => Err(CoreError::Internal(format!(
                "session adapter for {:?} not yet implemented",
                config.transport,
            ))),
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
            .map(|managed| managed.session.info())
            .collect()
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
    async fn create_session_rejects_unsupported_transport() {
        let mgr = test_manager();
        let config = SessionConfig {
            transport: SessionTransport::HttpApi,
            surface_id: None,
            model: None,
            system_prompt: None,
            tools_enabled: false,
        };
        let err_msg = expect_err_msg(mgr.create_session(config).await);
        assert!(err_msg.contains("not yet implemented"));
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
}
