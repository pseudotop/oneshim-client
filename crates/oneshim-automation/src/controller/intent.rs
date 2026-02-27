use std::time::Instant;

use tokio::sync::broadcast;

use crate::gui_interaction::{
    GuiConfirmRequest, GuiCreateSessionRequest, GuiCreateSessionResponse, GuiExecutionRequest,
    GuiHighlightRequest, GuiInteractionError,
};
use crate::policy::AuditLevel;
use oneshim_core::error::CoreError;
use oneshim_core::models::gui::{GuiExecutionTicket, GuiInteractionSession, GuiSessionEvent};
use oneshim_core::models::intent::{AutomationIntent, IntentCommand, IntentResult};
use oneshim_core::models::ui_scene::UiScene;

use super::types::{GuiExecutionResult, PlannedIntentResult};
use super::{AutomationController, GUI_ACTION_TIMEOUT_SECS, GUI_EXECUTE_TIMEOUT_SECS};

impl AutomationController {
    pub async fn execute_intent(&self, cmd: &IntentCommand) -> Result<IntentResult, CoreError> {
        self.ensure_enabled()?;
        let executor = self.require_intent_executor()?;

        {
            let mut logger = self.audit_logger.write().await;
            logger.log_start_if(
                AuditLevel::Basic,
                &cmd.command_id,
                &cmd.session_id,
                &format!("{:?}", cmd.intent),
            );
        }

        let start = Instant::now();
        let result = executor.execute(&cmd.intent).await?;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        {
            let mut logger = self.audit_logger.write().await;
            logger.log_complete_with_time(
                AuditLevel::Basic,
                &cmd.command_id,
                &cmd.session_id,
                &format!("success={}, elapsed={}ms", result.success, elapsed_ms),
                elapsed_ms,
            );
        }

        Ok(result)
    }

    pub async fn execute_intent_hint(
        &self,
        command_id: &str,
        session_id: &str,
        intent_hint: &str,
    ) -> Result<PlannedIntentResult, CoreError> {
        self.ensure_enabled()?;
        let executor = self.require_intent_executor()?;
        let planner = self.require_intent_planner()?;

        {
            let mut logger = self.audit_logger.write().await;
            logger.log_start_if(
                AuditLevel::Basic,
                command_id,
                session_id,
                &format!("intent_hint={intent_hint}"),
            );
        }

        let start = Instant::now();
        let planned_intent = planner.plan(intent_hint).await?;
        let result = executor.execute(&planned_intent).await?;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        {
            let mut logger = self.audit_logger.write().await;
            logger.log_complete_with_time(
                AuditLevel::Basic,
                command_id,
                session_id,
                &format!(
                    "planned_intent={:?}, success={}, elapsed={}ms",
                    planned_intent, result.success, elapsed_ms
                ),
                elapsed_ms,
            );
        }

        Ok(PlannedIntentResult {
            planned_intent,
            result,
        })
    }

    pub async fn analyze_scene(
        &self,
        app_name: Option<&str>,
        screen_id: Option<&str>,
    ) -> Result<UiScene, CoreError> {
        self.ensure_enabled()?;
        let finder = self.require_scene_finder()?;
        finder.analyze_scene(app_name, screen_id).await
    }

    pub async fn analyze_scene_from_image(
        &self,
        image_data: Vec<u8>,
        image_format: String,
        app_name: Option<&str>,
        screen_id: Option<&str>,
    ) -> Result<UiScene, CoreError> {
        self.ensure_enabled()?;
        let finder = self.require_scene_finder()?;
        finder
            .analyze_scene_from_image(image_data, image_format, app_name, screen_id)
            .await
    }

    pub async fn gui_create_session(
        &self,
        req: GuiCreateSessionRequest,
    ) -> Result<GuiCreateSessionResponse, GuiInteractionError> {
        self.ensure_enabled()
            .map_err(|e| GuiInteractionError::Unavailable(e.to_string()))?;
        let service = self.require_gui_service()?;
        service.create_session(req).await
    }

    pub async fn gui_get_session(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<GuiInteractionSession, GuiInteractionError> {
        self.ensure_enabled()
            .map_err(|e| GuiInteractionError::Unavailable(e.to_string()))?;
        let service = self.require_gui_service()?;
        service.get_session(session_id, capability_token).await
    }

    pub async fn gui_highlight_session(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiHighlightRequest,
    ) -> Result<GuiInteractionSession, GuiInteractionError> {
        self.ensure_enabled()
            .map_err(|e| GuiInteractionError::Unavailable(e.to_string()))?;
        let service = self.require_gui_service()?;
        service
            .highlight_session(session_id, capability_token, req)
            .await
    }

    pub async fn gui_confirm_candidate(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiConfirmRequest,
    ) -> Result<GuiExecutionTicket, GuiInteractionError> {
        self.ensure_enabled()
            .map_err(|e| GuiInteractionError::Unavailable(e.to_string()))?;
        let service = self.require_gui_service()?;
        service
            .confirm_candidate(session_id, capability_token, req)
            .await
    }

    pub async fn gui_execute(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiExecutionRequest,
    ) -> Result<GuiExecutionResult, GuiInteractionError> {
        self.ensure_enabled()
            .map_err(|e| GuiInteractionError::Unavailable(e.to_string()))?;
        let service = self.require_gui_service()?;
        let plan = service
            .prepare_execution(session_id, capability_token, req)
            .await?;

        let total_deadline = std::time::Duration::from_secs(GUI_EXECUTE_TIMEOUT_SECS);
        let action_timeout = std::time::Duration::from_secs(GUI_ACTION_TIMEOUT_SECS);
        let execution_start = Instant::now();

        let total_steps = plan.actions.len();
        tracing::info!(
            session_id,
            command_id = %plan.command_id,
            total_steps,
            timeout_secs = GUI_EXECUTE_TIMEOUT_SECS,
            "GUI execution started"
        );

        let actions_result = tokio::time::timeout(total_deadline, async {
            let mut last_result = IntentResult {
                success: false,
                element: None,
                verification: None,
                retry_count: 0,
                elapsed_ms: 0,
                error: Some("No executable actions in GUI plan".to_string()),
            };
            let mut execution_error: Option<String> = None;
            let mut steps_completed: usize = 0;

            for (index, action) in plan.actions.iter().enumerate() {
                let command_id = if index == 0 {
                    plan.command_id.clone()
                } else {
                    format!("{}:stage-{index}", plan.command_id)
                };

                let intent_command = IntentCommand {
                    command_id,
                    session_id: plan.session_id.clone(),
                    intent: AutomationIntent::Raw(action.clone()),
                    config: None,
                    timeout_ms: None,
                    policy_token: "gui-session".to_string(),
                };

                match tokio::time::timeout(action_timeout, self.execute_intent(&intent_command))
                    .await
                {
                    Ok(Ok(result)) => {
                        last_result = result;
                        if last_result.success {
                            steps_completed += 1;
                        } else {
                            tracing::warn!(
                                stage = index,
                                error = last_result.error.as_deref().unwrap_or("unknown"),
                                "GUI action failed at stage"
                            );
                            break;
                        }
                    }
                    Ok(Err(err)) => {
                        tracing::warn!(stage = index, error = %err, "GUI action error at stage");
                        execution_error = Some(err.to_string());
                        break;
                    }
                    Err(_elapsed) => {
                        tracing::warn!(
                            stage = index,
                            timeout_secs = GUI_ACTION_TIMEOUT_SECS,
                            "GUI action timed out at stage"
                        );
                        execution_error = Some(format!(
                            "GUI action timed out after {}s (stage {})",
                            GUI_ACTION_TIMEOUT_SECS, index
                        ));
                        break;
                    }
                }
            }

            (last_result, execution_error, steps_completed)
        })
        .await;

        let (last_result, execution_error, steps_completed) = match actions_result {
            Ok(inner) => inner,
            Err(_elapsed) => {
                let detail = format!("GUI execution timed out after {GUI_EXECUTE_TIMEOUT_SECS}s");
                tracing::error!(
                    session_id,
                    timeout_secs = GUI_EXECUTE_TIMEOUT_SECS,
                    elapsed_ms = execution_start.elapsed().as_millis() as u64,
                    "GUI total execution timeout exceeded"
                );
                let _ = service
                    .complete_execution(session_id, false, Some(detail.clone()), 0, total_steps)
                    .await;
                return Err(GuiInteractionError::Internal(detail));
            }
        };

        let succeeded = execution_error.is_none() && last_result.success;
        let detail = execution_error.clone().or_else(|| {
            if !last_result.success {
                if steps_completed > 0 {
                    Some(format!(
                        "Partial execution: {}/{} steps completed. {}",
                        steps_completed,
                        total_steps,
                        last_result.error.as_deref().unwrap_or("action failed")
                    ))
                } else {
                    last_result.error.clone()
                }
            } else {
                None
            }
        });
        let outcome = service
            .complete_execution(
                session_id,
                succeeded,
                detail.clone(),
                steps_completed,
                total_steps,
            )
            .await?;

        tracing::info!(
            session_id,
            succeeded,
            steps_completed,
            total_steps,
            elapsed_ms = execution_start.elapsed().as_millis() as u64,
            "GUI execution finished"
        );

        if let Some(err) = execution_error {
            return Err(GuiInteractionError::Internal(err));
        }

        Ok(GuiExecutionResult {
            command_id: plan.command_id,
            ticket: plan.ticket,
            result: last_result,
            outcome,
        })
    }

    pub async fn gui_cancel_session(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<GuiInteractionSession, GuiInteractionError> {
        self.ensure_enabled()
            .map_err(|e| GuiInteractionError::Unavailable(e.to_string()))?;
        let service = self.require_gui_service()?;
        service.cancel_session(session_id, capability_token).await
    }

    pub async fn gui_subscribe_events(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<broadcast::Receiver<GuiSessionEvent>, GuiInteractionError> {
        self.ensure_enabled()
            .map_err(|e| GuiInteractionError::Unavailable(e.to_string()))?;
        let service = self.require_gui_service()?;
        service
            .subscribe_session(session_id, capability_token)
            .await
    }
}
