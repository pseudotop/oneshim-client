# ADR-019 Follow-up #1 — Tauri IPC Typed Error DTO Design

**Date:** 2026-04-20
**Scope:** `src-tauri/src/commands/*.rs` (112 command signatures), `src-tauri/src/ipc_error.rs` (new), `crates/oneshim-web/frontend/src/api/desktop.ts` (or equivalent TS types)
**Origin:** ADR-019 §Known follow-ups #1 — "Tauri IPC code propagation"
**Parent ADR:** [ADR-019](../architecture/ADR-019-error-code-infrastructure.md)
**Target version:** post-ADR-019 merge (v0.4.40-rc.1+)

## Context

ADR-019 introduced `err.code() -> &'static str` — every `CoreError` variant now carries a typed `code: XxxCode` field and the wire format is locked via `tests/wire_contract_snapshot.rs`. Frontend logs, i18n, and Grafana dashboards can now key off `code` instead of regex-matching display strings.

**However, the typed code is lost at the Tauri IPC boundary.** Every command in `src-tauri/src/commands/*.rs` currently uses one of these patterns:

```rust
pub async fn foo() -> Result<Bar, String> {
    some_call().await.map_err(|e| e.to_string())?  // CoreError → String, code lost
}
```

Count: **112 command signatures** returning `Result<_, String>` across 20 command files, with **~106 callsites** using `.to_string()` to collapse errors. Frontend sees only the Display string, so if a user gets a `Config.Invalid` at the same callsite as `Config.Missing`, they're indistinguishable on the frontend.

Since ADR-019's whole value proposition to the frontend was "branch programmatically on `code`", this is a first-class gap.

## Goal

Change the Tauri IPC error boundary so `err.code()` survives the Rust → TypeScript serialization hop.

## Decision

### 1. Introduce `IpcError` DTO in `src-tauri`

```rust
// src-tauri/src/ipc_error.rs
use serde::Serialize;
use oneshim_core::error::CoreError;

/// Tauri IPC error envelope. Serializes as `{"code": "...", "message": "..."}`.
///
/// Callers outside src-tauri MUST NOT construct this type directly — use
/// `CoreError` everywhere and rely on the `From` impl at the command boundary.
#[derive(Debug, Serialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

impl From<CoreError> for IpcError {
    fn from(err: CoreError) -> Self {
        IpcError {
            code: err.code().to_string(),
            message: err.to_string(),  // Display includes "[code]" per ADR-019
        }
    }
}

// Convenience for adapter errors that already From<_> into CoreError.
impl From<oneshim_network::error::NetworkError> for IpcError {
    fn from(err: oneshim_network::error::NetworkError) -> Self {
        CoreError::from(err).into()
    }
}
// ... similar for StorageError, AutomationError, VisionError, AnalysisError,
//     SuggestionError as needed (6 more impls).
```

**Serialization shape** (stable wire contract after this lands):

```json
{ "code": "config.invalid", "message": "Configuration error [config.invalid]: bad value" }
```

### 2. Command signature migration

Replace every `Result<T, String>` with `Result<T, IpcError>`:

```rust
// Before
pub async fn get_session_info(id: String) -> Result<SessionInfo, String> {
    session_service.get(id).await.map_err(|e| e.to_string())
}

// After
pub async fn get_session_info(id: String) -> Result<SessionInfo, IpcError> {
    session_service.get(id).await.map_err(IpcError::from)
}
```

For commands that construct error strings directly (e.g., `return Err("invalid id".to_string())`), replace with:

```rust
return Err(IpcError {
    code: "validation.invalid_arguments".into(),
    message: "invalid id".into(),
});
```

**Migration policy**: prefer `.map_err(IpcError::from)` over hand-rolled strings. Only use direct construction when no upstream `CoreError` is available (e.g., input validation at command entry).

### 3. Frontend TypeScript type update

```typescript
// crates/oneshim-web/frontend/src/api/desktop.ts  (or create if missing)

export interface IpcError {
  readonly code: string;
  readonly message: string;
}

export function isIpcError(x: unknown): x is IpcError {
  return typeof x === "object" && x !== null
    && typeof (x as IpcError).code === "string"
    && typeof (x as IpcError).message === "string";
}

// Usage:
//   .catch((err) => {
//     if (isIpcError(err) && err.code === "config.invalid") { ... }
//     else if (isIpcError(err) && err.code.startsWith("network.")) { ... }
//     else { console.error(err); }
//   })
```

Existing `.catch((error) => string)` sites continue to compile because `IpcError.message` is a string; they just lose the `code` field if they stringify directly. A codemod pass can switch `error` → `isIpcError(error) ? error.message : String(error)` where meaningful.

### 4. Migration sequencing

Migrate one command file at a time, run `cargo check -p oneshim-app` after each, land as separate commits. Order by risk:

1. **Lowest risk (read-only, 20 sites)**: `onboarding.rs`, `dashboard.rs`, `coaching.rs`, `detection.rs`, `focus.rs`, `capture_status.rs`.
2. **Medium risk (state-mutating, 40 sites)**: `settings.rs`, `permissions.rs`, `automation.rs`, `sync.rs`, `audio.rs`, `suggestions.rs`.
3. **Higher risk (streaming/heavy IO, 50+ sites)**: `ai_session.rs`, `analysis.rs`, `integration.rs`, `capture.rs`, `bug_report.rs`, `error_report.rs`, `suggestion_parser.rs`.

### 5. Test strategy

**Contract test** (new): `src-tauri/tests/ipc_error_contract.rs`:

```rust
#[test]
fn ipc_error_preserves_wire_code_from_core_error() {
    let core = CoreError::Config {
        code: oneshim_core::error_codes::ConfigCode::Invalid,
        message: "bad".into(),
    };
    let ipc: IpcError = core.into();
    assert_eq!(ipc.code, "config.invalid");
    assert!(ipc.message.contains("bad"));
}

#[test]
fn ipc_error_from_adapter_error_chains_through_core() {
    let net = NetworkError::Timeout { timeout_ms: 5000 };
    let ipc: IpcError = net.into();
    assert_eq!(ipc.code, "network.timeout");
}
```

**Per-command tests**: integration tests for a representative sample (3-5 commands) verifying the `IpcError` envelope serializes correctly through the Tauri IPC layer. Use `tauri::test::mock_invoke` if available, or mock the underlying service trait.

### 6. Documentation

Add a new section to CLAUDE.md under "Coding Conventions":

```markdown
### Tauri IPC Error Boundary

Tauri commands return `Result<T, IpcError>` (not `Result<T, String>`).
Errors from service calls convert via `.map_err(IpcError::from)`.
The IPC payload shape is stable: `{"code": "...", "message": "..."}`
where `code` matches the ADR-019 wire-format registry.
```

## Consequences

### Positive
- Frontend can branch on `code` programmatically — i18n keying, user-visible messages, retry/dismiss logic.
- Observability pipelines (Grafana, log aggregators) see a stable wire code without parsing Display strings.
- Closes the last "code drops at the boundary" regression that ADR-019 intentionally deferred.

### Negative
- 112 call-site changes + new TS type means a sizable but mechanical diff.
- Minor ergonomic regression: hand-rolled `return Err("msg".into())` now requires a `code` — mostly at input validation sites.

### Neutral
- Tauri command signature becomes slightly more verbose (`IpcError` vs `String`) but enforces the contract at compile time.

## Alternatives Considered

**A. Embed code as `[code] message` prefix in the string.** Rejected — already done via `Display` impl, but then the frontend must regex-parse, which is exactly what ADR-019 set out to eliminate.

**B. Keep `Result<_, String>` but serialize via `#[serde(rename_all)]` on a tuple.** Rejected — Tauri serializes the `Err(E)` arm directly; a string cannot carry typed fields.

**C. Per-command error enums.** Rejected — would duplicate the CoreError taxonomy 100+ times and lose the single-source wire contract.

## Implementation Plan

See paired plan doc: `docs/reviews/2026-04-20-adr019-followup-ipc-error-dto-plan.md` (to be authored).

**Total effort estimate:** ~0.5 day (infrastructure + 1 command file migrated for PR1) + ~1 day (remaining 19 command files migrated + tests + frontend type) = **~1.5 day** staged across 2-3 PRs.
