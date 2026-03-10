use async_trait::async_trait;

use crate::models::audit::{AuditEntry, AuditLevel, AuditStats, AuditStatus};

/// 감사 로그 포트 — oneshim-web 핸들러가 사용하는 감사 로그 인터페이스
///
/// 구현체는 내부적으로 interior mutability를 사용하여 `&self`로 mutation을 처리합니다.
/// (ADR-001 §2: 포트 트레잇은 `&self` 사용, 구현체는 `RwLock` 사용)
#[async_trait]
pub trait AuditLogPort: Send + Sync {
    // ── Query methods ──

    /// 버퍼의 대기 중인 항목 수
    async fn pending_count(&self) -> usize;

    /// 최근 항목 조회 (비파괴적, 최신순)
    async fn recent_entries(&self, limit: usize) -> Vec<AuditEntry>;

    /// 상태 기준 필터 조회
    async fn entries_by_status(&self, status: &AuditStatus, limit: usize) -> Vec<AuditEntry>;

    /// 통계 집계
    async fn stats(&self) -> AuditStats;

    /// 배치 전송 가능 여부
    async fn has_pending_batch(&self) -> bool;

    // ── Mutation methods ──

    /// 일반 이벤트 로깅
    async fn log_event(&self, action_type: &str, session_id: &str, details: &str);

    /// 조건부 시작 로깅 (AuditLevel::None이면 스킵)
    async fn log_start_if(
        &self,
        level: AuditLevel,
        command_id: &str,
        session_id: &str,
        action_type: &str,
    );

    /// 실행 시간 포함 완료 로깅 (AuditLevel::None이면 스킵)
    async fn log_complete_with_time(
        &self,
        level: AuditLevel,
        command_id: &str,
        session_id: &str,
        details: &str,
        execution_time_ms: u64,
    );

    // ── Drain methods (batch upload) ──

    /// 배치 크기만큼 드레인
    async fn drain_batch(&self) -> Vec<AuditEntry>;

    /// 전체 드레인
    async fn drain_all(&self) -> Vec<AuditEntry>;
}
