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

- 19 code enums (`ConfigCode`, `NetworkCode`, ..., `GuiCode`) defined via a single `define_code_enum!` macro that generates enum body, `as_str` match, `Display` impl, and `all()` enumerator from one variant list.
- Every `CoreError` and `GuiInteractionError` variant carries a typed `code` field.
- Unified accessor `err.code() -> &'static str` for telemetry/logs/i18n.
- Wire-format codes follow `{domain}.{category}[.{qualifier}]` convention.
- Released code strings are immutable (wire contract). New codes append; renames require an RFC PR.

### 2. Naming convention

```
{domain}.{category}[.{qualifier}[.{sub_qualifier}]]
all lowercase, snake_case, dot-separated
```

Examples: `config.invalid`, `network.timeout`, `provider.bedrock.unsupported`.

### 3. AWS Bedrock: intentionally unsupported

- Bedrock vendor + `provider_surface.bedrock.direct_api` surface **removed** from `specs/providers/provider-surface-catalog.json`.
- 7 match arms across `oneshim-network` return `CoreError::Config { code: ConfigCode::UnsupportedProviderBedrock, .. }`:
  - `ai_ocr_client/mod.rs` (2 arms: auth + BedrockConverse request shape)
  - `ai_ocr_client/strategy.rs` (1 arm: strategy selection)
  - `ai_llm_client/request.rs` (3 arms: request build + auth + response parse)
  - `http_api_session/mod.rs` (1 arm: auth)
- Enum variants `AiProviderType::Bedrock`, `ProviderAuthScheme::AwsSignatureV4`, `ProviderRequestShape::BedrockConverse` **retained** (runtime-unreachable after catalog delete) for minimal-churn future re-introduction path.
- OCR `apply_auth_headers` signature changed from infallible to `Result<_, CoreError>` to close the silent no-auth fallthrough security bug.

### 4. Migration strategy (soft V1→V2)

4 phases / 16 PRs / 2-3 weeks realized as a single branch:
1. Phase 1: introduce V2 variants alongside V1 (deprecated).
2. Phase 2: 13 per-crate retrofits (12 crates + 1 verification-only sandbox-worker).
3. Phase 3: C5 Bedrock skip + this ADR.
4. Phase 4: V1 deletion + V2 → canonical rename (rust-analyzer LSP, not sed).

CI deprecation gating warn-only through Phase 3 (`-A deprecated` in lefthook clippy), flips to `-D deprecated` at Phase 4.

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
