//! 입력 드라이버 포트.
//!
//! 마우스/키보드 조작을 위한 크로스 플랫폼 인터페이스를 정의한다.

use async_trait::async_trait;

use crate::error::CoreError;

/// 입력 드라이버 — 마우스/키보드 시뮬레이션 인터페이스
///
/// 구현체: `EnigoInputDriver` (실제 입력), `NoOpInputDriver` (테스트용)
#[async_trait]
pub trait InputDriver: Send + Sync {
    /// 마우스 이동
    async fn mouse_move(&self, x: i32, y: i32) -> Result<(), CoreError>;

    /// 마우스 클릭
    async fn mouse_click(&self, button: &str, x: i32, y: i32) -> Result<(), CoreError>;

    /// 텍스트 입력
    async fn type_text(&self, text: &str) -> Result<(), CoreError>;

    /// 키 누름
    async fn key_press(&self, key: &str) -> Result<(), CoreError>;

    /// 키 놓음
    async fn key_release(&self, key: &str) -> Result<(), CoreError>;

    /// 단축키 (복합 키)
    async fn hotkey(&self, keys: &[String]) -> Result<(), CoreError>;

    /// 플랫폼 이름 (예: "macos", "windows", "linux")
    fn platform(&self) -> &str;
}
