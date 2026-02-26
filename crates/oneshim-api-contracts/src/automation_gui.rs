use oneshim_core::models::gui::{GuiActionRequest, GuiExecutionTicket, GuiInteractionSession};
use oneshim_core::models::intent::IntentResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct GuiSessionPath {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiCreateSessionRequest {
    pub app_name: Option<String>,
    pub screen_id: Option<String>,
    pub min_confidence: Option<f64>,
    pub max_candidates: Option<usize>,
    pub session_ttl_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiHighlightRequest {
    pub candidate_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiConfirmRequest {
    pub candidate_id: String,
    pub action: GuiActionRequest,
    pub ticket_ttl_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiExecutionRequest {
    pub ticket: GuiExecutionTicket,
}

#[derive(Debug, Serialize)]
pub struct GuiCreateSessionResponse {
    pub schema_version: String,
    pub session: GuiInteractionSession,
    pub capability_token: String,
}

#[derive(Debug, Serialize)]
pub struct GuiSessionResponse {
    pub schema_version: String,
    pub session: GuiInteractionSession,
}

#[derive(Debug, Serialize)]
pub struct GuiConfirmResponse {
    pub schema_version: String,
    pub ticket: GuiExecutionTicket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiExecutionOutcome {
    pub session: GuiInteractionSession,
    pub succeeded: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GuiExecuteResponse {
    pub schema_version: String,
    pub command_id: String,
    pub ticket: GuiExecutionTicket,
    pub result: IntentResult,
    pub outcome: GuiExecutionOutcome,
}
