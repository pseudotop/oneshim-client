// crates/oneshim-storage/src/error.rs
use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("secret store error: {0}")]
    SecretStore(String),

    #[error("{resource_type} not found: {id}")]
    NotFound { resource_type: String, id: String },

    #[error("encryption error: {0}")]
    Encryption(String),

    #[error("validation failed — {field}: {message}")]
    Validation { field: String, message: String },

    #[error("configuration error: {0}")]
    Config(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<StorageError> for CoreError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::Core(e) => e,
            StorageError::Sqlite(e) => CoreError::StorageV2 {
                code: oneshim_core::error_codes::StorageCode::Failed,
                message: e.to_string(),
            },
            StorageError::Io(e) => CoreError::Io(e),
            StorageError::SecretStore(msg) => CoreError::SecretStoreErrorV2 {
                code: oneshim_core::error_codes::SecretCode::Failed,
                message: msg,
            },
            StorageError::NotFound { resource_type, id } => CoreError::NotFoundV2 {
                code: oneshim_core::error_codes::NotFoundCode::ResourceMissing,
                resource_type,
                id,
            },
            StorageError::Encryption(msg) => CoreError::InternalV2 {
                code: oneshim_core::error_codes::InternalCode::Generic,
                message: msg,
            },
            StorageError::Validation { field, message } => CoreError::ValidationV2 {
                code: oneshim_core::error_codes::ValidationCode::InvalidField,
                field,
                message,
            },
            StorageError::Config(msg) => CoreError::ConfigV2 {
                code: oneshim_core::error_codes::ConfigCode::Invalid,
                message: msg,
            },
            StorageError::Internal(msg) => CoreError::InternalV2 {
                code: oneshim_core::error_codes::InternalCode::Generic,
                message: msg,
            },
        }
    }
}
