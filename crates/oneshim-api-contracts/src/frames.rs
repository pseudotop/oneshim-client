use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct FrameResponse {
    pub id: i64,
    pub timestamp: String,
    pub trigger_type: String,
    pub app_name: String,
    pub window_title: String,
    pub importance: f32,
    pub resolution: String,
    pub file_path: Option<String>,
    pub ocr_text: Option<String>,
    pub image_url: Option<String>,
    #[serde(default)]
    pub tag_ids: Vec<i64>,
}
