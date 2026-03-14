# OAuth Token Auto-Refresh Design

## Goal

Add proactive background token refresh for OAuth-managed credentials (OpenAI Codex) with concurrent refresh protection, failure taxonomy, retry/backoff, and UI notifications for re-authentication.

## Scope & Phase

**Phase 1 — OpenAI Codex only.** The provider ID `"openai"` is hardcoded in the scheduler loop. Multi-provider OAuth is out of scope (see § Scope Boundaries). All types and traits are generic (accept `provider_id: &str`), but the wiring and tests target a single provider.

## Context

### What Exists Today

- **OAuth PKCE flow**: `OAuthClient` handles authorization code exchange, stores tokens in OS keychain via `KeychainSecretStore`
- **On-demand refresh**: `get_access_token()` checks `expires_at` (60s buffer), calls `try_refresh()` if expired
- **Token exchange**: `token_exchange::refresh_token()` POSTs to `https://auth.openai.com/oauth/token` with `grant_type=refresh_token`
- **Keychain storage**: `access_token`, `refresh_token`, `expires_at` (RFC3339), `scopes` stored per provider namespace
- **9-loop scheduler**: Background task system in `src-tauri/src/scheduler/` with `tokio::select!` shutdown pattern

### What's Missing

1. **Proactive refresh** — tokens only refresh when a request needs one; if token expires between requests, the next request fails before refreshing
2. **Concurrent refresh guard** — multiple simultaneous `get_access_token()` calls can trigger parallel refresh requests
3. **Failure taxonomy** — `try_refresh()` treats all failures as `Ok(false)` with no distinction between terminal (revoked token) and transient (network) errors
4. **Retry/backoff** — refresh failure returns `None` silently with no retry strategy
5. **UI notification** — no way for the user to know when re-authentication is required

### OpenAI Codex Token Specifics

| Parameter | Value |
|-----------|-------|
| Token endpoint | `https://auth.openai.com/oauth/token` |
| Client ID | `app_EMoamEEZ73f0CkXaXp7hrann` |
| Recommended refresh interval | 8 minutes |
| Grant type | `refresh_token` |
| Scopes | `openid`, `profile`, `email`, `offline_access` |

**Provider operational note:** OpenAI uses short-lived access tokens (~10 min) with long-lived refresh tokens. The `invalid_grant` error code from the token endpoint means the refresh token has been revoked or expired server-side — this is terminal and requires a full re-authentication flow.

## Refresh Thresholds

Four separate thresholds govern different parts of the system. They share a general premise (refresh before expiry) but serve different purposes:

| Threshold | Value | Where | Purpose |
|-----------|-------|-------|---------|
| Token validity guard | 60 seconds | `OAuthClient.is_token_valid()` | Existing — prevents returning a token that would expire during a request |
| Proactive refresh trigger | 5 minutes | `TokenRefreshCoordinator.check_and_refresh()` | NEW — coordinator starts proactive refresh when expiry < 5 min |
| UI yellow badge | 1–5 minutes remaining | `OAuthConnectionPanel.tsx` | NEW — visual warning for the user |
| UI red badge | < 1 minute remaining | `OAuthConnectionPanel.tsx` | NEW — critical visual warning |

The coordinator's 5-minute threshold and the client's 60-second validity guard are complementary: the coordinator proactively refreshes well before the 60-second guard fires. The UI badges are display-only and have no effect on refresh logic.

## Port Contract Extension

### RefreshResult Enum

**Location**: `crates/oneshim-core/src/ports/oauth.rs` (add after `OAuthConnectionStatus`)

The existing `get_access_token()` returns `Result<Option<String>, CoreError>`, which cannot distinguish "token was already fresh" from "token was just refreshed" from "refresh failed due to revoked token". The coordinator needs a typed result to drive auto-recovery and failure classification.

```rust
/// Typed result from an explicit refresh attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum RefreshResult {
    /// Token has ≥ min_valid_for_secs remaining — no refresh was attempted.
    AlreadyFresh { expires_at: String },
    /// Token was refreshed successfully.
    Refreshed { expires_at: String },
    /// No stored credentials — user has never authenticated.
    NotAuthenticated,
    /// Refresh token is invalid/revoked — full re-authentication required.
    ReauthRequired { reason: String },
    /// Transient failure (network, server error) — retry later.
    TransientFailure { message: String },
}
```

### New OAuthPort Method

```rust
/// Attempt to refresh the access token if it expires within `min_valid_for_secs`.
///
/// Unlike `get_access_token()` (which silently refreshes), this method returns
/// a typed `RefreshResult` allowing callers to distinguish terminal from
/// transient failures and to detect "already fresh" without side effects.
async fn refresh_access_token(
    &self,
    provider_id: &str,
    min_valid_for_secs: i64,
) -> Result<RefreshResult, CoreError>;
```

### OAuthConnectionStatus Expansion

Add `has_refresh_token: bool` to `OAuthConnectionStatus`:

```rust
pub struct OAuthConnectionStatus {
    pub provider_id: String,
    pub connected: bool,
    pub expires_at: Option<String>,
    pub scopes: Vec<String>,
    #[serde(default)]
    pub api_base_url: Option<String>,
    /// Whether a refresh token is stored (indicates background refresh capability).
    #[serde(default)]
    pub has_refresh_token: bool,  // NEW
}
```

This lets the coordinator and UI differentiate "connected but no refresh token" (can't proactively refresh) from "connected with refresh token" (can proactively refresh).

## Failure Taxonomy

Token refresh failures fall into two categories with different coordinator responses:

| Category | Error Signals | Coordinator Action |
|----------|---------------|--------------------|
| **Terminal** | `invalid_grant` error code, HTTP 401 with error body, revoked refresh token | Immediately emit `ReauthRequired` — no backoff, no retries |
| **Transient** | Network timeout, DNS failure, HTTP 5xx, connection refused | Increment counter, backoff, retry up to 3 attempts |

**Implementation**: `token_exchange::refresh_token()` already returns `CoreError::OAuthError { provider, message }` where the message contains the error code (e.g., `"refresh failed: invalid_grant: token revoked"`). The new `refresh_access_token()` implementation parses this message to classify the failure:

```rust
// In OAuthClient::refresh_access_token()
match token_exchange::refresh_token(&self.http, config, &refresh_tok).await {
    Ok(result) => { /* store tokens, return Refreshed */ }
    Err(e) => {
        let msg = e.to_string();
        if msg.contains("invalid_grant") || msg.contains("invalid_client") {
            Ok(RefreshResult::ReauthRequired { reason: msg })
        } else {
            Ok(RefreshResult::TransientFailure { message: msg })
        }
    }
}
```

**Why parse the message string?** Adding a structured error variant to `CoreError` would require changes across the error hierarchy for a single use site. Parsing the message is pragmatic for Phase 1 — the message format is controlled by our own `token_exchange` module and the pattern is stable. If multi-provider support is added later, a structured `OAuthRefreshError` enum can be extracted.

## Architecture

Three components layered on the existing `OAuthClient`:

```
┌─────────────────────────────────────────────────────┐
│ Scheduler (src-tauri)                               │
│  spawn_oauth_refresh_loop (every 2 min)             │
│    └── calls coordinator.check_and_refresh()        │
│    └── emits Tauri event on ReauthRequired          │
├─────────────────────────────────────────────────────┤
│ TokenRefreshCoordinator (oneshim-network)            │
│  - Mutex<RefreshState> (coordinator-owned)            │
│  - Exponential backoff (2 min → 5 min → stop)       │
│  - Terminal failures → immediate ReauthRequired      │
│  - Auto-recovery: AlreadyFresh resets failure state  │
│  - tokio::broadcast for TokenEvent emission          │
│  NOTE: The coordinator is an orchestration utility,   │
│  not a port adapter. Direct dependency from scheduler │
│  is intentional — it coordinates refresh logic that   │
│  doesn't map to a single port responsibility.         │
├─────────────────────────────────────────────────────┤
│ OAuthClient (existing)                               │
│  - refresh_access_token() → typed RefreshResult      │
│  - try_refresh() → token_exchange::refresh_token()   │
│  - get_access_token() — on-demand path unchanged     │
│  - AtomicBool refresh_in_progress guard (new)        │
│  - store_tokens_static() → keychain persistence      │
├─────────────────────────────────────────────────────┤
│ UI Layer                                             │
│  - Tauri event: `oauth-reauth-required` (frontend    │
│    listens via appWindow.listen())                    │
│  - Dashboard: expiry badge based on expires_at field  │
│    (independent of `connected` flag)                  │
└─────────────────────────────────────────────────────┘
```

### Why a 10th Scheduler Loop?

The coordinator orchestrates refresh logic across existing ports (`OAuthPort`, `TokenEvent` broadcast) but doesn't map to a single port responsibility. It combines token status checking, refresh execution, backoff management, and event emission — which is orchestration, not adaptation.

Alternatives considered:
1. **Embedding in `OAuthClient` itself** — rejected because `OAuthClient` implements the `OAuthPort` trait and shouldn't own scheduling/backoff logic
2. **A standalone `tokio::spawn` in setup.rs** — rejected because it bypasses the scheduler's shutdown coordination (`tokio::select!` with `shutdown_rx`)
3. **A new port trait** — rejected because the coordinator is an internal optimization, not a domain boundary

The scheduler already manages 9 background loops with unified shutdown. Adding a 10th follows the established pattern with minimal conceptual overhead.

## Component Design

### 1. TokenRefreshCoordinator

**Location**: `crates/oneshim-network/src/oauth/refresh_coordinator.rs` (new file)

**Responsibility**: Orchestrate proactive token refresh with concurrency protection, failure classification, and event emission. This is an orchestration utility, not a port adapter — the scheduler depends on it directly (concrete type, not behind a trait).

**Struct**:

```rust
pub struct TokenRefreshCoordinator {
    oauth_port: Arc<dyn OAuthPort>,
    state: Mutex<RefreshState>,
    event_tx: broadcast::Sender<TokenEvent>,
}

struct RefreshState {
    in_progress: bool,
    consecutive_transient_failures: u8,
    last_attempt: Option<Instant>,
    backoff_until: Option<Instant>,
}
```

**Public API**:

- `new(oauth_port: Arc<dyn OAuthPort>, event_tx)` — constructor
- `check_and_refresh(provider_id) -> Result<RefreshOutcome, CoreError>` — main entry point
- `subscribe() -> broadcast::Receiver<TokenEvent>` — event subscription

**`check_and_refresh` flow** (uses `refresh_access_token` with typed results):

1. Acquire `RefreshState` lock
2. If `in_progress == true`, return `RefreshOutcome::AlreadyInProgress`
3. If `now < backoff_until`, return `RefreshOutcome::BackingOff`
4. Set `in_progress = true`, release lock
5. Call `oauth_port.refresh_access_token(provider_id, 300)` (300 = 5 min threshold)
6. Re-acquire lock, update state based on `RefreshResult`:
   - **`AlreadyFresh`**: If `consecutive_transient_failures > 0`, reset to 0 (auto-recovery after manual re-auth). Return `RefreshOutcome::NotNeeded`
   - **`Refreshed`**: Reset failures, emit `TokenEvent::Refreshed`. Return `RefreshOutcome::Refreshed`
   - **`NotAuthenticated`**: Return `RefreshOutcome::ReauthRequired` (no retries — user never authenticated)
   - **`ReauthRequired`**: Immediately emit `TokenEvent::ReauthRequired`, set backoff far into future. Return `RefreshOutcome::ReauthRequired`
   - **`TransientFailure`**: Increment `consecutive_transient_failures`, calculate backoff, emit `TokenEvent::RefreshFailed`. At 3 failures, emit `TokenEvent::ReauthRequired`. Return `RefreshOutcome::Failed`
7. Set `in_progress = false`

**Auto-recovery**: No explicit `reset_failure_state()` method is needed. When a user manually re-authenticates (completing a new OAuth flow), the next coordinator tick calls `refresh_access_token()` which returns `AlreadyFresh` (fresh token from the new flow). The coordinator detects `AlreadyFresh` with a non-zero failure count and resets automatically.

**Backoff schedule** (aligned with 2-minute loop interval):

| Consecutive Failures | Backoff Duration | Effect |
|---------------------|-----------------|--------|
| 1 | 2 minutes | Skip 1 scheduler tick |
| 2 | 5 minutes | Skip ~2 ticks |
| 3 | Stop | Emit `ReauthRequired`, no more retries until auto-recovery |

### 2. TokenEvent

**Location**: `crates/oneshim-core/src/ports/oauth.rs` (add to existing file)

```rust
#[derive(Clone, Debug)]
pub enum TokenEvent {
    /// Token successfully refreshed.
    Refreshed {
        provider_id: String,
        expires_at: String,  // RFC3339
    },
    /// Refresh attempt failed, will retry.
    RefreshFailed {
        provider_id: String,
        attempt: u8,
        max_attempts: u8,
    },
    /// All retry attempts exhausted or terminal failure — user must re-authenticate.
    ReauthRequired {
        provider_id: String,
    },
}
```

### 3. OAuthClient Concurrent Refresh Guard

**Location**: `crates/oneshim-network/src/oauth/mod.rs` (modify existing)

**Problem**: Without a guard, the scheduler's `check_and_refresh` and an on-demand `get_access_token()` call can both trigger `try_refresh()` simultaneously, causing duplicate token endpoint requests.

**Solution**: Add an `AtomicBool` field to `OAuthClient` at construction time:

```rust
pub struct OAuthClient {
    // ... existing fields ...
    refresh_in_progress: AtomicBool,  // NEW — initialized as `false` in constructor
}
```

In `try_refresh()`:
1. `compare_exchange(false, true)` — if already `true`, return `Ok(false)` (another refresh in progress)
2. Perform the actual refresh
3. `store(false)` in a `Drop` guard

This is lightweight, requires no `Mutex`, and works with the existing `Arc<OAuthClient>` pattern. The coordinator's `Mutex<RefreshState>` handles higher-level concerns (backoff, failure counting); the `AtomicBool` is a low-level dedup guard.

### 4. `refresh_access_token()` Implementation on OAuthClient

**Location**: `crates/oneshim-network/src/oauth/mod.rs` (add to `impl OAuthPort for OAuthClient`)

The new method implements the failure taxonomy:

```rust
async fn refresh_access_token(
    &self,
    provider_id: &str,
    min_valid_for_secs: i64,
) -> Result<RefreshResult, CoreError> {
    // 1. Check if token exists
    let has_token = self.secret_store
        .retrieve(provider_id, KEY_ACCESS_TOKEN)
        .await?
        .is_some();
    if !has_token {
        return Ok(RefreshResult::NotAuthenticated);
    }

    // 2. Check if token is still fresh enough
    if let Ok(Some(expires_str)) = self.secret_store
        .retrieve(provider_id, KEY_EXPIRES_AT)
        .await
    {
        if let Ok(expires_at) = chrono::DateTime::parse_from_rfc3339(&expires_str) {
            let remaining = expires_at.signed_duration_since(Utc::now());
            if remaining.num_seconds() > min_valid_for_secs {
                return Ok(RefreshResult::AlreadyFresh {
                    expires_at: expires_str,
                });
            }
        }
    }

    // 3. Get refresh token
    let refresh_tok = self.secret_store
        .retrieve(provider_id, KEY_REFRESH_TOKEN)
        .await?;
    let Some(refresh_tok) = refresh_tok else {
        return Ok(RefreshResult::ReauthRequired {
            reason: "no refresh token stored".into(),
        });
    };

    // 4. Attempt refresh with failure classification
    let config = self.get_provider(provider_id)?;
    match token_exchange::refresh_token(&self.http, config, &refresh_tok).await {
        Ok(result) => {
            let expires_at = result.expires_in
                .map(|s| (Utc::now() + chrono::Duration::seconds(s as i64)).to_rfc3339())
                .unwrap_or_default();
            self.store_tokens(provider_id, &result).await?;
            Ok(RefreshResult::Refreshed { expires_at })
        }
        Err(e) => {
            let msg = e.to_string();
            // Terminal: server explicitly rejected the refresh token
            if msg.contains("invalid_grant") || msg.contains("invalid_client") {
                Ok(RefreshResult::ReauthRequired { reason: msg })
            } else {
                // Transient: network error, 5xx, timeout, etc.
                Ok(RefreshResult::TransientFailure { message: msg })
            }
        }
    }
}
```

### 5. Scheduler Loop

**Location**: `src-tauri/src/scheduler/loops.rs` (add 10th loop)

**Function**: `spawn_oauth_refresh_loop` (as `&self` method on `Scheduler`)

**Behavior**:
- `tokio::time::interval(Duration::from_secs(OAUTH_REFRESH_INTERVAL_SECS))` — 2-minute cycle
- Each tick: call `coordinator.check_and_refresh(&provider_id)`
- On `ReauthRequired` event: emit Tauri event `oauth-reauth-required` (with 5-min cooldown)
- `tokio::select!` with shutdown signal for clean exit

**Activation condition**: Only spawned when `AiAccessMode::ProviderOAuth` is configured and `oauth_coordinator` is `Some`.

**Config constant**: `OAUTH_REFRESH_INTERVAL_SECS: u64 = 120` in `scheduler/config.rs`

**Scheduler struct changes**:
- Add `Option<Arc<TokenRefreshCoordinator>>` field to `Scheduler`
- Add `with_oauth_coordinator(self, coordinator) -> Self` builder method (follows `with_notification_manager()` pattern)
- In `run_scheduler_loops()`: conditionally spawn 10th task, conditionally abort on shutdown

### 6. UI: Tauri Event Emission

**Location**: `src-tauri/src/scheduler/loops.rs` (within `spawn_oauth_refresh_loop`)

When `TokenEvent::ReauthRequired` is received, the scheduler loop emits a Tauri event:

```rust
// In the event_rx branch of tokio::select!
if let Ok(TokenEvent::ReauthRequired { ref provider_id }) = event {
    let should_notify = last_reauth_notify
        .map_or(true, |t| t.elapsed() > Duration::from_secs(300));
    if should_notify {
        // Emit Tauri event for frontend consumption
        if let Some(ref app_handle) = app_handle {
            let _ = app_handle.emit("oauth-reauth-required", provider_id.clone());
        }
        last_reauth_notify = Some(tokio::time::Instant::now());
    }
}
```

**Frontend listener** (OAuthConnectionPanel.tsx):
```typescript
// Listen for backend reauth event → trigger refresh
useEffect(() => {
  const unlisten = appWindow.listen('oauth-reauth-required', () => {
    refreshStatus();
  });
  return () => { unlisten.then(fn => fn()); };
}, [refreshStatus]);
```

**Note**: Full Tauri `AppHandle` threading into the scheduler is deferred if it requires non-trivial plumbing. Phase 1 logs a `warn!` and relies on the dashboard auto-refresh (60s interval) to show the red badge. The Tauri event is a follow-up improvement documented as a known limitation.

### 7. UI: Dashboard Expiry Badge

**Location**: `crates/oneshim-web/frontend/src/pages/settingSections/OAuthConnectionPanel.tsx` (modify existing)

**Changes**:
- Read `expires_at` from existing `connection_status()` API response
- Badge logic uses `expires_at` independently of the `connected` flag (since `connected` flips to `false` at 60s before expiry, which is too late for the yellow warning badge)
- Compute time remaining from `expires_at`:
  - `> 5 min`: green (connected, normal)
  - `1-5 min`: yellow badge ("expiring soon")
  - `< 1 min` or expired: red badge ("expired / re-auth required")
  - No `expires_at` or not connected: gray ("not connected")
- Auto-refresh status every 60 seconds via `setInterval`
- No new API endpoint needed — uses existing `GET /api/settings/oauth/status`

**i18n keys** (add to `en.json` and `ko.json`, inside nested `settingsOAuth` object):

```json
"statusExpiringSoon": "Token expiring soon",
"statusExpired": "Token expired",
"reauthRequired": "Re-authentication required",
"reauthDescription": "Your OAuth session has expired. Please reconnect."
```

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Refresh succeeds | Silent. Reset failure counter. Emit `Refreshed` event. |
| Terminal failure (`invalid_grant`, no refresh token) | Immediate `ReauthRequired`. No retries, no backoff. Tauri event emitted. |
| Transient failure (network, 5xx) | Increment counter, backoff (2 min / 5 min), warn log. Emit `RefreshFailed`. |
| 3 consecutive transient failures | Emit `ReauthRequired`. Tauri event. Stop retrying until auto-recovery. |
| Auto-recovery after manual re-auth | Next tick returns `AlreadyFresh` → coordinator auto-resets failure count. |
| `refresh_token` itself expired | Server returns `invalid_grant` → terminal → immediate `ReauthRequired`. |
| New `refresh_token` in response | Store in keychain (rotation support, already handled by `store_tokens`). |
| Concurrent refresh attempts | `AtomicBool` guard on `OAuthClient` deduplicates at the low level; coordinator `Mutex<RefreshState>` deduplicates at the scheduling level. |
| OAuth not configured | Refresh loop not spawned. Zero overhead. |
| No refresh token stored | `refresh_access_token` returns `ReauthRequired` immediately. |

## Data Flow

```
[Every 2 min]
Scheduler tick
  → coordinator.check_and_refresh("openai")
    → lock RefreshState (coordinator-owned Mutex)
    → check in_progress, backoff
    → oauth_port.refresh_access_token("openai", 300)
      → OAuthClient: check keychain for token + expiry
      → If expiry > 5 min: return AlreadyFresh
      → Else: POST https://auth.openai.com/oauth/token
        → Success: store tokens, return Refreshed { expires_at }
        → invalid_grant: return ReauthRequired { reason }
        → Network error: return TransientFailure { message }
    → unlock, emit TokenEvent via broadcast
    → (ReauthRequired) → emit Tauri event / warn! log

[On request]
RemoteLlmProvider.send_and_parse()
  → credential.resolve_bearer_token()
    → oauth_port.get_access_token("openai")
      → OAuthClient.try_refresh() (AtomicBool prevents if coordinator already refreshing)
      → returns cached token if refresh in progress

[On ReauthRequired]
Tauri event "oauth-reauth-required"
  → Frontend listener triggers refreshStatus()
  → Dashboard shows red badge
  → User clicks "Reconnect" → start_flow() → new OAuth flow
  → On completion → next coordinator tick sees AlreadyFresh → auto-recovery

[Auto-recovery]
Coordinator tick after manual re-auth
  → refresh_access_token returns AlreadyFresh (fresh token from new flow)
  → Coordinator detects AlreadyFresh + non-zero failure count
  → Resets consecutive_transient_failures to 0
  → Normal operation resumes
```

## Testing Strategy

### Unit Tests

All coordinator tests go in `refresh_coordinator.rs` within `#[cfg(test)] mod tests` (per project convention). Mock `OAuthPort` is a manual trait implementation (not mockall, per ADR-001 §5).

**Coordinator tests** (mock `OAuthPort`):
- Token not expiring → `NotNeeded` (via `AlreadyFresh`)
- Token expiring → triggers refresh → `Refreshed`
- Transient failure → increments counter → `Failed`
- Terminal failure (`ReauthRequired` from port) → immediate `ReauthRequired` outcome, no backoff
- 3 transient failures → `ReauthRequired`
- Concurrent call → `AlreadyInProgress`
- Backoff period → `BackingOff`
- Auto-recovery: after failure, mock returns `AlreadyFresh` → counter resets to 0

**RefreshResult classification tests** (in `OAuthClient` test module):
- `refresh_access_token` with valid, non-expiring token → `AlreadyFresh`
- `refresh_access_token` with no stored token → `NotAuthenticated`
- `refresh_access_token` with no refresh token → `ReauthRequired`
- (End-to-end refresh tests require network — covered by integration tests)

**AtomicBool guard test**:
- Verify `try_refresh()` is a no-op when guard is already set

**TokenEvent emission**:
- Verify correct events on broadcast channel for each outcome

**Backoff schedule**:
- `backoff_duration(1)` → 120s, `(2)` → 300s, `(3)` → None

### Integration Tests

- Scheduler loop spawns and calls coordinator (mock OAuthPort)
- Auto-recovery: fail 2x → manual re-auth → AlreadyFresh → counter reset

### Frontend Tests

- Expiry badge color logic: green/yellow/red/gray based on `expires_at`
- i18n keys present in both `en.json` and `ko.json`

## Files Changed

| File | Change Type | Description |
|------|-------------|-------------|
| `crates/oneshim-core/src/ports/oauth.rs` | Modify | Add `RefreshResult` enum, `TokenEvent` enum, `refresh_access_token()` to `OAuthPort`, `has_refresh_token` to `OAuthConnectionStatus` |
| `crates/oneshim-network/src/oauth/refresh_coordinator.rs` | **NEW** | Coordinator + RefreshState + RefreshOutcome |
| `crates/oneshim-network/src/oauth/mod.rs` | Modify | Add `AtomicBool` refresh guard, implement `refresh_access_token()` with failure taxonomy, export coordinator module |
| `src-tauri/src/scheduler/loops.rs` | Modify | Add `spawn_oauth_refresh_loop` (10th loop) |
| `src-tauri/src/scheduler/mod.rs` | Modify | Add `Option<Arc<TokenRefreshCoordinator>>` field + `with_oauth_coordinator()` builder + conditional spawn/abort |
| `src-tauri/src/scheduler/config.rs` | Modify | Add `OAUTH_REFRESH_INTERVAL_SECS` constant |
| `src-tauri/src/setup.rs` | Modify | Create coordinator with `Arc<dyn OAuthPort>`, wire into scheduler via builder |
| `crates/oneshim-web/frontend/.../OAuthConnectionPanel.tsx` | Modify | Expiry badge + auto-refresh + optional Tauri event listener |
| `crates/oneshim-web/frontend/.../locales/en.json` | Modify | Add 4 i18n keys |
| `crates/oneshim-web/frontend/.../locales/ko.json` | Modify | Add 4 i18n keys |

## Scope Boundaries

**In scope**: Proactive refresh, concurrent guard, failure taxonomy, retry/backoff, Tauri event notification, dashboard badge, auto-recovery.

**Out of scope**: JWT claim parsing (plan-gated models), skill activation UX, multi-provider OAuth (future — currently OpenAI only), Tauri `AppHandle` threading into scheduler (Phase 1 uses `warn!` log fallback if plumbing is non-trivial).

## References

- [RFC 6749 §6](https://datatracker.ietf.org/doc/html/rfc6749#section-6) — Refreshing an Access Token
- [RFC 6749 §5.2](https://datatracker.ietf.org/doc/html/rfc6749#section-5.2) — Error Response (defines `invalid_grant`, `invalid_client`)
- [OpenAI OAuth docs](https://platform.openai.com/docs/guides/authentication) — Token endpoint behavior
