use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{extract::State, Json};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::error::ApiError;
use crate::update_control::{UpdateAction, UpdateStatus};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct UpdateActionRequest {
    pub action: UpdateAction,
}

#[derive(Debug, Serialize)]
pub struct UpdateActionResponse {
    pub accepted: bool,
    pub status: UpdateStatus,
}

pub async fn get_update_status(
    State(state): State<AppState>,
) -> Result<Json<UpdateStatus>, ApiError> {
    let Some(control) = state.update_control else {
        return Err(ApiError::NotFound("Update control is not enabled".to_string()));
    };

    let snapshot = control.state.read().await.clone();
    Ok(Json(snapshot))
}

pub async fn post_update_action(
    State(state): State<AppState>,
    Json(body): Json<UpdateActionRequest>,
) -> Result<Json<UpdateActionResponse>, ApiError> {
    let Some(control) = state.update_control else {
        return Err(ApiError::NotFound("Update control is not enabled".to_string()));
    };

    control
        .action_tx
        .send(body.action)
        .map_err(|e| ApiError::Internal(format!("Failed to send update action: {}", e)))?;

    let snapshot = control.state.read().await.clone();
    Ok(Json(UpdateActionResponse {
        accepted: true,
        status: snapshot,
    }))
}

pub async fn get_update_stream(
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let Some(control) = state.update_control else {
        return Err(ApiError::NotFound("Update control is not enabled".to_string()));
    };

    let rx = control.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(status) => {
            let json = serde_json::to_string(&status).ok()?;
            Some(Ok(Event::default().event("update_status").data(json)))
        }
        Err(_) => None,
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update_control::{UpdateControl, UpdatePhase};
    use crate::AppState;
    use oneshim_storage::sqlite::SqliteStorage;
    use std::sync::Arc;
    use tokio::sync::{broadcast, mpsc};

    async fn make_state_with_update_control() -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory sqlite"));
        let (event_tx, _) = broadcast::channel(16);
        let (action_tx, mut action_rx) = mpsc::unbounded_channel();
        tokio::spawn(async move { while action_rx.recv().await.is_some() {} });
        let control = UpdateControl::new(action_tx, UpdateStatus::default());

        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: None,
            audit_logger: None,
            automation_controller: None,
            update_control: Some(control),
        }
    }

    #[tokio::test]
    async fn get_update_status_returns_snapshot() {
        let state = make_state_with_update_control().await;
        let control = state
            .update_control
            .as_ref()
            .expect("update control should exist")
            .clone();

        {
            let mut guard = control.state.write().await;
            guard.phase = UpdatePhase::PendingApproval;
            guard.message = Some("pending".to_string());
        }

        let response = get_update_status(State(state))
            .await
            .expect("status endpoint should return payload")
            .0;

        assert_eq!(response.phase, UpdatePhase::PendingApproval);
        assert_eq!(response.message.as_deref(), Some("pending"));
    }

    #[tokio::test]
    async fn post_update_action_accepts_request() {
        let state = make_state_with_update_control().await;

        let response = post_update_action(
            State(state),
            Json(UpdateActionRequest {
                action: UpdateAction::CheckNow,
            }),
        )
        .await
        .expect("action endpoint should accept request")
        .0;

        assert!(response.accepted);
    }
}
