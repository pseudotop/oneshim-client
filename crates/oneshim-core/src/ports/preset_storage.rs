use crate::error::CoreError;
use crate::models::intent::WorkflowPreset;

/// Synchronous storage port for automation presets (follows FewShotStorage pattern).
///
/// Implementations persist user-created custom presets to durable storage.
/// Built-in presets are not stored — they are returned by `builtin_presets()`.
pub trait PresetStorage: Send + Sync {
    fn list_presets(&self) -> Result<Vec<WorkflowPreset>, CoreError>;
    fn get_preset(&self, id: &str) -> Result<Option<WorkflowPreset>, CoreError>;
    fn save_preset(&self, preset: &WorkflowPreset) -> Result<(), CoreError>;
    fn delete_preset(&self, id: &str) -> Result<bool, CoreError>;
}
