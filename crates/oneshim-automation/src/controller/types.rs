use serde::{Deserialize, Serialize};

use crate::gui_interaction::GuiExecutionOutcome;
use oneshim_core::models::gui::GuiExecutionTicket;
use oneshim_core::models::intent::{AutomationIntent, IntentResult};

pub use oneshim_core::models::automation::{AutomationAction, MouseButton};

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
