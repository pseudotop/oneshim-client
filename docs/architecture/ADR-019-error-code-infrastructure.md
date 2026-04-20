# ADR-019: Error Code Infrastructure + AWS Bedrock Intentional Non-Support

- **Status**: Accepted (2026-04-19)
- **Supersedes**: none
- **Related**: ADR-001 (error strategy), ADR-003 (directory module pattern)
- **Implementation**: `docs/superpowers/specs/2026-04-19-error-code-infrastructure-design.md`, `docs/superpowers/plans/2026-04-19-error-code-infrastructure.md`

## Context

The ONESHIM client-rust workspace (14 crates, ~1,150 `CoreError` construction sites) had no error-code convention. Telemetry (Grafana), i18n (frontend ko/en), and audit logs all benefit from stable machine-readable error identifiers. Separately, AWS Bedrock was listed as a supported provider surface but implementation was incomplete (no Signature V4 auth), producing a security bug in OCR (no-auth fallthrough on `ProviderAuthScheme::AwsSignatureV4`).

Two needs converged: introduce error-code infrastructure AND ship Bedrock as the first "intentionally unsupported" first-class citizen.

## Decision

### 1. Error code infrastructure

- 18 code enums (`ConfigCode`, `NetworkCode`, ..., `GuiCode`) defined via a single `define_code_enum!` macro that generates enum body, `as_str` match, `Display` impl, and `all()` enumerator from one variant list.
- Every struct-variant of `CoreError` and `GuiInteractionError` carries a typed `code` field; `#[from]`-wrapped external error types (`Serialization`, `Io`) derive their code via `impl code()` per §7.
- Unified accessor `err.code() -> &'static str` for telemetry/logs/i18n.
- Wire-format codes follow `{domain}.{category}[.{qualifier}]` convention.
- Released code strings are immutable (wire contract). New codes append; renames require an RFC PR.

### 2. Naming convention

```
{domain}.{category}[.{qualifier}[.{sub_qualifier}]]
all lowercase, snake_case, dot-separated
```

Examples: `config.invalid`, `network.timeout`, `provider.bedrock.unsupported`.

Wire-code `{domain}` prefix usually matches the Rust enum category (e.g., `ConfigCode::Invalid` → `config.invalid`). The `UnsupportedProviderBedrock` variant is a deliberate exception: it lives on `ConfigCode` (because Bedrock-unsupported surfaces at config-load/request-build time) but carries a `provider.*` wire code so observability dashboards can group all provider-unsupported signals into one namespace without crossing the `config.*` vs `provider.*` boundary. The file-level docstring in `crates/oneshim-core/src/error_codes/config.rs` records this exception.

### 3. AWS Bedrock: intentionally unsupported

- Bedrock vendor + `provider_surface.bedrock.direct_api` surface **removed** from `specs/providers/provider-surface-catalog.json`.
- 8 match arms across `oneshim-network` return `CoreError::Config { code: ConfigCode::UnsupportedProviderBedrock, .. }`:
  - `ai_ocr_client/mod.rs` (2 arms: auth + BedrockConverse request shape)
  - `ai_ocr_client/strategy.rs` (1 arm: strategy selection)
  - `ai_llm_client/request.rs` (3 arms: request build + auth + response parse)
  - `http_api_session/mod.rs` (2 arms: auth + BedrockConverse request shape — 2nd arm added during post-merge drift audit; previously the wildcard `_` arm returned `InternalCode::Generic` for BedrockConverse, silently mislabeling it)
- Enum variants `AiProviderType::Bedrock`, `ProviderAuthScheme::AwsSignatureV4`, `ProviderRequestShape::BedrockConverse` **retained** (runtime-unreachable after catalog delete) for minimal-churn future re-introduction path.
- OCR `apply_auth_headers` signature changed from infallible to `Result<_, CoreError>` to close the silent no-auth fallthrough security bug.
- Defense-in-depth guards in sibling client paths that bypass the 8 match arms above — both added during post-merge drift audit, both return the same `CoreError::Config { code: UnsupportedProviderBedrock, .. }`:
  - `crates/oneshim-network/src/analysis_client.rs::analyze` — early-return guard prevents silently sending OpenAI-format request with Bearer auth to a Bedrock endpoint.
  - `crates/oneshim-web/src/services/ai_model_catalog_web_service.rs::list_models` — early-return guard fires **before** `resolve_model_discovery_api_key()` so users without AWS credentials see the graceful unsupported notice rather than a generic "no API key" error.

### 4. Migration strategy (soft V1→V2)

4 phases / 16 PRs / 2-3 weeks realized as a single branch:
1. Phase 1: introduce V2 variants alongside V1 (deprecated).
2. Phase 2: 13 per-crate retrofits (12 crates + 1 verification-only sandbox-worker).
3. Phase 3: C5 Bedrock skip + this ADR.
4. Phase 4: V1 deletion + V2 → canonical rename (rust-analyzer LSP, not sed).

CI deprecation gating warn-only through Phase 3 (`-A deprecated` escape hatch in lefthook clippy). Phase 4 removes the escape hatch, restoring `-D warnings` (Rust's `deprecated` lint defaults to warn, so `-D warnings` fails CI on any remaining V1 usage).

### 5. Bedrock re-introduction checklist

If a future use case requires Bedrock support:

1. AWS Signature V4 signing implementation (`aws-sigv4` crate or equivalent).
2. AWS credential loader (`access_key` / `secret_key` / optional `session_token`).
3. Settings UI field for AWS credentials.
4. Re-add Bedrock vendor + surface to `provider-surface-catalog.json`.
5. Replace the 7 Bedrock match arms (currently returning `ConfigCode::UnsupportedProviderBedrock`) with working Bedrock handlers.
6. Live smoke test for Bedrock path (`--ignored`).
7. Update the wire-format snapshot fixture if new codes are added.
8. Remove `ConfigCode::UnsupportedProviderBedrock` from `ConfigCode` (follows wire-immutability deletion procedure — RFC PR required).

### 6. Public-API Exhaustiveness

`CoreError` and `GuiInteractionError` are **NOT** marked `#[non_exhaustive]`.

Rationale:
1. Both are internal to this workspace (14-member); all consumers are first-party.
2. Exhaustive matching catches forgotten variants during refactors — a feature, not a bug.
3. `err.code()` provides a forward-compat channel that does not require pattern matching.
4. If this library is ever extracted / published, this decision is reversible with a one-line change + downstream `match` site review.

Code enums (`ConfigCode`, `NetworkCode`, etc.) **ARE** `#[non_exhaustive]` because:
- They are internal-use but could grow with follow-ups.
- Protecting within-workspace consumers from variant-addition breakage is cheap and defensive.

### 7. Adding new `#[from]` variants

When a new `#[from]`-wrapped external error type is added to `CoreError`:

1. Allocate an `InternalCode::*` variant for the new type (e.g., `InternalCode::TokioIo` if adding `tokio::io::Error`).
2. Add the variant + `#[from]` attribute in the same PR.
3. Add the corresponding arm in `impl CoreError::code()` returning the new `InternalCode`.
4. Update `wire_contract_snapshot.expected.txt` fixture.

## Consequences

### Positive

- Machine-readable error identifiers enable Grafana label grouping.
- `err.code()` unlocks i18n (frontend consumes code as translation key).
- Bedrock UX deterministic: no silent fallthrough, catalog does not advertise the provider.
- Type-safe code registry; impossible to drift wire format from source via single-source macro + snapshot test.

### Negative

- 2-3 week migration effort; V1/V2 coexistence adds enum variant count temporarily (Phases 1-3).
- ~133 `#[deprecated]` warnings during migration window (expected signal, not regression).

### Neutral

- Phase 4 rename requires brief freeze on in-flight `CoreError` / `GuiInteractionError` PRs.
- Post-Phase-4 Grafana dashboard relabeling is a follow-up (not blocking).

### Known follow-ups (not blocking)

1. **Tauri IPC code propagation** — `src-tauri/src/commands/*.rs` uses `.map_err(|e| e.to_string())` at ~58 callsites, converting `CoreError` into a plain `String` for the frontend. The typed `err.code()` is lost in this boundary. Follow-up: introduce `IpcError { code: String, message: String }` DTO and update callsites; allow the frontend to branch programmatically on `code` instead of substring-matching the display message. Scope: ~0.5 day; independent of this ADR.
2. **Grafana dashboard relabeling** — replace message-regex panels with `code`-label group-by once log pipelines surface `err.code()`. ~0.5 day.
3. **Frontend i18n wiring** — feed `err.code()` strings as translation keys in the frontend i18n layer. Requires a fallback message when a key is missing. ~1 day.
4. **`Internal` code granularity refinement** — at Phase 4 end, `InternalCode` has `Generic`, `Io`, `Serialization`. Post-merge drift audit iters 88~109 re-routed ~122 Internal emissions to more specific variants (Config.Missing/Invalid/OutOfRange, NotFound, ServiceUnavailable, InvalidArguments, Analysis, OcrError). Current Internal callsite count: ~294 (was ~416 at Phase 4 end). Further subdivision driven by production telemetry signals remains evergreen.
5. **Sandbox variant consolidation** — `SandboxInit` + `SandboxExecution` + `SandboxUnsupported` + `ExecutionTimeout` overlap semantically; could unify under a single variant. Separate refactor, not blocking.
6. **`sync/lan_transport::authenticate_with_peer` regression tests** — LAN sync enforces TLS-only, so mockito HTTP mock doesn't work. Writing a rustls-TlsAcceptor test fixture with generated certs is feasible but disproportionate for a defensive code path (semantic mapping is implemented per the canonical HTTP status pattern; see `docs/guides/http-status-error-mapping.md` for the pattern registry and deferral rationale).

### Post-merge test coverage additions

Between the initial ADR and the current state, 85+ regression tests were added covering the semantic HTTP status mapping across 14 of 15 dispatchers. Each test verifies a specific status-code → CoreError variant mapping AND (for most dispatchers) a domain-fallback assertion. See [`docs/guides/http-status-error-mapping.md`](../guides/http-status-error-mapping.md) for the canonical pattern and the full dispatcher registry. The 15th dispatcher (`auth::refresh`) was added iter-98 with 5 regression tests (401/429/503/504/500).

### Post-merge orphan wire-code cleanup (pre-merge)

During final drift audit, 3 wire codes and 2 `CoreError` variants were identified as declared-but-never-constructed and removed before merge per YAGNI:

- `CoreError::BinaryHashMismatch` + `IntegrityCode::HashMismatch` (+ entire `IntegrityCode` enum) — Binary integrity is handled inside the updater via `UpdateError::Integrity`; this `CoreError` variant has no construction site since v0.1.0 and no `From<UpdateError> for CoreError` conversion. The whole enum file was deleted.
- `CoreError::ProcessNotAllowed` + `PolicyCode::ProcessDenied` — Redundant with `PolicyDenied` (same field shape, only display-text differed). All automation paths emit `PolicyDenied`; zero construction sites for `ProcessNotAllowed`.
- `NetworkCode::Failed` — Reserved for connection-level failure (per docstring intent) but never wired up; all non-timeout network errors use `NetworkCode::Generic`. Kept `NetworkCode::Generic` as the canonical fallback.

Wire snapshot: 57 → 54 codes (iter-87). Code enum count: 19 → 18 (iter-87). Removed entirely pre-merge since the wire contract hasn't yet been released to any external consumer. If any of these semantics resurface as a real need post-merge, normal wire-immutability procedure applies (append, don't replace).

Continued orphan cleanup in later iterations:
- **iter-148**: `GuiCode::Generic` / `gui.generic` — 0 emission sites; `GuiInteractionError::Internal` always uses `GuiCode::InternalError`. Snapshot 54 → 53.
- **iter-161**: 11 additional `*Code::Generic` placeholder variants (audio/config/consent/oauth/permission/policy/provider/secret/service/storage/validation) — all Phase 2 boilerplate with 0 emission sites after Phase 4 complete. Snapshot 53 → 42. Retained: `auth.generic` (1 site), `internal.generic` (Internal fallback, hundreds of sites), `network.generic` (HTTP status fallback, ~70 sites).

Current wire snapshot: **42 codes**.
