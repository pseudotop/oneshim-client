# ADR-001: Rust Client 아키텍처 패턴

**상태**: 승인됨
**날짜**: 2026-01-28
**범위**: client-rust/ 전체

---

## 컨텍스트

ONESHIM 서버는 DDD + Hexagonal Architecture를 ADR로 엄격히 통제한다.
Rust 클라이언트도 동일 수준의 아키텍처 일관성이 필요하지만, Rust 컴파일러가 이미 강제하는 부분(crate 경계, trait 구현 필수)이 있으므로 **컴파일러가 잡지 못하는 설계 결정만 명시적으로 규정**한다.

## 결정 사항

### 1. 에러 타입 전략

**규칙**: 라이브러리 crate는 `thiserror`, 바이너리 crate는 `anyhow`

```
oneshim-core      → CoreError (thiserror)     ← 다른 crate가 #[from]으로 래핑
oneshim-monitor   → MonitorError (thiserror)
oneshim-vision    → VisionError (thiserror)
oneshim-network   → NetworkError (thiserror)
oneshim-storage   → StorageError (thiserror)
oneshim-suggestion → SuggestionError (thiserror)
oneshim-ui        → UiError (thiserror)
oneshim-app       → anyhow::Result            ← 최상위에서만 사용
```

**패턴**:
```rust
// 라이브러리 crate — 구체적 에러
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("HTTP 요청 실패: {0}")]
    Http(#[from] reqwest::Error),
    #[error("SSE 연결 에러: {0}")]
    Sse(String),
    #[error("{0}")]
    Core(#[from] oneshim_core::error::CoreError),
}

// 바이너리 crate — anyhow로 통합
fn main() -> anyhow::Result<()> { ... }
```

**근거**: `thiserror`는 호출자가 에러를 패턴 매칭할 수 있어 라이브러리에 적합. `anyhow`는 "그냥 실패했다"를 표현하기 좋아 최종 바이너리에 적합.

### 2. 비동기 Trait 패턴 (Port 인터페이스)

**규칙**: `async_trait` 매크로 사용 (object safety 보장)

```rust
use async_trait::async_trait;

#[async_trait]
pub trait ApiClient: Send + Sync {
    async fn post(&self, path: &str, body: &[u8]) -> Result<Vec<u8>, CoreError>;
}
```

**근거**: Rust 1.75에서 `async fn in trait`이 안정화되었지만, `dyn Trait`으로 사용 시 object safety가 보장되지 않는다. DI 패턴(`Arc<dyn T>`)에 필수적인 `async_trait`을 일관 적용한다.

**적용 범위**: `oneshim-core/src/ports/` 내 모든 trait에 `#[async_trait]` 적용.

### 3. 의존성 주입 (DI) 패턴

**규칙**: 생성자 주입 + `Arc<dyn PortTrait>`

```rust
pub struct SuggestionReceiver {
    api_client: Arc<dyn ApiClient>,
    notifier: Arc<dyn DesktopNotifier>,
    storage: Arc<dyn StorageService>,
}

impl SuggestionReceiver {
    pub fn new(
        api_client: Arc<dyn ApiClient>,
        notifier: Arc<dyn DesktopNotifier>,
        storage: Arc<dyn StorageService>,
    ) -> Self {
        Self { api_client, notifier, storage }
    }
}
```

**와이어링 위치**: `oneshim-app/src/main.rs` (또는 `app.rs`)에서 수동 와이어링. DI 프레임워크 미사용.

**근거**: Rust 생태계에는 Spring/Guice 같은 DI 프레임워크가 필요 없다. 생성자 주입은 컴파일 타임에 검증되며, 테스트 시 mock 주입이 용이하다.

### 4. 모듈 가시성 규칙

| 가시성 | 사용 위치 | 예시 |
|--------|----------|------|
| `pub` | crate 외부에 노출하는 타입/trait | 모든 모델, 포트 trait, 에러 타입 |
| `pub(crate)` | crate 내부에서만 사용하는 헬퍼 | 유틸리티 함수, 내부 상수 |
| private | 모듈 내부 구현 | 파서, 변환 로직 |

**규칙**:
- `oneshim-core`의 `models/`, `ports/`, `error.rs`, `config.rs`는 모두 `pub`
- 어댑터 crate의 구현체는 `pub struct`이지만 내부 필드는 private
- `pub(crate)` 사용 시 반드시 이유를 주석으로 명시

### 5. 테스트 + Mock 전략

**규칙**: trait 기반 수동 mock (mockall 미사용)

```rust
// 테스트용 mock — 각 crate의 tests/ 또는 #[cfg(test)] mod에 정의
#[cfg(test)]
pub(crate) struct MockStorageService {
    pub events: std::sync::Mutex<Vec<Event>>,
}

#[cfg(test)]
#[async_trait]
impl StorageService for MockStorageService {
    async fn save_event(&self, event: &Event) -> Result<(), CoreError> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}
```

**근거**: `mockall`은 proc macro 오버헤드가 크고, 단순 trait mock은 수동 구현이 더 명확하다. trait 수가 적으므로(<10개) 수동 관리 가능.

**테스트 범위**:
- `oneshim-core`: 모델 serde 직렬화/역직렬화
- 어댑터 crate: port trait mock 주입 후 로직 테스트
- `oneshim-app`: 통합 테스트 (`tests/` 디렉토리)

### 6. Crate 의존성 방향 (불변)

```
oneshim-core  ←  oneshim-monitor
              ←  oneshim-vision
              ←  oneshim-network
              ←  oneshim-storage
              ←  oneshim-suggestion  ←  oneshim-network
              ←  oneshim-ui          ←  oneshim-suggestion
              ←  oneshim-app         ←  (모두)
```

**금지**: 어댑터 crate 간 직접 의존 (oneshim-monitor → oneshim-storage 등). 모든 cross-crate 통신은 `oneshim-core`의 trait을 통해서만.

**예외**: `oneshim-suggestion → oneshim-network` (SSE 수신 필요), `oneshim-ui → oneshim-suggestion` (제안 표시 필요)

---

## 서버 ADR과의 대응 관계

| 서버 ADR | Rust 클라이언트 대응 | 비고 |
|---------|-------------------|------|
| ADR-004 Hexagonal Architecture | Crate 경계 = Layer 경계 | 컴파일러가 강제 |
| ADR-010 Application Layer Structure | `oneshim-app` = orchestration | 수동 와이어링 |
| ADR-034 Selective DI | `Arc<dyn T>` 생성자 주입 | 본 ADR §3 |
| ADR-037 Event Sourcing + Hexagonal | 해당 없음 (클라이언트는 이벤트 소싱 미사용) | — |
| Port Patterns | `oneshim-core/src/ports/` | 본 ADR §2 |

---

## 결과

- Phase 1부터 모든 코드가 이 패턴을 따름
- `oneshim-core`에 구현된 trait/model이 계약(contract) 역할
- 새 crate 추가 시 이 ADR 참조 필수
