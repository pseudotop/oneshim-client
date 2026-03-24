//! Session manager implementation — creates, manages, and reaps AI conversation sessions.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{info, warn};

use oneshim_core::config::AiSessionConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::ai_session::{ConversationSessionInfo, SessionConfig, SessionState};
use oneshim_core::ports::audit_log::AuditLogPort;
use oneshim_core::ports::conversation_session::{ConversationSession, SessionManager};

// Phase 2: AuditingSession will wrap adapter sessions in create_session()
#[allow(unused_imports)]
use crate::auditing_session::AuditingSession;
use crate::session_context::SessionContextAssembler;

// Phase 2: used when session adapters are implemented
#[allow(dead_code)]
struct ManagedSession {
    session: Arc<dyn ConversationSession>,
    state: SessionState,
    created_at: Instant,
    last_active: Instant,
    retry_count: u32,
}

// Phase 2: wired into AppState and Tauri commands when adapters are ready
#[allow(dead_code)]
pub struct SessionManagerImpl {
    sessions: RwLock<HashMap<String, ManagedSession>>,
    config: Arc<AiSessionConfig>,
    audit: Arc<dyn AuditLogPort>,
    // context_assembler is Option to allow unit testing without real dependencies
    context_assembler: Option<Arc<SessionContextAssembler>>,
}

#[allow(dead_code)]
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

        // TODO(Phase 2): Create actual session adapter based on config.transport
        // SubprocessSession, HttpApiSession, LocalLlmSession adapters are deferred
        // to a follow-up plan (see spec Section 4.1 for adapter definitions).
        Err(CoreError::Internal(format!(
            "session adapter for {:?} not yet implemented",
            config.transport
        )))
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
    use oneshim_core::models::ai_session::*;

    fn test_config() -> Arc<AiSessionConfig> {
        Arc::new(AiSessionConfig {
            max_concurrent_sessions: 2,
            idle_timeout_secs: 1,
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn list_sessions_empty() {
        let mgr = SessionManagerImpl::new(
            test_config(),
            Arc::new(crate::auditing_session::tests::MockAudit::default()),
            None, // no context_assembler needed for list/kill tests
        );
        assert!(mgr.list_sessions().await.is_empty());
    }

    #[tokio::test]
    async fn kill_nonexistent_session_returns_error() {
        let mgr = SessionManagerImpl::new(
            test_config(),
            Arc::new(crate::auditing_session::tests::MockAudit::default()),
            None,
        );
        let result = mgr.kill_session("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_session_rejects_when_adapter_not_implemented() {
        let mgr = SessionManagerImpl::new(
            test_config(),
            Arc::new(crate::auditing_session::tests::MockAudit::default()),
            None,
        );
        let config = SessionConfig {
            transport: SessionTransport::Subprocess,
            surface_id: None,
            model: None,
            system_prompt: None,
            tools_enabled: false,
        };
        let result = mgr.create_session(config).await;
        assert!(result.is_err());
    }
}
