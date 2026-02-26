use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::gui::{ExecutionBinding, FocusSnapshot, FocusValidation};

#[async_trait]
pub trait FocusProbe: Send + Sync {
    async fn current_focus(&self) -> Result<FocusSnapshot, CoreError>;

    async fn validate_execution_binding(
        &self,
        binding: &ExecutionBinding,
    ) -> Result<FocusValidation, CoreError>;
}
