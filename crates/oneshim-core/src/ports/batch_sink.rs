//! 배치 이벤트 전송 포트 — 서버 동기화 추상화

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::event::Event;

/// 이벤트를 배치로 서버에 전송하는 포트.
/// `oneshim-network::BatchUploader`가 구현체.
#[async_trait]
pub trait BatchSink: Send + Sync {
    /// 이벤트를 전송 큐에 추가
    fn enqueue(&self, event: Event);

    /// 복수 이벤트를 전송 큐에 추가
    fn enqueue_many(&self, events: Vec<Event>);

    /// 큐에 쌓인 이벤트를 서버로 플러시. 전송된 건수 반환.
    async fn flush(&self) -> Result<usize, CoreError>;
}
