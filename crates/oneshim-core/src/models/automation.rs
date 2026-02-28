use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutomationAction {
    MouseMove { x: i32, y: i32 },
    MouseClick { button: String, x: i32, y: i32 },
    KeyType { text: String },
    KeyPress { key: String },
    KeyRelease { key: String },
    Hotkey { keys: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_action_serde_roundtrip() {
        let action = AutomationAction::MouseClick {
            button: "left".to_string(),
            x: 100,
            y: 200,
        };
        let json = serde_json::to_string(&action).unwrap();
        let deser: AutomationAction = serde_json::from_str(&json).unwrap();
        match deser {
            AutomationAction::MouseClick { x, y, .. } => {
                assert_eq!(x, 100);
                assert_eq!(y, 200);
            }
            other => unreachable!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn mouse_button_serde() {
        let btn = MouseButton::Left;
        let json = serde_json::to_string(&btn).unwrap();
        let deser: MouseButton = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, MouseButton::Left);
    }
}
