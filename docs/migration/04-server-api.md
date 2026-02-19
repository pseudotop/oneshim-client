# 4. Server API 연동 명세

[← 모듈 매핑](./03-module-mapping.md) | [마이그레이션 단계 →](./05-migration-phases.md)

---

## 클라이언트가 호출하는 엔드포인트 (31개)

### 인증 (5개) — REST 표준 경로 ⭐

> **2026-02-05 업데이트**: 리소스 중심 REST 표준 설계로 변경

| 메서드 | 경로 | 용도 |
|--------|------|------|
| POST | `/api/v1/auth/tokens` | 토큰 생성 (로그인) → access_token + refresh_token |
| DELETE | `/api/v1/auth/tokens` | 토큰 폐기 (로그아웃) |
| POST | `/api/v1/auth/tokens/refresh` | 토큰 갱신 (expires_in 전 자동) |
| GET | `/api/v1/auth/tokens/verify` | 토큰 유효성 검증 |
| DELETE | `/api/v1/auth/tokens/all` | 모든 토큰 폐기 (전체 세션 종료) |

### 세션 (6개)

| 메서드 | 경로 | 용도 |
|--------|------|------|
| POST | `/user_context/sessions/` | 세션 생성 |
| GET | `/user_context/sessions/{id}` | 세션 정보 |
| DELETE | `/user_context/sessions/{id}` | 세션 종료 |
| POST | `/user_context/sessions/{id}/heartbeat` | 세션 하트비트 ⭐ NEW |
| **GET** | **`/user_context/suggestions/stream`** | **★ SSE 스트림 (핵심!)** |
| GET | `/user_context/sessions/health` | 연결 상태 |

### 메시지 (3개)

| 메서드 | 경로 | 용도 |
|--------|------|------|
| POST | `/user_context/sessions/messages/send` | 메시지 전송 |
| POST | `/user_context/sessions/messages/broadcast` | 브로드캐스트 |
| GET | `/user_context/sessions/messages/history` | 이력 조회 |

### 제안 — Proactive Suggestion (6개) ★

| 메서드 | 경로 | 용도 |
|--------|------|------|
| POST | `/user_context/suggestions/generate` | 제안 생성 요청 |
| POST | `/user_context/suggestions/work-guidance` | 작업 가이드 |
| POST | `/user_context/suggestions/email-draft` | 이메일 초안 |
| POST | `/user_context/suggestions/feedback` | 수락/거절 피드백 |
| GET | `/user_context/suggestions/history` | 제안 이력 |
| POST | `/user_context/suggestions/apply/{id}` | 제안 적용 |

### 컨텍스트 (4개)

| 메서드 | 경로 | 용도 |
|--------|------|------|
| POST | `/user_context/contexts` | 컨텍스트 업로드 ⭐ NEW |
| POST | `/user_context/batches` | 배치 업로드 (이벤트 + 프레임 메타데이터) ⭐ NEW |
| GET | `/user_context/batches/stats` | 배치 통계 조회 ⭐ NEW |
| PUT | `/user_context/` | 컨텍스트 업데이트 (레거시) |

### 텔레메트리/동기화 (2개)

| 메서드 | 경로 | 용도 |
|--------|------|------|
| POST | `/user_context/telemetry` | 텔레메트리 업로드 |
| POST | `/user_context/sync/status` | 동기화 상태 |

### 헬스 (4개)

| 메서드 | 경로 | 용도 |
|--------|------|------|
| GET | `/health/` | 기본 헬스체크 |
| GET | `/health/readiness` | 준비 상태 |
| GET | `/health/liveness` | 생존 확인 |

## SSE 이벤트 타입 (REST fallback)

```
event: connection   → 연결 확립
event: suggestion   → ★ 제안 수신 (핵심!)
event: update       → 컨텍스트/상태 업데이트
event: heartbeat    → 연결 유지 (주기적)
event: error        → 에러 발생
event: close        → 연결 종료
```

---

## gRPC API (권장)

> **Proto 정의**: `api/proto/oneshim/v1/` — Single Source of Truth
>
> **Feature Flag**: `--features grpc` 또는 `GrpcConfig.use_grpc_*`

### AuthenticationService (포트 50052)

| RPC | 요청 | 응답 | 설명 |
|-----|------|------|------|
| `Login` | `LoginRequest` | `LoginResponse` | 로그인 |
| `Logout` | `LogoutRequest` | `LogoutResponse` | 로그아웃 |
| `RefreshToken` | `RefreshTokenRequest` | `TokenRefreshResponse` | 토큰 갱신 |
| `ValidateToken` | `ValidateTokenRequest` | `TokenValidationResponse` | 토큰 검증 |

### SessionService (포트 50052)

| RPC | 요청 | 응답 | 설명 |
|-----|------|------|------|
| `CreateSession` | `CreateSessionRequest` | `CreateSessionResponse` | 세션 생성 |
| `GetSession` | `GetSessionRequest` | `Session` | 세션 조회 |
| `EndSession` | `EndSessionRequest` | `Empty` | 세션 종료 |
| `Heartbeat` | `SessionHeartbeatRequest` | `SessionHeartbeatResponse` | 하트비트 |

### UserContextService (포트 50052) ★

| RPC | 요청 | 응답 | 설명 |
|-----|------|------|------|
| `UploadBatch` | `ContextBatchUploadRequest` | `ContextBatchUploadResponse` | 배치 업로드 |
| `SubscribeSuggestions` | `SubscribeRequest` | `stream Suggestion` | **★ Server Streaming (SSE 대체)** |
| `SendFeedback` | `SuggestionFeedback` | `Empty` | 피드백 전송 |
| `ListSuggestions` | `ListSuggestionsRequest` | `ListSuggestionsResponse` | 제안 목록 (타입 필터) |
| `Heartbeat` | `HeartbeatRequest` | `HeartbeatResponse` | 하트비트 |

### REST vs gRPC 비교

| 기능 | REST | gRPC |
|------|------|------|
| 제안 스트림 | SSE `/sessions/stream` | `SubscribeSuggestions` (Server Streaming) |
| 배치 업로드 | POST `/sync/batch` | `UploadBatch` (프레임 포함) |
| 피드백 | POST `/suggestions/feedback` | `SendFeedback` |
| 타입 안전성 | JSON 스키마 | Proto 컴파일타임 검증 |
| 페이로드 크기 | 텍스트 (100%) | 바이너리 (~70%) |

### 전환 전략

```rust
// UnifiedClient — gRPC 우선, REST fallback
let config = GrpcConfig {
    use_grpc_auth: true,     // gRPC 인증 사용
    use_grpc_context: true,  // gRPC 컨텍스트 사용
    grpc_endpoint: "http://127.0.0.1:50052".to_string(),
    grpc_fallback_ports: vec![50051, 50053],  // ⭐ 포트 폴백 (2026-02-05)
    rest_endpoint: "http://127.0.0.1:8000".to_string(),
    ..Default::default()
};

let client = UnifiedClient::new(config, token_manager);

// gRPC 실패 시:
// 1. 먼저 grpc_fallback_ports로 재시도 (50051 → 50053)
// 2. 모든 gRPC 포트 실패 시 REST fallback
let response = client.upload_batch(request).await?;
```

### gRPC 포트 폴백 전략 ⭐ NEW (2026-02-05)

```
┌──────────────────────────────────────────────────────────┐
│ 연결 시도 순서                                            │
├──────────────────────────────────────────────────────────┤
│ 1. grpc_endpoint (50052)    → 성공 시 사용               │
│ 2. fallback_ports[0] (50051) → 1 실패 시 시도            │
│ 3. fallback_ports[1] (50053) → 2 실패 시 시도            │
│ 4. REST endpoint (8000)      → 모든 gRPC 실패 시         │
└──────────────────────────────────────────────────────────┘
```

산업 현장 환경에서 HTTP/2 차단 또는 특정 포트 사용 불가 시 자동으로 대체 포트를 시도합니다.
