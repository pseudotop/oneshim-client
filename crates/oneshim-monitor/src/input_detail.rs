//! 상세 입력 수집.
//!
//! 키스트로크 타이밍, 마우스 궤적, 클릭 좌표, 스크롤 패턴 수집.
//! PII 필터가 적용되어 키 내용은 기록하지 않고 패턴만 수집.
//! rdev crate와 통합하여 사용 (실제 리스너는 oneshim-app에서 시작).

use chrono::{DateTime, Utc};
use oneshim_core::config::PiiFilterLevel;
use serde::{Deserialize, Serialize};

// ============================================================
// 수집 설정
// ============================================================

/// 상세 입력 수집 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputCollectionConfig {
    /// 키 타이밍 패턴 수집
    #[serde(default)]
    pub collect_keystroke_timing: bool,
    /// 마우스 궤적 수집
    #[serde(default)]
    pub collect_mouse_trajectory: bool,
    /// 클릭 좌표 수집
    #[serde(default)]
    pub collect_click_coordinates: bool,
    /// 스크롤 패턴 수집
    #[serde(default)]
    pub collect_scroll_patterns: bool,
    /// 마우스 궤적 샘플링 간격 (밀리초)
    #[serde(default = "default_trajectory_sample_rate")]
    pub trajectory_sample_rate_ms: u64,
    /// PII 필터 레벨
    #[serde(default)]
    pub pii_filter_level: PiiFilterLevel,
}

impl Default for InputCollectionConfig {
    fn default() -> Self {
        Self {
            collect_keystroke_timing: false,
            collect_mouse_trajectory: false,
            collect_click_coordinates: false,
            collect_scroll_patterns: false,
            trajectory_sample_rate_ms: default_trajectory_sample_rate(),
            pii_filter_level: PiiFilterLevel::Standard,
        }
    }
}

fn default_trajectory_sample_rate() -> u64 {
    50
}

// ============================================================
// 이벤트 모델
// ============================================================

/// 키 이벤트 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyEventType {
    Press,
    Release,
}

/// 키스트로크 이벤트 (타이밍만, 내용 마스킹)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeystrokeEvent {
    /// 이벤트 시각
    pub timestamp: DateTime<Utc>,
    /// 키 코드 (PII 필터 적용 — 문자키는 "[MASKED]")
    pub key_code: String,
    /// 이벤트 유형
    pub event_type: KeyEventType,
    /// 이전 키 이벤트로부터의 경과 시간 (밀리초)
    pub interval_ms: u64,
}

/// 마우스 궤적 포인트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseTrajectoryPoint {
    /// 시각
    pub timestamp: DateTime<Utc>,
    /// X 좌표
    pub x: f64,
    /// Y 좌표
    pub y: f64,
}

/// 클릭 이벤트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickEvent {
    /// 시각
    pub timestamp: DateTime<Utc>,
    /// 버튼 (left, right, middle 등)
    pub button: String,
    /// X 좌표
    pub x: f64,
    /// Y 좌표
    pub y: f64,
}

/// 스크롤 이벤트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollEvent {
    /// 시각
    pub timestamp: DateTime<Utc>,
    /// X 방향 스크롤량
    pub delta_x: i64,
    /// Y 방향 스크롤량
    pub delta_y: i64,
}

/// 통합 입력 이벤트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetailedInputEvent {
    Keystroke(KeystrokeEvent),
    MouseMove(MouseTrajectoryPoint),
    Click(ClickEvent),
    Scroll(ScrollEvent),
}

// ============================================================
// 키 새니타이징
// ============================================================

/// 키 이름을 PII 필터 레벨에 따라 새니타이징
///
/// Off: 모든 키 이름 그대로
/// 그 외: 수정키/특수키만 기록, 문자키는 마스킹
pub fn sanitize_key_name(key_name: &str, level: PiiFilterLevel) -> String {
    match level {
        PiiFilterLevel::Off => key_name.to_string(),
        _ => {
            // 허용된 특수키/수정키 목록
            let allowed_keys = [
                "Control",
                "ControlLeft",
                "ControlRight",
                "Shift",
                "ShiftLeft",
                "ShiftRight",
                "Alt",
                "AltGr",
                "Meta",
                "MetaLeft",
                "MetaRight",
                "Tab",
                "Return",
                "Enter",
                "Escape",
                "Backspace",
                "Delete",
                "Space",
                "ArrowUp",
                "ArrowDown",
                "ArrowLeft",
                "ArrowRight",
                "Home",
                "End",
                "PageUp",
                "PageDown",
                "CapsLock",
                "F1",
                "F2",
                "F3",
                "F4",
                "F5",
                "F6",
                "F7",
                "F8",
                "F9",
                "F10",
                "F11",
                "F12",
            ];

            if allowed_keys
                .iter()
                .any(|k| k.eq_ignore_ascii_case(key_name))
            {
                key_name.to_string()
            } else {
                "[MASKED]".to_string()
            }
        }
    }
}

// ============================================================
// 이벤트 버퍼 (배치 수집용)
// ============================================================

/// 입력 이벤트 버퍼 — 설정 기반 필터링 + 배치 수집
pub struct InputEventBuffer {
    config: InputCollectionConfig,
    events: Vec<DetailedInputEvent>,
    max_buffer_size: usize,
}

impl InputEventBuffer {
    /// 새 버퍼 생성
    pub fn new(config: InputCollectionConfig, max_buffer_size: usize) -> Self {
        Self {
            config,
            events: Vec::with_capacity(max_buffer_size),
            max_buffer_size,
        }
    }

    /// 키스트로크 이벤트 추가
    pub fn push_keystroke(&mut self, key_name: &str, event_type: KeyEventType, interval_ms: u64) {
        if !self.config.collect_keystroke_timing {
            return;
        }
        if self.events.len() >= self.max_buffer_size {
            return; // 버퍼 풀
        }

        let sanitized = sanitize_key_name(key_name, self.config.pii_filter_level);
        self.events
            .push(DetailedInputEvent::Keystroke(KeystrokeEvent {
                timestamp: Utc::now(),
                key_code: sanitized,
                event_type,
                interval_ms,
            }));
    }

    /// 마우스 이동 이벤트 추가
    pub fn push_mouse_move(&mut self, x: f64, y: f64) {
        if !self.config.collect_mouse_trajectory {
            return;
        }
        if self.events.len() >= self.max_buffer_size {
            return;
        }

        self.events
            .push(DetailedInputEvent::MouseMove(MouseTrajectoryPoint {
                timestamp: Utc::now(),
                x,
                y,
            }));
    }

    /// 클릭 이벤트 추가
    pub fn push_click(&mut self, button: &str, x: f64, y: f64) {
        if !self.config.collect_click_coordinates {
            return;
        }
        if self.events.len() >= self.max_buffer_size {
            return;
        }

        self.events.push(DetailedInputEvent::Click(ClickEvent {
            timestamp: Utc::now(),
            button: button.to_string(),
            x,
            y,
        }));
    }

    /// 스크롤 이벤트 추가
    pub fn push_scroll(&mut self, delta_x: i64, delta_y: i64) {
        if !self.config.collect_scroll_patterns {
            return;
        }
        if self.events.len() >= self.max_buffer_size {
            return;
        }

        self.events.push(DetailedInputEvent::Scroll(ScrollEvent {
            timestamp: Utc::now(),
            delta_x,
            delta_y,
        }));
    }

    /// 버퍼의 모든 이벤트를 꺼내고 비우기
    pub fn drain(&mut self) -> Vec<DetailedInputEvent> {
        std::mem::take(&mut self.events)
    }

    /// 현재 버퍼 크기
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// 버퍼가 비어있는지
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_modifier_keys_allowed() {
        let result = sanitize_key_name("ControlLeft", PiiFilterLevel::Standard);
        assert_eq!(result, "ControlLeft");
    }

    #[test]
    fn sanitize_character_keys_masked() {
        let result = sanitize_key_name("KeyA", PiiFilterLevel::Standard);
        assert_eq!(result, "[MASKED]");
    }

    #[test]
    fn sanitize_off_allows_all() {
        let result = sanitize_key_name("KeyA", PiiFilterLevel::Off);
        assert_eq!(result, "KeyA");
    }

    #[test]
    fn buffer_respects_config() {
        let config = InputCollectionConfig {
            collect_keystroke_timing: true,
            collect_mouse_trajectory: false,
            ..Default::default()
        };
        let mut buffer = InputEventBuffer::new(config, 100);

        buffer.push_keystroke("Tab", KeyEventType::Press, 50);
        buffer.push_mouse_move(100.0, 200.0); // 무시됨

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn buffer_max_size() {
        let config = InputCollectionConfig {
            collect_keystroke_timing: true,
            ..Default::default()
        };
        let mut buffer = InputEventBuffer::new(config, 2);

        buffer.push_keystroke("Tab", KeyEventType::Press, 10);
        buffer.push_keystroke("Enter", KeyEventType::Press, 20);
        buffer.push_keystroke("Space", KeyEventType::Press, 30); // 버퍼 풀

        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn buffer_drain() {
        let config = InputCollectionConfig {
            collect_keystroke_timing: true,
            ..Default::default()
        };
        let mut buffer = InputEventBuffer::new(config, 100);
        buffer.push_keystroke("Tab", KeyEventType::Press, 10);

        let events = buffer.drain();
        assert_eq!(events.len(), 1);
        assert!(buffer.is_empty());
    }
}
