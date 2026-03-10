//! `AutomationPort` trait implementation for `AutomationController`

use async_trait::async_trait;
use tokio::sync::broadcast;

use oneshim_core::error::{CoreError, GuiInteractionError};
use oneshim_core::models::automation::{
    AutomationCommand, CommandResult, GuiExecutionResult, PlannedIntentResult, WorkflowResult,
};
use oneshim_core::models::gui::{
    GuiConfirmRequest, GuiCreateSessionRequest, GuiCreateSessionResponse, GuiExecutionRequest,
    GuiExecutionTicket, GuiHighlightRequest, GuiInteractionSession, GuiSessionEvent,
};
use oneshim_core::models::intent::{IntentCommand, IntentResult, WorkflowPreset};
use oneshim_core::models::ui_scene::UiScene;
use oneshim_core::ports::automation::AutomationPort;

use super::AutomationController;

#[async_trait]
impl AutomationPort for AutomationController {
    async fn execute_command(&self, cmd: &AutomationCommand) -> Result<CommandResult, CoreError> {
        self.execute_command(cmd).await
    }

    async fn execute_intent(&self, cmd: &IntentCommand) -> Result<IntentResult, CoreError> {
        self.execute_intent(cmd).await
    }

    async fn execute_intent_hint(
        &self,
        command_id: &str,
        session_id: &str,
        intent_hint: &str,
    ) -> Result<PlannedIntentResult, CoreError> {
        self.execute_intent_hint(command_id, session_id, intent_hint)
            .await
    }

    async fn run_workflow(&self, preset: &WorkflowPreset) -> Result<WorkflowResult, CoreError> {
        self.run_workflow(preset).await
    }

    async fn analyze_scene(
        &self,
        app_name: Option<&str>,
        screen_id: Option<&str>,
    ) -> Result<UiScene, CoreError> {
        self.analyze_scene(app_name, screen_id).await
    }

    async fn analyze_scene_from_image(
        &self,
        image_data: Vec<u8>,
        image_format: String,
        app_name: Option<&str>,
        screen_id: Option<&str>,
    ) -> Result<UiScene, CoreError> {
        self.analyze_scene_from_image(image_data, image_format, app_name, screen_id)
            .await
    }

    async fn gui_create_session(
        &self,
        req: GuiCreateSessionRequest,
    ) -> Result<GuiCreateSessionResponse, GuiInteractionError> {
        self.gui_create_session(req).await
    }

    async fn gui_get_session(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<GuiInteractionSession, GuiInteractionError> {
        self.gui_get_session(session_id, capability_token).await
    }

    async fn gui_highlight_session(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiHighlightRequest,
    ) -> Result<GuiInteractionSession, GuiInteractionError> {
        self.gui_highlight_session(session_id, capability_token, req)
            .await
    }

    async fn gui_confirm_candidate(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiConfirmRequest,
    ) -> Result<GuiExecutionTicket, GuiInteractionError> {
        self.gui_confirm_candidate(session_id, capability_token, req)
            .await
    }

    async fn gui_execute(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiExecutionRequest,
    ) -> Result<GuiExecutionResult, GuiInteractionError> {
        self.gui_execute(session_id, capability_token, req).await
    }

    async fn gui_cancel_session(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<GuiInteractionSession, GuiInteractionError> {
        self.gui_cancel_session(session_id, capability_token).await
    }

    async fn gui_subscribe_events(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<broadcast::Receiver<GuiSessionEvent>, GuiInteractionError> {
        self.gui_subscribe_events(session_id, capability_token)
            .await
    }
}
