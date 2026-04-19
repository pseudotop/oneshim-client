use oneshim_core::error::CoreError;
use oneshim_core::error_codes::InternalCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SuggestionError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<SuggestionError> for CoreError {
    fn from(err: SuggestionError) -> Self {
        match err {
            SuggestionError::Core(e) => e,
            SuggestionError::Internal(msg) => CoreError::InternalV2 {
                code: InternalCode::Generic,
                message: msg,
            },
        }
    }
}
