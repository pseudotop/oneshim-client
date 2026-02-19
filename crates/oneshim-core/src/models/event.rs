//! 이벤트 모델.
//!
//! 사용자 이벤트, 시스템 이벤트, 배치 전송을 위한 이벤트 묶음을 정의.
//! Phase 35: 마우스/키보드/프로세스/창 이벤트 추가 (서버 패턴 분석용)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 클라이언트에서 발생하는 모든 이벤트의 통합 enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    /// 사용자 행동 이벤트 (창 전환, 클릭 등)
    User(UserEvent),
    /// 시스템 상태 이벤트 (메트릭 변화, 경고 등)
    System(SystemEvent),
    /// 컨텍스트 변경 이벤트 (활성 앱 전환, 포커스 변경 등)
    Context(ContextEvent),
    /// 입력 활동 이벤트 (마우스/키보드 패턴)
    Input(InputActivityEvent),
    /// 프로세스 스냅샷 (상세 정보 포함)
    Process(ProcessSnapshotEvent),
    /// 창 레이아웃 이벤트 (크기/위치 변경)
    Window(WindowLayoutEvent),
}

/// 사용자 행동 이벤트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserEvent {
    pub event_id: Uuid,
    pub event_type: UserEventType,
    pub timestamp: DateTime<Utc>,
    /// 이벤트 발생 시 활성 앱
    pub app_name: String,
    /// 이벤트 발생 시 창 제목
    pub window_title: String,
}

/// 사용자 이벤트 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UserEventType {
    /// 활성 창 변경
    WindowChange,
    /// 앱 전환 (IDE → 브라우저 등)
    AppSwitch,
    /// 유의미한 액션 (더블클릭, 우클릭 등)
    SignificantAction,
    /// 폼 제출 (Enter + form 컨텍스트)
    FormSubmission,
}

/// 시스템 상태 이벤트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    pub event_id: Uuid,
    pub event_type: SystemEventType,
    pub timestamp: DateTime<Utc>,
    /// 이벤트 상세 데이터 (JSON)
    pub data: serde_json::Value,
}

/// 시스템 이벤트 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SystemEventType {
    /// 시스템 메트릭 업데이트
    MetricsUpdate,
    /// 시스템 경고 발생
    Alert,
    /// 네트워크 상태 변경
    NetworkChange,
}

/// 컨텍스트 변경 이벤트 (캡처 트리거 판단 입력)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextEvent {
    /// 현재 활성 앱 이름
    pub app_name: String,
    /// 현재 창 제목
    pub window_title: String,
    /// 이전 활성 앱 이름 (전환 감지용)
    pub prev_app_name: Option<String>,
    /// 이벤트 시각
    pub timestamp: DateTime<Utc>,
}

/// 서버 전송용 이벤트 배치
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBatch {
    /// 세션 ID
    pub session_id: String,
    /// 배치에 포함된 이벤트 목록
    pub events: Vec<Event>,
    /// 배치 생성 시각
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Phase 35: 입력 활동 이벤트 (마우스/키보드 패턴)
// ============================================================================

/// 입력 활동 이벤트 — 마우스/키보드 패턴 (내용 제외, 패턴만)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputActivityEvent {
    /// 이벤트 발생 시각
    pub timestamp: DateTime<Utc>,
    /// 집계 기간 (초)
    pub period_secs: u32,
    /// 마우스 활동
    pub mouse: MouseActivity,
    /// 키보드 활동
    pub keyboard: KeyboardActivity,
    /// 이벤트 발생 시 활성 앱
    pub app_name: String,
}

/// 마우스 활동 패턴
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MouseActivity {
    /// 클릭 횟수
    pub click_count: u32,
    /// 이동 거리 (픽셀, 상대값)
    pub move_distance: f64,
    /// 스크롤 횟수
    pub scroll_count: u32,
    /// 마지막 위치 (화면 비율 0.0-1.0, 익명화)
    pub last_position: Option<(f32, f32)>,
    /// 더블클릭 횟수
    pub double_click_count: u32,
    /// 우클릭 횟수
    pub right_click_count: u32,
}

/// 키보드 활동 패턴 (키 내용 제외)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeyboardActivity {
    /// 분당 키 입력 수 (타이핑 속도)
    pub keystrokes_per_min: u32,
    /// 총 키 입력 수
    pub total_keystrokes: u32,
    /// 연속 타이핑 버스트 횟수
    pub typing_bursts: u32,
    /// 단축키 사용 횟수 (Cmd/Ctrl + 키)
    pub shortcut_count: u32,
    /// 백스페이스/삭제 키 횟수 (수정 빈도)
    pub correction_count: u32,
}

// ============================================================================
// Phase 35: 프로세스 스냅샷 이벤트
// ============================================================================

/// 프로세스 스냅샷 이벤트 — 상세 프로세스 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSnapshotEvent {
    /// 스냅샷 시각
    pub timestamp: DateTime<Utc>,
    /// 프로세스 목록
    pub processes: Vec<ProcessDetail>,
    /// 총 프로세스 수
    pub total_process_count: u32,
}

/// 프로세스 상세 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessDetail {
    /// 프로세스 이름
    pub name: String,
    /// PID
    pub pid: u32,
    /// CPU 사용률 (%)
    pub cpu_percent: f32,
    /// 메모리 사용량 (MB)
    pub memory_mb: f64,
    /// 창 개수 (GUI 프로세스인 경우)
    pub window_count: u32,
    /// 포그라운드 여부
    pub is_foreground: bool,
    /// 실행 시간 (초)
    pub running_secs: u64,
    /// 실행 파일 경로 (익명화된 상대 경로)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
}

// ============================================================================
// Phase 35: 창 레이아웃 이벤트
// ============================================================================

/// 창 레이아웃 이벤트 — 창 크기/위치 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowLayoutEvent {
    /// 이벤트 시각
    pub timestamp: DateTime<Utc>,
    /// 이벤트 유형
    pub event_type: WindowLayoutEventType,
    /// 창 정보
    pub window: WindowInfo,
    /// 전체 화면 해상도
    pub screen_resolution: (u32, u32),
    /// 모니터 번호 (멀티모니터)
    pub monitor_index: u32,
}

/// 창 레이아웃 이벤트 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WindowLayoutEventType {
    /// 창 포커스 획득
    Focus,
    /// 창 크기 변경
    Resize,
    /// 창 이동
    Move,
    /// 창 최대화
    Maximize,
    /// 창 최소화
    Minimize,
    /// 창 복원
    Restore,
}

/// 창 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    /// 앱 이름
    pub app_name: String,
    /// 창 제목
    pub window_title: String,
    /// 창 위치 (x, y)
    pub position: (i32, i32),
    /// 창 크기 (width, height)
    pub size: (u32, u32),
    /// 화면 대비 비율 (0.0-1.0)
    pub screen_ratio: f32,
    /// 전체화면 여부
    pub is_fullscreen: bool,
    /// Z-order (0이 최상위)
    pub z_order: u32,
}
