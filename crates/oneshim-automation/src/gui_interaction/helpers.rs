use chrono::{DateTime, Duration as ChronoDuration, Utc};

use oneshim_core::error::CoreError;
use oneshim_core::models::gui::{GuiActionType, GuiCandidate};
use oneshim_core::models::ui_scene::UiScene;

use super::types::GuiInteractionError;
use crate::controller::AutomationAction;

pub(super) fn build_candidates(
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

pub(super) fn build_actions_for_candidate(
    candidate: &GuiCandidate,
    action: &oneshim_core::models::gui::GuiActionRequest,
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

pub(super) fn is_expired(expires_at: &DateTime<Utc>) -> bool {
    *expires_at <= Utc::now()
}

pub(super) fn is_expired_past_grace(expires_at: &DateTime<Utc>, grace_secs: i64) -> bool {
    *expires_at + ChronoDuration::seconds(grace_secs) <= Utc::now()
}

pub(super) fn map_core_error(err: CoreError) -> GuiInteractionError {
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
