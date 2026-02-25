# oneshim-api-contracts

`oneshim-api-contracts` centralizes transport-layer DTOs for the local web API.

## Scope

- Shared request/response structs for web handlers and services.
- Serde defaults used for backward-compatible deserialization.
- Versioned contract modules that can later be mapped to OpenAPI generation.

## Current Modules

- `settings`: app settings, storage stats, and remote provider endpoint DTOs.
- `ai_providers`: provider preset catalog and model discovery request/response DTOs.

## Design Rules

1. Keep business/domain logic out of this crate.
2. Keep dependencies minimal (`serde` only by default).
3. Use additive schema evolution for compatibility.
4. Treat this crate as the transport SSOT for `oneshim-web` APIs.
