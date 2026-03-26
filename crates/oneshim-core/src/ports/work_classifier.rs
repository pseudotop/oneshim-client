use crate::models::tiered_memory::WorkType;

/// Classify current user activity into a WorkType.
/// Pure logic — no I/O, no async needed.
pub trait WorkTypeClassifier: Send + Sync {
    fn classify(
        &self,
        app_name: &str,
        window_title: &str,
        focused_role: Option<&str>,
        ocr_text_sample: Option<&str>,
    ) -> WorkType;
}
