use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::AiProviderType;
use crate::error::CoreError;

const POLICY_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/config/ai_model_lifecycle_policy.json"
));

static POLICY_CATALOG: OnceLock<Result<ModelLifecyclePolicyCatalog, String>> = OnceLock::new();

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelLifecyclePolicyCatalog {
    pub version: u32,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub rules: Vec<ModelLifecycleRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelLifecycleRule {
    pub provider_type: String,
    pub model: String,
    #[serde(default)]
    pub warn_at: Option<String>,
    #[serde(default)]
    pub block_at: Option<String>,
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default)]
    pub action: ModelLifecycleAction,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelLifecycleAction {
    WarnOnly,
    #[default]
    WarnThenBlock,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelLifecycleDecision {
    Allowed,
    Warn {
        message: String,
        replacement: Option<String>,
    },
    Block {
        message: String,
        replacement: Option<String>,
    },
}

impl ModelLifecycleDecision {
    pub fn is_blocking(&self) -> bool {
        matches!(self, Self::Block { .. })
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Allowed => None,
            Self::Warn { message, .. } | Self::Block { message, .. } => Some(message),
        }
    }
}

pub fn list_model_lifecycle_policies() -> Result<ModelLifecyclePolicyCatalog, CoreError> {
    Ok(policy_catalog()?.clone())
}

pub fn evaluate_model_lifecycle_now(
    provider_type: AiProviderType,
    model: &str,
) -> Result<ModelLifecycleDecision, CoreError> {
    evaluate_model_lifecycle_at(provider_type, model, Utc::now())
}

pub fn evaluate_model_lifecycle_at(
    provider_type: AiProviderType,
    model: &str,
    now: DateTime<Utc>,
) -> Result<ModelLifecycleDecision, CoreError> {
    let trimmed_model = model.trim();
    if trimmed_model.is_empty() {
        return Ok(ModelLifecycleDecision::Allowed);
    }

    let catalog = policy_catalog()?;
    let Some(rule) = catalog.rules.iter().find(|candidate| {
        provider_rule_matches(provider_type, &candidate.provider_type)
            && candidate.model.trim().eq_ignore_ascii_case(trimmed_model)
    }) else {
        return Ok(ModelLifecycleDecision::Allowed);
    };

    let warn_at = parse_utc_opt(rule.warn_at.as_deref())?;
    let block_at = parse_utc_opt(rule.block_at.as_deref())?;

    let warn_due = warn_at.map(|at| now >= at).unwrap_or(false);
    let block_due = block_at.map(|at| now >= at).unwrap_or(false);

    match rule.action {
        ModelLifecycleAction::WarnOnly => {
            if warn_due {
                Ok(ModelLifecycleDecision::Warn {
                    message: build_warning_message(provider_type, trimmed_model, rule, warn_at),
                    replacement: rule.replacement.clone(),
                })
            } else {
                Ok(ModelLifecycleDecision::Allowed)
            }
        }
        ModelLifecycleAction::WarnThenBlock => {
            if block_due {
                Ok(ModelLifecycleDecision::Block {
                    message: build_block_message(provider_type, trimmed_model, rule, block_at),
                    replacement: rule.replacement.clone(),
                })
            } else if warn_due {
                Ok(ModelLifecycleDecision::Warn {
                    message: build_warning_message(provider_type, trimmed_model, rule, warn_at),
                    replacement: rule.replacement.clone(),
                })
            } else {
                Ok(ModelLifecycleDecision::Allowed)
            }
        }
        ModelLifecycleAction::Block => {
            let should_block = block_at.map(|at| now >= at).unwrap_or(true);
            if should_block {
                Ok(ModelLifecycleDecision::Block {
                    message: build_block_message(provider_type, trimmed_model, rule, block_at),
                    replacement: rule.replacement.clone(),
                })
            } else {
                Ok(ModelLifecycleDecision::Allowed)
            }
        }
    }
}

pub fn enforce_model_lifecycle_now(
    provider_type: AiProviderType,
    model: &str,
) -> Result<(), CoreError> {
    match evaluate_model_lifecycle_now(provider_type, model)? {
        ModelLifecycleDecision::Allowed | ModelLifecycleDecision::Warn { .. } => Ok(()),
        ModelLifecycleDecision::Block { message, .. } => Err(CoreError::PolicyDenied(message)),
    }
}

fn validate_policy_catalog(catalog: &ModelLifecyclePolicyCatalog) -> Result<(), String> {
    for rule in &catalog.rules {
        if parse_provider_type_label(&rule.provider_type).is_none() {
            return Err(format!("unknown provider_type `{}`", rule.provider_type));
        }

        if rule.model.trim().is_empty() {
            return Err("model lifecycle rule has empty `model`".to_string());
        }

        let warn_at = parse_utc_opt(rule.warn_at.as_deref()).map_err(|e| e.to_string())?;
        let block_at = parse_utc_opt(rule.block_at.as_deref()).map_err(|e| e.to_string())?;

        if let (Some(warn), Some(block)) = (warn_at, block_at) {
            if warn > block {
                return Err(format!(
                    "warn_at must be <= block_at for model `{}`",
                    rule.model
                ));
            }
        }
    }

    Ok(())
}

fn policy_catalog() -> Result<&'static ModelLifecyclePolicyCatalog, CoreError> {
    match POLICY_CATALOG.get_or_init(load_policy_catalog) {
        Ok(catalog) => Ok(catalog),
        Err(message) => Err(CoreError::Internal(message.clone())),
    }
}

fn load_policy_catalog() -> Result<ModelLifecyclePolicyCatalog, String> {
    let catalog = serde_json::from_str::<ModelLifecyclePolicyCatalog>(POLICY_JSON)
        .map_err(|e| format!("Failed to parse model lifecycle policy JSON: {e}"))?;

    validate_policy_catalog(&catalog)
        .map_err(|e| format!("Invalid model lifecycle policy JSON: {e}"))?;

    Ok(catalog)
}

fn parse_utc_opt(raw: Option<&str>) -> Result<Option<DateTime<Utc>>, CoreError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let parsed = DateTime::parse_from_rfc3339(trimmed).map_err(|e| {
        CoreError::Internal(format!(
            "Invalid RFC3339 datetime in model lifecycle policy: `{trimmed}` ({e})"
        ))
    })?;

    Ok(Some(parsed.with_timezone(&Utc)))
}

fn provider_rule_matches(provider_type: AiProviderType, raw_rule_provider: &str) -> bool {
    parse_provider_type_label(raw_rule_provider) == Some(provider_type)
}

fn parse_provider_type_label(raw: &str) -> Option<AiProviderType> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Some(AiProviderType::Anthropic),
        "openai" | "open_ai" | "open-ai" | "openai-compatible" => Some(AiProviderType::OpenAi),
        "google" | "gemini" => Some(AiProviderType::Google),
        "generic" => Some(AiProviderType::Generic),
        _ => None,
    }
}

fn provider_label(provider_type: AiProviderType) -> &'static str {
    match provider_type {
        AiProviderType::Anthropic => "anthropic",
        AiProviderType::OpenAi => "openai",
        AiProviderType::Google => "google",
        AiProviderType::Generic => "generic",
    }
}

fn build_warning_message(
    provider_type: AiProviderType,
    model: &str,
    rule: &ModelLifecycleRule,
    warn_at: Option<DateTime<Utc>>,
) -> String {
    let replacement = rule
        .replacement
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let replacement_msg = replacement
        .map(|value| format!(" Use `{value}` instead."))
        .unwrap_or_default();

    if let Some(warn_at) = warn_at {
        format!(
            "Model `{model}` for provider `{}` is in deprecation window since {}.{}",
            provider_label(provider_type),
            warn_at.to_rfc3339(),
            replacement_msg,
        )
    } else {
        format!(
            "Model `{model}` for provider `{}` is deprecated.{}",
            provider_label(provider_type),
            replacement_msg,
        )
    }
}

fn build_block_message(
    provider_type: AiProviderType,
    model: &str,
    rule: &ModelLifecycleRule,
    block_at: Option<DateTime<Utc>>,
) -> String {
    let replacement = rule
        .replacement
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let replacement_msg = replacement
        .map(|value| format!(" Use `{value}` instead."))
        .unwrap_or_default();

    if let Some(block_at) = block_at {
        format!(
            "Model `{model}` for provider `{}` is retired as of {}.{}",
            provider_label(provider_type),
            block_at.to_rfc3339(),
            replacement_msg,
        )
    } else {
        format!(
            "Model `{model}` for provider `{}` is blocked by lifecycle policy.{}",
            provider_label(provider_type),
            replacement_msg,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_catalog_loads() {
        let catalog = list_model_lifecycle_policies().expect("policy should load");
        assert!(catalog.version >= 1);
        assert!(!catalog.updated_at.trim().is_empty());
        assert!(!catalog.rules.is_empty());
    }

    #[test]
    fn openai_gpt35_is_blocked_after_block_date() {
        let now = DateTime::parse_from_rfc3339("2026-02-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let decision =
            evaluate_model_lifecycle_at(AiProviderType::OpenAi, "gpt-3.5-turbo", now).unwrap();

        assert!(decision.is_blocking());
    }

    #[test]
    fn google_gemini15_is_warned_before_block_date() {
        let now = DateTime::parse_from_rfc3339("2026-03-10T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let decision =
            evaluate_model_lifecycle_at(AiProviderType::Google, "gemini-1.5-pro", now).unwrap();

        assert!(matches!(decision, ModelLifecycleDecision::Warn { .. }));
    }

    #[test]
    fn unknown_model_is_allowed() {
        let decision = evaluate_model_lifecycle_now(AiProviderType::OpenAi, "gpt-4.1-mini")
            .expect("evaluation should succeed");
        assert_eq!(decision, ModelLifecycleDecision::Allowed);
    }
}
