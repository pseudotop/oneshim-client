//! 사용자 컨텍스트 모델.
//!
//! 현재 활성 창, 프로세스, 마우스 위치 등 사용자의 작업 컨텍스트를 표현.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 사용자 컨텍스트 데이터 (모니터링 수집 결과)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    /// 수집 시각
    pub timestamp: DateTime<Utc>,
    /// 현재 활성 창 정보
    pub active_window: Option<WindowInfo>,
    /// 실행 중인 프로세스 목록 (상위 N개)
    pub processes: Vec<ProcessInfo>,
    /// 마우스 커서 위치
    pub mouse_position: Option<MousePosition>,
}

/// 활성 창 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    /// 창 제목
    pub title: String,
    /// 애플리케이션 이름
    pub app_name: String,
    /// 프로세스 ID
    pub pid: u32,
    /// 창 위치/크기
    pub bounds: Option<WindowBounds>,
}

/// 창 위치 및 크기
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// 프로세스 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// 프로세스 ID
    pub pid: u32,
    /// 프로세스 이름
    pub name: String,
    /// CPU 사용률 (0.0 ~ 100.0)
    pub cpu_usage: f32,
    /// 메모리 사용량 (바이트)
    pub memory_bytes: u64,
}

/// 마우스 커서 위치
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MousePosition {
    pub x: i32,
    pub y: i32,
}
