# Amazon Bedrock + GitHub Copilot Provider Support

> **⚠️ SUPERSEDED 2026-04-19**: The Amazon Bedrock portion of this spec is
> superseded by [ADR-019](../../architecture/ADR-019-error-code-infrastructure.md) §3.
> AWS Bedrock is now *intentionally unsupported* — catalog entry removed,
> 7 match arms in `oneshim-network` return `ConfigCode::UnsupportedProviderBedrock`.
> Re-introduction requires the 8-step checklist in ADR-019 §5. The GitHub
> Copilot portion of this spec is unaffected and may still be active.

## Goal

Add two new BYOK providers to oneshim-client, covering the remaining 6% of OpenClaw-supported providers that require custom auth.

## Provider 1: Amazon Bedrock

### API Format
- **Endpoint**: `POST https://bedrock-runtime.{region}.amazonaws.com/model/{modelId}/converse`
- **Auth**: AWS Signature Version 4 (access key + secret + optional session token + region)
- **Request shape**: Bedrock Converse format (messages array, system array, inferenceConfig)
- **Response shape**: `output.message.content[].text`, `usage.inputTokens`/`outputTokens`

### Design Decisions

**Auth scheme**: New `ProviderAuthScheme::AwsSignatureV4` variant. Uses the `aws-sigv4` crate for request signing (lightweight — only the signing logic, not the full AWS SDK).

**Credential source**: New `CredentialSource::AwsCredentials` variant:
```rust
AwsCredentials {
    access_key_id: String,
    secret_access_key: String,
    session_token: Option<String>,
    region: String,
}
```
Resolution: from `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`, `AWS_REGION` env vars.

**Request shape**: New `ProviderRequestShape::BedrockConverse`. Maps our chat format to Bedrock's:
- `messages[].content[].text` (Bedrock uses `content` as array of blocks)
- `system[].text` (system prompt as array)
- `inferenceConfig.maxTokens`, `inferenceConfig.temperature`

**Response parsing**: Extract from `output.message.content[].text`.

**Endpoint URL**: Dynamic model interpolation — `https://bedrock-runtime.{region}.amazonaws.com/model/{modelId}/converse`. The `region` comes from `AwsCredentials`, `modelId` from the selected model. Uses same pattern as Google's `rewrite_google_generate_content_endpoint()`.

**Model catalog**: `GET /foundation-models` with AWS SigV4 — new `ModelCatalogResponseShape::BedrockModels`.

### Catalog Entry
- vendor_id: `"bedrock"`
- provider_type: `"Bedrock"`
- surface_id: `"provider_surface.bedrock.direct_api"`
- Known models: `anthropic.claude-3-5-sonnet-20241022-v2:0`, `anthropic.claude-3-haiku-20240307-v1:0`, `meta.llama3-1-70b-instruct-v1:0`, `amazon.nova-pro-v1:0`

## Provider 2: GitHub Copilot

### API Format
- **Endpoint**: `POST https://api.githubcopilot.com/chat/completions` (OpenAI-compatible)
- **Auth**: OAuth device flow → Bearer token
- **Request/Response**: Standard OpenAI chat completions format

### Design Decisions

**Auth scheme**: New `ProviderAuthScheme::GitHubCopilotOAuth` variant. The flow:
1. POST `https://github.com/login/device/code` with `client_id` → get `device_code` + `user_code` + `verification_uri`
2. User visits verification_uri and enters user_code
3. Poll `https://github.com/login/oauth/access_token` until authorized
4. Exchange OAuth token for Copilot API token via `GET https://api.github.com/copilot_internal/v2/token`
5. Use token as Bearer in API calls

**Credential source**: Reuse `CredentialSource::ManagedOAuth` with a new `CopilotOAuthPort` implementing `OAuthPort`. The Copilot token has ~30min expiry — `ManagedOAuth` already handles refresh via `resolve_bearer_token()` which calls `oauth_port.get_access_token()`.

**Request shape**: Reuse `ProviderRequestShape::OpenAiChatCompletions` — Copilot API is OpenAI-compatible.

**Response parsing**: Reuse existing OpenAI parser.

**Model catalog**: `GET https://api.githubcopilot.com/models` with Bearer auth → `StandardDataOrModels` response.

### Catalog Entry
- vendor_id: `"copilot"`
- provider_type: `"Copilot"`
- surface_id: `"provider_surface.copilot.managed_oauth"`
- Known models: `gpt-5.3-codex`, `claude-sonnet-4-6`, `gemini-3-pro`

## Extension Points (Code Changes)

### Layer 1: Enums + Catalog (oneshim-api-contracts + oneshim-core)

| File | Change |
|------|--------|
| `config/enums.rs` | Add `Bedrock`, `Copilot` to `AiProviderType` |
| `provider_surface.rs` | Add vendor_id→AiProviderType mappings |
| `provider_specs/enums.rs` | Add `AwsSignatureV4`, `GitHubCopilotOAuth` to `ProviderAuthScheme`; Add `BedrockConverse` to `ProviderRequestShape`; Add `BedrockModels` to `ModelCatalogResponseShape` |
| `provider_specs/parsers.rs` | Add parse cases for new enums |
| `provider-surface-catalog.json` | Add Bedrock + Copilot vendor + surface entries |

### Layer 2: Credentials (oneshim-core)

| File | Change |
|------|--------|
| `ports/credential_source.rs` | Add `AwsCredentials` variant with region/keys |

### Layer 3: Network (oneshim-network)

| File | Change |
|------|--------|
| `ai_llm_client/mod.rs` | Add auth signing + request building for Bedrock |
| `ai_llm_client/request.rs` | Add `build_bedrock_converse_body()` |
| `ai_llm_client/parsers.rs` | Add `parse_bedrock_response()` |
| `copilot_auth.rs` (new) | GitHub OAuth device flow + Copilot token exchange |
| `Cargo.toml` | Add `aws-sigv4` + `aws-credential-types` dependencies |

### Layer 4: Wiring (src-tauri)

| File | Change |
|------|--------|
| `session_manager.rs` | Handle `Bedrock`/`Copilot` in `create_session` if needed (most flows go through existing Generic path) |

## Dependencies

| Crate | Version | Purpose | Size |
|-------|---------|---------|------|
| `aws-sigv4` | 1.x | AWS Signature V4 request signing | ~50KB |
| `aws-credential-types` | 1.x | AWS credential models | ~30KB |

Both are lightweight — just the signing math, not the full AWS SDK.

## Testing Strategy

### Unit tests
1. `build_bedrock_converse_body` — message format conversion
2. `parse_bedrock_response` — response text extraction
3. `copilot_device_flow_poll_logic` — polling state machine
4. `aws_sigv4_header_generation` — signature correctness (mock request)
5. `bedrock_endpoint_url_construction` — region + model ID interpolation

### Integration smoke (env-gated)
- `ONESHIM_AI_SMOKE_LLM_PROVIDER_TYPE=bedrock` + AWS creds → Bedrock converse
- `ONESHIM_AI_SMOKE_LLM_PROVIDER_TYPE=copilot` + GitHub OAuth → Copilot chat

## Non-Goals
- Bedrock streaming (ConverseStream) — can be added later
- Copilot embeddings endpoint — chat completions only for now
- AWS SSO / assume-role credential chains — direct credentials only
