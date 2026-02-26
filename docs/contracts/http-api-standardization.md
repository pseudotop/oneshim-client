[English](./http-api-standardization.md) | [한국어](./http-api-standardization.ko.md)

# HTTP API Standardization (OpenAPI Track)

This document defines how ONESHIM standardizes local web interfaces toward OpenAPI-compatible governance.

## Current SSOT

- Transport DTO SSOT: `crates/oneshim-api-contracts`
- Runtime route SSOT: `crates/oneshim-web/src/routes.rs`
- Machine-readable interface snapshot: `docs/contracts/http-interface-manifest.v1.json`

## Decision

1. Keep transport DTO ownership in `oneshim-api-contracts`; handlers must not define public DTOs.
2. Keep route ownership in `oneshim-web/src/routes.rs`.
3. Maintain a versioned interface manifest (`http.interface.manifest.v1`) as the bridge artifact for OpenAPI.
4. Enforce sync between routes and manifest in CI (`scripts/verify-http-interface-manifest.sh`).
5. Use additive evolution and explicit version bumps for breaking HTTP contract changes.

## Why this shape

- Preserves DDD/Hexagonal boundaries without pushing web-only metadata into domain crates.
- Gives immediate machine-checkable interface governance before full OpenAPI generation rollout.
- Supports provider growth (AI, automation, update, diagnostics) with explicit endpoint/module ownership.

## Next phase (planned)

1. Add a build-time OpenAPI generator stage from:
   - route map (`routes.rs`)
   - contract modules (`oneshim-api-contracts`)
   - manifest metadata (`http-interface-manifest.v1.json`)
2. Publish generated `oneshim-web.v1.openapi.yaml` as CI artifact and release attachment.
3. Add OpenAPI diff gate in CI for PRs that change route/contract surfaces.

## Non-goals (current phase)

- No runtime dependency on OpenAPI generator.
- No adapter/domain coupling changes.
- No handler-level manual schema duplication.
