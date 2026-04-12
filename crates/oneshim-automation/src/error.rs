use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AutomationError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("policy denied: {0}")]
    PolicyDenied(String),
    #[error("sandbox not supported: {0}")]
    SandboxUnsupported(String),
    #[error("sandbox init failed: {0}")]
    SandboxInit(String),
    #[error("sandbox execution failed: {0}")]
    SandboxExecution(String),
    #[error("sandbox enforcement failed: {0}")]
    SandboxEnforcement(String),
    #[error("execution timeout after {timeout_ms}ms")]
    ExecutionTimeout { timeout_ms: u64 },
    #[error("element not found: {0}")]
    ElementNotFound(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("privacy denied: {0}")]
    PrivacyDenied(String),
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("internal error: {0}")]
    Internal(String),
    /// User denied the confirmation prompt.
    #[error("user denied automation command")]
    UserDenied,
    /// Policy blocks this command from executing.
    #[error("policy blocks execution of this command")]
    PolicyBlocked,
}

impl From<AutomationError> for CoreError {
    fn from(err: AutomationError) -> Self {
        match err {
            AutomationError::Core(e) => e,
            AutomationError::Io(e) => CoreError::Io(e),
            AutomationError::PolicyDenied(msg) => CoreError::PolicyDenied(msg),
            AutomationError::SandboxUnsupported(msg) => CoreError::SandboxUnsupported(msg),
            AutomationError::SandboxInit(msg) => CoreError::SandboxInit(msg),
            AutomationError::SandboxExecution(msg) => CoreError::SandboxExecution(msg),
            AutomationError::SandboxEnforcement(msg) => CoreError::SandboxExecution(msg),
            AutomationError::ExecutionTimeout { timeout_ms } => {
                CoreError::ExecutionTimeout { timeout_ms }
            }
            AutomationError::ElementNotFound(msg) => CoreError::ElementNotFound(msg),
            AutomationError::Config(msg) => CoreError::Config(msg),
            AutomationError::ServiceUnavailable(msg) => CoreError::ServiceUnavailable(msg),
            AutomationError::PrivacyDenied(msg) => CoreError::PrivacyDenied(msg),
            AutomationError::InvalidArguments(msg) => CoreError::InvalidArguments(msg),
            AutomationError::Internal(msg) => CoreError::Internal(msg),
            AutomationError::UserDenied => {
                CoreError::PolicyDenied("user denied automation command".to_string())
            }
            AutomationError::PolicyBlocked => {
                CoreError::PolicyDenied("policy blocks execution of this command".to_string())
            }
        }
    }
}
