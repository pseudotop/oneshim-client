//!

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMetadata {
    pub timestamp: DateTime<Utc>,
    pub trigger_type: String,
    pub app_name: String,
    pub window_title: String,
    pub resolution: (u32, u32),
    pub importance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImagePayload {
    Full {
        data: String,
        format: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ocr_text: Option<String>,
    },
    Delta {
        data: String,
        region: Rect,
        changed_ratio: f32,
    },
    Thumbnail {
        data: String,
        width: u32,
        height: u32,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedFrame {
    pub metadata: FrameMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_payload: Option<ImagePayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUpload {
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: FrameMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImagePayload>,
}
