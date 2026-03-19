use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::frame::BoundingBox;

/// A GUI element inferred from OCR + input correlation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuiElement {
    /// Text content of the element.
    pub text: String,
    /// Spatial position on screen.
    pub bbox: BoundingBox,
    /// Inferred element type.
    pub element_type: GuiElementType,
    /// OCR confidence.
    pub confidence: f32,
}

/// Inferred type of a GUI element based on OCR text and position heuristics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GuiElementType {
    Button,
    TextInput,
    Link,
    MenuItem,
    TabLabel,
    StatusBar,
    TitleBar,
    Unknown,
}

/// Record of a user interacting with a GUI element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiInteractionEvent {
    pub timestamp: DateTime<Utc>,
    pub element: GuiElement,
    pub interaction_type: GuiInteractionType,
    pub app_name: String,
}

/// Type of user interaction with a GUI element.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GuiInteractionType {
    Click,
    DoubleClick,
    RightClick,
    Type,
    Hover,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_element_serde_roundtrip() {
        let element = GuiElement {
            text: "Save".to_string(),
            bbox: BoundingBox {
                x: 100,
                y: 200,
                width: 80,
                height: 30,
            },
            element_type: GuiElementType::Button,
            confidence: 0.92,
        };
        let json = serde_json::to_string(&element).unwrap();
        let parsed: GuiElement = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.text, "Save");
        assert_eq!(parsed.element_type, GuiElementType::Button);
    }

    #[test]
    fn gui_element_type_serde_screaming_case() {
        let json = serde_json::to_string(&GuiElementType::TextInput).unwrap();
        assert_eq!(json, "\"TEXT_INPUT\"");

        let parsed: GuiElementType = serde_json::from_str("\"TITLE_BAR\"").unwrap();
        assert_eq!(parsed, GuiElementType::TitleBar);
    }

    #[test]
    fn gui_interaction_type_serde_screaming_case() {
        let json = serde_json::to_string(&GuiInteractionType::DoubleClick).unwrap();
        assert_eq!(json, "\"DOUBLE_CLICK\"");

        let parsed: GuiInteractionType = serde_json::from_str("\"RIGHT_CLICK\"").unwrap();
        assert_eq!(parsed, GuiInteractionType::RightClick);
    }

    #[test]
    fn gui_interaction_event_serde_roundtrip() {
        let event = GuiInteractionEvent {
            timestamp: Utc::now(),
            element: GuiElement {
                text: "File".to_string(),
                bbox: BoundingBox {
                    x: 0,
                    y: 0,
                    width: 40,
                    height: 20,
                },
                element_type: GuiElementType::MenuItem,
                confidence: 0.88,
            },
            interaction_type: GuiInteractionType::Click,
            app_name: "Firefox".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: GuiInteractionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.app_name, "Firefox");
        assert_eq!(parsed.interaction_type, GuiInteractionType::Click);
        assert_eq!(parsed.element.element_type, GuiElementType::MenuItem);
    }
}
