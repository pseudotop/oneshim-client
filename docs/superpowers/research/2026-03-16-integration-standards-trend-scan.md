# Integration Standards Trend Scan

Date: 2026-03-16
Status: Draft
Audience: `oneshim-core`, `oneshim-network`, `src-tauri`, future server/integration work

## Purpose

Capture the current standards baseline before the next integration implementation phase.

This research focuses on the client-initiated external integration plane, not the local desktop control plane.

## Scope

Questions reviewed:

- Which standards remain the best fit for native-app outbound integration?
- Which standards changed materially since the initial integration design draft?
- What should be treated as the default interoperability baseline versus an optional adapter?

## Findings

### 1. Auth and session bootstrap should remain OAuth/OIDC-based, but token binding matters more now

The base direction is still correct:

- OAuth 2.0 / OIDC
- native app guidance from RFC 8252
- device authorization flow from RFC 8628 when local browser callback is not appropriate

What needs to be added explicitly:

- DPoP from RFC 9449 should be treated as the preferred replay-resistance mechanism for client-initiated integration sessions when the server supports it.
- Resource Indicators from RFC 8707 and Protected Resource Metadata from RFC 9728 make it easier to issue tokens that are bound to the right remote resource and capability scope.

Implication:

- the integration session model should stay transport-neutral
- the auth/bootstrap model should assume scoped access tokens
- replay-safe envelopes alone are not enough; token-bound proof is a better long-term target

### 2. CloudEvents is still the right event envelope baseline

CloudEvents remains the best fit for:

- outbound insight packets
- inbound prompt/task envelopes
- audit event forwarding

The important trend update is CESQL v1.

Implication:

- envelope attributes should be named and versioned so they remain easy to filter using CloudEvents SQL
- custom extension attributes should be deliberate and stable
- event type taxonomy matters more than before because filtering/routing is now a stronger interoperability concern

### 3. AsyncAPI should now be treated as an AsyncAPI 3.1 problem

AsyncAPI is still the right documentation layer for the bidirectional integration channel.

The current baseline should be AsyncAPI 3.1 rather than vague earlier wording.

Implication:

- contract artifacts should target AsyncAPI 3.1
- the transport binding story should be documented from the same channel model instead of diverging per adapter

### 4. MCP is maturing, but it remains an adapter-level standard

MCP is still valuable for tool/resource/prompt interoperability, not for the primary sync/session plane.

The latest stable MCP release adds more serious auth expectations:

- OAuth-protected MCP servers
- OIDC discovery
- authorization server metadata
- resource indicators
- incremental scopes

Implication:

- MCP should remain an optional adapter over the integration domain
- if MCP is added later, its auth model should align with the main integration session bootstrap assumptions instead of inventing a separate security story

### 5. A2A is relevant, but should be treated as a separate optional adapter

A2A is emerging as a standard for agent-to-agent task exchange.

That makes it useful for:

- future external agent collaboration
- task delegation across systems
- agent-facing interoperability that is broader than tool invocation

It should not replace the primary client-to-server integration domain.

Implication:

- A2A belongs beside MCP as an optional adapter layer
- the core integration domain should stay independent from both

### 6. Transport choice should stay open, but the first public-facing adapter should not assume gRPC

The core transport-neutral decision is still correct.

The current interoperability trend suggests:

- prefer client-initiated HTTPS-friendly transports first
- WebSocket over HTTPS is a strong default candidate for a bidirectional session
- HTTPS request/response plus SSE or long-poll remains a reasonable fallback
- gRPC bidirectional stream is still useful, but better treated as an optional controlled-environment adapter instead of the public default

Implication:

- first adapter work should bias toward standards-friendly outbound HTTPS/WebSocket semantics
- gRPC can stay available later where both ends are tightly controlled

## Recommended Updates To Our Design

1. Keep `integration` as the top-level domain name.
2. Update standards references to:
   - RFC 8252
   - RFC 8628
   - RFC 9449
   - RFC 8707
   - RFC 9728
   - AsyncAPI 3.1
   - CloudEvents with CESQL-aware attribute design
   - MCP `2025-11-25`
3. Treat MCP and A2A as optional adapter layers, not the main session/sync model.
4. Prefer a first outbound adapter that is friendly to HTTPS/WebSocket infrastructure.
5. Keep `/api` local-only and keep `/integration/v1` narrow; neither should become the primary integration abstraction.

## Review Outcome

No design blocker was found.

The existing direction remains valid, but the standards profile should be tightened before the next transport/bootstrap implementation phase.

## Sources

- OAuth 2.0 for Native Apps (RFC 8252): https://www.rfc-editor.org/rfc/rfc8252
- OAuth 2.0 Device Authorization Grant (RFC 8628): https://www.rfc-editor.org/rfc/rfc8628
- OAuth 2.0 Demonstrating Proof-of-Possession at the Application Layer (DPoP, RFC 9449): https://www.rfc-editor.org/rfc/rfc9449
- OAuth 2.0 Resource Indicators (RFC 8707): https://www.rfc-editor.org/rfc/rfc8707
- OAuth 2.0 Protected Resource Metadata (RFC 9728): https://www.rfc-editor.org/rfc/rfc9728
- AsyncAPI 3.1.0 release notes: https://www.asyncapi.com/blog/release-notes-3.1.0
- CloudEvents CESQL v1 announcement: https://cloudevents.io/blog/2024-06-13/
- MCP 2025-11-25 changelog: https://modelcontextprotocol.io/specification/2025-11-25/changelog
- A2A specification: https://a2a-protocol.org/latest/
