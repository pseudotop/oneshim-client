# 외부 gRPC 바인딩 가이드

## 개요

외부 gRPC 바인딩을 통해 데스크톱 에이전트가 루프백 인터페이스(127.0.0.1) 외부에서 연결을 수락할 수 있습니다. 이를 통해 LAN 대시보드 접근, 원격 팀 모니터링, 중앙 관리 시스템과의 통합 등의 사용 사례를 지원합니다. 이 기능은 `external_grpc.enabled: true` 설정 플래그를 통해 선택적으로 활성화되므로 기본 동작에는 영향을 주지 않습니다(기존 배포에 대한 영향 없음). 보안을 위해 TLS와 JWT 또는 mTLS 인증이 필수이며, 이는 선택 사항이 아닙니다.

## 설정

### 인증서 생성

`generate-external-cert` CLI (Tauri 메인 바이너리에서 argv 기반 dispatch)를 사용하여
TLS + JWT 키 번들을 일괄 생성합니다:

```bash
cargo run -p oneshim-app --features external-grpc-tools -- generate-external-cert \
    --output-dir ~/.oneshim/certs/ --bind-ip 0.0.0.0
```

이 커맨드는 출력 디렉토리에 다음 4개 파일을 생성합니다:

- `server.crt` — TLS 서버 인증서(1년 유효 자체 서명, `--bind-ip`에 바인드됨)
- `server.key` — TLS 서버 개인 키(PKCS#8 형식, 암호화 없음)
- `jwt_signing.pub` — JWT 검증용 공개 키(ES256 또는 RSA-2048, 생성 중 선택한 알고리즘에 따름)
- `jwt_signing.priv` — JWT 서명용 개인 키(에이전트에 보관; 중앙 인증 서비스에서 토큰을 발급하는 경우에만 배포)

**핵심 배포:**

- `server.crt`와 `server.key`는 에이전트의 파일시스템에 남아있습니다.
- `jwt_signing.pub`는 에이전트에 배치됩니다(로컬 JWT 검증을 사용하는 경우).
- `jwt_signing.priv`는 해당 서비스에서 토큰을 발급하는 경우에만 중앙 인증 서비스에 배포됩니다. 그 외에는 에이전트에 비공개로 유지합니다.
- `server.key`는 기밀로 유지하고 `server.crt`와 별도로 백업합니다.

### 설정

에이전트의 설정 파일(TOML 형식)에 다음 섹션을 추가합니다:

```toml
[external_grpc]
enabled = true
bind_address = "0.0.0.0"
port = 10092
tls_cert_path = "/path/to/server.crt"
tls_key_path = "/path/to/server.key"
auth_mode = "jwt"
jwt_algorithm = "ES256"
jwt_public_key_path = "/path/to/jwt_signing.pub"
jwt_expected_issuer = "your-auth-service"
jwt_expected_audience = "oneshim-agent-{device_id}"
```

**설정 필드:**

- `enabled` — 불린값. 외부 서버를 활성화하려면 `true`로 설정합니다. 기본값은 `false`입니다.
- `bind_address` — 문자열. 바인드할 IP 주소입니다. 모든 인터페이스를 사용하려면 `"0.0.0.0"`을 사용하거나 `"192.168.1.100"`과 같은 특정 IP를 사용합니다.
- `port` — 정수. 포트 번호(1024–65535). 기본값은 10092입니다.
- `tls_cert_path` — 문자열. TLS 인증서 파일의 절대 경로입니다.
- `tls_key_path` — 문자열. TLS 개인 키 파일의 절대 경로입니다.
- `auth_mode` — 문자열. `"jwt"`, `"mtls"`, 또는 `"jwt+mtls"` 중 하나입니다. 수락할 인증 방법을 결정합니다.
- `jwt_algorithm` — 문자열. `"ES256"`(ECDP-256, 64바이트 서명) 또는 `"RS256"`(RSA-2048, 256바이트 서명) 중 하나입니다. `jwt_signing.pub` 생성 시 사용한 알고리즘과 일치해야 합니다.
- `jwt_public_key_path` — 문자열. JWT 검증용 공개 키 파일의 절대 경로입니다.
- `jwt_expected_issuer` — 문자열. 수신 JWT의 예상 `iss` 클레임입니다. 다른 발급자의 토큰은 거부됩니다.
- `jwt_expected_audience` — 문자열. 예상 `aud` 클레임입니다. `{device_id}`와 같은 플레이스홀더를 사용할 수 있으며, 이는 시작 시 보간됩니다.

### 방화벽

구성된 포트를 시스템 방화벽에서 엽니다:

**macOS(앱 방화벽):**
```bash
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add /path/to/oneshim-app
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --unblockapp /path/to/oneshim-app
```

**Windows(Windows Defender 방화벽):**
```powershell
New-NetFirewallRule -DisplayName "ONESHIM gRPC" -Direction Inbound `
    -Program "C:\path\to\oneshim-app.exe" -Action Allow -Protocol TCP -LocalPort 10092
```

**Linux(UFW):**
```bash
sudo ufw allow 10092/tcp
sudo ufw reload
```

## 리버스 프록시 예시

외부 gRPC 트래픽은 일반적으로 리버스 프록시를 통해 노출되어 도메인 라우팅, 속도 제한 및 WAF 통합을 지원합니다.

### Caddy

간단하고 자동 HTTPS:

> ⚠️ **보안 주의**: `tls_insecure_skip_verify`는 Caddy와 에이전트 사이의 인증서 검증을 비활성화합니다.
> Caddy와 에이전트가 같은 호스트에 있는 경우(예: localhost 사이드카)에만 안전합니다.
> 크로스 호스트 배포의 경우 이 플래그를 제거하고 에이전트의 인증서를 Caddy에 제공하세요
> (`server.crt`를 Caddy 트러스트 스토어에 복사하거나 `transport http { tls_trusted_ca_certs server.crt }` 사용).

```caddy
oneshim.example.com:443 {
    reverse_proxy localhost:10092 {
        transport http {
            tls
            tls_insecure_skip_verify  # 자체 서명 인증서에만 사용; 프로덕션에서는 CA 검증 필수
        }
    }
}
```

### Nginx(스트림 모듈)

> ⚠️ **주의**: Nginx `stream`은 TCP 패스스루입니다 — L7 기능(HTTP 수준 라우팅,
> 인증 헤더, 리라이트 규칙)이 동작하지 않습니다. 에이전트가 TLS + gRPC를 직접 종료합니다.

```nginx
stream {
    upstream oneshim_backend {
        server 127.0.0.1:10092;
    }
    
    server {
        listen 443;
        listen [::]:443;
        proxy_pass oneshim_backend;
    }
}
```

### Cloudflare Tunnel

공개 IP가 필요하지 않습니다. Cloudflare가 인증 및 암호화를 처리합니다.

영구 터널 설정(`~/.cloudflared/config.yml`):
```yaml
tunnel: <your-tunnel-uuid>
credentials-file: /path/to/<uuid>.json
ingress:
  - hostname: oneshim.example.com
    service: https://localhost:10092
    originRequest:
      noTLSVerify: true  # 프로덕션에서는 적절한 인증서와 함께 이 옵션 제거
  - service: http_status:404
```

그런 다음 DNS CNAME을 만들거나 Cloudflare의 라우팅 규칙을 사용하여 `oneshim.example.com`을 터널로 지정합니다.

## 보안 체크리스트

이 체크리스트를 사용하여 외부 gRPC 배포를 검증합니다:

- [ ] **TLS 인증서가 365일 이내에 회전됩니다.** 에이전트는 파일 감시자를 통해 인증서를 핫 리로드합니다(원자적 이름 바꾸기); 재시작 불필요.
- [ ] **JWT 서명 키 쌍이 최소 연 1회 회전됩니다.** 에이전트는 새로운 공개 키를 받으려면 재시작이 필요합니다. 유지보수를 계획합니다.
- [ ] **mTLS 클라이언트 인증서의 수명이 48시간 이내로 제한됩니다.** 에이전트는 더 오래 사용된 인증서를 거부합니다.
- [ ] **mTLS 지문 허용 목록이 채워집니다** (예: 팀별 CI/CD 러너로 제한하는 다중 팀 배포의 경우).
- [ ] **IP 금지 임계값이 예상 트래픽 패턴에 대해 검토됩니다.** 기본값: IP당 5회 실패한 인증 시도 후 금지, 지수 백오프 적용(60초 → 10분 → 1시간).
- [ ] **감사 로그가 주기적으로 쿼리됩니다.** 에이전트는 로컬 SQLite 데이터베이스에 로컬 감사 추적을 기록합니다. 주기적인 수동 검토 또는 자동화된 내보내기가 권장됩니다("감사" 섹션 참조).
- [ ] **TLS 암호 제품군이 검증됩니다** (보안 정책과 규정 준수 요구사항에 따라 `rustls` 기본값 및 정책 참조).
- [ ] **리버스 프록시 로깅이 활성화됩니다** 및 예상치 못한 패턴 모니터링(예: 포트 스캔, 무작위 인증 시도).

## 감사

모든 외부 gRPC 요청은 에이전트의 로컬 감사 DB에 Started + Completed 페어로 기록됩니다.
인증 성공 시 `AuthLayer`가 Started를, 인증 거부 시 Failed를 기록합니다 (사유별 —
`invalid_jwt`, `missing_token`, `fingerprint_mismatch`, `missing_cert`).
`AuditLayer` (`AuthLayer`의 안쪽 레이어)는 핸들러 반환 후 Completed를 기록합니다.
쿼리 표면:

- `entries_by_status(AuditStatus::Completed, N)` — 성공한 RPC.
- `entries_by_status(AuditStatus::Failed, N)` — 인증 거부.
- `entries_by_action_prefix("external_grpc_", N)` — 모든 외부 행
  (`external_grpc_started`, `external_grpc_completed`, `external_grpc_failed`).
- `entries_by_command_id(command_id, N)` — 단일 클라이언트 요청과 상관관계가 있는
  모든 행 (Started + Completed + Failed). `command_id`는 `x-request-id` 헤더
  값과 일치합니다 (아래 "Request-ID 상관관계" 섹션 참조).

### gRPC 상태 → AuditStatus 매핑 (D28)

`AuditLayer`는 트레일러 프레임의 `grpc-status`를 관찰하고 (트레일러가 초기 HEADERS
프레임에 함께 전송될 경우 — 예: trailers-only 응답 — 헤더 우선 관찰), 이를
`AuditStatus`로 매핑합니다. 동시에 원시 숫자 코드를 함께 영속화하여, 동일한 상태로
collapse되는 코드들을 보안 대시보드에서 구분할 수 있게 합니다.

| `grpc-status` (숫자)         | tonic `Code`         | `AuditStatus` |
|-----------------------------|----------------------|---------------|
| 0                           | `Ok`                 | `Completed`   |
| 1                           | `Cancelled`          | `Timeout`     |
| 4                           | `DeadlineExceeded`   | `Timeout`     |
| 7                           | `PermissionDenied`   | `Denied`      |
| 16                          | `Unauthenticated`    | `Denied`      |
| 그 외 0이 아닌 값          | (예: `Internal`)     | `Failed`      |
| 부재 (트레일러 전 클라이언트 종료, OQ6-A) | —      | `Completed`   |

감사 상세 필드 (`ExternalGrpcAuditDetails`를 통해 `AuditEntry.details`에 JSON으로
직렬화):

- `transport` — 항상 `"external"`.
- `remote_addr` — 피어 소켓 주소(IP 및 포트).
- `auth_type` — `"jwt"`, `"mtls"`, 또는 `"jwt+mtls"`.
- `operation` — gRPC 메서드 이름(예: `/oneshim.v1.DashboardService/GetSessionStats`).
- `result` — 성공/실패 라벨.
- `request_size_bytes` / `response_size_bytes` — 페이로드 크기 (가용 시).
- `failure_reason` — 거부 사유 문자열 (예: `invalid_jwt`).
- `jti` — JWT `jti` 클레임 (JWT 인증인 경우).
- `response_message_count` — 서버 스트림 메시지 수 (스트리밍 RPC만).
- `grpc_status_code` — `tonic::Code`의 원시 `u32` 값. Completed/Denied 행에
  채워지므로 `PermissionDenied` (7)와 `Unauthenticated` (16)을 대시보드에서
  구분할 수 있습니다 — 두 코드 모두 그렇지 않으면 `AuditStatus::Denied`로
  collapse됩니다. Started 행에서는 생략됩니다.

로컬 REST API를 통해 감사 로그를 내보내려면 (loopback 전용):

```bash
# 모든 최근 항목 (기본 limit 100, 최대 1000)
curl http://localhost:10090/api/audit/export | jq

# command_id로 필터 (원래 요청의 x-request-id와 일치)
curl "http://localhost:10090/api/audit/export?command_id=<uuid>&limit=50" | jq
```

이 엔드포인트의 자세한 명세는 아래 "REST 엔드포인트 — `GET /api/audit/export`"를
참조하십시오.

CLI를 통해 쿼리하려면:

```bash
sqlite3 ~/.oneshim/oneshim.db "SELECT * FROM audit_log WHERE timestamp > datetime('now', '-7 days') ORDER BY timestamp DESC LIMIT 100;"
```

## Request-ID 상관관계

모든 외부 gRPC 요청에는 `x-request-id` 헤더가 포함되며, 이 값은 레이어 스택을 통해
end-to-end로 전파되어, 해당 요청에서 생성된 모든 감사 행의 `command_id`로 기록됩니다.
이 헤더는 정보성이므로, 잘못된 값이나 부재가 요청을 거부시키지 않습니다 — 서버는
대신 새로운 UUIDv4로 폴백합니다.

### 헤더 시맨틱

- **헤더 이름**: `x-request-id` (HTTP/2 와이어 컨벤션을 따라 소문자).
- **검증 규칙**: ASCII graphic 바이트만 (`0x21..=0x7E`), 길이 1..=128.
  공백, 제어 문자, 비-ASCII는 거부됩니다.
- **유효한 수신 값**: 서버는 그대로 보존하고 응답에 그대로 echo합니다.
- **잘못되거나 부재한 값**: 서버는 `warn` 로그를 기록 (거부된 입력의 처음 32자
  + `reason="validation_failed"`) 후 새로운 UUIDv4를 생성합니다. 생성된 ID가
  감사와 응답에 전파됩니다.
- **응답 echo**: 응답에는 항상 내부에서 사용된 값(수신-허용된 값 또는 생성된 값)과
  일치하는 `x-request-id` 헤더가 포함됩니다. D31 조건부 덮어쓰기: 핸들러가 같은
  값을 설정한 경우 그대로 보존됩니다.

### 레이어 스택 (D14 revised / U5)

요청 수신 시 외곽에서 내부로:

```
RequestIdLayer  →  AuthLayer  →  AuditLayer  →  핸들러
```

- `RequestIdLayer`는 request-ID를 검증/생성하고, `AuthLayer`가 실행되기 **이전에**
  `RequestId` extension을 `http::Request::extensions()`에 삽입합니다.
- `AuthLayer`의 Failed 경로(인증 거부)는 이 extension을 읽어 감사 행의
  `command_id`를 emit하므로, 인증 거부된 요청도 클라이언트의 `x-request-id`와
  상관관계를 유지합니다.
- `AuditLayer`(세 레이어 중 가장 안쪽)는 Completed/Denied 행에서 같은 extension을
  읽습니다.

순효과: 단일 클라이언트 요청에서 생성된 모든 감사 행 — Started, Completed, 인증
Failed 모두 — 동일한 `command_id`를 공유합니다. 전체 추적을 가져오려면
`entries_by_command_id` (또는 `GET /api/audit/export`의 `?command_id=` 쿼리)를
사용하십시오.

## Live config reload

외부 gRPC는 `AppConfig`의 일부를 live-mutable 상태로 추적합니다. `ConfigReloadTask`가
`ConfigManager`의 변경을 감시하고, 새 `LiveSnapshot`을 atomic 하게 swap하여
재시작 없이도 실행 중 서버를 갱신합니다.

### Watched 필드

| `AppConfig` 경로                          | 변경 시 효과                                            |
|-------------------------------------------|---------------------------------------------------------|
| `external_grpc.streaming_enabled` (Option<bool>) | external 전용 스트리밍 override. `Some(true/false)`은 override; `None`은 `web.grpc_streaming_enabled`로 폴스루(fall-through, 상위 필드로 전이) (D22). |
| `external_grpc.load_thresholds` / `web.grpc_load_thresholds` | 적응형 부하 정책 임계값 (CPU low/medium/high, 최소 free 메모리). `LoadPolicy::try_new`이 검증; 거부된 값은 이전 정책을 유지 (D23). |

### 시맨틱

- **D22 폴백 해석**: `external_grpc.streaming_enabled = Some(v)`은 공유 web 플래그를
  override합니다. `= None`이면 `web.grpc_streaming_enabled`로 폴스루합니다 — 즉
  loopback과 external이 공통 기본값을 공유하면서도 external만 opt-out 할 수
  있습니다.
- **D27 warmup 보존**: `LoadPolicy.started_at` (warmup 앵커)는 reload 간에
  보존됩니다. 임계값을 reload하더라도 30초 warmup 윈도우는 **리셋되지 않습니다** —
  운영자가 임계값을 토글하다가 실수로 warmup에 재진입하지 않도록 보호합니다.
- **부분 적용**: 잘못된 `load_thresholds`는 `error!` 로그와 함께 거부되지만,
  `streaming_enabled`(자명하게 유효)는 동일 swap에서 그대로 적용됩니다.
  D21의 단일 atomic store가 전이를 일관된 한 상태로 관찰되도록 보장합니다.
- **G3 수렴 한계**: `ConfigManager` 쓰기에서 live 스냅샷 가시성까지 ≤1s
  (`external_grpc_live_streaming_toggle_reflects_within_1s` 테스트로 CI 강제).
- **Coalescing**: 빠른 연속 업데이트는 마지막 상태로 collapse됩니다 —
  `tokio::sync::watch` 채널은 항상 최신 값만 노출합니다.

## REST 엔드포인트

아래 두 엔드포인트는 모두 로컬 웹 대시보드 (기본 `http://localhost:10090`)에서
제공되며, `require_loopback_client` 미들웨어로 보호됩니다 — `127.0.0.1` 출처
요청만 수락됩니다.

### `GET /api/external-grpc/live-config`

외부 gRPC 설정의 현재 live 스냅샷을 조회합니다. 사양 §5.11 / D29.

**응답** (`200 OK`, `application/json`) — `LiveConfigResponse`:

```json
{
  "streaming_enabled": true,
  "load_policy_snapshot": {
    "cpu_low_pct": 30.0,
    "cpu_medium_pct": 60.0,
    "cpu_high_pct": 85.0,
    "min_free_mem_gb": 1.0,
    "started_at_elapsed_ms": 42150,
    "in_warmup": false
  },
  "config_reload_task_alive": true
}
```

- `streaming_enabled` — 현재 적용 중인 값 (D22 폴백 해석 후).
- `load_policy_snapshot.cpu_*_pct` / `min_free_mem_gb` — 현재 `LoadPolicy`
  임계값.
- `load_policy_snapshot.started_at_elapsed_ms` — `LoadPolicy::started_at` 이후
  경과한 밀리초 (D27에 따라 reload 간 보존).
- `load_policy_snapshot.in_warmup` — 서버 시작 후 30초 warm-up 윈도우 동안
  `true`.
- `config_reload_task_alive` — `ConfigReloadTask`가 메인 루프에 진입한 후
  `true`; task가 종료되면 `false`로 돌아옵니다.

다음 경우 **`503 Service Unavailable`**을 반환합니다:

- 외부 gRPC가 컴파일에는 포함되어 있으나 런타임에 비활성화된 경우
  (`DiagnosticsState.external_grpc_live`가 `None`).
- 바이너리가 `grpc-dashboard-external` feature flag 없이 빌드된 경우 (핸들러가
  무조건 503 stub으로 대체됩니다).

### `GET /api/audit/export`

감사 항목을 JSON으로 내보냅니다. 사양 §5.9 / D25 / NV1.

**쿼리 파라미터**:

| 파라미터       | 타입    | 기본값  | 설명                                                 |
|---------------|---------|---------|------------------------------------------------------|
| `command_id`  | string  | 없음    | 존재하고 비어있지 않을 때, `command_id`가 정확히 일치하는 항목으로 필터링. 빈 문자열은 부재로 처리. |
| `status`      | string  | 없음    | 미래 사용 예약; 현재 no-op.                          |
| `limit`       | integer | 100     | 반환되는 최대 항목 수. `1000`으로 clamp (DoS 가드).  |

**응답** (`200 OK`, `application/json`): `Vec<AuditEntry>`, 최신순.

```bash
curl "http://localhost:10090/api/audit/export?command_id=550e8400-e29b-41d4-a716-446655440000&limit=20"
```

`automation.audit_logger`가 설정되지 않은 경우 (런타임에 감사 로깅 비활성화)
**`503 Service Unavailable`**을 반환합니다.

> **참고**: 현재 `entries_by_command_id`는 인메모리 감사 버퍼에서 읽습니다.
> SQLite 영속성 기반 lookup은 후속 Task 0.3.1에서 처리됩니다.

## 문제 해결

### 포트 10092 연결 거부

**증상:** 에이전트의 gRPC 엔드포인트에 연결할 때 `connection refused` 메시지가 표시됩니다.

**진단:**
1. 설정 플래그 확인: `external_grpc.enabled = true`.
2. 방화벽 확인: `lsof -i :10092`(macOS/Linux) 또는 `netstat -ano | findstr :10092`(Windows).
3. "포트 사용 중" 오류에 대한 에이전트 로그 확인: `grep -i "port\|address" ~/.oneshim/agent.log`.

**해결:**
- 포트가 다른 프로세스에서 사용 중이면 해당 프로세스를 중지하거나 `external_grpc.port` 설정을 변경합니다.
- `bind_address`가 네트워크 구성과 일치하는지 확인합니다(모든 인터페이스에 `0.0.0.0` 사용).

### TLS 핸드셰이크 실패

**증상:** 클라이언트 로그에 `tls: handshake failure` 또는 `x509: certificate verify failed` 메시지가 표시됩니다.

**진단:**
1. 인증서/키 경로가 올바른지 확인: `ls -la /path/to/server.crt /path/to/server.key`.
2. 인증서와 키가 일치하는 쌍인지 확인: `openssl x509 -in server.crt -text -noout` 및 `openssl pkey -in server.key -text -noout`의 모듈러스(RSA) 또는 공개 포인트(ECDSA)가 일치해야 합니다.
3. 인증서 만료 확인: `openssl x509 -enddate -noout -in server.crt`.

**해결:**
- 인증서 쌍 재생성: `cargo run -p oneshim-app --features external-grpc-tools -- generate-external-cert --output-dir ~/.oneshim/certs/ --bind-ip 0.0.0.0`
- 설정 경로를 업데이트하고 에이전트를 다시 시작합니다.
- 개발 중 자체 서명 인증서의 경우 클라이언트는 `tls_insecure_skip_verify`를 허용해야 합니다(curl `-k`와 동등).

### 인증되지 않음(JWT 또는 mTLS)

**증상:** 클라이언트 로그에 `rpc error: code = Unauthenticated desc = invalid token` 또는 `cert not allowed` 메시지가 표시됩니다.

**진단:**
1. JWT가 존재하고 올바른 형식인지 확인:
   ```bash
   echo "<token>" | jq .  # 오류 없이 파싱되어야 함
   ```
2. 클레임 확인: `echo "<token>" | jq '.iss, .aud, .exp'`.
3. 에이전트 설정 확인: `grep jwt_expected_ ~/.oneshim/config.toml`.

**해결:**
- gRPC 요청에 `Authorization: Bearer <token>` 헤더가 포함되어 있는지 확인합니다(참고: gRPC는 HTTP 헤더가 아닌 사용자 정의 메타데이터를 사용합니다. 클라이언트 라이브러리가 이를 매핑해야 함).
- 발급자 및 청중 클레임이 설정과 정확히 일치하는지 확인합니다(대소문자 구분).
- 토큰 만료 확인: `exp`가 과거인 경우 새 토큰을 가져옵니다.
- mTLS의 경우: 클라이언트 인증서가 허용 목록에 있고 만료되지 않았는지 확인합니다.

### IP 금지됨

**증상:** 몇 번의 연결 시도 후 `rpc error: code = Unavailable desc = ip banned` 메시지가 표시됩니다.

**진단:**
1. 에이전트는 IP 주소당 실패한 인증 시도를 추적합니다. 5번 연속 실패 후 IP는 60초 동안 금지됩니다.
2. 이후 금지(IP가 다시 실패한 경우)는 백오프를 증가시킵니다: 10분, 그 다음 1시간.

**해결:**
- 백오프 기간이 만료될 때까지 기다립니다(에이전트 로그에 `external_grpc: IP 192.168.1.100 banned until 2026-04-23T10:30:00Z`로 표시됨).
- 인증 문제(토큰, 인증서 등)를 해결하고 다시 시도합니다.
- IP를 즉시 금지 해제하려면 에이전트를 다시 시작합니다(메모리 내 금지 상태가 지워집니다).

### 인증서 만료 경고 로그

**증상:** 에이전트 로그에 `external_grpc: TLS cert expires in 3 days`(또는 유사)가 표시됩니다.

**진단:**
에이전트는 시작 시 인증서 만료를 확인하고 인증서가 7일 이내에 만료되면 경고를 기록합니다.

**해결:**
- 인증서를 즉시 재생성합니다:
  ```bash
  cargo run -p oneshim-app --features external-grpc-tools -- generate-external-cert \
      --output-dir ~/.oneshim/certs/ --bind-ip 0.0.0.0 --force
  ```
- 에이전트는 새 인증서를 초 단위로 핫 리로드합니다(파일 감시자를 통해).
- 재시작이 필요하지 않습니다.

## 고급 설정

### mTLS 클라이언트 인증서 지문 허용 목록

특정 클라이언트 인증서만 허용해야 하는 경우 허용 목록을 구성합니다:

```toml
[external_grpc]
mtls_fingerprint_allowlist = [
    "SHA256:abc123def456...",  # team-a-ci-runner
    "SHA256:xyz789uvw012...",  # team-b-automation
]
```

에이전트는 각 피어 인증서의 SHA-256 지문을 계산하고 목록에 없는 인증서의 연결을 거부합니다. 모든 유효한 mTLS 인증서를 수락하려면 비워 둡니다.

### JWT 토큰 새로 고침

장기 연결(예: 연속 스트리밍)의 경우 토큰 새로 고침 주기가 토큰 수명보다 짧아야 합니다. 예:

```bash
# 토큰 수명: 1시간
# 50분마다 새로 고침
while true; do
    TOKEN=$(curl -X POST https://auth.example.com/token \
        -H "Content-Type: application/json" \
        -d '{"client_id":"...","client_secret":"..."}' | jq -r .access_token)
    grpcurl -H "authorization: Bearer $TOKEN" \
        localhost:10092 list oneshim.v1.DashboardService
    sleep 3000  # 50분
done
```

### 모니터링 및 알림

감사 로그에 대한 모니터링을 설정하여 의심스러운 패턴을 감지합니다:

```bash
# 지난 1시간 동안의 실패한 인증 시도 쿼리
sqlite3 ~/.oneshim/oneshim.db \
    "SELECT peer_ip, COUNT(*) as failures FROM audit_log \
     WHERE status_code != 'OK' AND timestamp > datetime('now', '-1 hour') \
     GROUP BY peer_ip ORDER BY failures DESC;"
```

다음의 경우 알림:
- 모든 IP가 시간당 10회 이상의 실패한 인증 시도를 가집니다.
- 새로운 피어 인증서가 갑자기 나타납니다.
- 토큰 `iss` 또는 `aud` 클레임이 예상치 못하게 변경됩니다.

## 참고

- [gRPC 클라이언트 가이드](grpc-client.ko.md) — gRPC 엔드포인트에 연결(내부 및 외부).
- [gRPC 거버넌스](grpc-governance.ko.md) — RPC 버전 관리 및 API 안정성 정책.
- [gRPC 오류 매핑](grpc-error-mapping.ko.md) — gRPC 오류 코드 이해.
- [엔터프라이즈 배포](enterprise-deployment.md) — 플릿 전체에 에이전트 확장.
