# Provider Surface Single-PR Review Checklist

Date: 2026-03-16
Status: Draft

Companion documents:
- [2026-03-15-provider-surface-architecture-design.md](/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/docs/superpowers/specs/2026-03-15-provider-surface-architecture-design.md)
- [2026-03-15-provider-surface-migration-plan.md](/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/docs/superpowers/plans/2026-03-15-provider-surface-migration-plan.md)
- [2026-03-15-credential-backend-migration-plan.md](/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/docs/superpowers/plans/2026-03-15-credential-backend-migration-plan.md)

## Purpose

This checklist is for the final single PR that combines:

- credential backend migration work
- secret projection and bridge lifecycle work
- provider surface catalog migration
- subprocess CLI capability/runtime work
- provider surface UI/UX work

The goal is to keep review rigorous even when the implementation lands as one logical PR.

## Intended Scope

The combined PR should include, at minimum, the following completed tracks:

1. BYOK/API key secret backend metadata and migration support
2. env / temp-file / bridge projection lifecycle
3. provider surface catalog as the canonical authoring source
4. `surface_id`-aware settings and model discovery
5. feature capability + maturity snapshot
6. subprocess CLI runtime foundation
7. explicit UI/UX handling for:
   - direct API
   - installed provider CLI
   - direct OAuth compatibility

## Commit Groups To Audit

### Group A: Credential Backend / Projection

Representative commits:
- `46ee6be` scaffold backend-managed provider credentials
- `9d2060e` provider env projection templates
- `7db5430` CLI env projection
- `7330aba` projected provider secret exec
- `87e37e4` backend-managed API key state in settings
- `013b28d` selectable provider secret backends
- `3642922` settings defaults aligned to backend capabilities
- `0d46fa5` explicit CLI bridge lifecycle commands
- `a658634` projection opt-in metadata controls
- `9957586` opt-in gate for managed secret projection
- `393630b` preserve selected backend during migration
- `4c13f51` provider credential migration CLI
- `95c6e05` provider credential status CLI
- `5b7fce6` unsupported projection metadata validation
- `bb4a591` env-backed provider credentials by profile
- `e2b1bb6` binding backend store selection
- `f96270d` stop plaintext fallback for bound secrets
- `d195b5a` env-backed provider secret settings display
- `e8def8c` temp-file projection collision/fallback fixes
- `ac28d19` backend init and settings mutation hardening

### Group B: Provider Surface Catalog / Runtime

Representative commits:
- `7f3ebc0` normalized provider spec catalog
- `3512cd3` shared runtime provider specs
- `add0870` provider spec semantic validation
- `3552e8b` provider defaults refresh
- `abe93b2` feature capability snapshot
- `3c2a077` subprocess CLI LLM runtime foundation
- `2af4f3a` explicit provider surface metadata persistence
- `abce6d7` refreshed provider preset defaults by surface
- `e1d6957` provider surface catalog v2
- `5a4e80e` replace provider spec v1 with surface catalog
- `6b70014` resolve HTTP provider surfaces from surface ids
- `29ea07c` surface ids through model discovery
- `1f6882e` model lifecycle by provider surface
- `dd75a98` surface-aware model discovery / CLI selection hardening
- `3b91460` subprocess auth probe hardening
- `00acecb` settings defaults from provider surfaces
- `ad64374` explicit provider surfaces
- `a2de97e` CLI capabilities driven from provider surfaces
- `136ea13` explicit related provider surface links

### Group C: Surface-Aware UI/UX

Representative commits:
- `79fc604` explicit provider surface settings UX
- `b2f408b` provider surface operations UX

## Review Gate 1: Contract Integrity

Must verify:

- `surface_id` is canonical when present
- `provider_type` remains compatibility-only, not the semantic source of truth
- `ProviderSurfaceSpec` and frontend contract shape match exactly
- `related_surface_ids` are validated:
  - not empty
  - not self-referential
  - same vendor unless explicitly allowed in the future
- no remaining v1 preset assumptions are required for runtime correctness

Primary files:
- `crates/oneshim-api-contracts/src/provider_specs.rs`
- `crates/oneshim-api-contracts/src/settings.rs`
- `crates/oneshim-web/frontend/src/api/contracts.ts`
- `specs/providers/provider-surface-catalog.json`

## Review Gate 2: Hexagonal Boundary Check

Must verify:

- `oneshim-core` ports remain the only cross-layer contract
- `oneshim-web` does not become the source of provider-surface semantics
- subprocess runtime details are not re-encoded inside HTTP adapters
- settings handlers stay thin and do not own domain/provider logic

Primary files:
- `src-tauri/src/provider_adapters.rs`
- `src-tauri/src/subprocess_provider.rs`
- `crates/oneshim-network/src/ai_llm_client.rs`
- `crates/oneshim-network/src/ai_ocr_client.rs`
- `crates/oneshim-web/src/services/settings_service.rs`

## Review Gate 3: Secret Backend Behavior

Must verify:

- BYOK/API key settings no longer silently depend on plaintext config once a backend binding exists
- `legacy_config` is the only path that still permits plaintext behavior
- `os_secret_store`, `file_secret_store`, `env`, and `bridge_managed` semantics are distinct and explicit
- settings round-trip does not silently mutate backend kind or auth mode
- secret display hints are honest:
  - backend-managed secrets are shown as backend-managed
  - env-backed secrets are not misreported as stored locally

Primary files:
- `crates/oneshim-core/src/ports/credential_source.rs`
- `crates/oneshim-core/src/ports/secret_store.rs`
- `crates/oneshim-storage/src/file_secret_store.rs`
- `crates/oneshim-storage/src/env_secret_store.rs`
- `crates/oneshim-web/src/services/settings_service.rs`
- `src-tauri/src/provider_secret_backend.rs`

## Review Gate 4: Projection / Bridge Lifecycle

Must verify:

- managed projection never degrades silently to unmanaged plaintext temp files
- temp-file projection consumer identity is profile-aware and collision-safe
- bridge lifecycle commands are explicit and safe:
  - sync
  - revoke
- ONESHIM only deletes ONESHIM-managed bridge artifacts
- projection requires opt-in where intended

Primary files:
- `crates/oneshim-core/src/ports/secret_projection.rs`
- `crates/oneshim-storage/src/process_env_projection.rs`
- `crates/oneshim-storage/src/temp_file_projection.rs`
- `src-tauri/src/secret_cli.rs`
- `src-tauri/src/bridge_cli.rs`
- `src-tauri/src/cli_subscription_bridge.rs`

## Review Gate 5: Subprocess CLI Runtime

Must verify:

- `ProviderSubscriptionCli` is no longer just a label over local execution
- subprocess capability probing and runtime routing use the same surface catalog source
- unsupported subprocess surfaces remain `partially_available`, not falsely `available`
- missing runtime adapter surfaces do not overclaim support
- CLI auth/status probing has bounded runtime and does not stall startup

Primary files:
- `src-tauri/src/subprocess_provider.rs`
- `src-tauri/src/feature_capabilities.rs`
- `src-tauri/src/provider_adapters.rs`
- `specs/providers/provider-surface-catalog.json`

## Review Gate 6: Feature Capability / Maturity

Must verify:

- `direct_http` surfaces are surfaced as stable/available by default
- installed CLI surfaces are marked preferred only when product direction says so
- direct OAuth is always shown as experimental and non-preferred
- `status_copy_key` is used instead of hardcoded UI explanations where possible
- UI never claims a path is ready when capability says otherwise

Primary files:
- `src-tauri/src/feature_capabilities.rs`
- `src-tauri/src/commands.rs`
- `crates/oneshim-web/frontend/src/features/featureCapabilities.ts`
- `crates/oneshim-web/frontend/src/pages/settingSections/AiAutomationTab.tsx`
- `crates/oneshim-web/frontend/src/pages/settingSections/OAuthConnectionPanel.tsx`

## Review Gate 7: Settings UX

Must verify:

- `access_mode` is explicit in the UI
- surface selection is explicit and catalog-driven
- direct API, managed OAuth, and subprocess CLI render different forms
- current path, requirements, readiness, and recommended alternative path are visible
- switching to the preferred CLI path aligns:
  - `access_mode`
  - `surface_id`
  - auth/backend metadata
- no user-visible hardcoded strings bypass i18n

Primary files:
- `crates/oneshim-web/frontend/src/pages/Settings.tsx`
- `crates/oneshim-web/frontend/src/pages/settingSections/AiAutomationTab.tsx`
- `crates/oneshim-web/frontend/src/pages/settingSections/OAuthConnectionPanel.tsx`
- `crates/oneshim-web/frontend/src/i18n/locales/en.json`
- `crates/oneshim-web/frontend/src/i18n/locales/ko.json`

## Review Gate 8: Model Discovery / Lifecycle Policy

Must verify:

- model discovery keys on `surface_id`
- unsupported catalog strategies fail honestly instead of silently falling back
- lifecycle policy is applied by `surface_id + model`
- default model selection is surface-specific, not provider-family-specific
- subprocess-only models do not leak into unrelated direct API defaults

Primary files:
- `crates/oneshim-web/src/services/ai_model_catalog_service.rs`
- `crates/oneshim-web/src/services/ai_provider_spec_service.rs`
- `crates/oneshim-core/src/ai_model_lifecycle_policy.rs`
- `crates/oneshim-network/src/ai_llm_client.rs`
- `specs/providers/provider-surface-catalog.json`

## Review Gate 9: Compatibility Removal

Must verify:

- `/api/ai/providers/presets` is fully removed from live paths
- remaining compatibility logic is intentional and minimal
- v1 provider spec structures are not still required by runtime code
- `surface_id` drives selection in new code paths

Primary files:
- `crates/oneshim-web/src/routes.rs`
- `crates/oneshim-web/frontend/src/api/client.ts`
- `crates/oneshim-web/frontend/src/api/standalone.ts`
- `crates/oneshim-web/src/services/ai_provider_spec_service.rs`

## Review Gate 10: Documentation And Generated Contracts

Must verify:

- HTTP manifest matches route surface
- OpenAPI snapshot matches route surface
- provider-surface architecture docs reflect current implementation
- experimental OAuth positioning matches current product direction

Primary files:
- `docs/contracts/http-interface-manifest.v1.json`
- `docs/contracts/oneshim-web.v1.openapi.yaml`
- `docs/superpowers/specs/2026-03-15-provider-surface-architecture-design.md`
- `docs/superpowers/plans/2026-03-15-provider-surface-migration-plan.md`

## Required Validation Commands

Run before final PR review sign-off:

```bash
cargo check --workspace
cargo test -p oneshim-api-contracts provider_specs -- --nocapture
cargo test -p oneshim-web ai_provider_spec_service --lib -- --nocapture
cargo test -p oneshim-web ai_model_catalog_service --lib -- --nocapture
cargo test -p oneshim-app subprocess_provider -- --nocapture
cargo test -p oneshim-app feature_capabilities -- --nocapture
cargo test -p oneshim-app secret_cli -- --nocapture
npm run test -- providerSurfaces oauth-panel-support get-feature-capabilities featureCapabilities
npm run build
./scripts/verify-web-contract-boundary.sh
./scripts/verify-http-interface-manifest.sh
./scripts/verify-http-openapi-sync.sh
git diff --check
```

## Blockers That Still Require Explicit Attention

The final PR should not be approved unless these are answered explicitly:

1. Is Gemini subprocess still intentionally `partially_available` because runtime adapter work is incomplete?
2. Is direct OAuth still clearly marked experimental everywhere it appears?
3. Are any remaining plaintext BYOK writes still possible outside `legacy_config`?
4. Are all CLI-backed operational recommendations derived from catalog/capability data rather than vendor naming heuristics?
5. Are all user-visible strings in the new settings/provider UX routed through i18n resources?

## Approval Standard

The single PR is acceptable only if all of the following are true:

- no boundary violations are introduced
- no secret handling silently downgrades to weaker storage or projection behavior
- no provider-family fallback bypasses explicit `surface_id`
- UI accurately communicates path maturity and readiness
- subprocess support is not overstated
- compatibility-only OAuth remains experimental and non-preferred
