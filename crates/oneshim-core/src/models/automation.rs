use serde::{Deserialize, Serialize};

use super::gui::{GuiExecutionOutcome, GuiExecutionTicket};
use super::intent::{AutomationIntent, IntentResult};

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

// ── Automation result types ──
// 이전에는 oneshim-automation::controller::types에 있었으나,
// AutomationPort 추상화를 위해 oneshim-core로 이동 (ADR-001 §7)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationCommand {
    pub command_id: String,
    pub session_id: String,
    pub action: AutomationAction,
    pub timeout_ms: Option<u64>,
    pub policy_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandResult {
    Success,
    Failed(String),
    Timeout,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepResult {
    pub step_name: String,
    pub step_index: usize,
    pub success: bool,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    pub preset_id: String,
    pub success: bool,
    pub steps_executed: usize,
    pub total_steps: usize,
    pub total_elapsed_ms: u64,
    pub step_results: Vec<WorkflowStepResult>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedIntentResult {
    pub planned_intent: AutomationIntent,
    pub result: IntentResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiExecutionResult {
    pub command_id: String,
    pub ticket: GuiExecutionTicket,
    pub result: IntentResult,
    pub outcome: GuiExecutionOutcome,
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
