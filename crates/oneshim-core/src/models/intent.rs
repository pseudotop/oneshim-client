//! 자율 클라이언트 UI 자동화 의도 모델.
//!
//! 서버가 보내는 고수준 의도(Intent)와 화면 요소(UiElement) 모델을 정의한다.
//! 2계층 액션 모델: `AutomationIntent` (서버→클라이언트) → `AutomationAction` (클라이언트 내부)

use serde::{Deserialize, Serialize};

use super::automation::AutomationAction;

// ============================================================
// AutomationIntent — 서버가 보내는 고수준 의도
// ============================================================

/// 서버가 보내는 고수준 자동화 의도
///
/// 서버는 화면 렌더링 정보를 받지 않으므로 텍스트/역할 기반으로 의도를 전달하고,
/// 클라이언트가 자율적으로 UI 요소를 탐색하여 실행한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutomationIntent {
    /// UI 요소 클릭 (텍스트/역할 기반)
    ClickElement {
        /// 클릭 대상 텍스트 (예: "저장", "확인")
        text: Option<String>,
        /// 대상 역할 (예: "button", "link")
        role: Option<String>,
        /// 대상 앱 이름
        app_name: Option<String>,
        /// 마우스 버튼 ("left", "right", "middle")
        button: String,
    },
    /// UI 요소에 텍스트 입력
    TypeIntoElement {
        /// 입력 대상 요소의 텍스트 (예: "검색")
        element_text: Option<String>,
        /// 대상 역할 (예: "input", "textbox")
        role: Option<String>,
        /// 입력할 텍스트
        text: String,
    },
    /// 단축키 실행
    ExecuteHotkey {
        /// 키 조합 (예: ["Ctrl", "S"])
        keys: Vec<String>,
    },
    /// 텍스트가 화면에 나타날 때까지 대기
    WaitForText {
        /// 대기 대상 텍스트
        text: String,
        /// 최대 대기 시간 (밀리초)
        timeout_ms: u64,
    },
    /// 특정 앱 활성화
    ActivateApp {
        /// 활성화할 앱 이름
        app_name: String,
    },
    /// 저수준 액션 직접 전달 (기존 AutomationAction)
    Raw(AutomationAction),
}

// ============================================================
// UiElement — 화면에서 발견된 UI 요소
// ============================================================

/// 화면에서 발견된 UI 요소
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElement {
    /// 요소 텍스트
    pub text: String,
    /// 요소 경계 영역
    pub bounds: ElementBounds,
    /// 요소 역할 (button, input 등)
    pub role: Option<String>,
    /// 탐색 신뢰도 (0.0 ~ 1.0)
    pub confidence: f64,
    /// 탐색 소스 (OCR, 접근성 API 등)
    pub source: FinderSource,
}

/// UI 요소의 경계 영역 (화면 좌표)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementBounds {
    /// 좌상단 X 좌표
    pub x: i32,
    /// 좌상단 Y 좌표
    pub y: i32,
    /// 너비
    pub width: u32,
    /// 높이
    pub height: u32,
}

impl ElementBounds {
    /// 경계 영역의 중심 좌표 반환
    pub fn center(&self) -> (i32, i32) {
        (
            self.x + self.width as i32 / 2,
            self.y + self.height as i32 / 2,
        )
    }

    /// 지정 좌표가 경계 영역 내에 있는지 확인
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && px < self.x + self.width as i32
            && py >= self.y
            && py < self.y + self.height as i32
    }
}

/// UI 요소를 발견한 소스
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinderSource {
    /// OCR 텍스트 인식
    Ocr,
    /// OS 접근성 API
    Accessibility,
    /// 이미지 템플릿 매칭
    TemplateMatcher,
}

// ============================================================
// IntentResult — 의도 실행 결과
// ============================================================

/// 의도 실행 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentResult {
    /// 실행 성공 여부
    pub success: bool,
    /// 클릭/입력한 요소 정보
    pub element: Option<UiElement>,
    /// 실행 후 검증 결과
    pub verification: Option<VerificationResult>,
    /// 재시도 횟수
    pub retry_count: u32,
    /// 실행 시간 (밀리초)
    pub elapsed_ms: u64,
    /// 실패 사유
    pub error: Option<String>,
}

/// 실행 후 화면 변화 검증 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// 화면 변화 감지 여부
    pub screen_changed: bool,
    /// 변화 감지된 영역 수
    pub changed_regions: usize,
    /// 기대 텍스트 발견 여부 (None이면 검증 미수행)
    pub text_found: Option<bool>,
}

// ============================================================
// IntentCommand — 서버 메시지 구조
// ============================================================

/// 서버에서 수신한 의도 명령
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentCommand {
    /// 명령 고유 ID
    pub command_id: String,
    /// 세션 ID
    pub session_id: String,
    /// 실행할 의도
    pub intent: AutomationIntent,
    /// 의도 실행 설정 (None이면 기본값 사용)
    pub config: Option<IntentConfig>,
    /// 전체 타임아웃 (밀리초)
    pub timeout_ms: Option<u64>,
    /// 서버 정책 토큰
    pub policy_token: String,
}

// ============================================================
// IntentConfig — 의도 실행 설정
// ============================================================

/// 의도 실행 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentConfig {
    /// 최소 신뢰도 임계값 (기본 0.7)
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f64,
    /// 최대 재시도 횟수 (기본 3)
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// 재시도 간격 (밀리초, 기본 500)
    #[serde(default = "default_retry_interval_ms")]
    pub retry_interval_ms: u64,
    /// 실행 후 검증 여부 (기본 true)
    #[serde(default = "default_verify")]
    pub verify_after_action: bool,
    /// 검증 대기 시간 (밀리초, 기본 1000)
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

// ============================================================
// 워크플로우 프리셋
// ============================================================

/// 워크플로우 프리셋 — 여러 Intent를 순차 실행하는 시퀀스
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPreset {
    /// 프리셋 고유 ID
    pub id: String,
    /// 프리셋 이름 (표시용)
    pub name: String,
    /// 설명
    pub description: String,
    /// 카테고리
    pub category: PresetCategory,
    /// 실행할 Intent 단계 목록
    pub steps: Vec<WorkflowStep>,
    /// 내장 프리셋 여부 (false면 사용자 정의)
    #[serde(default)]
    pub builtin: bool,
    /// 플랫폼 제한 (None이면 모든 플랫폼)
    #[serde(default)]
    pub platform: Option<String>,
}

/// 워크플로우 단계
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// 단계 이름
    pub name: String,
    /// 실행할 의도
    pub intent: AutomationIntent,
    /// 이전 단계 완료 후 대기 시간 (밀리초)
    #[serde(default)]
    pub delay_ms: u64,
    /// 이 단계 실패 시 워크플로우 중단 여부
    #[serde(default = "default_stop_on_failure")]
    pub stop_on_failure: bool,
}

fn default_stop_on_failure() -> bool {
    true
}

/// 프리셋 카테고리
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresetCategory {
    /// 생산성 (단축키, 파일 작업)
    Productivity,
    /// 앱 관리 (앱 전환, 창 정리)
    AppManagement,
    /// 워크플로우 (다단계 자동화)
    Workflow,
    /// 사용자 정의
    Custom,
}

// ============================================================
// 테스트
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_click_element_serde() {
        let intent = AutomationIntent::ClickElement {
            text: Some("저장".to_string()),
            role: Some("button".to_string()),
            app_name: None,
            button: "left".to_string(),
        };
        let json = serde_json::to_string(&intent).unwrap();
        let deser: AutomationIntent = serde_json::from_str(&json).unwrap();
        match deser {
            AutomationIntent::ClickElement { text, button, .. } => {
                assert_eq!(text.unwrap(), "저장");
                assert_eq!(button, "left");
            }
            other => unreachable!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn intent_type_into_element_serde() {
        let intent = AutomationIntent::TypeIntoElement {
            element_text: Some("검색".to_string()),
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
            text: "완료".to_string(),
            timeout_ms: 5000,
        };
        let json = serde_json::to_string(&intent).unwrap();
        let deser: AutomationIntent = serde_json::from_str(&json).unwrap();
        match deser {
            AutomationIntent::WaitForText { text, timeout_ms } => {
                assert_eq!(text, "완료");
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
            text: "저장".to_string(),
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
        assert_eq!(deser.text, "저장");
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
            name: "파일 저장".to_string(),
            description: "현재 파일을 저장합니다".to_string(),
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
