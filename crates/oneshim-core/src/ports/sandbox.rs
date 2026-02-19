//! 크로스 플랫폼 샌드박스 포트.
//!
//! OS 네이티브 커널 기능을 활용한 격리 실행 환경을 정의한다.
//! Linux(Landlock+seccomp), macOS(Seatbelt), Windows(Job Objects)
//! 각 플랫폼 어댑터가 이 trait을 구현한다.

use async_trait::async_trait;

use crate::config::SandboxConfig;
use crate::error::CoreError;
use crate::models::automation::AutomationAction;

/// 샌드박스 기능 정보 (디버깅/로깅용)
#[derive(Debug, Clone)]
pub struct SandboxCapabilities {
    /// 파일시스템 격리 지원
    pub filesystem_isolation: bool,
    /// 시스콜 필터링 지원
    pub syscall_filtering: bool,
    /// 네트워크 격리 지원
    pub network_isolation: bool,
    /// 리소스 제한 지원
    pub resource_limits: bool,
    /// 프로세스 격리 지원
    pub process_isolation: bool,
}

/// 크로스 플랫폼 샌드박스 포트
#[async_trait]
pub trait Sandbox: Send + Sync {
    /// 플랫폼 이름 (linux, macos, windows, noop)
    fn platform(&self) -> &str;

    /// 현재 플랫폼에서 샌드박스 사용 가능 여부
    fn is_available(&self) -> bool;

    /// 샌드박스 환경에서 자동화 액션 실행
    async fn execute_sandboxed(
        &self,
        action: &AutomationAction,
        config: &SandboxConfig,
    ) -> Result<(), CoreError>;

    /// 샌드박스 기능 정보 (디버깅/로깅용)
    fn capabilities(&self) -> SandboxCapabilities;
}
