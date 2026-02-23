//! UI Scene 도메인 모델.
//!
//! OCR/검출 결과를 좌표 + 라벨 중심으로 보관하기 위한 구조.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::intent::ElementBounds;

/// UI Scene 계약 버전.
pub const UI_SCENE_SCHEMA_VERSION: &str = "ui_scene.v1";

fn default_ui_scene_schema_version() -> String {
    UI_SCENE_SCHEMA_VERSION.to_string()
}

/// 정규화된 경계 박스 (0.0 ~ 1.0)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NormalizedBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl NormalizedBounds {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
            width: width.clamp(0.0, 1.0),
            height: height.clamp(0.0, 1.0),
        }
    }
}

/// 장면 내 UI 요소.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSceneElement {
    /// 요소 ID (scene 내 unique)
    pub element_id: String,
    /// 절대 좌표
    pub bbox_abs: ElementBounds,
    /// 정규화 좌표
    pub bbox_norm: NormalizedBounds,
    /// 사람이 읽을 수 있는 라벨
    pub label: String,
    /// 역할 (button/input/menu/tab 등)
    pub role: Option<String>,
    /// 의도 라벨 (compose/review/execute 등)
    pub intent: Option<String>,
    /// 상태 라벨 (enabled/disabled/selected/error 등)
    pub state: Option<String>,
    /// 검출 신뢰도
    pub confidence: f64,
    /// 마스킹 텍스트
    pub text_masked: Option<String>,
    /// 부모 요소 ID (컨테이너 추론 시)
    pub parent_id: Option<String>,
}

/// 단일 화면 분석 스냅샷.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiScene {
    /// 계약 버전 (플랫폼/클라이언트 호환성 추적)
    #[serde(default = "default_ui_scene_schema_version")]
    pub schema_version: String,
    /// 장면 ID
    pub scene_id: String,
    /// 앱 이름
    pub app_name: Option<String>,
    /// 화면/모니터 ID
    pub screen_id: Option<String>,
    /// 캡처 시각
    pub captured_at: DateTime<Utc>,
    /// 화면 너비(px)
    pub screen_width: u32,
    /// 화면 높이(px)
    pub screen_height: u32,
    /// 검출 요소 목록
    pub elements: Vec<UiSceneElement>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_bounds_clamps_to_zero_one() {
        let bounds = NormalizedBounds::new(-0.2, 0.4, 1.8, -0.1);
        assert_eq!(bounds.x, 0.0);
        assert_eq!(bounds.y, 0.4);
        assert_eq!(bounds.width, 1.0);
        assert_eq!(bounds.height, 0.0);
    }

    #[test]
    fn ui_scene_serde_roundtrip() {
        let scene = UiScene {
            schema_version: UI_SCENE_SCHEMA_VERSION.to_string(),
            scene_id: "scene-1".to_string(),
            app_name: Some("VSCode".to_string()),
            screen_id: Some("screen-main".to_string()),
            captured_at: Utc::now(),
            screen_width: 1920,
            screen_height: 1080,
            elements: vec![UiSceneElement {
                element_id: "el-1".to_string(),
                bbox_abs: ElementBounds {
                    x: 100,
                    y: 80,
                    width: 240,
                    height: 48,
                },
                bbox_norm: NormalizedBounds::new(0.05, 0.07, 0.12, 0.04),
                label: "Save".to_string(),
                role: Some("button".to_string()),
                intent: Some("execute".to_string()),
                state: Some("enabled".to_string()),
                confidence: 0.91,
                text_masked: Some("Save".to_string()),
                parent_id: None,
            }],
        };

        let json = serde_json::to_string(&scene).unwrap();
        let deserialized: UiScene = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.scene_id, "scene-1");
        assert_eq!(deserialized.elements.len(), 1);
        assert_eq!(deserialized.elements[0].label, "Save");
    }
}
