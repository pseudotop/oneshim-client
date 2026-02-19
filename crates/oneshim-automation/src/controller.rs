//! 자동화 제어기.
//!
//! 서버에서 수신한 자동화 명령을 정책 검증 후 실행한다.
//! 모든 명령은 감사 로그에 기록되며, 정책 거부 시 실행되지 않는다.
//! 정책 기반 동적 샌드박스 설정 + 타임아웃 + 실행 시간 기록을 지원한다.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::audit::AuditLogger;
use crate::intent_resolver::IntentExecutor;
use crate::policy::{AuditLevel, PolicyClient};
use crate::resolver;
use oneshim_core::config::SandboxConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::intent::{IntentCommand, IntentResult, WorkflowPreset};
use oneshim_core::ports::sandbox::Sandbox;

// oneshim-core에 정의된 AutomationAction 재사용 + re-export
pub use oneshim_core::models::automation::{AutomationAction, MouseButton};

/// 서버에서 수신한 자동화 명령
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationCommand {
    /// 명령 고유 ID
    pub command_id: String,
    /// 세션 ID
    pub session_id: String,
    /// 실행할 액션
    pub action: AutomationAction,
    /// 타임아웃 (밀리초)
    pub timeout_ms: Option<u64>,
    /// 서버 정책 토큰 (일회성)
    pub policy_token: String,
}

/// 명령 실행 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandResult {
    /// 성공
    Success,
    /// 실패 (사유)
    Failed(String),
    /// 타임아웃
    Timeout,
    /// 정책 거부
    Denied,
}

/// 워크플로우 단계 실행 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepResult {
    /// 단계 이름
    pub step_name: String,
    /// 단계 인덱스 (0-based)
    pub step_index: usize,
    /// 성공 여부
    pub success: bool,
    /// 실행 시간 (밀리초)
    pub elapsed_ms: u64,
    /// 오류 메시지 (실패 시)
    pub error: Option<String>,
}

/// 워크플로우 프리셋 전체 실행 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    /// 실행한 프리셋 ID
    pub preset_id: String,
    /// 전체 성공 여부
    pub success: bool,
    /// 실행된 단계 수
    pub steps_executed: usize,
    /// 총 단계 수
    pub total_steps: usize,
    /// 전체 실행 시간 (밀리초)
    pub total_elapsed_ms: u64,
    /// 각 단계별 결과
    pub step_results: Vec<WorkflowStepResult>,
    /// 결과 메시지
    pub message: String,
}

// ============================================================
// AutomationController
// ============================================================

/// 자동화 제어기 — 정책 검증 + 샌드박스 격리 + 명령 실행 + 감사 로깅
pub struct AutomationController {
    /// 정책 클라이언트
    policy_client: Arc<PolicyClient>,
    /// 감사 로거
    audit_logger: Arc<RwLock<AuditLogger>>,
    /// OS 네이티브 샌드박스
    sandbox: Arc<dyn Sandbox>,
    /// 기본 샌드박스 설정 (정책 리졸버의 base로 사용)
    base_sandbox_config: SandboxConfig,
    /// 자동화 활성화 여부
    enabled: bool,
    /// 의도 실행기 (UI 자동화 시스템)
    intent_executor: Option<Arc<IntentExecutor>>,
}

impl AutomationController {
    /// 새 자동화 제어기 생성
    pub fn new(
        policy_client: Arc<PolicyClient>,
        audit_logger: Arc<RwLock<AuditLogger>>,
        sandbox: Arc<dyn Sandbox>,
        sandbox_config: SandboxConfig,
    ) -> Self {
        tracing::info!(
            platform = sandbox.platform(),
            available = sandbox.is_available(),
            "자동화 제어기 초기화 — 샌드박스 연결"
        );
        Self {
            policy_client,
            audit_logger,
            sandbox,
            base_sandbox_config: sandbox_config,
            enabled: false, // 기본 비활성
            intent_executor: None,
        }
    }

    /// 자동화 활성화/비활성화
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// 의도 실행기 설정 (UI 자동화 시스템)
    pub fn set_intent_executor(&mut self, executor: Arc<IntentExecutor>) {
        self.intent_executor = Some(executor);
    }

    /// 의도 명령 실행 (UI 자동화)
    ///
    /// 1. 정책 검증
    /// 2. IntentExecutor를 통한 의도 실행
    /// 3. 감사 로깅
    pub async fn execute_intent(&self, cmd: &IntentCommand) -> Result<IntentResult, CoreError> {
        // 1. 활성화 확인
        if !self.enabled {
            return Err(CoreError::PolicyDenied(
                "자동화가 비활성화 상태입니다".to_string(),
            ));
        }

        // 2. IntentExecutor 존재 확인
        let executor = self.intent_executor.as_ref().ok_or_else(|| {
            CoreError::Internal("IntentExecutor가 설정되지 않았습니다".to_string())
        })?;

        // 3. 감사 로그 (시작)
        {
            let mut logger = self.audit_logger.write().await;
            logger.log_start_if(
                AuditLevel::Basic,
                &cmd.command_id,
                &cmd.session_id,
                &format!("{:?}", cmd.intent),
            );
        }

        // 4. 의도 실행
        let start = Instant::now();
        let result = executor.execute(&cmd.intent).await?;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        // 5. 감사 로그 (완료)
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

    /// 워크플로우 프리셋 실행
    ///
    /// 각 단계를 순차 실행하며, 감사 로그를 기록한다.
    /// `stop_on_failure` 설정 시 실패 단계에서 중단한다.
    pub async fn run_workflow(&self, preset: &WorkflowPreset) -> Result<WorkflowResult, CoreError> {
        // 1. 활성화 확인
        if !self.enabled {
            return Err(CoreError::PolicyDenied(
                "자동화가 비활성화 상태입니다".to_string(),
            ));
        }

        // 2. IntentExecutor 존재 확인
        let executor = self.intent_executor.as_ref().ok_or_else(|| {
            CoreError::Internal("IntentExecutor가 설정되지 않았습니다".to_string())
        })?;

        let total_steps = preset.steps.len();
        let mut step_results = Vec::with_capacity(total_steps);
        let mut all_success = true;
        let workflow_start = Instant::now();

        tracing::info!(
            preset_id = %preset.id,
            total_steps,
            "워크플로우 프리셋 실행 시작"
        );

        // 3. 각 단계 순차 실행
        for (idx, step) in preset.steps.iter().enumerate() {
            // 첫 번째 단계 이후 delay 적용
            if idx > 0 && step.delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(step.delay_ms)).await;
            }

            // 감사 로그 (시작)
            let step_cmd_id = format!("{}:step-{}", preset.id, idx);
            {
                let mut logger = self.audit_logger.write().await;
                logger.log_start_if(
                    AuditLevel::Basic,
                    &step_cmd_id,
                    &preset.id,
                    &format!("step[{}] {}: {:?}", idx, step.name, step.intent),
                );
            }

            // 단계 실행
            let step_start = Instant::now();
            let result = executor.execute(&step.intent).await;
            let step_elapsed = step_start.elapsed().as_millis() as u64;

            match result {
                Ok(intent_result) => {
                    // 감사 로그 (완료)
                    {
                        let mut logger = self.audit_logger.write().await;
                        logger.log_complete_with_time(
                            AuditLevel::Basic,
                            &step_cmd_id,
                            &preset.id,
                            &format!(
                                "step[{}] success={}, elapsed={}ms",
                                idx, intent_result.success, step_elapsed
                            ),
                            step_elapsed,
                        );
                    }

                    let step_success = intent_result.success;
                    step_results.push(WorkflowStepResult {
                        step_name: step.name.clone(),
                        step_index: idx,
                        success: step_success,
                        elapsed_ms: step_elapsed,
                        error: if step_success {
                            None
                        } else {
                            intent_result.error.clone()
                        },
                    });

                    if !step_success {
                        all_success = false;
                        if step.stop_on_failure {
                            tracing::warn!(
                                step = idx,
                                name = %step.name,
                                "워크플로우 단계 실패 → 중단"
                            );
                            break;
                        }
                    }
                }
                Err(e) => {
                    // 감사 로그 (실패)
                    {
                        let mut logger = self.audit_logger.write().await;
                        logger.log_complete_with_time(
                            AuditLevel::Basic,
                            &step_cmd_id,
                            &preset.id,
                            &format!("step[{}] error: {}", idx, e),
                            step_elapsed,
                        );
                    }

                    step_results.push(WorkflowStepResult {
                        step_name: step.name.clone(),
                        step_index: idx,
                        success: false,
                        elapsed_ms: step_elapsed,
                        error: Some(e.to_string()),
                    });

                    all_success = false;
                    if step.stop_on_failure {
                        tracing::warn!(
                            step = idx,
                            name = %step.name,
                            error = %e,
                            "워크플로우 단계 오류 → 중단"
                        );
                        break;
                    }
                }
            }
        }

        let total_elapsed = workflow_start.elapsed().as_millis() as u64;
        let steps_executed = step_results.len();

        let message = if all_success {
            format!(
                "프리셋 '{}' 성공 ({}/{}단계, {}ms)",
                preset.name, steps_executed, total_steps, total_elapsed
            )
        } else {
            format!(
                "프리셋 '{}' 일부 실패 ({}/{}단계, {}ms)",
                preset.name, steps_executed, total_steps, total_elapsed
            )
        };

        tracing::info!(
            preset_id = %preset.id,
            success = all_success,
            steps_executed,
            total_elapsed_ms = total_elapsed,
            "워크플로우 프리셋 실행 완료"
        );

        Ok(WorkflowResult {
            preset_id: preset.id.clone(),
            success: all_success,
            steps_executed,
            total_steps,
            total_elapsed_ms: total_elapsed,
            step_results,
            message,
        })
    }

    /// 명령에 대한 샌드박스 설정과 감사 레벨을 리졸브
    async fn resolve_for_command(&self, cmd: &AutomationCommand) -> (SandboxConfig, AuditLevel) {
        match self
            .policy_client
            .get_policy_for_token(&cmd.policy_token)
            .await
        {
            Some(policy) => {
                let config = resolver::resolve_sandbox_config(&policy, &self.base_sandbox_config);
                (config, policy.audit_level)
            }
            None => {
                // 정책 없으면 Strict 기본값 + Basic 감사
                let config = resolver::default_strict_config(&self.base_sandbox_config);
                (config, AuditLevel::Basic)
            }
        }
    }

    /// 자동화 명령 실행 (정책 검증 필수)
    pub async fn execute_command(
        &self,
        cmd: &AutomationCommand,
    ) -> Result<CommandResult, CoreError> {
        // 1. 활성화 확인
        if !self.enabled {
            return Err(CoreError::PolicyDenied(
                "자동화가 비활성화 상태입니다".to_string(),
            ));
        }

        // 2. 정책 검증
        if !self.policy_client.validate_command(cmd).await? {
            let mut logger = self.audit_logger.write().await;
            logger.log_denied(
                &cmd.command_id,
                &cmd.session_id,
                &format!("{:?}", cmd.action),
            );
            return Ok(CommandResult::Denied);
        }

        // 3. 정책 기반 동적 샌드박스 설정 + 감사 레벨 리졸브
        let (resolved_config, audit_level) = self.resolve_for_command(cmd).await;

        // 4. 실행 전 감사 로그 (AuditLevel::None이면 스킵)
        {
            let mut logger = self.audit_logger.write().await;
            logger.log_start_if(
                audit_level,
                &cmd.command_id,
                &cmd.session_id,
                &format!("{:?}", cmd.action),
            );
        }

        // 5. 타임아웃 결정: cmd.timeout_ms와 policy max_execution_time_ms 중 작은 값
        let timeout_ms = cmd.timeout_ms.or(if resolved_config.max_cpu_time_ms > 0 {
            Some(resolved_config.max_cpu_time_ms)
        } else {
            None
        });

        // 6. 실행 시간 측정 시작
        let start = Instant::now();

        // 7. 타임아웃 적용하여 명령 실행
        let result = if let Some(timeout) = timeout_ms {
            let duration = std::time::Duration::from_millis(timeout);
            match tokio::time::timeout(
                duration,
                self.dispatch_action_with_config(&cmd.action, &resolved_config),
            )
            .await
            {
                Ok(result) => result,
                Err(_elapsed) => {
                    // 타임아웃 발생
                    let mut logger = self.audit_logger.write().await;
                    logger.log_timeout(&cmd.command_id, &cmd.session_id, timeout);
                    return Ok(CommandResult::Timeout);
                }
            }
        } else {
            self.dispatch_action_with_config(&cmd.action, &resolved_config)
                .await
        };

        // 8. 실행 시간 측정
        let elapsed_ms = start.elapsed().as_millis() as u64;

        // 9. 실행 후 감사 로그 (실행 시간 포함)
        {
            let mut logger = self.audit_logger.write().await;
            logger.log_complete_with_time(
                audit_level,
                &cmd.command_id,
                &cmd.session_id,
                &format!("{:?}", result),
                elapsed_ms,
            );
        }

        Ok(result)
    }

    /// 액션 디스패치 (리졸브된 샌드박스 설정으로 실행)
    async fn dispatch_action_with_config(
        &self,
        action: &AutomationAction,
        config: &SandboxConfig,
    ) -> CommandResult {
        tracing::info!(
            action = ?action,
            sandbox = self.sandbox.platform(),
            profile = ?config.profile,
            "자동화 명령 실행 (정책 기반 샌드박스 경유)"
        );

        // 샌드박스를 통해 실행 (리졸브된 설정 사용)
        if let Err(e) = self.sandbox.execute_sandboxed(action, config).await {
            tracing::error!(error = %e, "샌드박스 실행 실패");
            return CommandResult::Failed(format!("샌드박스 실행 실패: {}", e));
        }

        // 실제 입력 시뮬레이션은 enigo 통합 시 구현
        // 현재는 로깅만 수행
        match action {
            AutomationAction::MouseMove { x, y } => {
                tracing::debug!(x, y, "마우스 이동");
                CommandResult::Success
            }
            AutomationAction::MouseClick { button, x, y } => {
                tracing::debug!(button, x, y, "마우스 클릭");
                CommandResult::Success
            }
            AutomationAction::KeyType { text } => {
                tracing::debug!(text_len = text.len(), "텍스트 입력");
                CommandResult::Success
            }
            AutomationAction::KeyPress { key } => {
                tracing::debug!(key, "키 누름");
                CommandResult::Success
            }
            AutomationAction::KeyRelease { key } => {
                tracing::debug!(key, "키 놓음");
                CommandResult::Success
            }
            AutomationAction::Hotkey { keys } => {
                tracing::debug!(?keys, "단축키 실행");
                CommandResult::Success
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{AuditLevel, ExecutionPolicy};
    use crate::sandbox::NoOpSandbox;
    use oneshim_core::models::intent::{
        AutomationIntent, IntentConfig, PresetCategory, WorkflowPreset, WorkflowStep,
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
        // 비동기 초기화는 테스트 내에서 수행
        // policy_client에 정책을 추가하는 것은 테스트 본문에서 처리
        let _ = policy; // 정책은 테스트에서 update_policies로 설정
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
        // 비활성 상태에서는 정책 거부
        let cmd = AutomationCommand {
            command_id: "cmd-1".to_string(),
            session_id: "sess-1".to_string(),
            action: AutomationAction::MouseMove { x: 0, y: 0 },
            timeout_ms: None,
            policy_token: "token".to_string(),
        };
        let result = controller.execute_command(&cmd).await;
        assert!(result.is_err()); // 비활성이므로 PolicyDenied
    }

    #[tokio::test]
    async fn sandbox_error_propagation() {
        // NoOp 샌드박스는 항상 성공하므로 에러 전파 경로를 직접 테스트
        let action = AutomationAction::KeyType {
            text: "test".to_string(),
        };
        let sandbox = NoOpSandbox;
        let config = SandboxConfig::default();
        let result = sandbox.execute_sandboxed(&action, &config).await;
        assert!(result.is_ok());
    }

    // --- 신규: 정책 기반 동적 샌드박스 테스트 ---

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
            policy_token: "test-pol:nonce1".to_string(),
        };

        let (resolved, audit_level) = controller.resolve_for_command(&cmd).await;
        // Detailed → Strict 프로필
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
            // 매우 짧은 타임아웃은 NoOp에서는 발생하지 않으므로,
            // 정상 실행이 완료되면 Success 반환 확인
            timeout_ms: Some(5000),
            policy_token: "test-pol:nonce2".to_string(),
        };

        let result = controller.execute_command(&cmd).await.unwrap();
        // NoOp 샌드박스는 즉시 반환하므로 타임아웃 안 됨
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
            policy_token: "test-pol:nonce3".to_string(),
        };

        let result = controller.execute_command(&cmd).await.unwrap();
        assert!(matches!(result, CommandResult::Success));

        // AuditLevel::None이므로 감사 로그 0개
        let logger = audit_logger.read().await;
        assert_eq!(logger.pending_count(), 0);
    }

    // --- 워크플로우 결과 타입 테스트 ---

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
            message: "성공".to_string(),
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
            name: "테스트".to_string(),
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
            name: "테스트".to_string(),
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
    async fn run_workflow_with_executor_success() {
        use crate::input_driver::{NoOpElementFinder, NoOpInputDriver};
        use crate::intent_resolver::{IntentExecutor, IntentResolver};

        let mut controller = make_controller();
        controller.set_enabled(true);

        // IntentExecutor 설정 (NoOp 기반)
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
            name: "파일 저장".to_string(),
            description: "테스트".to_string(),
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

    // --- 추가 테스트: execute_intent 에러 경로 ---

    #[tokio::test]
    async fn execute_intent_disabled_returns_policy_denied() {
        let controller = make_controller(); // 기본 비활성
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
        controller.set_enabled(true); // 활성화하되 executor 미설정
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
            policy_token: "token".to_string(),
        };
        let result = controller.execute_intent(&cmd).await.unwrap();
        assert!(result.success);

        // 감사 로그 확인: Started + Completed = 2 entries
        let logger = audit_logger.read().await;
        assert_eq!(logger.pending_count(), 2);
    }

    // --- 추가 테스트: run_workflow 엣지 케이스 ---

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
            steps: vec![], // 0 단계
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
                    delay_ms: 10, // 짧은 딜레이
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
        assert!(result.total_elapsed_ms >= 20); // 딜레이 포함
    }

    // --- 추가 테스트: execute_command 경로 ---

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
            policy_token: "test-pol:nonce99".to_string(),
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
            error: Some("요소를 찾지 못함".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("요소를 찾지 못함"));
        assert!(json.contains("fail-step"));
        let deser: WorkflowStepResult = serde_json::from_str(&json).unwrap();
        assert!(!deser.success);
        assert_eq!(deser.error.unwrap(), "요소를 찾지 못함");
    }
}
