//! # oneshim-suggestion
//!
//! 제안 파이프라인.
//! SSE 이벤트에서 제안을 수신하고, UI/트레이 알림으로 변환하며
//! 수락/거절 피드백을 서버에 전송한다.
//! 로컬 제안 큐(우선순위)와 이력 캐시를 관리한다.

pub mod feedback;
pub mod history;
pub mod presenter;
pub mod queue;
pub mod receiver;
