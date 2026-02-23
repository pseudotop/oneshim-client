//! 워크플로우 프리셋 — 내장 프리셋 정의.
//!
//! 생산성, 앱 관리, 워크플로우 카테고리의 내장 프리셋을 제공한다.
//! 플랫폼에 따라 modifier 키가 자동 매핑된다 (macOS: Cmd, 기타: Ctrl).

use oneshim_core::models::intent::{
    AutomationIntent, PresetCategory, WorkflowPreset, WorkflowStep,
};

/// 플랫폼에 맞는 modifier 키 반환 (macOS: "Cmd", 기타: "Ctrl")
fn platform_modifier() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

/// 앱 전환용 키 (macOS: "Cmd", 기타: "Alt")
fn platform_alt_modifier() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Alt"
    }
}

/// 내장 프리셋 목록 반환
pub fn builtin_presets() -> Vec<WorkflowPreset> {
    let m = platform_modifier();
    let alt = platform_alt_modifier();

    let mut presets = Vec::new();

    // ── 생산성 프리셋 (4개) ──

    presets.push(WorkflowPreset {
        id: "save-file".to_string(),
        name: "파일 저장".to_string(),
        description: "현재 파일을 저장합니다".to_string(),
        category: PresetCategory::Productivity,
        steps: vec![WorkflowStep {
            name: format!("{}+S", m),
            intent: AutomationIntent::ExecuteHotkey {
                keys: vec![m.to_string(), "S".to_string()],
            },
            delay_ms: 0,
            stop_on_failure: true,
        }],
        builtin: true,
        platform: None,
    });

    presets.push(WorkflowPreset {
        id: "undo".to_string(),
        name: "실행 취소".to_string(),
        description: "마지막 작업을 실행 취소합니다".to_string(),
        category: PresetCategory::Productivity,
        steps: vec![WorkflowStep {
            name: format!("{}+Z", m),
            intent: AutomationIntent::ExecuteHotkey {
                keys: vec![m.to_string(), "Z".to_string()],
            },
            delay_ms: 0,
            stop_on_failure: true,
        }],
        builtin: true,
        platform: None,
    });

    presets.push(WorkflowPreset {
        id: "select-all-copy".to_string(),
        name: "전체 선택 후 복사".to_string(),
        description: "전체 선택(Ctrl+A) 후 복사(Ctrl+C)를 실행합니다".to_string(),
        category: PresetCategory::Productivity,
        steps: vec![
            WorkflowStep {
                name: format!("{}+A", m),
                intent: AutomationIntent::ExecuteHotkey {
                    keys: vec![m.to_string(), "A".to_string()],
                },
                delay_ms: 0,
                stop_on_failure: true,
            },
            WorkflowStep {
                name: format!("{}+C", m),
                intent: AutomationIntent::ExecuteHotkey {
                    keys: vec![m.to_string(), "C".to_string()],
                },
                delay_ms: 200,
                stop_on_failure: true,
            },
        ],
        builtin: true,
        platform: None,
    });

    presets.push(WorkflowPreset {
        id: "find-replace".to_string(),
        name: "찾기/바꾸기".to_string(),
        description: "찾기/바꾸기 대화상자를 엽니다".to_string(),
        category: PresetCategory::Productivity,
        steps: vec![WorkflowStep {
            name: format!("{}+H", m),
            intent: AutomationIntent::ExecuteHotkey {
                keys: vec![m.to_string(), "H".to_string()],
            },
            delay_ms: 0,
            stop_on_failure: true,
        }],
        builtin: true,
        platform: None,
    });

    // ── 앱 관리 프리셋 (3개) ──

    presets.push(WorkflowPreset {
        id: "switch-next-app".to_string(),
        name: "다음 앱 전환".to_string(),
        description: "다음 애플리케이션으로 전환합니다".to_string(),
        category: PresetCategory::AppManagement,
        steps: vec![WorkflowStep {
            name: format!("{}+Tab", alt),
            intent: AutomationIntent::ExecuteHotkey {
                keys: vec![alt.to_string(), "Tab".to_string()],
            },
            delay_ms: 0,
            stop_on_failure: true,
        }],
        builtin: true,
        platform: None,
    });

    presets.push(WorkflowPreset {
        id: "close-window".to_string(),
        name: "현재 창 닫기".to_string(),
        description: "현재 활성 창을 닫습니다".to_string(),
        category: PresetCategory::AppManagement,
        steps: vec![WorkflowStep {
            name: format!("{}+W", m),
            intent: AutomationIntent::ExecuteHotkey {
                keys: vec![m.to_string(), "W".to_string()],
            },
            delay_ms: 0,
            stop_on_failure: true,
        }],
        builtin: true,
        platform: None,
    });

    if cfg!(target_os = "macos") {
        presets.push(WorkflowPreset {
            id: "minimize-all".to_string(),
            name: "전체 최소화".to_string(),
            description: "모든 창을 최소화합니다".to_string(),
            category: PresetCategory::AppManagement,
            steps: vec![WorkflowStep {
                name: "Cmd+Option+H+M".to_string(),
                intent: AutomationIntent::ExecuteHotkey {
                    keys: vec![
                        "Cmd".to_string(),
                        "Option".to_string(),
                        "H".to_string(),
                        "M".to_string(),
                    ],
                },
                delay_ms: 0,
                stop_on_failure: false,
            }],
            builtin: true,
            platform: Some("macos".to_string()),
        });
    } else {
        presets.push(WorkflowPreset {
            id: "minimize-all".to_string(),
            name: "전체 최소화".to_string(),
            description: "모든 창을 최소화합니다".to_string(),
            category: PresetCategory::AppManagement,
            steps: vec![WorkflowStep {
                name: "Win+D".to_string(),
                intent: AutomationIntent::ExecuteHotkey {
                    keys: vec!["Win".to_string(), "D".to_string()],
                },
                delay_ms: 0,
                stop_on_failure: false,
            }],
            builtin: true,
            platform: Some("windows".to_string()),
        });
    }

    // ── 워크플로우 프리셋 (8개) ──

    presets.push(WorkflowPreset {
        id: "morning-routine".to_string(),
        name: "업무 시작".to_string(),
        description: "Mail → Calendar → VSCode 순으로 앱을 활성화합니다".to_string(),
        category: PresetCategory::Workflow,
        steps: vec![
            WorkflowStep {
                name: "Mail 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Mail".to_string(),
                },
                delay_ms: 0,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "Calendar 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Calendar".to_string(),
                },
                delay_ms: 2000,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "VSCode 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Visual Studio Code".to_string(),
                },
                delay_ms: 2000,
                stop_on_failure: false,
            },
        ],
        builtin: true,
        platform: None,
    });

    presets.push(WorkflowPreset {
        id: "meeting-prep".to_string(),
        name: "회의 준비".to_string(),
        description: "Zoom과 Notes를 열어 회의를 준비합니다".to_string(),
        category: PresetCategory::Workflow,
        steps: vec![
            WorkflowStep {
                name: "Zoom 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Zoom".to_string(),
                },
                delay_ms: 0,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "Notes 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Notes".to_string(),
                },
                delay_ms: 1000,
                stop_on_failure: false,
            },
        ],
        builtin: true,
        platform: None,
    });

    presets.push(WorkflowPreset {
        id: "end-of-day".to_string(),
        name: "업무 종료".to_string(),
        description: "파일 저장 후 앱을 종료합니다".to_string(),
        category: PresetCategory::Workflow,
        steps: vec![
            WorkflowStep {
                name: "파일 저장".to_string(),
                intent: AutomationIntent::ExecuteHotkey {
                    keys: vec![m.to_string(), "S".to_string()],
                },
                delay_ms: 0,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "앱 종료".to_string(),
                intent: AutomationIntent::ExecuteHotkey {
                    keys: vec![m.to_string(), "Q".to_string()],
                },
                delay_ms: 1000,
                stop_on_failure: false,
            },
        ],
        builtin: true,
        platform: None,
    });

    presets.push(WorkflowPreset {
        id: "daily-priority-sync".to_string(),
        name: "우선순위 점검".to_string(),
        description: "캘린더, 이슈 보드, 메신저를 순서대로 열어 당일 우선순위를 정리합니다"
            .to_string(),
        category: PresetCategory::Workflow,
        steps: vec![
            WorkflowStep {
                name: "Calendar 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Calendar".to_string(),
                },
                delay_ms: 0,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "Jira 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Jira".to_string(),
                },
                delay_ms: 1200,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "Slack 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Slack".to_string(),
                },
                delay_ms: 1200,
                stop_on_failure: false,
            },
        ],
        builtin: true,
        platform: None,
    });

    presets.push(WorkflowPreset {
        id: "bug-triage-loop".to_string(),
        name: "버그 트리아지".to_string(),
        description: "이슈 트래커, 로그/모니터링, IDE를 순환하며 버그를 정리합니다".to_string(),
        category: PresetCategory::Workflow,
        steps: vec![
            WorkflowStep {
                name: "Issue Tracker 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Issue Tracker".to_string(),
                },
                delay_ms: 0,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "Monitoring 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Monitoring".to_string(),
                },
                delay_ms: 1200,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "VSCode 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Visual Studio Code".to_string(),
                },
                delay_ms: 1200,
                stop_on_failure: false,
            },
        ],
        builtin: true,
        platform: None,
    });

    presets.push(WorkflowPreset {
        id: "customer-followup".to_string(),
        name: "고객 팔로업".to_string(),
        description: "고객 피드백 확인 후 문서와 메일을 열어 후속 조치를 준비합니다".to_string(),
        category: PresetCategory::Workflow,
        steps: vec![
            WorkflowStep {
                name: "CRM 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "CRM".to_string(),
                },
                delay_ms: 0,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "Notion 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Notion".to_string(),
                },
                delay_ms: 1000,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "Mail 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Mail".to_string(),
                },
                delay_ms: 1000,
                stop_on_failure: false,
            },
        ],
        builtin: true,
        platform: None,
    });

    presets.push(WorkflowPreset {
        id: "release-readiness".to_string(),
        name: "릴리스 준비".to_string(),
        description: "코드 저장 후 터미널과 브라우저를 열어 릴리스 체크를 시작합니다".to_string(),
        category: PresetCategory::Workflow,
        steps: vec![
            WorkflowStep {
                name: "파일 저장".to_string(),
                intent: AutomationIntent::ExecuteHotkey {
                    keys: vec![m.to_string(), "S".to_string()],
                },
                delay_ms: 0,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "Terminal 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Terminal".to_string(),
                },
                delay_ms: 500,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "Browser 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Google Chrome".to_string(),
                },
                delay_ms: 1000,
                stop_on_failure: false,
            },
        ],
        builtin: true,
        platform: None,
    });

    presets.push(WorkflowPreset {
        id: "deep-work-start".to_string(),
        name: "집중 세션 시작".to_string(),
        description: "IDE를 열고 메신저를 뒤로 보내 집중 세션을 시작합니다".to_string(),
        category: PresetCategory::Workflow,
        steps: vec![
            WorkflowStep {
                name: "VSCode 열기".to_string(),
                intent: AutomationIntent::ActivateApp {
                    app_name: "Visual Studio Code".to_string(),
                },
                delay_ms: 0,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "다음 앱 전환".to_string(),
                intent: AutomationIntent::ExecuteHotkey {
                    keys: vec![alt.to_string(), "Tab".to_string()],
                },
                delay_ms: 800,
                stop_on_failure: false,
            },
            WorkflowStep {
                name: "현재 창 닫기".to_string(),
                intent: AutomationIntent::ExecuteHotkey {
                    keys: vec![m.to_string(), "W".to_string()],
                },
                delay_ms: 500,
                stop_on_failure: false,
            },
        ],
        builtin: true,
        platform: None,
    });

    presets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_presets_load() {
        let presets = builtin_presets();
        assert_eq!(presets.len(), 15);
        assert!(presets.iter().all(|p| p.builtin));
    }

    #[test]
    fn platform_modifier_keys() {
        let m = platform_modifier();
        if cfg!(target_os = "macos") {
            assert_eq!(m, "Cmd");
        } else {
            assert_eq!(m, "Ctrl");
        }
    }

    #[test]
    fn all_presets_have_steps() {
        let presets = builtin_presets();
        for preset in &presets {
            assert!(
                !preset.steps.is_empty(),
                "프리셋 '{}'에 단계 없음",
                preset.id
            );
        }
    }

    #[test]
    fn preset_ids_unique() {
        let presets = builtin_presets();
        let ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
        let mut unique_ids = ids.clone();
        unique_ids.sort();
        unique_ids.dedup();
        assert_eq!(ids.len(), unique_ids.len(), "중복 프리셋 ID 발견");
    }

    #[test]
    fn preset_categories_coverage() {
        let presets = builtin_presets();
        let has_productivity = presets
            .iter()
            .any(|p| p.category == PresetCategory::Productivity);
        let has_app = presets
            .iter()
            .any(|p| p.category == PresetCategory::AppManagement);
        let has_workflow = presets
            .iter()
            .any(|p| p.category == PresetCategory::Workflow);
        assert!(has_productivity);
        assert!(has_app);
        assert!(has_workflow);
    }
}
