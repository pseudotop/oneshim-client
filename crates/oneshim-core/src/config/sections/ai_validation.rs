// AI 검증 설정 — OCR 신뢰도, 장면 행동 오버라이드, 장면 지능 검증 설정
use super::super::enums::AiProviderType;
use crate::error::CoreError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── ExternalApiEndpoint ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalApiEndpoint {
    pub endpoint: String,
    #[serde(default)]
    pub api_key: String,
    pub model: Option<String>,
    #[serde(default = "default_api_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub provider_type: AiProviderType,
}

// ── OcrValidationConfig ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrValidationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_ocr_min_confidence")]
    pub min_confidence: f64,
    #[serde(default = "default_ocr_max_invalid_ratio")]
    pub max_invalid_ratio: f64,
}

impl Default for OcrValidationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: default_ocr_min_confidence(),
            max_invalid_ratio: default_ocr_max_invalid_ratio(),
        }
    }
}

impl OcrValidationConfig {
    pub fn validate(&self) -> Result<(), CoreError> {
        if !self.enabled {
            return Ok(());
        }

        if !self.min_confidence.is_finite() || !(0.0..=1.0).contains(&self.min_confidence) {
            return Err(CoreError::Config(
                "`ai_provider.ocr_validation.min_confidence` must be within 0.0..=1.0.".to_string(),
            ));
        }

        if !self.max_invalid_ratio.is_finite() || !(0.0..=1.0).contains(&self.max_invalid_ratio) {
            return Err(CoreError::Config(
                "`ai_provider.ocr_validation.max_invalid_ratio` must be within 0.0..=1.0."
                    .to_string(),
            ));
        }

        Ok(())
    }
}

// ── SceneActionOverrideConfig ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SceneActionOverrideConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

impl SceneActionOverrideConfig {
    pub fn is_active_at(&self, now: DateTime<Utc>) -> bool {
        if !self.enabled {
            return false;
        }

        let reason = self.reason.as_deref().map(str::trim).unwrap_or_default();
        let approved_by = self
            .approved_by
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        let Some(expires_at) = self.expires_at else {
            return false;
        };

        !reason.is_empty() && !approved_by.is_empty() && expires_at > now
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if !self.enabled {
            return Ok(());
        }

        let reason = self.reason.as_deref().map(str::trim).unwrap_or_default();
        if reason.is_empty() {
            return Err(CoreError::Config(
                "`ai_provider.scene_action_override.reason` is required.".to_string(),
            ));
        }

        let approved_by = self
            .approved_by
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        if approved_by.is_empty() {
            return Err(CoreError::Config(
                "`ai_provider.scene_action_override.approved_by` is required.".to_string(),
            ));
        }

        let expires_at = self.expires_at.ok_or_else(|| {
            CoreError::Config(
                "`ai_provider.scene_action_override.expires_at` is required.".to_string(),
            )
        })?;

        if expires_at <= Utc::now() {
            return Err(CoreError::Config(
                "`ai_provider.scene_action_override.expires_at` must be in the future.".to_string(),
            ));
        }

        Ok(())
    }
}

// ── SceneIntelligenceConfig ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneIntelligenceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub overlay_enabled: bool,
    #[serde(default = "default_false")]
    pub allow_action_execution: bool,
    #[serde(default = "default_scene_min_confidence")]
    pub min_confidence: f64,
    #[serde(default = "default_scene_max_elements")]
    pub max_elements: usize,
    #[serde(default = "default_true")]
    pub calibration_enabled: bool,
    #[serde(default = "default_scene_calibration_min_elements")]
    pub calibration_min_elements: usize,
    #[serde(default = "default_scene_calibration_min_avg_confidence")]
    pub calibration_min_avg_confidence: f64,
}

impl Default for SceneIntelligenceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            overlay_enabled: true,
            allow_action_execution: default_false(),
            min_confidence: default_scene_min_confidence(),
            max_elements: default_scene_max_elements(),
            calibration_enabled: true,
            calibration_min_elements: default_scene_calibration_min_elements(),
            calibration_min_avg_confidence: default_scene_calibration_min_avg_confidence(),
        }
    }
}

impl SceneIntelligenceConfig {
    pub fn validate(&self) -> Result<(), CoreError> {
        if !self.min_confidence.is_finite() || !(0.0..=1.0).contains(&self.min_confidence) {
            return Err(CoreError::Config(
                "`ai_provider.scene_intelligence.min_confidence` must be within 0.0..=1.0."
                    .to_string(),
            ));
        }
        if self.max_elements == 0 || self.max_elements > 1000 {
            return Err(CoreError::Config(
                "`ai_provider.scene_intelligence.max_elements` must be within 1..=1000."
                    .to_string(),
            ));
        }
        if self.calibration_min_elements == 0 || self.calibration_min_elements > 1000 {
            return Err(CoreError::Config(
                "`ai_provider.scene_intelligence.calibration_min_elements` must be within 1..=1000."
                    .to_string(),
            ));
        }
        if !self.calibration_min_avg_confidence.is_finite()
            || !(0.0..=1.0).contains(&self.calibration_min_avg_confidence)
        {
            return Err(CoreError::Config(
                "`ai_provider.scene_intelligence.calibration_min_avg_confidence` must be within 0.0..=1.0."
                    .to_string(),
            ));
        }
        Ok(())
    }
}

// ── Private default helpers ─────────────────────────────────────────

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_api_timeout_secs() -> u64 {
    30
}

fn default_ocr_min_confidence() -> f64 {
    0.25
}

fn default_ocr_max_invalid_ratio() -> f64 {
    0.6
}

fn default_scene_min_confidence() -> f64 {
    0.35
}

fn default_scene_max_elements() -> usize {
    120
}

fn default_scene_calibration_min_elements() -> usize {
    8
}

fn default_scene_calibration_min_avg_confidence() -> f64 {
    0.55
}
