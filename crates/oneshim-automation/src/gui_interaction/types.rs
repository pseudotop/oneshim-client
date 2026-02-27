use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use oneshim_core::models::gui::{GuiExecutionTicket, GuiInteractionSession};

use crate::controller::AutomationAction;

#[derive(Debug, thiserror::Error)]
pub enum GuiInteractionError {
    #[error("GUI session token is invalid")]
    Unauthorized,

    #[error("GUI session '{0}' not found")]
    NotFound(String),

    #[error("Invalid GUI request: {0}")]
    BadRequest(String),

    #[error("GUI request forbidden: {0}")]
    Forbidden(String),

    #[error("GUI focus drift detected: {0}")]
    FocusDrift(String),

    #[error("GUI ticket is no longer valid: {0}")]
    TicketInvalid(String),

    #[error("GUI runtime unavailable: {0}")]
    Unavailable(String),

    #[error("GUI runtime failed: {0}")]
    Internal(String),
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
pub struct GuiCreateSessionResponse {
    pub session: GuiInteractionSession,
    pub capability_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiHighlightRequest {
    pub candidate_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiConfirmRequest {
    pub candidate_id: String,
    pub action: oneshim_core::models::gui::GuiActionRequest,
    pub ticket_ttl_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiExecutionRequest {
    pub ticket: GuiExecutionTicket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiExecutionPlan {
    pub session_id: String,
    pub command_id: String,
    pub actions: Vec<AutomationAction>,
    pub ticket: GuiExecutionTicket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiExecutionOutcome {
    pub session: GuiInteractionSession,
    pub succeeded: bool,
    pub detail: Option<String>,
    pub steps_completed: usize,
    pub total_steps: usize,
}

#[derive(Debug, Clone)]
pub(super) struct ConfirmedAction {
    pub(super) candidate_id: String,
    pub(super) actions: Vec<AutomationAction>,
    pub(super) action_hash: String,
    pub(super) ticket: GuiExecutionTicket,
}

#[derive(Debug, Clone)]
pub(super) struct StoredSession {
    pub(super) session: GuiInteractionSession,
    pub(super) capability_token: String,
    pub(super) overlay_handle_id: Option<String>,
    pub(super) confirmed_action: Option<ConfirmedAction>,
    pub(super) used_ticket_nonces: HashSet<String>,
}
