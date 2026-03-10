use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::error::{CoreError, GuiInteractionError};
use crate::models::automation::{
    AutomationCommand, CommandResult, GuiExecutionResult, PlannedIntentResult, WorkflowResult,
};
use crate::models::gui::{
    GuiConfirmRequest, GuiCreateSessionRequest, GuiCreateSessionResponse, GuiExecutionRequest,
    GuiExecutionTicket, GuiHighlightRequest, GuiInteractionSession, GuiSessionEvent,
};
use crate::models::intent::{IntentCommand, IntentResult, WorkflowPreset};
use crate::models::ui_scene::UiScene;

/// 자동화 실행 포트 — oneshim-web 핸들러가 사용하는 자동화 컨트롤러 인터페이스
///
/// 이 trait은 `oneshim-automation::AutomationController`의 퍼블릭 API를
/// 추상화합니다. oneshim-web이 oneshim-automation에 직접 의존하지 않고
/// 이 포트를 통해 자동화 기능에 접근합니다. (ADR-001 §7)
#[async_trait]
pub trait AutomationPort: Send + Sync {
    // ── Core automation ──

    /// 저수준 커맨드 실행 (policy 검증 + sandbox)
    async fn execute_command(&self, cmd: &AutomationCommand) -> Result<CommandResult, CoreError>;

    /// 인텐트 기반 실행 (직접 인텐트 지정)
    async fn execute_intent(&self, cmd: &IntentCommand) -> Result<IntentResult, CoreError>;

    /// 자연어 힌트 기반 인텐트 실행 (IntentPlanner → IntentExecutor)
    async fn execute_intent_hint(
        &self,
        command_id: &str,
        session_id: &str,
        intent_hint: &str,
    ) -> Result<PlannedIntentResult, CoreError>;

    /// 워크플로우 프리셋 실행
    async fn run_workflow(&self, preset: &WorkflowPreset) -> Result<WorkflowResult, CoreError>;

    // ── Scene analysis ──

    /// 화면 장면 분석 (현재 포커스 또는 특정 앱)
    async fn analyze_scene(
        &self,
        app_name: Option<&str>,
        screen_id: Option<&str>,
    ) -> Result<UiScene, CoreError>;

    /// 이미지 데이터로부터 장면 분석
    async fn analyze_scene_from_image(
        &self,
        image_data: Vec<u8>,
        image_format: String,
        app_name: Option<&str>,
        screen_id: Option<&str>,
    ) -> Result<UiScene, CoreError>;

    // ── GUI interaction ──

    /// GUI 상호작용 세션 생성
    async fn gui_create_session(
        &self,
        req: GuiCreateSessionRequest,
    ) -> Result<GuiCreateSessionResponse, GuiInteractionError>;

    /// GUI 세션 조회
    async fn gui_get_session(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<GuiInteractionSession, GuiInteractionError>;

    /// GUI 후보 하이라이트
    async fn gui_highlight_session(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiHighlightRequest,
    ) -> Result<GuiInteractionSession, GuiInteractionError>;

    /// GUI 후보 확인 → 실행 티켓 발급
    async fn gui_confirm_candidate(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiConfirmRequest,
    ) -> Result<GuiExecutionTicket, GuiInteractionError>;

    /// GUI 실행 (티켓 기반)
    async fn gui_execute(
        &self,
        session_id: &str,
        capability_token: &str,
        req: GuiExecutionRequest,
    ) -> Result<GuiExecutionResult, GuiInteractionError>;

    /// GUI 세션 취소
    async fn gui_cancel_session(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<GuiInteractionSession, GuiInteractionError>;

    /// GUI 세션 이벤트 구독
    async fn gui_subscribe_events(
        &self,
        session_id: &str,
        capability_token: &str,
    ) -> Result<broadcast::Receiver<GuiSessionEvent>, GuiInteractionError>;
}
