//!

use serde::{Deserialize, Serialize};

use super::automation::AutomationAction;

///
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutomationIntent {
    ClickElement {
        text: Option<String>,
        role: Option<String>,
        app_name: Option<String>,
        button: String,
    },
    TypeIntoElement {
        element_text: Option<String>,
        role: Option<String>,
        text: String,
    },
    ExecuteHotkey {
        keys: Vec<String>,
    },
    WaitForText {
        text: String,
        timeout_ms: u64,
    },
    ActivateApp {
        app_name: String,
    },
    Raw(AutomationAction),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElement {
    pub text: String,
    pub bounds: ElementBounds,
    pub role: Option<String>,
    pub confidence: f64,
    pub source: FinderSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ElementBounds {
    pub fn center(&self) -> (i32, i32) {
        (
            self.x + self.width as i32 / 2,
            self.y + self.height as i32 / 2,
        )
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && px < self.x + self.width as i32
            && py >= self.y
            && py < self.y + self.height as i32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinderSource {
    Ocr,
    Accessibility,
    TemplateMatcher,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentResult {
    pub success: bool,
    pub element: Option<UiElement>,
    pub verification: Option<VerificationResult>,
    pub retry_count: u32,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub screen_changed: bool,
    pub changed_regions: usize,
    pub text_found: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentCommand {
    pub command_id: String,
    pub session_id: String,
    pub intent: AutomationIntent,
    pub config: Option<IntentConfig>,
    pub timeout_ms: Option<u64>,
    pub policy_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentConfig {
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_interval_ms")]
    pub retry_interval_ms: u64,
    #[serde(default = "default_verify")]
    pub verify_after_action: bool,
    #[serde(default = "default_verify_delay_ms")]
    pub verify_delay_ms: u64,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            min_confidence: default_min_confidence(),
            max_retries: default_max_retries(),
            retry_interval_ms: default_retry_interval_ms(),
            verify_after_action: default_verify(),
            verify_delay_ms: default_verify_delay_ms(),
        }
    }
}

fn default_min_confidence() -> f64 {
    0.7
}
fn default_max_retries() -> u32 {
    3
}
fn default_retry_interval_ms() -> u64 {
    500
}
fn default_verify() -> bool {
    true
}
fn default_verify_delay_ms() -> u64 {
    1000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: PresetCategory,
    pub steps: Vec<WorkflowStep>,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub platform: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub name: String,
    pub intent: AutomationIntent,
    #[serde(default)]
    pub delay_ms: u64,
    #[serde(default = "default_stop_on_failure")]
    pub stop_on_failure: bool,
}

fn default_stop_on_failure() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresetCategory {
    Productivity,
    AppManagement,
    Workflow,
    Custom,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_click_element_serde() {
        let intent = AutomationIntent::ClickElement {
            text: Some("save".to_string()),
            role: Some("button".to_string()),
            app_name: None,
            button: "left".to_string(),
        };
        let json = serde_json::to_string(&intent).unwrap();
        let deser: AutomationIntent = serde_json::from_str(&json).unwrap();
        match deser {
            AutomationIntent::ClickElement { text, button, .. } => {
                assert_eq!(text.unwrap(), "save");
                assert_eq!(button, "left");
            }
            other => unreachable!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn intent_type_into_element_serde() {
        let intent = AutomationIntent::TypeIntoElement {
            element_text: Some("search".to_string()),
            role: None,
            text: "hello world".to_string(),
        };
        let json = serde_json::to_string(&intent).unwrap();
        let deser: AutomationIntent = serde_json::from_str(&json).unwrap();
        match deser {
            AutomationIntent::TypeIntoElement { text, .. } => {
                assert_eq!(text, "hello world");
            }
            other => unreachable!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn intent_execute_hotkey_serde() {
        let intent = AutomationIntent::ExecuteHotkey {
            keys: vec!["Ctrl".to_string(), "S".to_string()],
        };
        let json = serde_json::to_string(&intent).unwrap();
        let deser: AutomationIntent = serde_json::from_str(&json).unwrap();
        match deser {
            AutomationIntent::ExecuteHotkey { keys } => {
                assert_eq!(keys.len(), 2);
            }
            other => unreachable!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn intent_wait_for_text_serde() {
        let intent = AutomationIntent::WaitForText {
            text: "completed".to_string(),
            timeout_ms: 5000,
        };
        let json = serde_json::to_string(&intent).unwrap();
        let deser: AutomationIntent = serde_json::from_str(&json).unwrap();
        match deser {
            AutomationIntent::WaitForText { text, timeout_ms } => {
                assert_eq!(text, "completed");
                assert_eq!(timeout_ms, 5000);
            }
            other => unreachable!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn intent_raw_serde() {
        let intent = AutomationIntent::Raw(AutomationAction::MouseClick {
            button: "left".to_string(),
            x: 100,
            y: 200,
        });
        let json = serde_json::to_string(&intent).unwrap();
        let deser: AutomationIntent = serde_json::from_str(&json).unwrap();
        assert!(matches!(deser, AutomationIntent::Raw(_)));
    }

    #[test]
    fn intent_activate_app_serde() {
        let intent = AutomationIntent::ActivateApp {
            app_name: "Visual Studio Code".to_string(),
        };
        let json = serde_json::to_string(&intent).unwrap();
        let deser: AutomationIntent = serde_json::from_str(&json).unwrap();
        match deser {
            AutomationIntent::ActivateApp { app_name } => {
                assert_eq!(app_name, "Visual Studio Code");
            }
            other => unreachable!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn element_bounds_center() {
        let bounds = ElementBounds {
            x: 100,
            y: 200,
            width: 80,
            height: 40,
        };
        let (cx, cy) = bounds.center();
        assert_eq!(cx, 140);
        assert_eq!(cy, 220);
    }

    #[test]
    fn element_bounds_contains() {
        let bounds = ElementBounds {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        };
        assert!(bounds.contains(10, 20));
        assert!(bounds.contains(50, 40));
        assert!(!bounds.contains(110, 20));
        assert!(!bounds.contains(10, 70));
        assert!(!bounds.contains(9, 20));
    }

    #[test]
    fn ui_element_serde() {
        let elem = UiElement {
            text: "save".to_string(),
            bounds: ElementBounds {
                x: 100,
                y: 200,
                width: 80,
                height: 30,
            },
            role: Some("button".to_string()),
            confidence: 0.95,
            source: FinderSource::Ocr,
        };
        let json = serde_json::to_string(&elem).unwrap();
        let deser: UiElement = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.text, "save");
        assert_eq!(deser.confidence, 0.95);
        assert_eq!(deser.source, FinderSource::Ocr);
    }

    #[test]
    fn intent_config_defaults() {
        let config = IntentConfig::default();
        assert!((config.min_confidence - 0.7).abs() < f64::EPSILON);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_interval_ms, 500);
        assert!(config.verify_after_action);
        assert_eq!(config.verify_delay_ms, 1000);
    }

    #[test]
    fn intent_config_serde_with_defaults() {
        let json = "{}";
        let config: IntentConfig = serde_json::from_str(json).unwrap();
        assert!((config.min_confidence - 0.7).abs() < f64::EPSILON);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn intent_config_serde_override() {
        let json = r#"{"min_confidence": 0.9, "max_retries": 5}"#;
        let config: IntentConfig = serde_json::from_str(json).unwrap();
        assert!((config.min_confidence - 0.9).abs() < f64::EPSILON);
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn intent_command_serde() {
        let cmd = IntentCommand {
            command_id: "cmd-1".to_string(),
            session_id: "sess-1".to_string(),
            intent: AutomationIntent::ExecuteHotkey {
                keys: vec!["Ctrl".to_string(), "C".to_string()],
            },
            config: None,
            timeout_ms: Some(10000),
            policy_token: "token-abc".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let deser: IntentCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.command_id, "cmd-1");
        assert_eq!(deser.policy_token, "token-abc");
    }

    #[test]
    fn verification_result_serde() {
        let result = VerificationResult {
            screen_changed: true,
            changed_regions: 3,
            text_found: Some(true),
        };
        let json = serde_json::to_string(&result).unwrap();
        let deser: VerificationResult = serde_json::from_str(&json).unwrap();
        assert!(deser.screen_changed);
        assert_eq!(deser.changed_regions, 3);
    }

    #[test]
    fn intent_result_serde() {
        let result = IntentResult {
            success: true,
            element: None,
            verification: Some(VerificationResult {
                screen_changed: true,
                changed_regions: 1,
                text_found: None,
            }),
            retry_count: 0,
            elapsed_ms: 250,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deser: IntentResult = serde_json::from_str(&json).unwrap();
        assert!(deser.success);
        assert_eq!(deser.elapsed_ms, 250);
    }

    #[test]
    fn finder_source_serde() {
        let source = FinderSource::Accessibility;
        let json = serde_json::to_string(&source).unwrap();
        let deser: FinderSource = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, FinderSource::Accessibility);
    }

    #[test]
    fn workflow_preset_serde() {
        let preset = WorkflowPreset {
            id: "save-file".to_string(),
            name: "file save".to_string(),
            description: "Save the current file.".to_string(),
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
        let json = serde_json::to_string(&preset).unwrap();
        let deser: WorkflowPreset = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.id, "save-file");
        assert_eq!(deser.category, PresetCategory::Productivity);
        assert_eq!(deser.steps.len(), 1);
        assert!(deser.builtin);
    }

    #[test]
    fn workflow_step_defaults() {
        let json = r#"{"name":"step1","intent":{"ExecuteHotkey":{"keys":["Ctrl","Z"]}}}"#;
        let step: WorkflowStep = serde_json::from_str(json).unwrap();
        assert_eq!(step.delay_ms, 0);
        assert!(step.stop_on_failure);
    }

    #[test]
    fn preset_category_serde() {
        for cat in [
            PresetCategory::Productivity,
            PresetCategory::AppManagement,
            PresetCategory::Workflow,
            PresetCategory::Custom,
        ] {
            let json = serde_json::to_string(&cat).unwrap();
            let deser: PresetCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, cat);
        }
    }
}
