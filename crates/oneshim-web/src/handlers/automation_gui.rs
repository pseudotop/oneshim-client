use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures::stream::Stream;
use oneshim_api_contracts::automation_gui::{
    GuiConfirmRequest, GuiConfirmResponse, GuiCreateSessionRequest, GuiCreateSessionResponse,
    GuiExecuteResponse, GuiExecutionOutcome, GuiExecutionRequest, GuiHighlightRequest,
    GuiSessionPath, GuiSessionResponse,
};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use oneshim_automation::controller::GuiExecutionResult;
use oneshim_automation::gui_interaction::{
    GuiConfirmRequest as AutomationGuiConfirmRequest,
    GuiCreateSessionRequest as AutomationGuiCreateSessionRequest,
    GuiExecutionRequest as AutomationGuiExecutionRequest,
    GuiHighlightRequest as AutomationGuiHighlightRequest, GuiInteractionError,
};

use crate::error::ApiError;
use crate::AppState;

const GUI_SESSION_HEADER: &str = "x-gui-session-token";
const GUI_SCHEMA_VERSION: &str = "automation.gui.v2";

pub async fn create_gui_session(
    State(state): State<AppState>,
    Json(req): Json<GuiCreateSessionRequest>,
) -> Result<Json<GuiCreateSessionResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let req = AutomationGuiCreateSessionRequest {
        app_name: req.app_name,
        screen_id: req.screen_id,
        min_confidence: req.min_confidence,
        max_candidates: req.max_candidates,
        session_ttl_secs: req.session_ttl_secs,
    };
    let created = controller
        .gui_create_session(req)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiCreateSessionResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        session: created.session,
        capability_token: created.capability_token,
    }))
}

pub async fn get_gui_session(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
) -> Result<Json<GuiSessionResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;

    let session = controller
        .gui_get_session(&path.id, &capability_token)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiSessionResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        session,
    }))
}

pub async fn highlight_gui_session(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
    Json(req): Json<GuiHighlightRequest>,
) -> Result<Json<GuiSessionResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;
    let req = AutomationGuiHighlightRequest {
        candidate_ids: req.candidate_ids,
    };

    let session = controller
        .gui_highlight_session(&path.id, &capability_token, req)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiSessionResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        session,
    }))
}

pub async fn confirm_gui_session(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
    Json(req): Json<GuiConfirmRequest>,
) -> Result<Json<GuiConfirmResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;
    let req = AutomationGuiConfirmRequest {
        candidate_id: req.candidate_id,
        action: req.action,
        ticket_ttl_secs: req.ticket_ttl_secs,
    };

    let ticket = controller
        .gui_confirm_candidate(&path.id, &capability_token, req)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiConfirmResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        ticket,
    }))
}

pub async fn execute_gui_session(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
    Json(req): Json<GuiExecutionRequest>,
) -> Result<Json<GuiExecuteResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;
    let req = AutomationGuiExecutionRequest { ticket: req.ticket };

    let result: GuiExecutionResult = controller
        .gui_execute(&path.id, &capability_token, req)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiExecuteResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        command_id: result.command_id,
        ticket: result.ticket,
        result: result.result,
        outcome: GuiExecutionOutcome {
            session: result.outcome.session,
            succeeded: result.outcome.succeeded,
            detail: result.outcome.detail,
        },
    }))
}

pub async fn delete_gui_session(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
) -> Result<Json<GuiSessionResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;

    let session = controller
        .gui_cancel_session(&path.id, &capability_token)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiSessionResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        session,
    }))
}

pub async fn gui_session_event_stream(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;
    let rx = controller
        .gui_subscribe_events(&path.id, &capability_token)
        .await
        .map_err(map_gui_error)?;

    let stream = BroadcastStream::new(rx);
    let session_id = path.id;
    let sse_stream = stream.filter_map(move |result| match result {
        Ok(event) if event.session_id == session_id => {
            let data = serde_json::to_string(&event).ok()?;
            Some(Ok(Event::default()
                .event(event.event_type.clone())
                .data(data)))
        }
        _ => None,
    });

    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

fn require_controller(
    state: &AppState,
) -> Result<&oneshim_automation::controller::AutomationController, ApiError> {
    state.automation_controller.as_deref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Automation controller is disabled".to_string())
    })
}

fn read_capability_token(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get(GUI_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            ApiError::Unauthorized(format!(
                "Missing required header '{}': session capability token",
                GUI_SESSION_HEADER
            ))
        })
}

fn map_gui_error(err: GuiInteractionError) -> ApiError {
    match err {
        GuiInteractionError::Unauthorized => {
            ApiError::Unauthorized("Invalid GUI session token".to_string())
        }
        GuiInteractionError::NotFound(msg) => ApiError::NotFound(msg),
        GuiInteractionError::BadRequest(msg) => ApiError::BadRequest(msg),
        GuiInteractionError::Forbidden(msg) => ApiError::Forbidden(msg),
        GuiInteractionError::FocusDrift(msg) => ApiError::Conflict(msg),
        GuiInteractionError::TicketInvalid(msg) => ApiError::Unprocessable(msg),
        GuiInteractionError::Unavailable(msg) => ApiError::ServiceUnavailable(msg),
        GuiInteractionError::Internal(msg) => ApiError::Internal(msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_automation::gui_interaction::GuiInteractionError;

    // ── read_capability_token tests ─────────────────────────────────────

    #[test]
    fn token_header_is_enforced() {
        let headers = HeaderMap::new();
        let err = read_capability_token(&headers).unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn token_header_rejects_empty_value() {
        let mut headers = HeaderMap::new();
        headers.insert(GUI_SESSION_HEADER, "".parse().unwrap());
        let err = read_capability_token(&headers).unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn token_header_rejects_whitespace_only() {
        let mut headers = HeaderMap::new();
        headers.insert(GUI_SESSION_HEADER, "   ".parse().unwrap());
        let err = read_capability_token(&headers).unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn token_header_accepts_valid_token() {
        let mut headers = HeaderMap::new();
        headers.insert(GUI_SESSION_HEADER, "abc123".parse().unwrap());
        let token = read_capability_token(&headers).unwrap();
        assert_eq!(token, "abc123");
    }

    #[test]
    fn token_header_trims_whitespace() {
        let mut headers = HeaderMap::new();
        headers.insert(GUI_SESSION_HEADER, " tok123 ".parse().unwrap());
        let token = read_capability_token(&headers).unwrap();
        assert_eq!(token, "tok123");
    }

    // ── map_gui_error tests ─────────────────────────────────────────────

    #[test]
    fn maps_unauthorized_to_401() {
        let err = map_gui_error(GuiInteractionError::Unauthorized);
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn maps_not_found_to_404() {
        let err = map_gui_error(GuiInteractionError::NotFound("s1".to_string()));
        assert!(matches!(err, ApiError::NotFound(_)));
    }

    #[test]
    fn maps_bad_request_to_400() {
        let err = map_gui_error(GuiInteractionError::BadRequest("bad".to_string()));
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn maps_forbidden_to_403() {
        let err = map_gui_error(GuiInteractionError::Forbidden("denied".to_string()));
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn maps_focus_drift_to_409_conflict() {
        let err = map_gui_error(GuiInteractionError::FocusDrift("drift".to_string()));
        assert!(matches!(err, ApiError::Conflict(_)));
    }

    #[test]
    fn maps_ticket_invalid_to_422() {
        let err = map_gui_error(GuiInteractionError::TicketInvalid("expired".to_string()));
        assert!(matches!(err, ApiError::Unprocessable(_)));
    }

    #[test]
    fn maps_unavailable_to_503() {
        let err = map_gui_error(GuiInteractionError::Unavailable("down".to_string()));
        assert!(matches!(err, ApiError::ServiceUnavailable(_)));
    }

    #[test]
    fn maps_internal_to_500() {
        let err = map_gui_error(GuiInteractionError::Internal("crash".to_string()));
        assert!(matches!(err, ApiError::Internal(_)));
    }

    // ── Schema version constant ─────────────────────────────────────────

    #[test]
    fn gui_schema_version_matches_core() {
        assert_eq!(
            GUI_SCHEMA_VERSION,
            oneshim_core::models::gui::GUI_INTERACTION_SCHEMA_VERSION
        );
    }
}
