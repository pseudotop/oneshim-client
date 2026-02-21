[English](./04-server-api.md) | [한국어](./04-server-api.ko.md)

# 4. Server API Integration Specification

[← Module Mapping](./03-module-mapping.md) | [Migration Phases →](./05-migration-phases.md)

---

## Client-Invoked Endpoints (31)

### Authentication (5) — REST Standard Routes ⭐

> **Updated 2026-02-05**: Changed to resource-centric REST standard design

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/api/v1/auth/tokens` | Token creation (login) → access_token + refresh_token |
| DELETE | `/api/v1/auth/tokens` | Token revocation (logout) |
| POST | `/api/v1/auth/tokens/refresh` | Token refresh (automatic before expires_in) |
| GET | `/api/v1/auth/tokens/verify` | Token validation |
| DELETE | `/api/v1/auth/tokens/all` | Revoke all tokens (terminate all sessions) |

### Sessions (6)

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/user_context/sessions/` | Create session |
| GET | `/user_context/sessions/{id}` | Session info |
| DELETE | `/user_context/sessions/{id}` | End session |
| POST | `/user_context/sessions/{id}/heartbeat` | Session heartbeat ⭐ NEW |
| **GET** | **`/user_context/suggestions/stream`** | **★ SSE Stream (core!)** |
| GET | `/user_context/sessions/health` | Connection status |

### Messages (3)

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/user_context/sessions/messages/send` | Send message |
| POST | `/user_context/sessions/messages/broadcast` | Broadcast |
| GET | `/user_context/sessions/messages/history` | Message history |

### Suggestions — Proactive Suggestion (6) ★

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/user_context/suggestions/generate` | Request suggestion generation |
| POST | `/user_context/suggestions/work-guidance` | Work guidance |
| POST | `/user_context/suggestions/email-draft` | Email draft |
| POST | `/user_context/suggestions/feedback` | Accept/reject feedback |
| GET | `/user_context/suggestions/history` | Suggestion history |
| POST | `/user_context/suggestions/apply/{id}` | Apply suggestion |

### Context (4)

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/user_context/contexts` | Context upload ⭐ NEW |
| POST | `/user_context/batches` | Batch upload (events + frame metadata) ⭐ NEW |
| GET | `/user_context/batches/stats` | Batch statistics ⭐ NEW |
| PUT | `/user_context/` | Context update (legacy) |

### Telemetry/Sync (2)

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/user_context/telemetry` | Telemetry upload |
| POST | `/user_context/sync/status` | Sync status |

### Health (4)

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health/` | Basic health check |
| GET | `/health/readiness` | Readiness probe |
| GET | `/health/liveness` | Liveness probe |

## SSE Event Types (REST fallback)

```
event: connection   → Connection established
event: suggestion   → ★ Suggestion received (core!)
event: update       → Context/status update
event: heartbeat    → Keep-alive (periodic)
event: error        → Error occurred
event: close        → Connection closed
```

---

## gRPC API (Recommended)

> **Proto definitions**: `api/proto/oneshim/v1/` — Single Source of Truth
>
> **Feature Flag**: `--features grpc` or `GrpcConfig.use_grpc_*`

### AuthenticationService (Port 50052)

| RPC | Request | Response | Description |
|-----|---------|----------|-------------|
| `Login` | `LoginRequest` | `LoginResponse` | Login |
| `Logout` | `LogoutRequest` | `LogoutResponse` | Logout |
| `RefreshToken` | `RefreshTokenRequest` | `TokenRefreshResponse` | Token refresh |
| `ValidateToken` | `ValidateTokenRequest` | `TokenValidationResponse` | Token validation |

### SessionService (Port 50052)

| RPC | Request | Response | Description |
|-----|---------|----------|-------------|
| `CreateSession` | `CreateSessionRequest` | `CreateSessionResponse` | Create session |
| `GetSession` | `GetSessionRequest` | `Session` | Get session |
| `EndSession` | `EndSessionRequest` | `Empty` | End session |
| `Heartbeat` | `SessionHeartbeatRequest` | `SessionHeartbeatResponse` | Heartbeat |

### UserContextService (Port 50052) ★

| RPC | Request | Response | Description |
|-----|---------|----------|-------------|
| `UploadBatch` | `ContextBatchUploadRequest` | `ContextBatchUploadResponse` | Batch upload |
| `SubscribeSuggestions` | `SubscribeRequest` | `stream Suggestion` | **★ Server Streaming (SSE replacement)** |
| `SendFeedback` | `SuggestionFeedback` | `Empty` | Send feedback |
| `ListSuggestions` | `ListSuggestionsRequest` | `ListSuggestionsResponse` | List suggestions (type filter) |
| `Heartbeat` | `HeartbeatRequest` | `HeartbeatResponse` | Heartbeat |

### REST vs gRPC Comparison

| Feature | REST | gRPC |
|---------|------|------|
| Suggestion stream | SSE `/sessions/stream` | `SubscribeSuggestions` (Server Streaming) |
| Batch upload | POST `/sync/batch` | `UploadBatch` (includes frames) |
| Feedback | POST `/suggestions/feedback` | `SendFeedback` |
| Type safety | JSON schema | Proto compile-time validation |
| Payload size | Text (100%) | Binary (~70%) |

### Transition Strategy

```rust
// UnifiedClient — gRPC first, REST fallback
let config = GrpcConfig {
    use_grpc_auth: true,     // Use gRPC for auth
    use_grpc_context: true,  // Use gRPC for context
    grpc_endpoint: "http://127.0.0.1:50052".to_string(),
    grpc_fallback_ports: vec![50051, 50053],  // ⭐ Port fallback (2026-02-05)
    rest_endpoint: "http://127.0.0.1:8000".to_string(),
    ..Default::default()
};

let client = UnifiedClient::new(config, token_manager);

// On gRPC failure:
// 1. First retry with grpc_fallback_ports (50051 → 50053)
// 2. If all gRPC ports fail, fall back to REST
let response = client.upload_batch(request).await?;
```

### gRPC Port Fallback Strategy ⭐ NEW (2026-02-05)

```
┌──────────────────────────────────────────────────────────┐
│ Connection Attempt Order                                  │
├──────────────────────────────────────────────────────────┤
│ 1. grpc_endpoint (50052)     → Use if successful         │
│ 2. fallback_ports[0] (50051) → Try if 1 fails            │
│ 3. fallback_ports[1] (50053) → Try if 2 fails            │
│ 4. REST endpoint (8000)      → If all gRPC fail          │
└──────────────────────────────────────────────────────────┘
```

In industrial environments where HTTP/2 is blocked or specific ports are unavailable, alternative ports are tried automatically.
