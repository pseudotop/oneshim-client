use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Bounding box in pixel coordinates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundingBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl BoundingBox {
    /// Check if a pixel coordinate falls within this bounding box.
    pub fn contains_point(&self, px: u32, py: u32) -> bool {
        px >= self.x
            && px < self.x.saturating_add(self.width)
            && py >= self.y
            && py < self.y.saturating_add(self.height)
    }

    /// Return the center point of the bounding box.
    pub fn center(&self) -> (u32, u32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }

    /// Return the area in pixels.
    pub fn area(&self) -> u32 {
        self.width * self.height
    }
}

/// OCR text extraction with spatial position information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrRegion {
    /// Extracted text content.
    pub text: String,
    /// Bounding box in pixels.
    pub bbox: BoundingBox,
    /// OCR confidence score [0.0, 1.0].
    pub confidence: f32,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounding_box_contains_point_inside() {
        let bbox = BoundingBox { x: 10, y: 20, width: 100, height: 50 };
        assert!(bbox.contains_point(10, 20)); // top-left corner
        assert!(bbox.contains_point(50, 40)); // middle
        assert!(bbox.contains_point(109, 69)); // bottom-right edge (exclusive boundary)
    }

    #[test]
    fn bounding_box_contains_point_outside() {
        let bbox = BoundingBox { x: 10, y: 20, width: 100, height: 50 };
        assert!(!bbox.contains_point(9, 20)); // left of box
        assert!(!bbox.contains_point(10, 19)); // above box
        assert!(!bbox.contains_point(110, 20)); // right edge (exclusive)
        assert!(!bbox.contains_point(10, 70)); // bottom edge (exclusive)
    }

    #[test]
    fn bounding_box_center() {
        let bbox = BoundingBox { x: 10, y: 20, width: 100, height: 50 };
        assert_eq!(bbox.center(), (60, 45));
    }

    #[test]
    fn bounding_box_center_zero_origin() {
        let bbox = BoundingBox { x: 0, y: 0, width: 200, height: 100 };
        assert_eq!(bbox.center(), (100, 50));
    }

    #[test]
    fn bounding_box_area() {
        let bbox = BoundingBox { x: 0, y: 0, width: 100, height: 50 };
        assert_eq!(bbox.area(), 5000);
    }

    #[test]
    fn bounding_box_area_zero() {
        let bbox = BoundingBox { x: 0, y: 0, width: 0, height: 50 };
        assert_eq!(bbox.area(), 0);
    }

    #[test]
    fn bounding_box_serde_roundtrip() {
        let bbox = BoundingBox { x: 10, y: 20, width: 100, height: 50 };
        let json = serde_json::to_string(&bbox).unwrap();
        let parsed: BoundingBox = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, bbox);
    }

    #[test]
    fn ocr_region_serde_roundtrip() {
        let region = OcrRegion {
            text: "Hello".to_string(),
            bbox: BoundingBox { x: 10, y: 20, width: 100, height: 50 },
            confidence: 0.95,
        };
        let json = serde_json::to_string(&region).unwrap();
        let parsed: OcrRegion = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.text, "Hello");
        assert_eq!(parsed.bbox, region.bbox);
        assert!((parsed.confidence - 0.95).abs() < f32::EPSILON);
    }
}
