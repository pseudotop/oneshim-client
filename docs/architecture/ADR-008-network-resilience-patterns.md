# ADR-008: Network Resilience Patterns

**Status**: Proposed
**Date**: 2026-03-09
**Scope**: `oneshim-network` crate, all network-facing adapters

---

## Context

The desktop agent communicates with the ONESHIM server via HTTP REST, SSE,
WebSocket, and gRPC. Desktop environments produce network failures a server
process never sees: WiFi drops, VPN reconnects, sleep/wake cycles, and rolling
server deployments. The agent must handle these without losing buffered data or
overwhelming a recovering server.

Three incremental fixes surfaced the gaps this ADR closes:

| Pivot commit | Date | Path | Finding |
|---|---|---|---|
| `b13a46b` | 2026-02-28 | `http_client.rs` | `RequestTimeout` + `is_retryable`: backoff present, **no jitter** |
| `ffa2478` | 2026-03-01 | `batch_uploader.rs` | Queue OOM fixed; flush retry added, **no circuit breaker** |
| `50ac66b` | 2026-03-08 | `sse_client.rs` | SSE reconnect loop added, **no jitter** |

---

## Decisions

### 1. Exponential Backoff with Jitter

**Rule**: All retry loops MUST use exponential backoff with jitter. Cap at a
configurable maximum.

```rust
// 지수 백오프 + 지터 계산
fn backoff_delay(attempt: u32, base_ms: u64, max_ms: u64) -> Duration {
    let exp = base_ms.saturating_mul(2u64.saturating_pow(attempt.min(10)));
    let jitter = rand::thread_rng().gen_range(0..=(exp / 4));
    Duration::from_millis((exp + jitter).min(max_ms))
}
```

Current status:

| Location | State | Action |
|---|---|---|
| `HttpApiClient::execute_with_retry()` | Backoff, no jitter | Use `backoff_delay()` |
| `SseStreamClient::connect()` | Backoff (`retry_delay * 2`), no jitter | Use `backoff_delay()` |
| `BatchUploader::flush()` | Backoff, no jitter | Use `backoff_delay()` |

Default caps: 30 s for SSE/HTTP, 60 s for batch flush. Without jitter, all
clients that dropped simultaneously reconnect at identical timestamps, spiking
server load during recovery.

---

### 2. Token Refresh De-duplication

**Rule**: Only one refresh request may be in-flight at any time. Concurrent
callers that see `needs_refresh = true` MUST wait for the in-progress refresh.

Current problem in `auth.rs`: every caller releases the `RwLock` guard and
individually calls `refresh()`, firing N parallel POST requests.

Required pattern — `AtomicBool` + `Notify`:

```rust
pub struct TokenManager {
    state: Arc<RwLock<Option<TokenState>>>,
    refreshing: AtomicBool,           // 리프레시 진행 중 여부
    refresh_notify: Arc<Notify>,      // 완료 시 대기 태스크 일괄 깨움
    client: reqwest::Client,
    base_url: String,
}

pub async fn get_token(&self) -> Result<String, CoreError> {
    if self.refreshing.load(Ordering::Acquire) {
        self.refresh_notify.notified().await;
    }

    let needs_refresh = { /* expiry check via RwLock */ };
    if needs_refresh {
        if self.refreshing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let result = self.do_refresh().await;
            self.refreshing.store(false, Ordering::Release);
            self.refresh_notify.notify_waiters();
            result?;
        } else {
            self.refresh_notify.notified().await; // 다른 태스크가 리프레시 중
        }
    }
    // state RwLock에서 토큰 반환
}
```

`refresh_notify` is `Arc<Notify>` — shared across all `TokenManager` clones.

---

### 3. Circuit Breaker

**Rule**: Network clients that experience repeated failures MUST implement a
circuit breaker to prevent overwhelming a recovering server.

States: **Closed** (normal) → **Open** (block requests) → **Half-Open** (probe).

```rust
/// 서킷 브레이커 — 연속 장애 시 요청 차단
pub struct CircuitBreaker {
    state: AtomicU8,             // 0=Closed, 1=Open, 2=HalfOpen
    failure_count: AtomicU32,
    failure_threshold: u32,      // 기본값: 5
    recovery_timeout: Duration,  // 기본값: 30 s
    last_failure_ms: AtomicU64,  // Unix ms 타임스탬프
}
```

Scope: Apply to `BatchUploader`. The flush path currently retries `max_retries`
times per call with no memory across scheduler ticks, making it possible to
hammer a permanently-down server on every 5-second cycle.

`HttpApiClient::execute_with_retry()` is already bounded per-call and is exempt.

---

### 4. Rate Limit Header Parsing

**Rule**: HTTP 429 responses MUST parse the `Retry-After` header. A hardcoded
fallback is only acceptable when the header is absent.

Current problem in `http_client.rs`:

```rust
// 현재: Retry-After 헤더 무시, 60초 하드코딩
429 => Err(CoreError::RateLimit { retry_after_secs: 60 }),
```

Required replacement:

```rust
/// 429 응답의 Retry-After 헤더를 파싱한다. 부재/파싱 실패 시 60초 기본값 반환.
fn extract_retry_after(response: &reqwest::Response) -> u64 {
    response.headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60)
}

429 => Err(CoreError::RateLimit { retry_after_secs: extract_retry_after(&resp) }),
```

`execute_with_retry()` already overrides the delay with `retry_after_secs`; no
further change is needed there.

---

## Consequences

**Must do** (gates new network code merges):

1. `backoff_delay()` lands in `oneshim-network/src/resilience.rs` and replaces
   all inline delay calculations.
2. `extract_retry_after()` replaces the hardcoded `60` in `check_response`.
3. `TokenManager` gains `AtomicBool` + `Arc<Notify>` to de-duplicate refreshes.

**Should do** (next sprint):

4. `CircuitBreaker` implemented in `resilience.rs` and wired into `BatchUploader`.
5. Unit tests for each pattern: jitter range, single-refresh assertion, circuit
   state transitions, and header fallback.

**Constraints**: No new workspace dependencies are required. `rand` is already
present via `oneshim-vision`. All changes are contained within `oneshim-network`
— consistent with the crate dependency rules in ADR-001 §6.
