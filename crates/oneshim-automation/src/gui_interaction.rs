use chrono::{DateTime, Duration as ChronoDuration, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use oneshim_core::error::CoreError;
use oneshim_core::models::gui::{
    ExecutionBinding, GuiActionRequest, GuiActionType, GuiCandidate, GuiExecutionTicket,
    GuiInteractionSession, GuiSessionEvent, GuiSessionState, HighlightRequest, HighlightTarget,
};
use oneshim_core::models::intent::ElementBounds;
use oneshim_core::models::ui_scene::UiScene;
use oneshim_core::ports::element_finder::ElementFinder;
use oneshim_core::ports::focus_probe::FocusProbe;
use oneshim_core::ports::overlay_driver::OverlayDriver;

use crate::controller::AutomationAction;

const GUI_HMAC_SECRET_ENV: &str = "ONESHIM_GUI_TICKET_HMAC_SECRET";
const DEFAULT_MAX_CANDIDATES: usize = 20;
const DEFAULT_MIN_CONFIDENCE: f64 = 0.5;
const DEFAULT_SESSION_TTL_SECS: i64 = 300;
const DEFAULT_TICKET_TTL_SECS: i64 = 30;
const CLEANUP_INTERVAL_SECS: u64 = 30;
const GUI_EVENT_CHANNEL_CAPACITY: usize = 256;

type HmacSha256 = Hmac<Sha256>;

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
    pub action: GuiActionRequest,
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
}

#[derive(Debug, Clone)]
struct ConfirmedAction {
    candidate_id: String,
    actions: Vec<AutomationAction>,
    action_hash: String,
    ticket: GuiExecutionTicket,
}

#[derive(Debug, Clone)]
struct StoredSession {
    session: GuiInteractionSession,
    capability_token: String,
    overlay_handle_id: Option<String>,
    confirmed_action: Option<ConfirmedAction>,
    used_ticket_nonces: HashSet<String>,
}

pub struct GuiInteractionService {
    scene_finder: Arc<dyn ElementFinder>,
    focus_probe: Arc<dyn FocusProbe>,
    overlay_driver: Arc<dyn OverlayDriver>,
    sessions: RwLock<HashMap<String, StoredSession>>,
    event_tx: broadcast::Sender<GuiSessionEvent>,
    cleanup_started: AtomicBool,
    hmac_secret: Option<Vec<u8>>,
}

impl GuiInteractionService {
    pub fn new(
        scene_finder: Arc<dyn ElementFinder>,
        focus_probe: Arc<dyn FocusProbe>,
        overlay_driver: Arc<dyn OverlayDriver>,
        hmac_secret: Option<String>,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(GUI_EVENT_CHANNEL_CAPACITY);
        Self {
            scene_finder,
            focus_probe,
            overlay_driver,
            sessions: RwLock::new(HashMap::new()),
            event_tx,
            cleanup_started: AtomicBool::new(false),
            hmac_secret: hmac_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .map(|value| value.into_bytes()),
        }
    }

    pub fn ensure_cleanup_task(self: &Arc<Self>) {
        if self.cleanup_started.swap(true, Ordering::SeqCst) {
            return;
        }

        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(CLEANUP_INTERVAL_SECS));
            loop {
                interval.tick().await;
                let Some(service) = weak.upgrade() else {
                    break;
                };
                service.expire_sessions().await;
            }
        });
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GuiSessionEvent> {
        self.event_tx.subscribe()
    }

    pub async fn subscribe_session(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<broadcast::Receiver<GuiSessionEvent>, GuiInteractionError> {
        self.assert_capability_token(session_id, capability_token)
            .await?;
        Ok(self.subscribe())
    }

    pub async fn create_session(
        &self,
        req: GuiCreateSessionRequest,
    ) -> Result<GuiCreateSessionResponse, GuiInteractionError> {
        self.require_hmac_secret()?;

        let focus = self
            .focus_probe
            .current_focus()
            .await
            .map_err(map_core_error)?;
        let scene = self
            .scene_finder
            .analyze_scene(req.app_name.as_deref(), req.screen_id.as_deref())
            .await
            .map_err(map_core_error)?;

        let now = Utc::now();
        let max_candidates = req
            .max_candidates
            .unwrap_or(DEFAULT_MAX_CANDIDATES)
            .clamp(1, 1000);
        let min_confidence = req
            .min_confidence
            .unwrap_or(DEFAULT_MIN_CONFIDENCE)
            .clamp(0.0, 1.0);
        let ttl_secs = req
            .session_ttl_secs
            .map(|value| value as i64)
            .unwrap_or(DEFAULT_SESSION_TTL_SECS)
            .clamp(30, 3600);

        let candidates = build_candidates(&scene, min_confidence, max_candidates);
        if candidates.is_empty() {
            return Err(GuiInteractionError::BadRequest(
                "No eligible GUI candidates found in scene".to_string(),
            ));
        }

        let session_id = Uuid::new_v4().to_string();
        let session = GuiInteractionSession {
            schema_version: oneshim_core::models::gui::GUI_INTERACTION_SCHEMA_VERSION.to_string(),
            session_id: session_id.clone(),
            state: GuiSessionState::Proposed,
            scene,
            focus,
            candidates,
            selected_element_id: None,
            created_at: now,
            updated_at: now,
            expires_at: now + ChronoDuration::seconds(ttl_secs),
        };

        let capability_token = new_capability_token();

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                session_id.clone(),
                StoredSession {
                    session: session.clone(),
                    capability_token: capability_token.clone(),
                    overlay_handle_id: None,
                    confirmed_action: None,
                    used_ticket_nonces: HashSet::new(),
                },
            );
        }

        self.publish_event(
            session_id,
            GuiSessionState::Proposed,
            "gui_session.proposed",
            None,
        );

        Ok(GuiCreateSessionResponse {
            session,
            capability_token,
        })
    }

    pub async fn get_session(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<GuiInteractionSession, GuiInteractionError> {
        self.assert_capability_token(session_id, capability_token)
            .await?;

        let mut sessions = self.sessions.write().await;
        let Some(stored) = sessions.get_mut(session_id) else {
            return Err(GuiInteractionError::NotFound(session_id.to_string()));
        };

        if is_expired(&stored.session.expires_at) {
            stored.session.state = GuiSessionState::Expired;
        }

        Ok(stored.session.clone())
    }

    pub async fn highlight_session(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiHighlightRequest,
    ) -> Result<GuiInteractionSession, GuiInteractionError> {
        self.assert_capability_token(session_id, capability_token)
            .await?;

        let (scene_id, targets, previous_handle_id) = {
            let mut sessions = self.sessions.write().await;
            let Some(stored) = sessions.get_mut(session_id) else {
                return Err(GuiInteractionError::NotFound(session_id.to_string()));
            };

            if is_expired(&stored.session.expires_at) {
                stored.session.state = GuiSessionState::Expired;
                return Err(GuiInteractionError::TicketInvalid(
                    "Session already expired".to_string(),
                ));
            }

            let candidate_ids: Option<HashSet<String>> = req.candidate_ids.as_ref().map(|ids| {
                ids.iter()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect()
            });

            let targets: Vec<HighlightTarget> = stored
                .session
                .candidates
                .iter()
                .filter(|candidate| {
                    candidate.eligible
                        && candidate_ids
                            .as_ref()
                            .map(|ids| ids.contains(&candidate.element.element_id))
                            .unwrap_or(true)
                })
                .map(|candidate| HighlightTarget {
                    candidate_id: candidate.element.element_id.clone(),
                    bbox_abs: ElementBounds {
                        x: candidate.element.bbox_abs.x,
                        y: candidate.element.bbox_abs.y,
                        width: candidate.element.bbox_abs.width,
                        height: candidate.element.bbox_abs.height,
                    },
                    color: "#22c55e".to_string(),
                    label: candidate.element.text_masked.clone(),
                })
                .collect();

            if targets.is_empty() {
                return Err(GuiInteractionError::BadRequest(
                    "No highlight targets available".to_string(),
                ));
            }

            (
                stored.session.scene.scene_id.clone(),
                targets,
                stored.overlay_handle_id.clone(),
            )
        };

        if let Some(handle_id) = previous_handle_id {
            let _ = self.overlay_driver.clear_highlights(&handle_id).await;
        }

        let handle = self
            .overlay_driver
            .show_highlights(HighlightRequest {
                session_id: session_id.to_string(),
                scene_id,
                targets,
            })
            .await
            .map_err(map_core_error)?;

        let updated = {
            let mut sessions = self.sessions.write().await;
            let Some(stored) = sessions.get_mut(session_id) else {
                return Err(GuiInteractionError::NotFound(session_id.to_string()));
            };
            stored.overlay_handle_id = Some(handle.handle_id);
            stored.session.state = GuiSessionState::Highlighted;
            stored.session.updated_at = Utc::now();
            stored.session.clone()
        };

        self.publish_event(
            session_id.to_string(),
            GuiSessionState::Highlighted,
            "gui_session.highlighted",
            None,
        );

        Ok(updated)
    }

    pub async fn confirm_candidate(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiConfirmRequest,
    ) -> Result<GuiExecutionTicket, GuiInteractionError> {
        let secret = self.require_hmac_secret()?;
        self.assert_capability_token(session_id, capability_token)
            .await?;

        let (session_focus, candidate, scene_id) = {
            let mut sessions = self.sessions.write().await;
            let Some(stored) = sessions.get_mut(session_id) else {
                return Err(GuiInteractionError::NotFound(session_id.to_string()));
            };

            if is_expired(&stored.session.expires_at) {
                stored.session.state = GuiSessionState::Expired;
                return Err(GuiInteractionError::TicketInvalid(
                    "Session already expired".to_string(),
                ));
            }

            let Some(candidate) = stored
                .session
                .candidates
                .iter()
                .find(|candidate| candidate.element.element_id == req.candidate_id)
                .cloned()
            else {
                return Err(GuiInteractionError::BadRequest(format!(
                    "Unknown candidate_id '{}'",
                    req.candidate_id
                )));
            };

            if !candidate.eligible {
                return Err(GuiInteractionError::BadRequest(
                    "Selected candidate is not eligible".to_string(),
                ));
            }

            (
                stored.session.focus.clone(),
                candidate,
                stored.session.scene.scene_id.clone(),
            )
        };

        let binding = ExecutionBinding {
            focus_hash: session_focus.focus_hash.clone(),
            app_name: Some(session_focus.app_name.clone()),
            pid: Some(session_focus.pid),
        };
        let focus_validation = self
            .focus_probe
            .validate_execution_binding(&binding)
            .await
            .map_err(map_core_error)?;
        if !focus_validation.valid {
            return Err(GuiInteractionError::FocusDrift(
                focus_validation
                    .reason
                    .unwrap_or_else(|| "Focused window changed".to_string()),
            ));
        }

        let actions = build_actions_for_candidate(&candidate, &req.action)?;
        let action_hash = hash_actions(&actions)?;
        let now = Utc::now();
        let ticket_ttl_secs = req
            .ticket_ttl_secs
            .map(|value| value as i64)
            .unwrap_or(DEFAULT_TICKET_TTL_SECS)
            .clamp(5, 300);
        let ticket = GuiExecutionTicket {
            schema_version: oneshim_core::models::gui::GUI_TICKET_SCHEMA_VERSION.to_string(),
            ticket_id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            scene_id,
            element_id: candidate.element.element_id.clone(),
            action_hash: action_hash.clone(),
            focus_hash: session_focus.focus_hash,
            issued_at: now,
            expires_at: now + ChronoDuration::seconds(ticket_ttl_secs),
            nonce: Uuid::new_v4().simple().to_string(),
            signature: String::new(),
        };
        let signature = sign_ticket(secret, &ticket)?;
        let mut signed_ticket = ticket;
        signed_ticket.signature = signature;

        {
            let mut sessions = self.sessions.write().await;
            let Some(stored) = sessions.get_mut(session_id) else {
                return Err(GuiInteractionError::NotFound(session_id.to_string()));
            };
            stored.confirmed_action = Some(ConfirmedAction {
                candidate_id: req.candidate_id.clone(),
                actions,
                action_hash,
                ticket: signed_ticket.clone(),
            });
            stored.session.state = GuiSessionState::Confirmed;
            stored.session.selected_element_id = Some(req.candidate_id.clone());
            stored.session.updated_at = Utc::now();
        }

        self.publish_event(
            session_id.to_string(),
            GuiSessionState::Confirmed,
            "gui_session.confirmed",
            Some(format!("candidate_id={}", req.candidate_id)),
        );

        Ok(signed_ticket)
    }

    pub async fn prepare_execution(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiExecutionRequest,
    ) -> Result<GuiExecutionPlan, GuiInteractionError> {
        let secret = self.require_hmac_secret()?;
        self.assert_capability_token(session_id, capability_token)
            .await?;

        let (session_focus, confirmed_action, session_state, expires_at) = {
            let sessions = self.sessions.read().await;
            let Some(stored) = sessions.get(session_id) else {
                return Err(GuiInteractionError::NotFound(session_id.to_string()));
            };

            let Some(confirmed_action) = stored.confirmed_action.clone() else {
                return Err(GuiInteractionError::TicketInvalid(
                    "Session has no confirmed action".to_string(),
                ));
            };

            (
                stored.session.focus.clone(),
                confirmed_action,
                stored.session.state,
                stored.session.expires_at,
            )
        };

        if is_expired(&expires_at) {
            return Err(GuiInteractionError::TicketInvalid(
                "Session already expired".to_string(),
            ));
        }

        if session_state != GuiSessionState::Confirmed {
            return Err(GuiInteractionError::TicketInvalid(format!(
                "Session state must be confirmed, current={:?}",
                session_state
            )));
        }

        verify_ticket(secret, &req.ticket)?;

        if req.ticket.session_id != session_id {
            return Err(GuiInteractionError::TicketInvalid(
                "ticket.session_id mismatch".to_string(),
            ));
        }
        if req.ticket.ticket_id != confirmed_action.ticket.ticket_id {
            return Err(GuiInteractionError::TicketInvalid(
                "ticket_id mismatch".to_string(),
            ));
        }
        if req.ticket.element_id != confirmed_action.candidate_id {
            return Err(GuiInteractionError::TicketInvalid(
                "element_id mismatch".to_string(),
            ));
        }
        if req.ticket.action_hash != confirmed_action.action_hash {
            return Err(GuiInteractionError::TicketInvalid(
                "action_hash mismatch".to_string(),
            ));
        }
        if is_expired(&req.ticket.expires_at) {
            return Err(GuiInteractionError::TicketInvalid(
                "ticket expired".to_string(),
            ));
        }

        let binding = ExecutionBinding {
            focus_hash: session_focus.focus_hash,
            app_name: Some(session_focus.app_name),
            pid: Some(session_focus.pid),
        };
        let focus_validation = self
            .focus_probe
            .validate_execution_binding(&binding)
            .await
            .map_err(map_core_error)?;
        if !focus_validation.valid {
            return Err(GuiInteractionError::FocusDrift(
                focus_validation
                    .reason
                    .unwrap_or_else(|| "Focused window changed".to_string()),
            ));
        }

        {
            let mut sessions = self.sessions.write().await;
            let Some(stored) = sessions.get_mut(session_id) else {
                return Err(GuiInteractionError::NotFound(session_id.to_string()));
            };
            if stored.used_ticket_nonces.contains(&req.ticket.nonce) {
                return Err(GuiInteractionError::TicketInvalid(
                    "ticket nonce replay detected".to_string(),
                ));
            }
            stored.used_ticket_nonces.insert(req.ticket.nonce.clone());
            stored.session.state = GuiSessionState::Executing;
            stored.session.updated_at = Utc::now();
        }

        self.publish_event(
            session_id.to_string(),
            GuiSessionState::Executing,
            "gui_session.executing",
            Some(format!("ticket_id={}", req.ticket.ticket_id)),
        );

        Ok(GuiExecutionPlan {
            session_id: session_id.to_string(),
            command_id: format!("gui-action-{}", Utc::now().timestamp_millis().abs()),
            actions: confirmed_action.actions,
            ticket: req.ticket,
        })
    }

    pub async fn complete_execution(
        &self,
        session_id: &str,
        succeeded: bool,
        detail: Option<String>,
    ) -> Result<GuiExecutionOutcome, GuiInteractionError> {
        let (updated_session, overlay_handle_id) = {
            let mut sessions = self.sessions.write().await;
            let Some(stored) = sessions.get_mut(session_id) else {
                return Err(GuiInteractionError::NotFound(session_id.to_string()));
            };

            if succeeded {
                stored.session.state = GuiSessionState::Executed;
            } else {
                stored.session.state = GuiSessionState::Confirmed;
            }
            stored.session.updated_at = Utc::now();

            (
                stored.session.clone(),
                if succeeded {
                    stored.overlay_handle_id.take()
                } else {
                    None
                },
            )
        };

        if let Some(handle_id) = overlay_handle_id {
            let _ = self.overlay_driver.clear_highlights(&handle_id).await;
        }

        self.publish_event(
            session_id.to_string(),
            updated_session.state,
            if succeeded {
                "gui_session.executed"
            } else {
                "gui_session.execution_failed"
            },
            detail.clone(),
        );

        Ok(GuiExecutionOutcome {
            session: updated_session,
            succeeded,
            detail,
        })
    }

    pub async fn cancel_session(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<GuiInteractionSession, GuiInteractionError> {
        self.assert_capability_token(session_id, capability_token)
            .await?;

        let (session, overlay_handle_id) = {
            let mut sessions = self.sessions.write().await;
            let Some(stored) = sessions.get_mut(session_id) else {
                return Err(GuiInteractionError::NotFound(session_id.to_string()));
            };
            stored.session.state = GuiSessionState::Cancelled;
            stored.session.updated_at = Utc::now();
            (stored.session.clone(), stored.overlay_handle_id.take())
        };

        if let Some(handle_id) = overlay_handle_id {
            let _ = self.overlay_driver.clear_highlights(&handle_id).await;
        }

        self.publish_event(
            session_id.to_string(),
            GuiSessionState::Cancelled,
            "gui_session.cancelled",
            None,
        );

        Ok(session)
    }

    async fn assert_capability_token(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<(), GuiInteractionError> {
        let sessions = self.sessions.read().await;
        let Some(stored) = sessions.get(session_id) else {
            return Err(GuiInteractionError::NotFound(session_id.to_string()));
        };

        if stored.capability_token != capability_token.trim() {
            return Err(GuiInteractionError::Unauthorized);
        }

        Ok(())
    }

    fn publish_event(
        &self,
        session_id: String,
        state: GuiSessionState,
        event_type: &str,
        message: Option<String>,
    ) {
        let _ = self.event_tx.send(GuiSessionEvent {
            schema_version: oneshim_core::models::gui::GUI_SESSION_EVENT_SCHEMA_VERSION.to_string(),
            event_type: event_type.to_string(),
            session_id,
            state,
            emitted_at: Utc::now(),
            message,
        });
    }

    async fn expire_sessions(&self) {
        let expired_ids = {
            let mut sessions = self.sessions.write().await;
            let now = Utc::now();
            let expired_ids: Vec<String> = sessions
                .iter()
                .filter(|(_, stored)| stored.session.expires_at <= now)
                .map(|(session_id, _)| session_id.clone())
                .collect();

            for session_id in &expired_ids {
                sessions.remove(session_id);
            }

            expired_ids
        };

        for session_id in expired_ids {
            self.publish_event(
                session_id,
                GuiSessionState::Expired,
                "gui_session.expired",
                None,
            );
        }
    }

    fn require_hmac_secret(&self) -> Result<&[u8], GuiInteractionError> {
        self.hmac_secret.as_deref().ok_or_else(|| {
            GuiInteractionError::Unavailable(format!("{GUI_HMAC_SECRET_ENV} is missing or empty"))
        })
    }
}

fn build_candidates(
    scene: &UiScene,
    min_confidence: f64,
    max_candidates: usize,
) -> Vec<GuiCandidate> {
    let mut candidates: Vec<GuiCandidate> = scene
        .elements
        .iter()
        .filter(|element| element.confidence >= min_confidence)
        .map(|element| GuiCandidate {
            element: element.clone(),
            ranking_reason: Some(format!("confidence={:.2}", element.confidence)),
            eligible: true,
        })
        .collect();

    candidates.sort_by(|a, b| {
        b.element
            .confidence
            .partial_cmp(&a.element.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(max_candidates);
    candidates
}

fn build_actions_for_candidate(
    candidate: &GuiCandidate,
    action: &GuiActionRequest,
) -> Result<Vec<AutomationAction>, GuiInteractionError> {
    let center_x = candidate.element.bbox_abs.x + (candidate.element.bbox_abs.width as i32 / 2);
    let center_y = candidate.element.bbox_abs.y + (candidate.element.bbox_abs.height as i32 / 2);

    let actions = match action.action_type {
        GuiActionType::Click => vec![AutomationAction::MouseClick {
            button: "left".to_string(),
            x: center_x,
            y: center_y,
        }],
        GuiActionType::DoubleClick => vec![
            AutomationAction::MouseClick {
                button: "left".to_string(),
                x: center_x,
                y: center_y,
            },
            AutomationAction::MouseClick {
                button: "left".to_string(),
                x: center_x,
                y: center_y,
            },
        ],
        GuiActionType::RightClick => vec![AutomationAction::MouseClick {
            button: "right".to_string(),
            x: center_x,
            y: center_y,
        }],
        GuiActionType::TypeText => {
            let text = action
                .text
                .clone()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    GuiInteractionError::BadRequest(
                        "type_text action requires non-empty text".to_string(),
                    )
                })?;
            vec![
                AutomationAction::MouseClick {
                    button: "left".to_string(),
                    x: center_x,
                    y: center_y,
                },
                AutomationAction::KeyType { text },
            ]
        }
    };

    Ok(actions)
}

fn hash_actions(actions: &[AutomationAction]) -> Result<String, GuiInteractionError> {
    let payload = serde_json::to_vec(actions).map_err(|e| {
        GuiInteractionError::Internal(format!("action hash serialization failed: {e}"))
    })?;
    let digest = Sha256::digest(payload);
    Ok(encode_hex(digest.as_slice()))
}

fn sign_ticket(secret: &[u8], ticket: &GuiExecutionTicket) -> Result<String, GuiInteractionError> {
    let mut mac = HmacSha256::new_from_slice(secret)
        .map_err(|e| GuiInteractionError::Internal(format!("hmac key init failed: {e}")))?;
    mac.update(ticket_signature_payload(ticket).as_bytes());
    let signature = mac.finalize().into_bytes();
    Ok(encode_hex(signature.as_slice()))
}

fn verify_ticket(secret: &[u8], ticket: &GuiExecutionTicket) -> Result<(), GuiInteractionError> {
    let signature_bytes = decode_hex(&ticket.signature).ok_or_else(|| {
        GuiInteractionError::TicketInvalid("ticket signature format is invalid".to_string())
    })?;

    let mut mac = HmacSha256::new_from_slice(secret)
        .map_err(|e| GuiInteractionError::Internal(format!("hmac key init failed: {e}")))?;
    mac.update(ticket_signature_payload(ticket).as_bytes());

    mac.verify_slice(&signature_bytes)
        .map_err(|_| GuiInteractionError::TicketInvalid("ticket signature mismatch".to_string()))
}

fn ticket_signature_payload(ticket: &GuiExecutionTicket) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        ticket.session_id,
        ticket.scene_id,
        ticket.element_id,
        ticket.action_hash,
        ticket.focus_hash,
        ticket.issued_at.timestamp_millis(),
        ticket.expires_at.timestamp_millis(),
        ticket.nonce,
    )
}

fn new_capability_token() -> String {
    let random = Uuid::new_v4().to_string();
    let digest = Sha256::digest(random.as_bytes());
    encode_hex(digest.as_slice())
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn decode_hex(input: &str) -> Option<Vec<u8>> {
    if input.len() % 2 != 0 {
        return None;
    }

    (0..input.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&input[i..i + 2], 16).ok())
        .collect()
}

fn is_expired(expires_at: &DateTime<Utc>) -> bool {
    *expires_at <= Utc::now()
}

fn map_core_error(err: CoreError) -> GuiInteractionError {
    match err {
        CoreError::PolicyDenied(msg) | CoreError::PrivacyDenied(msg) => {
            GuiInteractionError::Forbidden(msg)
        }
        CoreError::ElementNotFound(msg) | CoreError::InvalidArguments(msg) => {
            GuiInteractionError::BadRequest(msg)
        }
        CoreError::ServiceUnavailable(msg)
        | CoreError::SandboxUnsupported(msg)
        | CoreError::SandboxInit(msg) => GuiInteractionError::Unavailable(msg),
        other => GuiInteractionError::Internal(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let data = b"hello";
        let encoded = encode_hex(data);
        let decoded = decode_hex(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_hex_rejects_invalid_length() {
        assert!(decode_hex("abc").is_none());
    }
}
