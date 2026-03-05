use std::time::Instant;

use crate::policy::AuditLevel;
use crate::resolver;
use oneshim_core::config::SandboxConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::intent::WorkflowPreset;

use super::types::{AutomationCommand, CommandResult, WorkflowResult, WorkflowStepResult};
use super::AutomationController;

impl AutomationController {
    pub async fn run_workflow(&self, preset: &WorkflowPreset) -> Result<WorkflowResult, CoreError> {
        self.ensure_enabled()?;
        let executor = self.require_intent_executor()?;

        let total_steps = preset.steps.len();
        let mut step_results = Vec::with_capacity(total_steps);
        let mut all_success = true;
        let workflow_start = Instant::now();

        tracing::info!(
            preset_id = %preset.id,
            total_steps,
            "workflow preset execution started"
        );

        for (idx, step) in preset.steps.iter().enumerate() {
            if idx > 0 && step.delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(step.delay_ms)).await;
            }

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

            let step_start = Instant::now();
            let result = executor.execute(&step.intent).await;
            let step_elapsed = step_start.elapsed().as_millis() as u64;

            match result {
                Ok(intent_result) => {
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
                                "workflow step failed -> stopping"
                            );
                            break;
                        }
                    }
                }
                Err(e) => {
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
                            "workflow step error -> stopping"
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
                "preset '{}' succeeded ({}/{} steps, {}ms)",
                preset.name, steps_executed, total_steps, total_elapsed
            )
        } else {
            format!(
                "preset '{}' partially failed ({}/{} steps, {}ms)",
                preset.name, steps_executed, total_steps, total_elapsed
            )
        };

        tracing::info!(
            preset_id = %preset.id,
            success = all_success,
            steps_executed,
            total_elapsed_ms = total_elapsed,
            "workflow preset execution completed"
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

    pub(super) async fn resolve_for_command(
        &self,
        cmd: &AutomationCommand,
    ) -> (SandboxConfig, AuditLevel) {
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
                let config = resolver::default_strict_config(&self.base_sandbox_config);
                (config, AuditLevel::Basic)
            }
        }
    }

    pub async fn execute_command(
        &self,
        cmd: &AutomationCommand,
    ) -> Result<CommandResult, CoreError> {
        self.ensure_enabled()?;

        if !self.policy_client.validate_command(cmd).await? {
            let mut logger = self.audit_logger.write().await;
            logger.log_denied(
                &cmd.command_id,
                &cmd.session_id,
                &format!("{:?}", cmd.action),
            );
            return Ok(CommandResult::Denied);
        }

        let (resolved_config, audit_level) = self.resolve_for_command(cmd).await;

        {
            let mut logger = self.audit_logger.write().await;
            logger.log_start_if(
                audit_level,
                &cmd.command_id,
                &cmd.session_id,
                &format!("{:?}", cmd.action),
            );
        }

        let timeout_ms = cmd.timeout_ms.or(if resolved_config.max_cpu_time_ms > 0 {
            Some(resolved_config.max_cpu_time_ms)
        } else {
            None
        });

        let start = Instant::now();

        let result = if let Some(timeout) = timeout_ms {
            let duration = std::time::Duration::from_millis(timeout);
            match tokio::time::timeout(
                duration,
                self.action_dispatcher
                    .dispatch(&cmd.action, &resolved_config),
            )
            .await
            {
                Ok(result) => result,
                Err(_elapsed) => {
                    let mut logger = self.audit_logger.write().await;
                    logger.log_timeout(&cmd.command_id, &cmd.session_id, timeout);
                    return Ok(CommandResult::Timeout);
                }
            }
        } else {
            self.action_dispatcher
                .dispatch(&cmd.action, &resolved_config)
                .await
        };

        let elapsed_ms = start.elapsed().as_millis() as u64;

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
}
