//! 로컬 저장소 포트.
//!
//! 구현: `oneshim-storage` crate (rusqlite)

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::CoreError;
use crate::models::activity::{IdlePeriod, ProcessSnapshot, SessionStats};
use crate::models::event::Event;
use crate::models::system::SystemMetrics;

/// 로컬 이벤트 저장소
#[async_trait]
pub trait StorageService: Send + Sync {
    /// 이벤트 저장
    async fn save_event(&self, event: &Event) -> Result<(), CoreError>;

    /// 시간 범위로 이벤트 조회
    async fn get_events(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<Event>, CoreError>;

    /// 미전송 이벤트 조회 (배치 업로드용)
    async fn get_pending_events(&self, limit: usize) -> Result<Vec<Event>, CoreError>;

    /// 이벤트를 전송 완료로 마킹
    async fn mark_as_sent(&self, event_ids: &[String]) -> Result<(), CoreError>;

    /// 보존 정책 적용 (오래된 이벤트 삭제)
    async fn enforce_retention(&self) -> Result<usize, CoreError>;
}

/// 메트릭 저장소 포트
///
/// 시스템 메트릭, 프로세스 스냅샷, 유휴 기간, 세션 통계 저장
#[async_trait]
pub trait MetricsStorage: Send + Sync {
    // ============================================================
    // 시스템 메트릭
    // ============================================================

    /// 시스템 메트릭 저장
    async fn save_metrics(&self, metrics: &SystemMetrics) -> Result<(), CoreError>;

    /// 시간 범위로 시스템 메트릭 조회
    async fn get_metrics(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<SystemMetrics>, CoreError>;

    /// 시간별 집계 생성 (해당 시간의 메트릭을 집계)
    async fn aggregate_hourly_metrics(&self, hour: DateTime<Utc>) -> Result<(), CoreError>;

    /// 오래된 상세 메트릭 삭제 (보존 기간 초과)
    async fn cleanup_old_metrics(&self, before: DateTime<Utc>) -> Result<usize, CoreError>;

    // ============================================================
    // 프로세스 스냅샷
    // ============================================================

    /// 프로세스 스냅샷 저장
    async fn save_process_snapshot(&self, snapshot: &ProcessSnapshot) -> Result<(), CoreError>;

    /// 시간 범위로 프로세스 스냅샷 조회
    async fn get_process_snapshots(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<ProcessSnapshot>, CoreError>;

    /// 오래된 프로세스 스냅샷 삭제
    async fn cleanup_old_process_snapshots(
        &self,
        before: DateTime<Utc>,
    ) -> Result<usize, CoreError>;

    // ============================================================
    // 유휴 기간
    // ============================================================

    /// 유휴 기간 시작 기록
    async fn start_idle_period(&self, start_time: DateTime<Utc>) -> Result<i64, CoreError>;

    /// 유휴 기간 종료 기록
    async fn end_idle_period(&self, id: i64, end_time: DateTime<Utc>) -> Result<(), CoreError>;

    /// 진행 중인 유휴 기간 조회
    async fn get_ongoing_idle_period(&self) -> Result<Option<(i64, IdlePeriod)>, CoreError>;

    /// 시간 범위로 유휴 기간 조회
    async fn get_idle_periods(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<IdlePeriod>, CoreError>;

    /// 오래된 유휴 기간 삭제
    async fn cleanup_old_idle_periods(&self, before: DateTime<Utc>) -> Result<usize, CoreError>;

    // ============================================================
    // 세션 통계
    // ============================================================

    /// 세션 시작 또는 업데이트
    async fn upsert_session(&self, stats: &SessionStats) -> Result<(), CoreError>;

    /// 세션 통계 조회
    async fn get_session(&self, session_id: &str) -> Result<Option<SessionStats>, CoreError>;

    /// 세션 종료 기록
    async fn end_session(&self, session_id: &str, ended_at: DateTime<Utc>)
        -> Result<(), CoreError>;

    /// 세션 통계 증가 (이벤트/프레임/유휴 시간)
    async fn increment_session_counters(
        &self,
        session_id: &str,
        events: u64,
        frames: u64,
        idle_secs: u64,
    ) -> Result<(), CoreError>;
}
