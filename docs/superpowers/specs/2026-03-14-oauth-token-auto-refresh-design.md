# OAuth Token Auto-Refresh Design

## Goal

Add proactive background token refresh for OAuth-managed credentials (OpenAI Codex) with concurrent refresh protection, retry/backoff, and UI notifications for re-authentication.

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
3. **Retry/backoff** — refresh failure returns `None` silently with no retry strategy
4. **UI notification** — no way for the user to know when re-authentication is required

### OpenAI Codex Token Specifics

| Parameter | Value |
|-----------|-------|
| Token endpoint | `https://auth.openai.com/oauth/token` |
| Client ID | `app_EMoamEEZ73f0CkXaXp7hrann` |
| Recommended refresh interval | 8 minutes |
| Grant type | `refresh_token` |
| Scopes | `openid`, `profile`, `email`, `offline_access` |

## Architecture

Three components layered on the existing `OAuthClient`:

```
┌─────────────────────────────────────────────────────┐
│ Scheduler (src-tauri)                               │
│  spawn_oauth_refresh_loop (every 2 min)             │
│    └── calls coordinator.check_and_refresh()        │
├─────────────────────────────────────────────────────┤
│ TokenRefreshCoordinator (oneshim-network)            │
│  - Mutex<RefreshState> (concurrent guard)            │
│  - Exponential backoff (30s → 60s → 120s)            │
│  - 3 consecutive failures → ReauthRequired event     │
│  - tokio::broadcast for TokenEvent emission          │
├─────────────────────────────────────────────────────┤
│ OAuthClient (existing)                               │
│  - try_refresh() → token_exchange::refresh_token()   │
│  - get_access_token() → now shares RefreshState      │
│  - store_tokens_static() → keychain persistence      │
├─────────────────────────────────────────────────────┤
│ UI Layer                                             │
│  - Desktop toast: ReauthRequired only (5 min cooldown)│
│  - Dashboard: expiry badge (yellow <5min, red expired)│
└─────────────────────────────────────────────────────┘
```

## Component Design

### 1. TokenRefreshCoordinator

**Location**: `crates/oneshim-network/src/oauth/refresh_coordinator.rs` (new file)

**Responsibility**: Orchestrate proactive token refresh with concurrency protection and event emission.

**Struct**:

```rust
pub struct TokenRefreshCoordinator {
    oauth_client: Arc<OAuthClient>,
    state: Mutex<RefreshState>,
    event_tx: broadcast::Sender<TokenEvent>,
}

struct RefreshState {
    in_progress: bool,
    consecutive_failures: u8,
    last_attempt: Option<Instant>,
    backoff_until: Option<Instant>,
}
```

**Public API**:

- `new(oauth_client, event_tx)` — constructor
- `check_and_refresh(provider_id) -> Result<RefreshOutcome, CoreError>` — main entry point
- `subscribe() -> broadcast::Receiver<TokenEvent>` — event subscription
- `reset_failure_state(provider_id)` — called on successful manual re-auth

**`check_and_refresh` flow**:

1. Acquire `RefreshState` lock
2. If `in_progress == true`, return `RefreshOutcome::AlreadyInProgress`
3. If `now < backoff_until`, return `RefreshOutcome::BackingOff`
4. Call `oauth_client.connection_status(provider_id)`
5. If `expires_at` is more than 5 minutes away, return `RefreshOutcome::NotNeeded`
6. Set `in_progress = true`, release lock
7. Call `oauth_client.get_access_token(provider_id)` (triggers internal refresh)
8. Re-acquire lock:
   - **Success**: reset `consecutive_failures` to 0, emit `TokenEvent::Refreshed`
   - **Failure**: increment `consecutive_failures`, calculate backoff, emit `TokenEvent::RefreshFailed`
   - **3 consecutive failures**: emit `TokenEvent::ReauthRequired`
9. Set `in_progress = false`

**Backoff schedule**: 30s after 1st failure, 60s after 2nd, 120s after 3rd.

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
    /// All retry attempts exhausted — user must re-authenticate.
    ReauthRequired {
        provider_id: String,
    },
}
```

### 3. Scheduler Loop

**Location**: `src-tauri/src/scheduler/loops.rs` (add 10th loop)

**Function**: `spawn_oauth_refresh_loop`

**Parameters**: `coordinator: Arc<TokenRefreshCoordinator>`, `provider_id: String`, `shutdown_rx`

**Behavior**:
- `tokio::time::interval(Duration::from_secs(120))` — 2-minute cycle
- Each tick: call `coordinator.check_and_refresh(&provider_id)`
- Log outcome at `debug!` level (success) or `warn!` level (failure)
- `tokio::select!` with shutdown signal for clean exit

**Activation condition**: Only spawned when `AiAccessMode::ProviderOAuth` is configured.

**Config constant**: `OAUTH_REFRESH_INTERVAL_SECS: u64 = 120` in `scheduler/config.rs`

### 4. OAuthClient Integration

**Location**: `crates/oneshim-network/src/oauth/mod.rs` (modify existing)

**Changes**:
- `get_access_token()` delegates to the same `RefreshState` mutex when triggering on-demand refresh
- This prevents the scheduler's proactive refresh and an on-demand refresh from racing
- Implementation: `OAuthClient` gains an `Option<Arc<Mutex<RefreshState>>>` field, set by coordinator after construction
- If `RefreshState` is `None` (no coordinator attached), behaves exactly as today (backward compatible)

### 5. UI: Desktop Toast

**Location**: `src-tauri/src/scheduler/loops.rs` (within the refresh loop, or a separate event listener task)

**Behavior**:
- Subscribe to `TokenEvent` broadcast channel
- On `ReauthRequired`: send desktop notification via `DesktopNotifier` port
- Message: "OAuth authentication expired — please re-login" (i18n key: `oauthReauthRequired`)
- Cooldown: 5 minutes between repeated notifications (reuse existing `NotificationManager` cooldown pattern)

### 6. UI: Dashboard Expiry Badge

**Location**: `crates/oneshim-web/frontend/src/pages/settingSections/OAuthConnectionPanel.tsx` (modify existing)

**Changes**:
- Read `expires_at` from existing `connection_status()` API response
- Compute time remaining:
  - `> 5 min`: green (connected, normal)
  - `1-5 min`: yellow badge ("expiring soon")
  - `< 1 min` or expired: red badge ("expired")
  - Not connected: gray ("not connected")
- Auto-refresh status every 60 seconds via `setInterval`
- No new API endpoint needed — uses existing `GET /api/settings/oauth/status` → Tauri IPC `get_oauth_status`

**i18n keys** (add to `en.json` and `ko.json`):

```json
"settingsOAuth.statusExpiringSoon": "Token expiring soon",
"settingsOAuth.statusExpired": "Token expired",
"settingsOAuth.reauthRequired": "Re-authentication required",
"settingsOAuth.reauthDescription": "Your OAuth session has expired. Please reconnect."
```

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Refresh succeeds | Silent. Reset failure counter. Emit `Refreshed` event. |
| Transient failure (network) | Increment counter, backoff, warn log. Emit `RefreshFailed`. |
| 3 consecutive failures | Emit `ReauthRequired`. Desktop toast. Stop retrying until manual re-auth. |
| `refresh_token` itself expired | Server returns 401/error. Treated as failure → eventually `ReauthRequired`. |
| New `refresh_token` in response | Store in keychain (rotation support, already handled by `try_refresh`). |
| Concurrent refresh attempts | Second caller gets `AlreadyInProgress`, does not trigger duplicate request. |
| OAuth not configured | Refresh loop not spawned. Zero overhead. |

## Data Flow

```
[Every 2 min]
Scheduler tick
  → coordinator.check_and_refresh("openai")
    → lock RefreshState
    → check in_progress, backoff, expiry
    → oauth_client.get_access_token("openai")
      → try_refresh()
        → POST https://auth.openai.com/oauth/token
        → store new tokens in keychain
    → unlock, emit TokenEvent

[On request]
RemoteLlmProvider.send_and_parse()
  → credential.resolve_bearer_token()
    → oauth_client.get_access_token("openai")
      → shares same RefreshState mutex
      → if refresh already in_progress, waits briefly or returns cached token

[On ReauthRequired]
Event listener
  → DesktopNotifier.notify("Re-auth required")
  → Dashboard polls connection_status() → shows red badge
  → User clicks "Reconnect" → start_flow() → new OAuth flow
  → On completion → coordinator.reset_failure_state()
```

## Testing Strategy

### Unit Tests

- **RefreshState transitions**: 0→1→2→3 failures, backoff calculation, reset on success
- **Coordinator.check_and_refresh**: mock OAuthClient
  - Token not expiring → `NotNeeded`
  - Token expiring → triggers refresh → `Refreshed`
  - Refresh fails → increments counter → `RefreshFailed`
  - 3 failures → `ReauthRequired`
  - Concurrent call → `AlreadyInProgress`
  - Backoff period → `BackingOff`
- **TokenEvent emission**: verify correct events on broadcast channel

### Integration Tests

- Scheduler loop spawns and calls coordinator (mock OAuthClient)
- Desktop notification fires on `ReauthRequired` (mock DesktopNotifier)

### Frontend Tests

- Expiry badge color logic: green/yellow/red/gray based on `expires_at`
- i18n keys present in both `en.json` and `ko.json`

## Files Changed

| File | Change Type | Description |
|------|-------------|-------------|
| `crates/oneshim-network/src/oauth/refresh_coordinator.rs` | **NEW** | Coordinator, RefreshState, RefreshOutcome |
| `crates/oneshim-network/src/oauth/mod.rs` | Modify | Export coordinator, integrate RefreshState into get_access_token |
| `crates/oneshim-core/src/ports/oauth.rs` | Modify | Add `TokenEvent` enum |
| `src-tauri/src/scheduler/loops.rs` | Modify | Add `spawn_oauth_refresh_loop` |
| `src-tauri/src/scheduler/mod.rs` | Modify | Register 10th loop |
| `src-tauri/src/scheduler/config.rs` | Modify | Add `OAUTH_REFRESH_INTERVAL_SECS` constant |
| `src-tauri/src/setup.rs` | Modify | Create coordinator, wire DI |
| `crates/oneshim-web/frontend/.../OAuthConnectionPanel.tsx` | Modify | Expiry badge + auto-refresh |
| `crates/oneshim-web/frontend/.../locales/en.json` | Modify | Add 4 i18n keys |
| `crates/oneshim-web/frontend/.../locales/ko.json` | Modify | Add 4 i18n keys |

## Scope Boundaries

**In scope**: Proactive refresh, concurrent guard, retry/backoff, toast notification, dashboard badge.

**Out of scope**: JWT claim parsing (plan-gated models), skill activation UX, multi-provider OAuth (future — currently OpenAI only).
