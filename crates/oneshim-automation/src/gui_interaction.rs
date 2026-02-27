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
    use async_trait::async_trait;
    use oneshim_core::models::gui::{
        FocusSnapshot, FocusValidation, GuiActionType, HighlightHandle,
    };
    use oneshim_core::models::intent::{ElementBounds, UiElement};
    use oneshim_core::models::ui_scene::{NormalizedBounds, UiScene, UiSceneElement};
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;

    // ── Test constants ──────────────────────────────────────────────────

    const TEST_HMAC_SECRET: &str = "test-hmac-secret-32-bytes-long!!";

    // ── MockElementFinder ───────────────────────────────────────────────

    struct MockElementFinder {
        scene: Mutex<UiScene>,
    }

    impl MockElementFinder {
        fn new(scene: UiScene) -> Self {
            Self {
                scene: Mutex::new(scene),
            }
        }
    }

    #[async_trait]
    impl ElementFinder for MockElementFinder {
        async fn find_element(
            &self,
            _text: Option<&str>,
            _role: Option<&str>,
            _region: Option<&ElementBounds>,
        ) -> Result<Vec<UiElement>, CoreError> {
            Ok(vec![])
        }

        async fn analyze_scene(
            &self,
            _app_name: Option<&str>,
            _screen_id: Option<&str>,
        ) -> Result<UiScene, CoreError> {
            Ok(self.scene.lock().unwrap().clone())
        }

        fn name(&self) -> &str {
            "mock"
        }
    }

    // ── MockFocusProbe ──────────────────────────────────────────────────

    struct MockFocusProbe {
        focus: Mutex<FocusSnapshot>,
        validation_valid: Mutex<bool>,
    }

    impl MockFocusProbe {
        fn new(focus: FocusSnapshot) -> Self {
            Self {
                focus: Mutex::new(focus),
                validation_valid: Mutex::new(true),
            }
        }

        fn set_validation_valid(&self, valid: bool) {
            *self.validation_valid.lock().unwrap() = valid;
        }
    }

    #[async_trait]
    impl FocusProbe for MockFocusProbe {
        async fn current_focus(&self) -> Result<FocusSnapshot, CoreError> {
            Ok(self.focus.lock().unwrap().clone())
        }

        async fn validate_execution_binding(
            &self,
            _binding: &ExecutionBinding,
        ) -> Result<FocusValidation, CoreError> {
            let valid = *self.validation_valid.lock().unwrap();
            Ok(FocusValidation {
                valid,
                reason: if valid {
                    None
                } else {
                    Some("Focus changed".to_string())
                },
                current_focus: None,
            })
        }
    }

    // ── MockOverlayDriver ───────────────────────────────────────────────

    struct MockOverlayDriver {
        show_count: AtomicUsize,
        clear_count: AtomicUsize,
    }

    impl MockOverlayDriver {
        fn new() -> Self {
            Self {
                show_count: AtomicUsize::new(0),
                clear_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl OverlayDriver for MockOverlayDriver {
        async fn show_highlights(
            &self,
            req: HighlightRequest,
        ) -> Result<HighlightHandle, CoreError> {
            self.show_count.fetch_add(1, Ordering::SeqCst);
            Ok(HighlightHandle {
                handle_id: format!("handle-{}", self.show_count.load(Ordering::SeqCst)),
                rendered_at: Utc::now(),
                target_count: req.targets.len(),
            })
        }

        async fn clear_highlights(&self, _handle_id: &str) -> Result<(), CoreError> {
            self.clear_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    // ── Fixture builders ────────────────────────────────────────────────

    fn make_element(id: &str, label: &str, confidence: f64) -> UiSceneElement {
        UiSceneElement {
            element_id: id.to_string(),
            bbox_abs: ElementBounds {
                x: 100,
                y: 80,
                width: 200,
                height: 40,
            },
            bbox_norm: NormalizedBounds::new(0.05, 0.07, 0.10, 0.04),
            label: label.to_string(),
            role: Some("button".to_string()),
            intent: None,
            state: Some("enabled".to_string()),
            confidence,
            text_masked: Some(label.to_string()),
            parent_id: None,
        }
    }

    fn make_scene(elements: Vec<UiSceneElement>) -> UiScene {
        UiScene {
            schema_version: "ui_scene.v1".to_string(),
            scene_id: "test-scene-1".to_string(),
            app_name: Some("TestApp".to_string()),
            screen_id: Some("screen-main".to_string()),
            captured_at: Utc::now(),
            screen_width: 1920,
            screen_height: 1080,
            elements,
        }
    }

    fn make_focus() -> FocusSnapshot {
        FocusSnapshot {
            app_name: "TestApp".to_string(),
            window_title: "Test Window".to_string(),
            pid: 1234,
            bounds: None,
            captured_at: Utc::now(),
            focus_hash: "abc123hash".to_string(),
        }
    }

    fn make_service(
        scene: UiScene,
        focus: FocusSnapshot,
    ) -> (Arc<GuiInteractionService>, Arc<MockFocusProbe>) {
        let probe = Arc::new(MockFocusProbe::new(focus));
        let service = Arc::new(GuiInteractionService::new(
            Arc::new(MockElementFinder::new(scene)),
            probe.clone(),
            Arc::new(MockOverlayDriver::new()),
            Some(TEST_HMAC_SECRET.to_string()),
        ));
        (service, probe)
    }

    fn default_create_request() -> GuiCreateSessionRequest {
        GuiCreateSessionRequest {
            app_name: Some("TestApp".to_string()),
            screen_id: None,
            min_confidence: None,
            max_candidates: None,
            session_ttl_secs: Some(300),
        }
    }

    /// Helper: create a session and return (session_id, capability_token)
    async fn create_test_session(service: &GuiInteractionService) -> (String, String) {
        let resp = service
            .create_session(default_create_request())
            .await
            .expect("create_session should succeed");
        (resp.session.session_id, resp.capability_token)
    }

    /// Helper: create session + highlight it, returns (session_id, token)
    async fn create_and_highlight(service: &GuiInteractionService) -> (String, String) {
        let (sid, token) = create_test_session(service).await;
        service
            .highlight_session(
                &sid,
                &token,
                GuiHighlightRequest {
                    candidate_ids: None,
                },
            )
            .await
            .expect("highlight should succeed");
        (sid, token)
    }

    /// Helper: create session + highlight + confirm, returns (session_id, token, ticket)
    async fn create_highlight_and_confirm(
        service: &GuiInteractionService,
    ) -> (String, String, GuiExecutionTicket) {
        let (sid, token) = create_and_highlight(service).await;

        let session = service.get_session(&sid, &token).await.unwrap();
        let candidate_id = session.candidates[0].element.element_id.clone();

        let ticket = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id,
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(60),
                },
            )
            .await
            .expect("confirm should succeed");
        (sid, token, ticket)
    }

    // ── Utility tests ───────────────────────────────────────────────────

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

    // ── Session creation tests ──────────────────────────────────────────

    #[tokio::test]
    async fn create_session_returns_proposed_state() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let resp = service
            .create_session(default_create_request())
            .await
            .unwrap();

        assert_eq!(resp.session.state, GuiSessionState::Proposed);
        assert!(!resp.capability_token.is_empty());
        assert!(!resp.session.session_id.is_empty());
        assert_eq!(resp.session.candidates.len(), 1);
        assert_eq!(resp.session.candidates[0].element.label, "Save");
    }

    #[tokio::test]
    async fn create_session_filters_low_confidence_candidates() {
        let scene = make_scene(vec![
            make_element("el-high", "Save", 0.9),
            make_element("el-low", "Cancel", 0.2),
        ]);
        let (service, _) = make_service(scene, make_focus());

        let resp = service
            .create_session(default_create_request())
            .await
            .unwrap();

        assert_eq!(resp.session.candidates.len(), 1);
        assert_eq!(resp.session.candidates[0].element.element_id, "el-high");
    }

    #[tokio::test]
    async fn create_session_respects_max_candidates() {
        let elements: Vec<UiSceneElement> = (0..10)
            .map(|i| make_element(&format!("el-{i}"), &format!("Btn{i}"), 0.8))
            .collect();
        let scene = make_scene(elements);
        let (service, _) = make_service(scene, make_focus());

        let mut req = default_create_request();
        req.max_candidates = Some(3);

        let resp = service.create_session(req).await.unwrap();
        assert_eq!(resp.session.candidates.len(), 3);
    }

    #[tokio::test]
    async fn create_session_rejects_empty_scene() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.1)]);
        let (service, _) = make_service(scene, make_focus());

        let mut req = default_create_request();
        req.min_confidence = Some(0.99);

        let err = service.create_session(req).await.unwrap_err();
        assert!(matches!(err, GuiInteractionError::BadRequest(_)));
    }

    #[tokio::test]
    async fn create_session_requires_hmac_secret() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let service = GuiInteractionService::new(
            Arc::new(MockElementFinder::new(scene)),
            Arc::new(MockFocusProbe::new(make_focus())),
            Arc::new(MockOverlayDriver::new()),
            None, // no HMAC secret
        );

        let err = service
            .create_session(default_create_request())
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::Unavailable(_)));
    }

    // ── Get session tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn get_session_returns_current_state() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_test_session(&service).await;
        let session = service.get_session(&sid, &token).await.unwrap();

        assert_eq!(session.state, GuiSessionState::Proposed);
        assert_eq!(session.session_id, sid);
    }

    #[tokio::test]
    async fn get_session_rejects_invalid_token() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, _) = create_test_session(&service).await;
        let err = service.get_session(&sid, "wrong-token").await.unwrap_err();
        assert!(matches!(err, GuiInteractionError::Unauthorized));
    }

    #[tokio::test]
    async fn get_session_rejects_unknown_session() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let err = service
            .get_session("nonexistent", "some-token")
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::NotFound(_)));
    }

    // ── Highlight tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn highlight_transitions_to_highlighted() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_test_session(&service).await;
        let session = service
            .highlight_session(
                &sid,
                &token,
                GuiHighlightRequest {
                    candidate_ids: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(session.state, GuiSessionState::Highlighted);
    }

    #[tokio::test]
    async fn highlight_filters_by_candidate_ids() {
        let scene = make_scene(vec![
            make_element("el-1", "Save", 0.9),
            make_element("el-2", "Cancel", 0.8),
        ]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_test_session(&service).await;

        // highlight only el-1
        let session = service
            .highlight_session(
                &sid,
                &token,
                GuiHighlightRequest {
                    candidate_ids: Some(vec!["el-1".to_string()]),
                },
            )
            .await
            .unwrap();

        assert_eq!(session.state, GuiSessionState::Highlighted);
    }

    #[tokio::test]
    async fn highlight_rejects_invalid_token() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, _) = create_test_session(&service).await;
        let err = service
            .highlight_session(
                &sid,
                "wrong-token",
                GuiHighlightRequest {
                    candidate_ids: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::Unauthorized));
    }

    // ── Confirm tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn confirm_transitions_to_confirmed_with_ticket() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_and_highlight(&service).await;

        let ticket = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "el-1".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: None,
                },
            )
            .await
            .unwrap();

        assert!(!ticket.signature.is_empty());
        assert_eq!(ticket.session_id, sid);
        assert_eq!(ticket.element_id, "el-1");

        let session = service.get_session(&sid, &token).await.unwrap();
        assert_eq!(session.state, GuiSessionState::Confirmed);
        assert_eq!(session.selected_element_id, Some("el-1".to_string()));
    }

    #[tokio::test]
    async fn confirm_rejects_unknown_candidate() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_and_highlight(&service).await;

        let err = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "nonexistent".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::BadRequest(_)));
    }

    #[tokio::test]
    async fn confirm_rejects_when_focus_changed() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, probe) = make_service(scene, make_focus());

        let (sid, token) = create_and_highlight(&service).await;

        // Simulate focus drift
        probe.set_validation_valid(false);

        let err = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "el-1".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::FocusDrift(_)));
    }

    #[tokio::test]
    async fn confirm_type_text_requires_text() {
        let scene = make_scene(vec![make_element("el-1", "Input", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_and_highlight(&service).await;

        let err = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "el-1".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::TypeText,
                        text: None,
                    },
                    ticket_ttl_secs: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::BadRequest(_)));
    }

    // ── Execution tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn prepare_execution_transitions_to_executing() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;

        let plan = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();

        assert_eq!(plan.session_id, sid);
        assert!(!plan.actions.is_empty());

        let session = service.get_session(&sid, &token).await.unwrap();
        assert_eq!(session.state, GuiSessionState::Executing);
    }

    #[tokio::test]
    async fn prepare_execution_rejects_nonce_replay() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;

        // First execution succeeds
        let _ = service
            .prepare_execution(
                &sid,
                &token,
                GuiExecutionRequest {
                    ticket: ticket.clone(),
                },
            )
            .await
            .unwrap();

        // Complete execution to go back to Confirmed for re-test
        service.complete_execution(&sid, false, None).await.unwrap();

        // Replay same ticket nonce — should be rejected
        let err = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::TicketInvalid(_)));
    }

    #[tokio::test]
    async fn prepare_execution_rejects_tampered_signature() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, mut ticket) = create_highlight_and_confirm(&service).await;

        ticket.signature = "00".repeat(32); // tampered

        let err = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::TicketInvalid(_)));
    }

    #[tokio::test]
    async fn prepare_execution_rejects_wrong_session_state() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        // Session is only Proposed (not Confirmed), so prepare_execution should fail
        let (sid, token) = create_test_session(&service).await;
        let dummy_ticket = GuiExecutionTicket {
            schema_version: "automation.gui.ticket.v1".to_string(),
            ticket_id: "t1".to_string(),
            session_id: sid.clone(),
            scene_id: "s1".to_string(),
            element_id: "el-1".to_string(),
            action_hash: "hash".to_string(),
            focus_hash: "focus".to_string(),
            issued_at: Utc::now(),
            expires_at: Utc::now() + ChronoDuration::seconds(60),
            nonce: "nonce1".to_string(),
            signature: "sig".to_string(),
        };

        let err = service
            .prepare_execution(
                &sid,
                &token,
                GuiExecutionRequest {
                    ticket: dummy_ticket,
                },
            )
            .await
            .unwrap_err();
        // Should fail because there is no confirmed_action
        assert!(matches!(err, GuiInteractionError::TicketInvalid(_)));
    }

    #[tokio::test]
    async fn prepare_execution_rejects_focus_drift() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, probe) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;

        // Focus drifts after confirm
        probe.set_validation_valid(false);

        let err = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::FocusDrift(_)));
    }

    // ── Complete execution tests ────────────────────────────────────────

    #[tokio::test]
    async fn complete_execution_success_transitions_to_executed() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;
        service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();

        let outcome = service.complete_execution(&sid, true, None).await.unwrap();

        assert!(outcome.succeeded);
        assert_eq!(outcome.session.state, GuiSessionState::Executed);
    }

    #[tokio::test]
    async fn complete_execution_failure_reverts_to_confirmed() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;
        service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();

        let outcome = service
            .complete_execution(&sid, false, Some("click missed".to_string()))
            .await
            .unwrap();

        assert!(!outcome.succeeded);
        assert_eq!(outcome.session.state, GuiSessionState::Confirmed);
        assert_eq!(outcome.detail, Some("click missed".to_string()));
    }

    // ── Cancel tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn cancel_transitions_to_cancelled() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_test_session(&service).await;
        let session = service.cancel_session(&sid, &token).await.unwrap();

        assert_eq!(session.state, GuiSessionState::Cancelled);
    }

    #[tokio::test]
    async fn cancel_rejects_invalid_token() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, _) = create_test_session(&service).await;
        let err = service
            .cancel_session(&sid, "wrong-token")
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::Unauthorized));
    }

    // ── Expiry tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn expired_session_is_detected_on_get() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let mut req = default_create_request();
        req.session_ttl_secs = Some(30); // minimum allowed

        let resp = service.create_session(req).await.unwrap();
        let sid = resp.session.session_id.clone();
        let token = resp.capability_token.clone();

        // Manually expire the session by setting expires_at in the past
        {
            let mut sessions = service.sessions.write().await;
            if let Some(stored) = sessions.get_mut(&sid) {
                stored.session.expires_at = Utc::now() - ChronoDuration::seconds(1);
            }
        }

        let session = service.get_session(&sid, &token).await.unwrap();
        assert_eq!(session.state, GuiSessionState::Expired);
    }

    #[tokio::test]
    async fn highlight_rejects_expired_session() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_test_session(&service).await;

        // Expire it
        {
            let mut sessions = service.sessions.write().await;
            if let Some(stored) = sessions.get_mut(&sid) {
                stored.session.expires_at = Utc::now() - ChronoDuration::seconds(1);
            }
        }

        let err = service
            .highlight_session(
                &sid,
                &token,
                GuiHighlightRequest {
                    candidate_ids: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::TicketInvalid(_)));
    }

    // ── Full flow integration test ──────────────────────────────────────

    #[tokio::test]
    async fn full_propose_highlight_confirm_execute_flow() {
        let scene = make_scene(vec![
            make_element("el-1", "Save", 0.95),
            make_element("el-2", "Cancel", 0.85),
        ]);
        let (service, _) = make_service(scene, make_focus());

        // 1. Propose
        let resp = service
            .create_session(default_create_request())
            .await
            .unwrap();
        assert_eq!(resp.session.state, GuiSessionState::Proposed);
        let sid = resp.session.session_id.clone();
        let token = resp.capability_token.clone();

        // 2. Highlight
        let session = service
            .highlight_session(
                &sid,
                &token,
                GuiHighlightRequest {
                    candidate_ids: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(session.state, GuiSessionState::Highlighted);

        // 3. Confirm
        let ticket = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "el-1".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(60),
                },
            )
            .await
            .unwrap();
        assert!(!ticket.signature.is_empty());

        let session = service.get_session(&sid, &token).await.unwrap();
        assert_eq!(session.state, GuiSessionState::Confirmed);

        // 4. Execute
        let plan = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();
        assert!(!plan.actions.is_empty());

        let session = service.get_session(&sid, &token).await.unwrap();
        assert_eq!(session.state, GuiSessionState::Executing);

        // 5. Complete
        let outcome = service.complete_execution(&sid, true, None).await.unwrap();
        assert!(outcome.succeeded);
        assert_eq!(outcome.session.state, GuiSessionState::Executed);
    }

    // ── Event subscription test ─────────────────────────────────────────

    #[tokio::test]
    async fn subscribe_receives_session_events() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let mut rx = service.subscribe();

        let _ = service
            .create_session(default_create_request())
            .await
            .unwrap();

        let event = rx.try_recv().unwrap();
        assert_eq!(event.event_type, "gui_session.proposed");
        assert_eq!(event.state, GuiSessionState::Proposed);
    }

    // ── Build candidates tests ──────────────────────────────────────────

    #[test]
    fn build_candidates_sorts_by_confidence_descending() {
        let scene = make_scene(vec![
            make_element("el-low", "A", 0.6),
            make_element("el-high", "B", 0.95),
            make_element("el-mid", "C", 0.8),
        ]);

        let candidates = build_candidates(&scene, 0.5, 10);

        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].element.element_id, "el-high");
        assert_eq!(candidates[1].element.element_id, "el-mid");
        assert_eq!(candidates[2].element.element_id, "el-low");
    }

    #[test]
    fn build_candidates_filters_below_min_confidence() {
        let scene = make_scene(vec![
            make_element("el-1", "A", 0.9),
            make_element("el-2", "B", 0.3),
        ]);

        let candidates = build_candidates(&scene, 0.5, 10);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].element.element_id, "el-1");
    }

    #[test]
    fn build_candidates_truncates_to_max() {
        let elements: Vec<UiSceneElement> = (0..10)
            .map(|i| make_element(&format!("el-{i}"), &format!("Btn{i}"), 0.8))
            .collect();
        let scene = make_scene(elements);

        let candidates = build_candidates(&scene, 0.5, 3);
        assert_eq!(candidates.len(), 3);
    }

    // ── HMAC ticket signing tests ───────────────────────────────────────

    #[test]
    fn sign_and_verify_ticket_roundtrip() {
        let secret = TEST_HMAC_SECRET.as_bytes();
        let ticket = GuiExecutionTicket {
            schema_version: "automation.gui.ticket.v1".to_string(),
            ticket_id: "t-1".to_string(),
            session_id: "s-1".to_string(),
            scene_id: "sc-1".to_string(),
            element_id: "el-1".to_string(),
            action_hash: "ahash".to_string(),
            focus_hash: "fhash".to_string(),
            issued_at: Utc::now(),
            expires_at: Utc::now() + ChronoDuration::seconds(30),
            nonce: "nonce-1".to_string(),
            signature: String::new(),
        };

        let sig = sign_ticket(secret, &ticket).unwrap();
        let signed = GuiExecutionTicket {
            signature: sig,
            ..ticket
        };

        assert!(verify_ticket(secret, &signed).is_ok());
    }

    #[test]
    fn verify_ticket_rejects_tampered_nonce() {
        let secret = TEST_HMAC_SECRET.as_bytes();
        let ticket = GuiExecutionTicket {
            schema_version: "automation.gui.ticket.v1".to_string(),
            ticket_id: "t-1".to_string(),
            session_id: "s-1".to_string(),
            scene_id: "sc-1".to_string(),
            element_id: "el-1".to_string(),
            action_hash: "ahash".to_string(),
            focus_hash: "fhash".to_string(),
            issued_at: Utc::now(),
            expires_at: Utc::now() + ChronoDuration::seconds(30),
            nonce: "nonce-1".to_string(),
            signature: String::new(),
        };

        let sig = sign_ticket(secret, &ticket).unwrap();
        let tampered = GuiExecutionTicket {
            signature: sig,
            nonce: "tampered-nonce".to_string(),
            ..ticket
        };

        assert!(verify_ticket(secret, &tampered).is_err());
    }

    // ── Action builder tests ────────────────────────────────────────────

    #[test]
    fn build_actions_click_generates_mouse_click() {
        let candidate = GuiCandidate {
            element: make_element("el-1", "Save", 0.9),
            ranking_reason: None,
            eligible: true,
        };
        let action = GuiActionRequest {
            action_type: GuiActionType::Click,
            text: None,
        };

        let actions = build_actions_for_candidate(&candidate, &action).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], AutomationAction::MouseClick { .. }));
    }

    #[test]
    fn build_actions_double_click_generates_two_clicks() {
        let candidate = GuiCandidate {
            element: make_element("el-1", "File", 0.9),
            ranking_reason: None,
            eligible: true,
        };
        let action = GuiActionRequest {
            action_type: GuiActionType::DoubleClick,
            text: None,
        };

        let actions = build_actions_for_candidate(&candidate, &action).unwrap();
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn build_actions_type_text_generates_click_then_type() {
        let candidate = GuiCandidate {
            element: make_element("el-1", "Input", 0.9),
            ranking_reason: None,
            eligible: true,
        };
        let action = GuiActionRequest {
            action_type: GuiActionType::TypeText,
            text: Some("hello".to_string()),
        };

        let actions = build_actions_for_candidate(&candidate, &action).unwrap();
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], AutomationAction::MouseClick { .. }));
        assert!(matches!(actions[1], AutomationAction::KeyType { .. }));
    }

    #[test]
    fn build_actions_type_text_rejects_empty_text() {
        let candidate = GuiCandidate {
            element: make_element("el-1", "Input", 0.9),
            ranking_reason: None,
            eligible: true,
        };
        let action = GuiActionRequest {
            action_type: GuiActionType::TypeText,
            text: Some("  ".to_string()),
        };

        assert!(build_actions_for_candidate(&candidate, &action).is_err());
    }
}
