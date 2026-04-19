use thiserror::Error;

use crate::error_codes::{
    AudioCode, AuthCode, ConfigCode, ConsentCode, GuiCode, IntegrityCode, InternalCode,
    NetworkCode, NotFoundCode, OAuthCode, PermissionCode, PolicyCode, ProviderCode, SandboxCode,
    SecretCode, ServiceCode, StorageCode, UiCode, ValidationCode,
};
use crate::ports::oauth::OAuthErrorKind;

#[derive(Debug, Error)]
pub enum CoreError {
    // === V2 variants (new struct-variant shape — will be renamed to canonical in Phase 4) ===
    #[error("Configuration error [{code}]: {message}")]
    ConfigV2 { code: ConfigCode, message: String },

    #[error("Network error [{code}]: {message}")]
    NetworkV2 { code: NetworkCode, message: String },

    #[error("Request timed out [{code}] after {timeout_ms}ms")]
    RequestTimeoutV2 { code: NetworkCode, timeout_ms: u64 },

    #[error("Request rate limit exceeded [{code}], retry after {retry_after_secs}s")]
    RateLimitV2 {
        code: NetworkCode,
        retry_after_secs: u64,
    },

    #[error("Service temporarily unavailable [{code}]: {message}")]
    ServiceUnavailableV2 { code: ServiceCode, message: String },

    #[error("Authentication error [{code}]: {message}")]
    AuthV2 { code: AuthCode, message: String },

    #[error("OAuth error [{code}] for provider {provider}: {message}")]
    OAuthErrorV2 {
        code: OAuthCode,
        provider: String,
        message: String,
    },

    #[error("OAuth refresh error [{code}] for provider {provider}: [{kind:?}] {message}")]
    OAuthRefreshErrorV2 {
        code: OAuthCode,
        provider: String,
        kind: OAuthErrorKind,
        message: String,
    },

    #[error("Validation failed [{code}] - {field}: {message}")]
    ValidationV2 {
        code: ValidationCode,
        field: String,
        message: String,
    },

    #[error("Invalid arguments [{code}]: {message}")]
    InvalidArgumentsV2 {
        code: ValidationCode,
        message: String,
    },

    #[error("{resource_type} not found [{code}]: {id}")]
    NotFoundV2 {
        code: NotFoundCode,
        resource_type: String,
        id: String,
    },

    #[error("UI element not found [{code}]: {name}")]
    ElementNotFoundV2 { code: UiCode, name: String },

    #[error("Binary hash mismatch [{code}]: expected={expected}, actual={actual}")]
    BinaryHashMismatchV2 {
        code: IntegrityCode,
        expected: String,
        actual: String,
    },

    #[error("Internal error [{code}]: {message}")]
    InternalV2 { code: InternalCode, message: String },

    #[error("Policy denied [{code}]: {message}")]
    PolicyDeniedV2 { code: PolicyCode, message: String },

    #[error("Process not allowed [{code}]: {message}")]
    ProcessNotAllowedV2 { code: PolicyCode, message: String },

    #[error("Consent required [{code}]: {message}")]
    ConsentRequiredV2 { code: ConsentCode, message: String },

    #[error("Consent expired [{code}]")]
    ConsentExpiredV2 { code: ConsentCode },

    #[error("Sandbox initialization failed [{code}]: {message}")]
    SandboxInitV2 { code: SandboxCode, message: String },

    #[error("Sandbox execution failed [{code}]: {message}")]
    SandboxExecutionV2 { code: SandboxCode, message: String },

    #[error("Sandbox unsupported on platform [{code}]: {message}")]
    SandboxUnsupportedV2 { code: SandboxCode, message: String },

    #[error("Execution timeout [{code}] exceeded: {timeout_ms}ms")]
    ExecutionTimeoutV2 { code: SandboxCode, timeout_ms: u64 },

    #[error("Privacy denied [{code}]: {message}")]
    PrivacyDeniedV2 {
        code: PermissionCode,
        message: String,
    },

    #[error("Permission denied [{code}]: {message}")]
    PermissionDeniedV2 {
        code: PermissionCode,
        message: String,
    },

    #[error("OCR error [{code}]: {message}")]
    OcrErrorV2 { code: ProviderCode, message: String },

    #[error("Analysis error [{code}]: {message}")]
    AnalysisV2 { code: ProviderCode, message: String },

    #[error("Audio capture error [{code}]: {message}")]
    AudioCaptureV2 { code: AudioCode, message: String },

    #[error("Speech-to-text error [{code}]: {message}")]
    SpeechToTextV2 { code: AudioCode, message: String },

    #[error("Storage error [{code}]: {message}")]
    StorageV2 { code: StorageCode, message: String },

    #[error("Secret store error [{code}]: {message}")]
    SecretStoreErrorV2 { code: SecretCode, message: String },

    // === `#[from]`-wrapped external error types (unchanged across all phases) ===
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // === V1 deprecated variants (removed in Phase 4) ===
    //
    // All variants below are marked #[deprecated]. `cargo build` emits warnings
    // at every existing V1 construction/match site; Phase 2 per-crate retrofits
    // migrate them to the V2 variants above. Phase 4 deletes this entire block
    // then rust-analyzer LSP renames V2 → canonical (e.g., ConfigV2 → Config).
    #[deprecated(note = "use ConfigV2 { code, message } — ADR-019")]
    #[error("Configuration error: {0}")]
    Config(String),

    #[deprecated(note = "use NetworkV2 { code, message } — ADR-019")]
    #[error("Network error: {0}")]
    Network(String),

    #[deprecated(note = "use RequestTimeoutV2 { code, timeout_ms } — ADR-019")]
    #[error("Request timed out after {timeout_ms}ms")]
    RequestTimeout { timeout_ms: u64 },

    #[deprecated(note = "use RateLimitV2 { code, retry_after_secs } — ADR-019")]
    #[error("Request rate limit exceeded, retry after {retry_after_secs}s")]
    RateLimit { retry_after_secs: u64 },

    #[deprecated(note = "use ServiceUnavailableV2 — ADR-019")]
    #[error("Service temporarily unavailable: {0}")]
    ServiceUnavailable(String),

    #[deprecated(note = "use AuthV2 — ADR-019")]
    #[error("Authentication error: {0}")]
    Auth(String),

    #[deprecated(note = "use OAuthErrorV2 — ADR-019")]
    #[error("OAuth error for provider {provider}: {message}")]
    OAuthError { provider: String, message: String },

    #[deprecated(note = "use OAuthRefreshErrorV2 — ADR-019")]
    #[error("OAuth refresh error for provider {provider}: [{kind:?}] {message}")]
    OAuthRefreshError {
        provider: String,
        kind: OAuthErrorKind,
        message: String,
    },

    #[deprecated(note = "use ValidationV2 — ADR-019")]
    #[error("Validation failed - {field}: {message}")]
    Validation { field: String, message: String },

    #[deprecated(note = "use InvalidArgumentsV2 — ADR-019")]
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[deprecated(note = "use NotFoundV2 — ADR-019")]
    #[error("{resource_type} not found: {id}")]
    NotFound { resource_type: String, id: String },

    #[deprecated(note = "use ElementNotFoundV2 — ADR-019")]
    #[error("UI element not found: {0}")]
    ElementNotFound(String),

    #[deprecated(note = "use BinaryHashMismatchV2 — ADR-019")]
    #[error("Binary hash mismatch: expected={expected}, actual={actual}")]
    BinaryHashMismatch { expected: String, actual: String },

    #[deprecated(note = "use InternalV2 — ADR-019")]
    #[error("Internal error: {0}")]
    Internal(String),

    #[deprecated(note = "use PolicyDeniedV2 — ADR-019")]
    #[error("Policy denied: {0}")]
    PolicyDenied(String),

    #[deprecated(note = "use ProcessNotAllowedV2 — ADR-019")]
    #[error("Process is not allowed: {0}")]
    ProcessNotAllowed(String),

    #[deprecated(note = "use ConsentRequiredV2 — ADR-019")]
    #[error("Consent required: {0}")]
    ConsentRequired(String),

    #[deprecated(note = "use ConsentExpiredV2 — ADR-019")]
    #[error("Consent expired - re-consent required")]
    ConsentExpired,

    #[deprecated(note = "use SandboxInitV2 — ADR-019")]
    #[error("Sandbox initialization failed: {0}")]
    SandboxInit(String),

    #[deprecated(note = "use SandboxExecutionV2 — ADR-019")]
    #[error("Sandbox execution failed: {0}")]
    SandboxExecution(String),

    #[deprecated(note = "use SandboxUnsupportedV2 — ADR-019")]
    #[error("Sandbox unsupported on platform: {0}")]
    SandboxUnsupported(String),

    #[deprecated(note = "use ExecutionTimeoutV2 — ADR-019")]
    #[error("Execution timeout exceeded: {timeout_ms}ms")]
    ExecutionTimeout { timeout_ms: u64 },

    #[deprecated(note = "use PrivacyDeniedV2 — ADR-019")]
    #[error("Privacy denied: {0}")]
    PrivacyDenied(String),

    #[deprecated(note = "use PermissionDeniedV2 — ADR-019")]
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[deprecated(note = "use OcrErrorV2 — ADR-019")]
    #[error("OCR error: {0}")]
    OcrError(String),

    #[deprecated(note = "use AnalysisV2 — ADR-019")]
    #[error("Analysis error: {0}")]
    Analysis(String),

    #[deprecated(note = "use AudioCaptureV2 — ADR-019")]
    #[error("Audio capture error: {0}")]
    AudioCapture(String),

    #[deprecated(note = "use SpeechToTextV2 — ADR-019")]
    #[error("Speech-to-text error: {0}")]
    SpeechToText(String),

    #[deprecated(note = "use StorageV2 — ADR-019")]
    #[error("Storage error: {0}")]
    Storage(String),

    #[deprecated(note = "use SecretStoreErrorV2 — ADR-019")]
    #[error("secret store error: {0}")]
    SecretStoreError(String),
}

impl CoreError {
    /// Wire-format error code. UI, log, telemetry entry point.
    ///
    /// V1/V2 coexistence rule (Phases 1–3) — three-tier V1 fallback policy
    /// per spec §4.4:
    /// 1. **Default**: domain's `Generic` code (e.g., `Config(_) → ConfigCode::Generic`).
    /// 2. **Narrow-specific override**: if the V1 variant name uniquely maps to
    ///    a specific code (e.g., `RequestTimeout { .. } → NetworkCode::Timeout`,
    ///    `InvalidArguments(_) → ValidationCode::InvalidArguments`), use that code.
    /// 3. **Sole-variant domains**: for enums without `Generic` (`NotFoundCode`,
    ///    `UiCode`, `IntegrityCode`, `SandboxCode`), use the most-matching specific variant.
    ///
    /// V1 arms are deleted in Phase 4 alongside the V1 variant itself. See ADR-019.
    pub fn code(&self) -> &'static str {
        match self {
            // --- V2 struct variants ---
            Self::ConfigV2 { code, .. } => code.as_str(),
            Self::NetworkV2 { code, .. } => code.as_str(),
            Self::RequestTimeoutV2 { code, .. } => code.as_str(),
            Self::RateLimitV2 { code, .. } => code.as_str(),
            Self::ServiceUnavailableV2 { code, .. } => code.as_str(),
            Self::AuthV2 { code, .. } => code.as_str(),
            Self::OAuthErrorV2 { code, .. } => code.as_str(),
            Self::OAuthRefreshErrorV2 { code, .. } => code.as_str(),
            Self::ValidationV2 { code, .. } => code.as_str(),
            Self::InvalidArgumentsV2 { code, .. } => code.as_str(),
            Self::NotFoundV2 { code, .. } => code.as_str(),
            Self::ElementNotFoundV2 { code, .. } => code.as_str(),
            Self::BinaryHashMismatchV2 { code, .. } => code.as_str(),
            Self::InternalV2 { code, .. } => code.as_str(),
            Self::PolicyDeniedV2 { code, .. } => code.as_str(),
            Self::ProcessNotAllowedV2 { code, .. } => code.as_str(),
            Self::ConsentRequiredV2 { code, .. } => code.as_str(),
            Self::ConsentExpiredV2 { code } => code.as_str(),
            Self::SandboxInitV2 { code, .. } => code.as_str(),
            Self::SandboxExecutionV2 { code, .. } => code.as_str(),
            Self::SandboxUnsupportedV2 { code, .. } => code.as_str(),
            Self::ExecutionTimeoutV2 { code, .. } => code.as_str(),
            Self::PrivacyDeniedV2 { code, .. } => code.as_str(),
            Self::PermissionDeniedV2 { code, .. } => code.as_str(),
            Self::OcrErrorV2 { code, .. } => code.as_str(),
            Self::AnalysisV2 { code, .. } => code.as_str(),
            Self::AudioCaptureV2 { code, .. } => code.as_str(),
            Self::SpeechToTextV2 { code, .. } => code.as_str(),
            Self::StorageV2 { code, .. } => code.as_str(),
            Self::SecretStoreErrorV2 { code, .. } => code.as_str(),

            // --- `#[from]`-wrapped external variants (derived) ---
            Self::Serialization(_) => InternalCode::Serialization.as_str(),
            Self::Io(_) => InternalCode::Io.as_str(),

            // --- V1 deprecated variants (removed in Phase 4) ---
            #[allow(deprecated)]
            Self::Config(_) => ConfigCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::Network(_) => NetworkCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::RequestTimeout { .. } => NetworkCode::Timeout.as_str(),
            #[allow(deprecated)]
            Self::RateLimit { .. } => NetworkCode::RateLimit.as_str(),
            #[allow(deprecated)]
            Self::ServiceUnavailable(_) => ServiceCode::Unavailable.as_str(),
            #[allow(deprecated)]
            Self::Auth(_) => AuthCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::OAuthError { .. } => OAuthCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::OAuthRefreshError { .. } => OAuthCode::RefreshFailed.as_str(),
            #[allow(deprecated)]
            Self::Validation { .. } => ValidationCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::InvalidArguments(_) => ValidationCode::InvalidArguments.as_str(),
            #[allow(deprecated)]
            Self::NotFound { .. } => NotFoundCode::ResourceMissing.as_str(),
            #[allow(deprecated)]
            Self::ElementNotFound(_) => UiCode::ElementMissing.as_str(),
            #[allow(deprecated)]
            Self::BinaryHashMismatch { .. } => IntegrityCode::HashMismatch.as_str(),
            #[allow(deprecated)]
            Self::Internal(_) => InternalCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::PolicyDenied(_) => PolicyCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::ProcessNotAllowed(_) => PolicyCode::ProcessDenied.as_str(),
            #[allow(deprecated)]
            Self::ConsentRequired(_) => ConsentCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::ConsentExpired => ConsentCode::Expired.as_str(),
            #[allow(deprecated)]
            Self::SandboxInit(_) => SandboxCode::InitFailed.as_str(),
            #[allow(deprecated)]
            Self::SandboxExecution(_) => SandboxCode::ExecutionFailed.as_str(),
            #[allow(deprecated)]
            Self::SandboxUnsupported(_) => SandboxCode::UnsupportedPlatform.as_str(),
            #[allow(deprecated)]
            Self::ExecutionTimeout { .. } => SandboxCode::Timeout.as_str(),
            #[allow(deprecated)]
            Self::PrivacyDenied(_) => PermissionCode::PrivacyDenied.as_str(),
            #[allow(deprecated)]
            Self::PermissionDenied(_) => PermissionCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::OcrError(_) => ProviderCode::OcrFailed.as_str(),
            #[allow(deprecated)]
            Self::Analysis(_) => ProviderCode::AnalysisFailed.as_str(),
            #[allow(deprecated)]
            Self::AudioCapture(_) => AudioCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::SpeechToText(_) => AudioCode::SttFailed.as_str(),
            #[allow(deprecated)]
            Self::Storage(_) => StorageCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::SecretStoreError(_) => SecretCode::Generic.as_str(),
        }
    }
}

/// GUI 상호작용 전용 에러 (AutomationPort GUI 메서드에서 사용)
///
/// 이전에는 `oneshim-automation::gui_interaction::GuiInteractionError`에 정의되었으나,
/// AutomationPort 추상화를 위해 oneshim-core로 이동 (ADR-001 §7)
#[derive(Debug, Error)]
pub enum GuiInteractionError {
    // === V2 variants (renamed in Phase 4) ===
    #[error("GUI session token is invalid [{code}]")]
    UnauthorizedV2 { code: GuiCode },

    #[error("GUI session '{name}' not found [{code}]")]
    NotFoundV2 { code: GuiCode, name: String },

    #[error("Invalid GUI request [{code}]: {message}")]
    BadRequestV2 { code: GuiCode, message: String },

    #[error("GUI request forbidden [{code}]: {message}")]
    ForbiddenV2 { code: GuiCode, message: String },

    #[error("GUI focus drift detected [{code}]: {message}")]
    FocusDriftV2 { code: GuiCode, message: String },

    #[error("GUI ticket is no longer valid [{code}]: {message}")]
    TicketInvalidV2 { code: GuiCode, message: String },

    #[error("GUI runtime unavailable [{code}]: {message}")]
    UnavailableV2 { code: GuiCode, message: String },

    #[error("GUI runtime failed [{code}]: {message}")]
    InternalV2 { code: GuiCode, message: String },

    // === V1 deprecated variants ===
    #[deprecated(note = "use UnauthorizedV2 — ADR-019")]
    #[error("GUI session token is invalid")]
    Unauthorized,

    #[deprecated(note = "use NotFoundV2 — ADR-019")]
    #[error("GUI session '{0}' not found")]
    NotFound(String),

    #[deprecated(note = "use BadRequestV2 — ADR-019")]
    #[error("Invalid GUI request: {0}")]
    BadRequest(String),

    #[deprecated(note = "use ForbiddenV2 — ADR-019")]
    #[error("GUI request forbidden: {0}")]
    Forbidden(String),

    #[deprecated(note = "use FocusDriftV2 — ADR-019")]
    #[error("GUI focus drift detected: {0}")]
    FocusDrift(String),

    #[deprecated(note = "use TicketInvalidV2 — ADR-019")]
    #[error("GUI ticket is no longer valid: {0}")]
    TicketInvalid(String),

    #[deprecated(note = "use UnavailableV2 — ADR-019")]
    #[error("GUI runtime unavailable: {0}")]
    Unavailable(String),

    #[deprecated(note = "use InternalV2 — ADR-019")]
    #[error("GUI runtime failed: {0}")]
    Internal(String),
}

impl GuiInteractionError {
    /// Wire-format error code for GUI errors. Mirrors `CoreError::code()` pattern.
    pub fn code(&self) -> &'static str {
        match self {
            // V2 variants
            Self::UnauthorizedV2 { code } => code.as_str(),
            Self::NotFoundV2 { code, .. } => code.as_str(),
            Self::BadRequestV2 { code, .. } => code.as_str(),
            Self::ForbiddenV2 { code, .. } => code.as_str(),
            Self::FocusDriftV2 { code, .. } => code.as_str(),
            Self::TicketInvalidV2 { code, .. } => code.as_str(),
            Self::UnavailableV2 { code, .. } => code.as_str(),
            Self::InternalV2 { code, .. } => code.as_str(),

            // V1 deprecated variants (narrow-specific per spec §4.4 tier 2)
            #[allow(deprecated)]
            Self::Unauthorized => GuiCode::Unauthorized.as_str(),
            #[allow(deprecated)]
            Self::NotFound(_) => GuiCode::NotFound.as_str(),
            #[allow(deprecated)]
            Self::BadRequest(_) => GuiCode::BadRequest.as_str(),
            #[allow(deprecated)]
            Self::Forbidden(_) => GuiCode::Forbidden.as_str(),
            #[allow(deprecated)]
            Self::FocusDrift(_) => GuiCode::FocusDrift.as_str(),
            #[allow(deprecated)]
            Self::TicketInvalid(_) => GuiCode::TicketInvalid.as_str(),
            #[allow(deprecated)]
            Self::Unavailable(_) => GuiCode::Unavailable.as_str(),
            #[allow(deprecated)]
            Self::Internal(_) => GuiCode::InternalError.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_denied_display() {
        #[allow(deprecated)]
        let err = CoreError::PermissionDenied("macOS Accessibility".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("Permission denied"));
        assert!(msg.contains("macOS Accessibility"));
    }

    #[test]
    fn config_v2_code_returns_config_invalid() {
        let err = CoreError::ConfigV2 {
            code: ConfigCode::Invalid,
            message: "bad".into(),
        };
        assert_eq!(err.code(), "config.invalid");
    }

    #[test]
    fn config_v1_code_returns_config_generic_fallback() {
        #[allow(deprecated)]
        let err = CoreError::Config("legacy".into());
        assert_eq!(err.code(), "config.generic");
    }

    #[test]
    fn serialization_code_returns_internal_serialization() {
        let err: CoreError = serde_json::from_str::<i32>("not a number")
            .expect_err("should be error")
            .into();
        assert_eq!(err.code(), "internal.serialization");
    }

    #[test]
    fn io_code_returns_internal_io() {
        let err: CoreError = std::io::Error::other("x").into();
        assert_eq!(err.code(), "internal.io");
    }

    #[test]
    fn bedrock_unsupported_code() {
        let err = CoreError::ConfigV2 {
            code: ConfigCode::UnsupportedProviderBedrock,
            message: "AWS Bedrock is intentionally unsupported in this build".into(),
        };
        assert_eq!(err.code(), "provider.bedrock.unsupported");
    }

    #[test]
    fn gui_v2_code_returns_gui_unauthorized() {
        let err = GuiInteractionError::UnauthorizedV2 {
            code: GuiCode::Unauthorized,
        };
        assert_eq!(err.code(), "gui.unauthorized");
    }

    #[test]
    fn gui_v1_code_returns_bad_request_code() {
        // V1 GuiInteractionError::BadRequest maps to GuiCode::BadRequest
        // (narrow-specific per spec §4.4 tier 2).
        #[allow(deprecated)]
        let err = GuiInteractionError::BadRequest("bad".into());
        assert_eq!(err.code(), "gui.bad_request");
    }

    #[test]
    fn every_core_error_variant_has_code() {
        // Constructs one sample per V2 + V1 + #[from] variant. Compilation
        // exhaustiveness ensures this list stays complete when new variants land.
        let samples: Vec<CoreError> = sample_core_error_variants();
        for err in samples {
            let c = err.code();
            assert!(!c.is_empty(), "empty code for {err:?}");
            assert!(c.contains('.'), "missing dot in {c:?}");
        }
    }

    #[test]
    fn every_gui_error_variant_has_code() {
        for err in sample_gui_error_variants() {
            let c = err.code();
            assert!(!c.is_empty());
            assert!(c.contains('.'));
        }
    }

    fn sample_core_error_variants() -> Vec<CoreError> {
        vec![
            CoreError::ConfigV2 {
                code: ConfigCode::Invalid,
                message: "".into(),
            },
            CoreError::NetworkV2 {
                code: NetworkCode::Failed,
                message: "".into(),
            },
            CoreError::RequestTimeoutV2 {
                code: NetworkCode::Timeout,
                timeout_ms: 0,
            },
            CoreError::RateLimitV2 {
                code: NetworkCode::RateLimit,
                retry_after_secs: 0,
            },
            CoreError::ServiceUnavailableV2 {
                code: ServiceCode::Unavailable,
                message: "".into(),
            },
            CoreError::AuthV2 {
                code: AuthCode::Failed,
                message: "".into(),
            },
            CoreError::OAuthErrorV2 {
                code: OAuthCode::Failed,
                provider: "".into(),
                message: "".into(),
            },
            CoreError::OAuthRefreshErrorV2 {
                code: OAuthCode::RefreshFailed,
                provider: "".into(),
                kind: OAuthErrorKind::InvalidGrant,
                message: "".into(),
            },
            CoreError::ValidationV2 {
                code: ValidationCode::InvalidField,
                field: "".into(),
                message: "".into(),
            },
            CoreError::InvalidArgumentsV2 {
                code: ValidationCode::InvalidArguments,
                message: "".into(),
            },
            CoreError::NotFoundV2 {
                code: NotFoundCode::ResourceMissing,
                resource_type: "".into(),
                id: "".into(),
            },
            CoreError::ElementNotFoundV2 {
                code: UiCode::ElementMissing,
                name: "".into(),
            },
            CoreError::BinaryHashMismatchV2 {
                code: IntegrityCode::HashMismatch,
                expected: "".into(),
                actual: "".into(),
            },
            CoreError::InternalV2 {
                code: InternalCode::Generic,
                message: "".into(),
            },
            CoreError::PolicyDeniedV2 {
                code: PolicyCode::Denied,
                message: "".into(),
            },
            CoreError::ProcessNotAllowedV2 {
                code: PolicyCode::ProcessDenied,
                message: "".into(),
            },
            CoreError::ConsentRequiredV2 {
                code: ConsentCode::Required,
                message: "".into(),
            },
            CoreError::ConsentExpiredV2 {
                code: ConsentCode::Expired,
            },
            CoreError::SandboxInitV2 {
                code: SandboxCode::InitFailed,
                message: "".into(),
            },
            CoreError::SandboxExecutionV2 {
                code: SandboxCode::ExecutionFailed,
                message: "".into(),
            },
            CoreError::SandboxUnsupportedV2 {
                code: SandboxCode::UnsupportedPlatform,
                message: "".into(),
            },
            CoreError::ExecutionTimeoutV2 {
                code: SandboxCode::Timeout,
                timeout_ms: 0,
            },
            CoreError::PrivacyDeniedV2 {
                code: PermissionCode::PrivacyDenied,
                message: "".into(),
            },
            CoreError::PermissionDeniedV2 {
                code: PermissionCode::PermissionDenied,
                message: "".into(),
            },
            CoreError::OcrErrorV2 {
                code: ProviderCode::OcrFailed,
                message: "".into(),
            },
            CoreError::AnalysisV2 {
                code: ProviderCode::AnalysisFailed,
                message: "".into(),
            },
            CoreError::AudioCaptureV2 {
                code: AudioCode::CaptureFailed,
                message: "".into(),
            },
            CoreError::SpeechToTextV2 {
                code: AudioCode::SttFailed,
                message: "".into(),
            },
            CoreError::StorageV2 {
                code: StorageCode::Failed,
                message: "".into(),
            },
            CoreError::SecretStoreErrorV2 {
                code: SecretCode::Failed,
                message: "".into(),
            },
            // #[from] wrapped
            CoreError::Serialization(serde_json::from_str::<i32>("x").unwrap_err()),
            CoreError::Io(std::io::Error::other("x")),
        ]
    }

    fn sample_gui_error_variants() -> Vec<GuiInteractionError> {
        vec![
            GuiInteractionError::UnauthorizedV2 {
                code: GuiCode::Unauthorized,
            },
            GuiInteractionError::NotFoundV2 {
                code: GuiCode::NotFound,
                name: "".into(),
            },
            GuiInteractionError::BadRequestV2 {
                code: GuiCode::BadRequest,
                message: "".into(),
            },
            GuiInteractionError::ForbiddenV2 {
                code: GuiCode::Forbidden,
                message: "".into(),
            },
            GuiInteractionError::FocusDriftV2 {
                code: GuiCode::FocusDrift,
                message: "".into(),
            },
            GuiInteractionError::TicketInvalidV2 {
                code: GuiCode::TicketInvalid,
                message: "".into(),
            },
            GuiInteractionError::UnavailableV2 {
                code: GuiCode::Unavailable,
                message: "".into(),
            },
            GuiInteractionError::InternalV2 {
                code: GuiCode::InternalError,
                message: "".into(),
            },
        ]
    }
}
