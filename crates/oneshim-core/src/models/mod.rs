//! ONESHIM 도메인 모델.
//!
//! 서버-클라이언트 간 공유하는 핵심 데이터 구조체를 정의한다.
//! 모든 모델은 `serde` Serialize/Deserialize를 구현한다.

pub mod activity;
pub mod automation;
pub mod context;
pub mod event;
pub mod frame;
pub mod intent;
pub mod session;
pub mod suggestion;
pub mod system;
pub mod telemetry;
pub mod work_session;
