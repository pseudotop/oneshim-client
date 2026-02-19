//! 자동화 명령 모델.
//!
//! Sandbox 포트가 AutomationAction을 참조하므로
//! oneshim-core에 정의하여 순환 의존을 방지한다.

use serde::{Deserialize, Serialize};

/// 마우스 버튼 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// 자동화 액션 유형
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutomationAction {
    /// 마우스 이동
    MouseMove { x: i32, y: i32 },
    /// 마우스 클릭
    MouseClick { button: String, x: i32, y: i32 },
    /// 텍스트 입력
    KeyType { text: String },
    /// 키 누름
    KeyPress { key: String },
    /// 키 놓음
    KeyRelease { key: String },
    /// 단축키 (복합 키)
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
