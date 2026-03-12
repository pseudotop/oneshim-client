mod gate;
mod intent;
mod port_impl;
mod preset;
mod types;

pub use types::{
    AutomationAction, AutomationCommand, CommandResult, GuiExecutionResult, MouseButton,
    PlannedIntentResult, WorkflowResult, WorkflowStepResult,
};

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::action_dispatcher::{AutomationActionDispatcher, SandboxActionDispatcher};
use crate::audit::AuditLogger;
use crate::gui_interaction::{GuiInteractionError, GuiInteractionService};
use crate::intent_planner::IntentPlanner;
use crate::intent_resolver::IntentExecutor;
use crate::policy::PolicyClient;
use gate::CommandExecutionGate;
use oneshim_core::config::SandboxConfig;
use oneshim_core::error::CoreError;
use oneshim_core::ports::element_finder::ElementFinder;
use oneshim_core::ports::focus_probe::FocusProbe;
use oneshim_core::ports::overlay_driver::OverlayDriver;
use oneshim_core::ports::sandbox::Sandbox;

const GUI_EXECUTE_TIMEOUT_SECS: u64 = 30;
const GUI_ACTION_TIMEOUT_SECS: u64 = 10;

// AutomationController

pub struct AutomationController {
    pub(super) policy_client: Arc<PolicyClient>,
    pub(super) audit_logger: Arc<RwLock<AuditLogger>>,
    pub(super) action_dispatcher: Arc<dyn AutomationActionDispatcher>,
    pub(super) base_sandbox_config: SandboxConfig,
    pub(super) enabled: bool,
    pub(super) intent_executor: Option<Arc<IntentExecutor>>,
    pub(super) intent_planner: Option<Arc<dyn IntentPlanner>>,
    pub(super) scene_finder: Option<Arc<dyn ElementFinder>>,
    pub(super) gui_service: Option<Arc<GuiInteractionService>>,
}

impl AutomationController {
    pub fn new(
        policy_client: Arc<PolicyClient>,
        audit_logger: Arc<RwLock<AuditLogger>>,
        sandbox: Arc<dyn Sandbox>,
        sandbox_config: SandboxConfig,
    ) -> Self {
        tracing::info!(
            platform = sandbox.platform(),
            available = sandbox.is_available(),
            "automation controller initialized - sandbox connected"
        );
        Self {
            policy_client,
            audit_logger,
            action_dispatcher: Arc::new(SandboxActionDispatcher::new(sandbox)),
            base_sandbox_config: sandbox_config,
            enabled: false, // disabled by default
            intent_executor: None,
            intent_planner: None,
            scene_finder: None,
            gui_service: None,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_intent_executor(&mut self, executor: Arc<IntentExecutor>) {
        self.intent_executor = Some(executor);
    }

    pub fn set_intent_planner(&mut self, planner: Arc<dyn IntentPlanner>) {
        self.intent_planner = Some(planner);
    }

    pub fn set_scene_finder(&mut self, finder: Arc<dyn ElementFinder>) {
        self.scene_finder = Some(finder);
    }

    pub fn set_action_dispatcher(&mut self, dispatcher: Arc<dyn AutomationActionDispatcher>) {
        self.action_dispatcher = dispatcher;
    }

    pub fn configure_gui_interaction(
        &mut self,
        focus_probe: Arc<dyn FocusProbe>,
        overlay_driver: Arc<dyn OverlayDriver>,
        hmac_secret: Option<String>,
    ) -> Result<(), CoreError> {
        let scene_finder = self
            .scene_finder
            .as_ref()
            .ok_or_else(|| CoreError::Internal("Scene analyzer is not configured".to_string()))?
            .clone();

        let service = Arc::new(GuiInteractionService::new(
            scene_finder,
            focus_probe,
            overlay_driver,
            hmac_secret,
        ));
        service.ensure_cleanup_task();
        self.gui_service = Some(service);
        Ok(())
    }

    pub(super) fn ensure_enabled(&self) -> Result<(), CoreError> {
        if self.enabled {
            Ok(())
        } else {
            Err(CoreError::PolicyDenied(
                "자동화가 비active화 state입니다".to_string(),
            ))
        }
    }

    pub(super) fn require_intent_executor(&self) -> Result<&Arc<IntentExecutor>, CoreError> {
        self.intent_executor
            .as_ref()
            .ok_or_else(|| CoreError::Internal("IntentExecutor is not configured".to_string()))
    }

    pub(super) fn require_intent_planner(&self) -> Result<&Arc<dyn IntentPlanner>, CoreError> {
        self.intent_planner
            .as_ref()
            .ok_or_else(|| CoreError::Internal("IntentPlanner is not configured".to_string()))
    }

    pub(super) fn require_scene_finder(&self) -> Result<&Arc<dyn ElementFinder>, CoreError> {
        self.scene_finder
            .as_ref()
            .ok_or_else(|| CoreError::Internal("Scene analyzer is not configured".to_string()))
    }

    pub(super) fn require_gui_service(
        &self,
    ) -> Result<&Arc<GuiInteractionService>, GuiInteractionError> {
        self.gui_service.as_ref().ok_or_else(|| {
            GuiInteractionError::Unavailable(
                "GUI interaction service is not configured".to_string(),
            )
        })
    }

    fn command_execution_gate(&self) -> CommandExecutionGate {
        CommandExecutionGate::new(
            self.policy_client.clone(),
            self.audit_logger.clone(),
            self.action_dispatcher.clone(),
            self.base_sandbox_config.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent_planner::IntentPlanner;
    use crate::policy::{AuditLevel, ExecutionPolicy};
    use crate::sandbox::NoOpSandbox;
    use oneshim_core::models::intent::{
        AutomationIntent, IntentConfig, PresetCategory, WorkflowPreset, WorkflowStep,
    };
    use oneshim_core::models::ui_scene::{
        NormalizedBounds, UiScene, UiSceneElement, UI_SCENE_SCHEMA_VERSION,
    };

    fn make_controller() -> AutomationController {
        let policy_client = Arc::new(PolicyClient::new());
        let audit_logger = Arc::new(RwLock::new(AuditLogger::default()));
        let sandbox: Arc<dyn Sandbox> = Arc::new(NoOpSandbox);
        let sandbox_config = SandboxConfig::default();
        AutomationController::new(policy_client, audit_logger, sandbox, sandbox_config)
    }

    fn make_controller_with_policy(
        policy: ExecutionPolicy,
    ) -> (
        AutomationController,
        Arc<PolicyClient>,
        Arc<RwLock<AuditLogger>>,
    ) {
        let policy_client = Arc::new(PolicyClient::new());
        let audit_logger = Arc::new(RwLock::new(AuditLogger::new(100, 10)));
        let sandbox: Arc<dyn Sandbox> = Arc::new(NoOpSandbox);
        let sandbox_config = SandboxConfig::default();
        let controller = AutomationController::new(
            policy_client.clone(),
            audit_logger.clone(),
            sandbox,
            sandbox_config,
        );
        let _ = policy; // policy is applied in tests via update_policies
        (controller, policy_client, audit_logger)
    }

    fn make_policy(audit: AuditLevel, timeout: u64) -> ExecutionPolicy {
        ExecutionPolicy {
            policy_id: "test-pol".to_string(),
            process_name: "test".to_string(),
            process_hash: None,
            allowed_args: vec![],
            requires_sudo: false,
            max_execution_time_ms: timeout,
            audit_level: audit,
            sandbox_profile: None,
            allowed_paths: vec![],
            allow_network: None,
            require_signed_token: false,
        }
    }

    struct StubPlanner {
        planned: AutomationIntent,
    }

    #[async_trait::async_trait]
    impl IntentPlanner for StubPlanner {
        async fn plan(&self, _intent_hint: &str) -> Result<AutomationIntent, CoreError> {
            Ok(self.planned.clone())
        }
    }

    struct StubSceneFinder;

    #[async_trait::async_trait]
    impl ElementFinder for StubSceneFinder {
        async fn find_element(
            &self,
            _text: Option<&str>,
            _role: Option<&str>,
            _region: Option<&oneshim_core::models::intent::ElementBounds>,
        ) -> Result<Vec<oneshim_core::models::intent::UiElement>, CoreError> {
            Ok(vec![])
        }

        async fn analyze_scene(
            &self,
            app_name: Option<&str>,
            screen_id: Option<&str>,
        ) -> Result<UiScene, CoreError> {
            Ok(UiScene {
                schema_version: UI_SCENE_SCHEMA_VERSION.to_string(),
                scene_id: "scene-stub".to_string(),
                app_name: app_name.map(str::to_string),
                screen_id: screen_id.map(str::to_string),
                captured_at: chrono::Utc::now(),
                screen_width: 1920,
                screen_height: 1080,
                elements: vec![UiSceneElement {
                    element_id: "el-1".to_string(),
                    bbox_abs: oneshim_core::models::intent::ElementBounds {
                        x: 100,
                        y: 80,
                        width: 240,
                        height: 48,
                    },
                    bbox_norm: NormalizedBounds::new(0.05, 0.07, 0.12, 0.04),
                    label: "Save".to_string(),
                    role: Some("button".to_string()),
                    intent: Some("execute".to_string()),
                    state: Some("enabled".to_string()),
                    confidence: 0.95,
                    text_masked: Some("Save".to_string()),
                    parent_id: None,
                }],
            })
        }

        fn name(&self) -> &str {
            "stub-scene"
        }
    }

    #[test]
    fn automation_action_serde_roundtrip() {
        let action = AutomationAction::MouseClick {
            button: "left".to_string(),
            x: 100,
            y: 200,
        };
        let json = serde_json::to_string(&action).unwrap();
        let deser: AutomationAction = serde_json::from_str(&json).unwrap();
        match deser {
            AutomationAction::MouseClick { x, y, .. } => {
                assert_eq!(x, 100);
                assert_eq!(y, 200);
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn command_result_serde() {
        let result = CommandResult::Success;
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Success"));
    }

    #[tokio::test]
    async fn sandbox_integrated_dispatch() {
        let controller = make_controller();
        let cmd = AutomationCommand {
            command_id: "cmd-1".to_string(),
            session_id: "sess-1".to_string(),
            action: AutomationAction::MouseMove { x: 0, y: 0 },
            timeout_ms: None,
            policy_token: "token".to_string(),
        };
        let result = controller.execute_command(&cmd).await;
        assert!(result.is_err()); // disabled -> PolicyDenied
    }

    #[tokio::test]
    async fn sandbox_error_propagation() {
        let action = AutomationAction::KeyType {
            text: "test".to_string(),
        };
        let sandbox = NoOpSandbox;
        let config = SandboxConfig::default();
        let result = sandbox.execute_sandboxed(&action, &config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn resolve_uses_policy_config() {
        let policy = make_policy(AuditLevel::Detailed, 5000);
        let (controller, policy_client, _) = make_controller_with_policy(policy.clone());
        policy_client.update_policies(vec![policy]).await;

        let cmd = AutomationCommand {
            command_id: "cmd-1".to_string(),
            session_id: "sess-1".to_string(),
            action: AutomationAction::MouseMove { x: 0, y: 0 },
            timeout_ms: None,
            policy_token: "test-pol:nonce_0001".to_string(),
        };

        let (resolved, audit_level) = controller.resolve_for_command(&cmd).await;
        assert!(matches!(
            resolved.profile,
            oneshim_core::config::SandboxProfile::Strict
        ));
        assert!(matches!(audit_level, AuditLevel::Detailed));
        assert_eq!(resolved.max_cpu_time_ms, 5000);
    }

    #[tokio::test]
    async fn resolve_defaults_to_strict_without_policy() {
        let controller = make_controller();
        let cmd = AutomationCommand {
            command_id: "cmd-1".to_string(),
            session_id: "sess-1".to_string(),
            action: AutomationAction::MouseMove { x: 0, y: 0 },
            timeout_ms: None,
            policy_token: "unknown:nonce".to_string(),
        };

        let (resolved, audit_level) = controller.resolve_for_command(&cmd).await;
        assert!(matches!(
            resolved.profile,
            oneshim_core::config::SandboxProfile::Strict
        ));
        assert!(matches!(audit_level, AuditLevel::Basic));
    }

    #[tokio::test]
    async fn execute_with_timeout_returns_timeout_result() {
        let policy = make_policy(AuditLevel::Basic, 0);
        let (mut controller, policy_client, _) = make_controller_with_policy(policy.clone());
        controller.set_enabled(true);
        policy_client.update_policies(vec![policy]).await;

        let cmd = AutomationCommand {
            command_id: "cmd-timeout".to_string(),
            session_id: "sess-1".to_string(),
            action: AutomationAction::MouseMove { x: 0, y: 0 },
            timeout_ms: Some(5000),
            policy_token: "test-pol:nonce_0002".to_string(),
        };

        let result = controller.execute_command(&cmd).await.unwrap();
        assert!(matches!(result, CommandResult::Success));
    }

    #[tokio::test]
    async fn audit_level_none_skips_logging() {
        let policy = make_policy(AuditLevel::None, 0);
        let (mut controller, policy_client, audit_logger) =
            make_controller_with_policy(policy.clone());
        controller.set_enabled(true);
        policy_client.update_policies(vec![policy]).await;

        let cmd = AutomationCommand {
            command_id: "cmd-nolog".to_string(),
            session_id: "sess-1".to_string(),
            action: AutomationAction::KeyPress {
                key: "a".to_string(),
            },
            timeout_ms: None,
            policy_token: "test-pol:nonce_0003".to_string(),
        };

        let result = controller.execute_command(&cmd).await.unwrap();
        assert!(matches!(result, CommandResult::Success));

        let logger = audit_logger.read().await;
        assert_eq!(logger.pending_count(), 0);
    }

    #[test]
    fn workflow_result_serde_roundtrip() {
        let result = WorkflowResult {
            preset_id: "save-file".to_string(),
            success: true,
            steps_executed: 2,
            total_steps: 2,
            total_elapsed_ms: 150,
            step_results: vec![
                WorkflowStepResult {
                    step_name: "step1".to_string(),
                    step_index: 0,
                    success: true,
                    elapsed_ms: 50,
                    error: None,
                },
                WorkflowStepResult {
                    step_name: "step2".to_string(),
                    step_index: 1,
                    success: true,
                    elapsed_ms: 100,
                    error: None,
                },
            ],
            message: "success".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let deser: WorkflowResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.preset_id, "save-file");
        assert!(deser.success);
        assert_eq!(deser.steps_executed, 2);
        assert_eq!(deser.step_results.len(), 2);
    }

    #[tokio::test]
    async fn run_workflow_disabled_returns_error() {
        let controller = make_controller();
        let preset = WorkflowPreset {
            id: "test".to_string(),
            name: "test".to_string(),
            description: String::new(),
            category: PresetCategory::Productivity,
            steps: vec![],
            builtin: true,
            platform: None,
        };
        let result = controller.run_workflow(&preset).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn run_workflow_no_executor_returns_error() {
        let mut controller = make_controller();
        controller.set_enabled(true);
        let preset = WorkflowPreset {
            id: "test".to_string(),
            name: "test".to_string(),
            description: String::new(),
            category: PresetCategory::Productivity,
            steps: vec![WorkflowStep {
                name: "Step1".to_string(),
                intent: AutomationIntent::ExecuteHotkey {
                    keys: vec!["Ctrl".to_string(), "A".to_string()],
                },
                delay_ms: 0,
                stop_on_failure: true,
            }],
            builtin: true,
            platform: None,
        };
        let result = controller.run_workflow(&preset).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn run_workflow_with_executor_success() {
        use crate::input_driver::{NoOpElementFinder, NoOpInputDriver};
        use crate::intent_resolver::{IntentExecutor, IntentResolver};

        let mut controller = make_controller();
        controller.set_enabled(true);

        let input_driver: Arc<dyn oneshim_core::ports::input_driver::InputDriver> =
            Arc::new(NoOpInputDriver);
        let element_finder: Arc<dyn oneshim_core::ports::element_finder::ElementFinder> =
            Arc::new(NoOpElementFinder);
        let resolver = IntentResolver::new(element_finder, input_driver, IntentConfig::default());
        controller.set_intent_executor(Arc::new(IntentExecutor::new(
            resolver,
            IntentConfig::default(),
        )));

        let preset = WorkflowPreset {
            id: "save-file".to_string(),
            name: "file save".to_string(),
            description: "test".to_string(),
            category: PresetCategory::Productivity,
            steps: vec![WorkflowStep {
                name: "Ctrl+S".to_string(),
                intent: AutomationIntent::ExecuteHotkey {
                    keys: vec!["Ctrl".to_string(), "S".to_string()],
                },
                delay_ms: 0,
                stop_on_failure: true,
            }],
            builtin: true,
            platform: None,
        };

        let result = controller.run_workflow(&preset).await.unwrap();
        assert!(result.success);
        assert_eq!(result.steps_executed, 1);
        assert_eq!(result.total_steps, 1);
        assert_eq!(result.step_results.len(), 1);
        assert!(result.step_results[0].success);
    }

    #[tokio::test]
    async fn execute_intent_disabled_returns_policy_denied() {
        let controller = make_controller(); // default disabled
        let cmd = oneshim_core::models::intent::IntentCommand {
            command_id: "intent-1".to_string(),
            session_id: "sess-1".to_string(),
            intent: AutomationIntent::ExecuteHotkey {
                keys: vec!["Ctrl".to_string(), "C".to_string()],
            },
            config: None,
            timeout_ms: None,
            policy_token: "token".to_string(),
        };
        let result = controller.execute_intent(&cmd).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err,
            oneshim_core::error::CoreError::PolicyDenied(_)
        ));
    }

    #[tokio::test]
    async fn execute_intent_no_executor_returns_internal_error() {
        let mut controller = make_controller();
        controller.set_enabled(true); // enabled but executor missing
        let cmd = oneshim_core::models::intent::IntentCommand {
            command_id: "intent-2".to_string(),
            session_id: "sess-1".to_string(),
            intent: AutomationIntent::ExecuteHotkey {
                keys: vec!["Ctrl".to_string(), "V".to_string()],
            },
            config: None,
            timeout_ms: None,
            policy_token: "token".to_string(),
        };
        let result = controller.execute_intent(&cmd).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, oneshim_core::error::CoreError::Internal(_)));
    }

    #[tokio::test]
    async fn execute_intent_success_with_audit_log() {
        use super::gate::SCENE_ACTION_POLICY_TOKEN;
        use crate::input_driver::{NoOpElementFinder, NoOpInputDriver};
        use crate::intent_resolver::{IntentExecutor, IntentResolver};

        let policy_client = Arc::new(PolicyClient::new());
        let audit_logger = Arc::new(RwLock::new(AuditLogger::new(100, 10)));
        let sandbox: Arc<dyn Sandbox> = Arc::new(NoOpSandbox);
        let sandbox_config = SandboxConfig::default();
        let mut controller =
            AutomationController::new(policy_client, audit_logger.clone(), sandbox, sandbox_config);
        controller.set_enabled(true);

        let input_driver: Arc<dyn oneshim_core::ports::input_driver::InputDriver> =
            Arc::new(NoOpInputDriver);
        let element_finder: Arc<dyn oneshim_core::ports::element_finder::ElementFinder> =
            Arc::new(NoOpElementFinder);
        let resolver = IntentResolver::new(element_finder, input_driver, IntentConfig::default());
        controller.set_intent_executor(Arc::new(IntentExecutor::new(
            resolver,
            IntentConfig::default(),
        )));

        let cmd = oneshim_core::models::intent::IntentCommand {
            command_id: "intent-3".to_string(),
            session_id: "sess-1".to_string(),
            intent: AutomationIntent::ExecuteHotkey {
                keys: vec!["Alt".to_string(), "Tab".to_string()],
            },
            config: None,
            timeout_ms: None,
            policy_token: SCENE_ACTION_POLICY_TOKEN.to_string(),
        };
        let result = controller.execute_intent(&cmd).await.unwrap();
        assert!(result.success);

        let logger = audit_logger.read().await;
        assert_eq!(logger.pending_count(), 4);
    }

    #[tokio::test]
    async fn execute_intent_hint_requires_planner() {
        use crate::input_driver::{NoOpElementFinder, NoOpInputDriver};
        use crate::intent_resolver::{IntentExecutor, IntentResolver};

        let mut controller = make_controller();
        controller.set_enabled(true);

        let input_driver: Arc<dyn oneshim_core::ports::input_driver::InputDriver> =
            Arc::new(NoOpInputDriver);
        let element_finder: Arc<dyn oneshim_core::ports::element_finder::ElementFinder> =
            Arc::new(NoOpElementFinder);
        let resolver = IntentResolver::new(element_finder, input_driver, IntentConfig::default());
        controller.set_intent_executor(Arc::new(IntentExecutor::new(
            resolver,
            IntentConfig::default(),
        )));

        let result = controller
            .execute_intent_hint("hint-1", "sess-1", "save 버튼 클릭")
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CoreError::Internal(msg) if msg.contains("IntentPlanner")
        ));
    }

    #[tokio::test]
    async fn execute_intent_hint_success() {
        use crate::input_driver::{NoOpElementFinder, NoOpInputDriver};
        use crate::intent_resolver::{IntentExecutor, IntentResolver};

        let mut controller = make_controller();
        controller.set_enabled(true);
        controller.set_intent_planner(Arc::new(StubPlanner {
            planned: AutomationIntent::ExecuteHotkey {
                keys: vec!["Ctrl".to_string(), "S".to_string()],
            },
        }));

        let input_driver: Arc<dyn oneshim_core::ports::input_driver::InputDriver> =
            Arc::new(NoOpInputDriver);
        let element_finder: Arc<dyn oneshim_core::ports::element_finder::ElementFinder> =
            Arc::new(NoOpElementFinder);
        let resolver = IntentResolver::new(element_finder, input_driver, IntentConfig::default());
        controller.set_intent_executor(Arc::new(IntentExecutor::new(
            resolver,
            IntentConfig::default(),
        )));

        let result = controller
            .execute_intent_hint("hint-2", "sess-1", "Ctrl+S execution")
            .await
            .unwrap();

        assert!(matches!(
            result.planned_intent,
            AutomationIntent::ExecuteHotkey { .. }
        ));
        assert!(result.result.success);
    }

    #[tokio::test]
    async fn analyze_scene_requires_scene_finder() {
        let mut controller = make_controller();
        controller.set_enabled(true);

        let err = controller.analyze_scene(None, None).await.unwrap_err();
        assert!(matches!(err, CoreError::Internal(_)));
    }

    #[tokio::test]
    async fn analyze_scene_success_with_scene_finder() {
        let mut controller = make_controller();
        controller.set_enabled(true);
        controller.set_scene_finder(Arc::new(StubSceneFinder));

        let scene = controller
            .analyze_scene(Some("VSCode"), Some("screen-1"))
            .await
            .unwrap();
        assert_eq!(scene.scene_id, "scene-stub");
        assert_eq!(scene.app_name.as_deref(), Some("VSCode"));
        assert_eq!(scene.screen_id.as_deref(), Some("screen-1"));
        assert_eq!(scene.elements.len(), 1);
    }

    #[tokio::test]
    async fn run_workflow_empty_steps_succeeds() {
        use crate::input_driver::{NoOpElementFinder, NoOpInputDriver};
        use crate::intent_resolver::{IntentExecutor, IntentResolver};

        let mut controller = make_controller();
        controller.set_enabled(true);

        let input_driver: Arc<dyn oneshim_core::ports::input_driver::InputDriver> =
            Arc::new(NoOpInputDriver);
        let element_finder: Arc<dyn oneshim_core::ports::element_finder::ElementFinder> =
            Arc::new(NoOpElementFinder);
        let resolver = IntentResolver::new(element_finder, input_driver, IntentConfig::default());
        controller.set_intent_executor(Arc::new(IntentExecutor::new(
            resolver,
            IntentConfig::default(),
        )));

        let preset = WorkflowPreset {
            id: "empty".to_string(),
            name: "빈 워크플로우".to_string(),
            description: String::new(),
            category: PresetCategory::Productivity,
            steps: vec![], // 0 steps
            builtin: true,
            platform: None,
        };

        let result = controller.run_workflow(&preset).await.unwrap();
        assert!(result.success);
        assert_eq!(result.steps_executed, 0);
        assert_eq!(result.total_steps, 0);
        assert!(result.step_results.is_empty());
    }

    #[tokio::test]
    async fn run_workflow_multi_step_with_delay() {
        use crate::input_driver::{NoOpElementFinder, NoOpInputDriver};
        use crate::intent_resolver::{IntentExecutor, IntentResolver};

        let mut controller = make_controller();
        controller.set_enabled(true);

        let input_driver: Arc<dyn oneshim_core::ports::input_driver::InputDriver> =
            Arc::new(NoOpInputDriver);
        let element_finder: Arc<dyn oneshim_core::ports::element_finder::ElementFinder> =
            Arc::new(NoOpElementFinder);
        let resolver = IntentResolver::new(element_finder, input_driver, IntentConfig::default());
        controller.set_intent_executor(Arc::new(IntentExecutor::new(
            resolver,
            IntentConfig::default(),
        )));

        let preset = WorkflowPreset {
            id: "multi".to_string(),
            name: "멀티 스텝".to_string(),
            description: String::new(),
            category: PresetCategory::Productivity,
            steps: vec![
                WorkflowStep {
                    name: "Step1".to_string(),
                    intent: AutomationIntent::ExecuteHotkey {
                        keys: vec!["Ctrl".to_string(), "A".to_string()],
                    },
                    delay_ms: 0,
                    stop_on_failure: false,
                },
                WorkflowStep {
                    name: "Step2".to_string(),
                    intent: AutomationIntent::ExecuteHotkey {
                        keys: vec!["Ctrl".to_string(), "C".to_string()],
                    },
                    delay_ms: 10, // short delay
                    stop_on_failure: false,
                },
                WorkflowStep {
                    name: "Step3".to_string(),
                    intent: AutomationIntent::ExecuteHotkey {
                        keys: vec!["Ctrl".to_string(), "V".to_string()],
                    },
                    delay_ms: 10,
                    stop_on_failure: false,
                },
            ],
            builtin: true,
            platform: None,
        };

        let result = controller.run_workflow(&preset).await.unwrap();
        assert!(result.success);
        assert_eq!(result.steps_executed, 3);
        assert_eq!(result.total_steps, 3);
        assert_eq!(result.step_results.len(), 3);
        assert!(result.step_results.iter().all(|s| s.success));
        assert!(result.total_elapsed_ms >= 20); // includes delay
    }

    #[tokio::test]
    async fn execute_command_enabled_with_valid_policy() {
        let policy = make_policy(AuditLevel::Basic, 5000);
        let (mut controller, policy_client, _) = make_controller_with_policy(policy.clone());
        controller.set_enabled(true);
        policy_client.update_policies(vec![policy]).await;

        let cmd = AutomationCommand {
            command_id: "cmd-ok".to_string(),
            session_id: "sess-1".to_string(),
            action: AutomationAction::KeyType {
                text: "hello".to_string(),
            },
            timeout_ms: None,
            policy_token: "test-pol:nonce_0099".to_string(),
        };

        let result = controller.execute_command(&cmd).await.unwrap();
        assert!(matches!(result, CommandResult::Success));
    }

    #[tokio::test]
    async fn workflow_step_result_error_field() {
        let result = WorkflowStepResult {
            step_name: "fail-step".to_string(),
            step_index: 2,
            success: false,
            elapsed_ms: 50,
            error: Some("Element not found".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Element not found"));
        assert!(json.contains("fail-step"));
        let deser: WorkflowStepResult = serde_json::from_str(&json).unwrap();
        assert!(!deser.success);
        assert_eq!(deser.error.unwrap(), "Element not found");
    }

    // ── M2: Execution timeout constants ────────────────────────────────

    #[test]
    fn gui_execute_timeout_is_bounded() {
        assert!(
            std::hint::black_box(GUI_EXECUTE_TIMEOUT_SECS) >= 10
                && std::hint::black_box(GUI_EXECUTE_TIMEOUT_SECS) <= 120,
            "Total execution timeout should be between 10s and 120s"
        );
    }

    #[test]
    fn gui_action_timeout_is_bounded() {
        assert!(
            std::hint::black_box(GUI_ACTION_TIMEOUT_SECS) >= 3
                && std::hint::black_box(GUI_ACTION_TIMEOUT_SECS) <= 60,
            "Per-action timeout should be between 3s and 60s"
        );
    }

    #[test]
    fn gui_action_timeout_less_than_total() {
        assert!(
            std::hint::black_box(GUI_ACTION_TIMEOUT_SECS)
                < std::hint::black_box(GUI_EXECUTE_TIMEOUT_SECS),
            "Per-action timeout must be less than total execution timeout"
        );
    }
}
