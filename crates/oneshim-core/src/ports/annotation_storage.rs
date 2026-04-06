use crate::error::CoreError;
use crate::models::annotation::FrameAnnotation;

/// Synchronous storage port for frame annotations (highlights, memos, arrows).
///
/// Follows the same synchronous pattern as `TagStorage` and `PresetStorage`
/// since all operations go through the single-connection `Mutex<Connection>`.
pub trait AnnotationStorage: Send + Sync {
    /// List all annotations for a given frame.
    fn list_annotations(&self, frame_id: i64) -> Result<Vec<FrameAnnotation>, CoreError>;

    /// Persist a new annotation.
    fn save_annotation(&self, annotation: &FrameAnnotation) -> Result<(), CoreError>;

    /// Delete an annotation by ID. Returns `Ok(())` even if the ID does not exist.
    fn delete_annotation(&self, annotation_id: &str) -> Result<(), CoreError>;
}
