[English](./grpc-client.md) | [한국어](./grpc-client.ko.md)

# Rust gRPC 클라이언트 가이드

> **작성일**: 2026-02-04
> **Phase**: 36 (gRPC 클라이언트)
> **관련 문서**: [oneshim-network 크레이트](../crates/oneshim-network.md)
> **거버넌스**: [gRPC 거버넌스 가이드](./grpc-governance.ko.md)
> **호환성 매트릭스**: [gRPC 호환성 매트릭스](./grpc-compatibility-matrix.ko.md)

## 개요

ONESHIM Rust 클라이언트는 **tonic + prost** 기반 gRPC 클라이언트를 제공합니다. Feature Flag를 통해 gRPC와 REST를 선택적으로 사용할 수 있으며, gRPC 실패 시 자동으로 REST로 폴백됩니다.

## 빠른 시작

### 1. Feature Flag 활성화

```bash
# gRPC 기능 포함 빌드
cargo build -p oneshim-app --features grpc

# 또는 oneshim-network만 빌드
cargo build -p oneshim-network --features grpc
```

### 2. 기본 사용법

```rust
use oneshim_network::grpc::{GrpcConfig, UnifiedClient};
use oneshim_network::TokenManager;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 토큰 매니저 생성
    let token_manager = Arc::new(TokenManager::new(
        "http://localhost:8000",
        "user@example.com",
        "password",
    ));

    // gRPC 설정
    let config = GrpcConfig {
        use_grpc_auth: true,
        use_grpc_context: true,
        grpc_endpoint: "http://localhost:50052".to_string(),
        rest_endpoint: "http://localhost:8000".to_string(),
        ..Default::default()
    };

    // UnifiedClient 생성 (gRPC + REST 통합)
    let client = UnifiedClient::new(config, token_manager);

    // 로그인
    let login_response = client.login("user@example.com", "password", None).await?;
    println!("로그인 성공: {}", login_response.user_id);

    Ok(())
}
```

## 설정

### GrpcConfig

```rust
/// gRPC 클라이언트 설정
#[derive(Debug, Clone)]
pub struct GrpcConfig {
    /// gRPC 인증 사용 여부 (Login, Logout, RefreshToken, ValidateToken)
    pub use_grpc_auth: bool,

    /// gRPC 컨텍스트 사용 여부 (UploadBatch, SubscribeSuggestions, etc.)
    pub use_grpc_context: bool,

    /// gRPC 서버 엔드포인트
    pub grpc_endpoint: String,

    /// REST API 엔드포인트 (폴백용)
    pub rest_endpoint: String,

    /// 연결 타임아웃 (초)
    pub connect_timeout_secs: u64,

    /// 요청 타임아웃 (초)
    pub request_timeout_secs: u64,

    /// TLS 사용 여부
    pub use_tls: bool,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            use_grpc_auth: false,      // 기본값: REST 사용
            use_grpc_context: false,   // 기본값: REST 사용
            grpc_endpoint: "http://127.0.0.1:50052".to_string(),
            rest_endpoint: "http://127.0.0.1:8000".to_string(),
            connect_timeout_secs: 10,
            request_timeout_secs: 30,
            use_tls: false,
        }
    }
}
```

### 환경 변수 설정

```bash
# gRPC 활성화
export GRPC_ENABLED=true
export GRPC_ENDPOINT=http://localhost:50052

# REST 폴백 엔드포인트
export REST_ENDPOINT=http://localhost:8000

# 산업 현장용 ASCII 출력 (이모지 비활성화)
export NO_EMOJI=1
```

## 클라이언트 모듈

### GrpcAuthClient — 인증 서비스

```rust
use oneshim_network::grpc::GrpcAuthClient;

let auth_client = GrpcAuthClient::new(&config).await?;

// 로그인
let response = auth_client.login(
    "user@example.com",
    "password123",
    Some("org-id"),
).await?;

// 토큰 갱신
let response = auth_client.refresh_token(&refresh_token).await?;

// 토큰 검증
let validation = auth_client.validate_token(&access_token).await?;

// 로그아웃
auth_client.logout(&session_id).await?;
```

### GrpcSessionClient — 세션 서비스

```rust
use oneshim_network::grpc::GrpcSessionClient;

let session_client = GrpcSessionClient::new(&config, token_manager).await?;

// 세션 생성
let session = session_client.create_session(
    "client-123",
    DeviceInfo { os: "macOS".into(), ..Default::default() },
).await?;

// 하트비트
session_client.heartbeat(&session.id, ClientStatus::Active).await?;

// 세션 종료
session_client.end_session(&session.id).await?;
```

### GrpcContextClient — 컨텍스트 서비스

```rust
use oneshim_network::grpc::GrpcContextClient;

let context_client = GrpcContextClient::new(&config, token_manager).await?;

// 배치 업로드
let response = context_client.upload_batch(
    ContextBatchUploadRequest {
        session_id: session.id.clone(),
        events: vec![event1, event2],
        frames: vec![frame1],
        client_stats: Some(stats),
    },
).await?;

// 제안 피드백
context_client.send_feedback(
    SuggestionFeedback {
        suggestion_id: "sugg-123".into(),
        accepted: true,
        ..Default::default()
    },
).await?;

// 제안 목록 조회
let suggestions = context_client.list_suggestions(
    ListSuggestionsRequest {
        session_id: session.id.clone(),
        suggestion_type: Some(SuggestionType::WorkGuidance),
        limit: 10,
        ..Default::default()
    },
).await?;
```

### Server Streaming — 실시간 제안 구독

```rust
use oneshim_network::grpc::GrpcContextClient;
use futures::StreamExt;

// 제안 스트림 구독
let mut stream = context_client.subscribe_suggestions(
    &session_id,
    &client_id,
).await?;

// 제안 수신 루프
while let Some(result) = stream.next().await {
    match result {
        Ok(suggestion) => {
            println!("제안 수신: {:?}", suggestion);
            // 제안 처리
            handle_suggestion(suggestion).await;
        }
        Err(e) => {
            eprintln!("스트림 에러: {}", e);
            // 재연결 로직
            break;
        }
    }
}
```

### GrpcHealthClient — 서버 상태 확인

```rust
use oneshim_network::grpc::{GrpcHealthClient, ServingStatus};

// Health Check 클라이언트 연결
let mut health = GrpcHealthClient::connect(config).await?;

// 전체 서버 상태 확인
let status = health.check("").await?;
match status {
    ServingStatus::Serving => println!("서버 정상"),
    ServingStatus::NotServing => println!("서버 중지됨"),
    _ => println!("상태 알 수 없음"),
}

// 서버 정상 여부 확인 (간편 메서드)
if health.is_healthy().await {
    println!("서버 준비 완료");
}

// 특정 서비스 상태 확인
let auth_status = health.check("oneshim.v1.auth.AuthenticationService").await?;
println!("인증 서비스: {}", auth_status);

// 모든 ONESHIM 서비스 상태 확인
let statuses = health.check_oneshim_services().await;
for s in statuses {
    println!("{}: {}", s.service, s.status);
}
// 출력 예시:
// <server>: SERVING
// oneshim.v1.auth.AuthenticationService: SERVING
// oneshim.v1.auth.SessionService: SERVING
// oneshim.v1.user_context.UserContextService: SERVING
```

### 연결 전 Health Check 패턴

```rust
use oneshim_network::grpc::{GrpcHealthClient, GrpcConfig, UnifiedClient};

async fn create_client_with_health_check(
    config: GrpcConfig,
    token_manager: Arc<TokenManager>,
) -> Result<UnifiedClient, CoreError> {
    // 1. Health Check로 서버 상태 확인
    match GrpcHealthClient::connect(config.clone()).await {
        Ok(mut health) => {
            if health.is_healthy().await {
                info!("gRPC 서버 정상, gRPC 사용");
            } else {
                warn!("gRPC 서버 NOT_SERVING, REST 폴백");
            }
        }
        Err(e) => {
            warn!("gRPC 연결 불가 ({}), REST 사용", e);
        }
    }

    // 2. UnifiedClient 생성 (자동 폴백 지원)
    Ok(UnifiedClient::new(config, token_manager))
}
```

## UnifiedClient — 통합 클라이언트

### REST 폴백 메커니즘

`UnifiedClient`는 gRPC 실패 시 자동으로 REST API로 폴백합니다.

```rust
use oneshim_network::grpc::UnifiedClient;

let client = UnifiedClient::new(config, token_manager);

// gRPC 우선 시도, 실패 시 REST 폴백
let response = client.upload_batch(request).await?;

// 내부 동작:
// 1. use_grpc_context == true → gRPC 시도
// 2. gRPC 실패 (연결 오류, 타임아웃 등) → REST API 호출
// 3. REST도 실패 → 에러 반환
```

### 폴백 시나리오

| 상황 | gRPC | REST | 결과 |
|------|------|------|------|
| 정상 | ✅ | - | gRPC 응답 |
| gRPC 연결 실패 | ❌ | ✅ | REST 응답 |
| 둘 다 실패 | ❌ | ❌ | 에러 반환 |
| 산업 현장 (HTTP/2 차단) | ❌ | ✅ | REST 응답 |

### REST 미지원 기능

일부 기능은 REST fallback을 지원하지 않습니다:

```rust
// 배치 업로드 — 프레임 데이터는 REST 미지원
let response = client.upload_batch(request).await;
// gRPC 실패 시: 이벤트만 REST로 전송, 프레임은 경고 로그

// 제안 목록 — REST 미지원
let suggestions = client.list_suggestions(request).await;
// gRPC 실패 시: 빈 목록 반환 + 경고 로그
```

## 에러 처리

### CoreError 매핑

```rust
use oneshim_core::error::CoreError;

match client.login(email, password, org_id).await {
    Ok(response) => {
        // 성공
    }
    Err(CoreError::Network(msg)) => {
        // 네트워크 연결 오류
        eprintln!("네트워크 오류: {}", msg);
    }
    Err(CoreError::RateLimit { retry_after }) => {
        // 429 Too Many Requests
        if let Some(duration) = retry_after {
            tokio::time::sleep(duration).await;
        }
    }
    Err(CoreError::ServiceUnavailable) => {
        // 503 Service Unavailable
        // 백오프 후 재시도
    }
    Err(e) => {
        eprintln!("기타 오류: {}", e);
    }
}
```

### gRPC Status → CoreError 매핑

| gRPC Status | CoreError |
|-------------|-----------|
| `UNAVAILABLE` | `ServiceUnavailable` |
| `DEADLINE_EXCEEDED` | `Network("timeout")` |
| `UNAUTHENTICATED` | `Unauthorized` |
| `PERMISSION_DENIED` | `Forbidden` |
| `NOT_FOUND` | `NotFound` |
| `RESOURCE_EXHAUSTED` | `RateLimit` |

## 재시도 로직

### 자동 재시도

`UnifiedClient`는 특정 에러에 대해 자동 재시도를 수행합니다:

```rust
// 내부 재시도 로직
async fn execute_with_retry<F, T>(&self, operation: F) -> Result<T, CoreError>
where
    F: Fn() -> Pin<Box<dyn Future<Output = Result<T, CoreError>>>>,
{
    let mut delay = Duration::from_secs(1);
    let max_delay = Duration::from_secs(30);
    let max_retries = 3;

    for attempt in 0..max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(CoreError::Network(_)) |
            Err(CoreError::RateLimit { .. }) |
            Err(CoreError::ServiceUnavailable) => {
                if attempt < max_retries - 1 {
                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay * 2, max_delay);
                }
            }
            Err(e) => return Err(e),
        }
    }

    Err(CoreError::Network("Max retries exceeded".into()))
}
```

## 빌드 및 테스트

### Feature Flag 빌드

```bash
# gRPC 포함 빌드
cargo build --features grpc

# gRPC 없이 빌드 (REST만)
cargo build

# 테스트 실행
cargo test --features grpc
```

### Proto 코드 재생성

```bash
# api 디렉토리에서
cd api
./scripts/generate.sh

# Rust 코드는 build.rs에서 자동 생성
cargo build --features grpc
```

### Mock 서버 테스트

```bash
# 서버 측 Mock 서버 실행
uv run python scripts/run_grpc_server.py

# 클라이언트 테스트
cargo test --features grpc -- --test-threads=1
```

## 산업 현장 지원

### HTTP/2 차단 환경

일부 산업 현장에서는 HTTP/2가 차단될 수 있습니다:

```rust
// 자동 REST 폴백 활성화
let config = GrpcConfig {
    use_grpc_auth: true,
    use_grpc_context: true,
    ..Default::default()
};

let client = UnifiedClient::new(config, token_manager);

// HTTP/2 차단 시 자동으로 REST 사용
let response = client.upload_batch(request).await?;
```

### ASCII 출력 모드

```bash
# 이모지 비활성화 (산업 터미널 호환)
NO_EMOJI=1 cargo run -p oneshim-app --features grpc
```

## 참조

- Proto 정의 — `api/proto/oneshim/v1/` (서버 저장소 참조)
- [Server API 명세](../migration/04-server-api.md) — REST + gRPC 엔드포인트
- [마이그레이션 단계](../migration/05-migration-phases.md) — Phase 36
- [tonic 문서](https://github.com/hyperium/tonic)
- [prost 문서](https://github.com/tokio-rs/prost)

---

_마지막 업데이트: 2026-02-04_
