# Integration Domain Architecture Design

Date: 2026-03-16
Status: Draft
Audience: `oneshim-core`, `src-tauri`, `oneshim-network`, `oneshim-web`, future server/integration work

## Purpose

Define the durable architecture for external solution interoperability.

This document intentionally distinguishes:

- the local desktop control plane
- the future integration plane
- optional agent/tool interoperability adapters

The goal is to avoid treating the current local HTTP API as the long-term external integration surface.

## Problem Statement

The client now supports substantial local AI/runtime functionality, but external system connectivity still needs a domain model that can support:

- server-backed teamwork and insight delivery
- proactive prompts delivered back to the client
- third-party solution interoperability
- standardized envelopes and reviewable contracts
- strict privacy, policy, and audit controls

The current loopback `/api` surface is an internal UI/control plane. It is not the right primary abstraction for future external integrations.

## Core Decision

External interoperability is modeled as an `integration` domain, not a `collab` domain.

Why:

- `collab` is only one product use case
- `integration` can cover collaboration, analytics backends, automation hubs, external agents, and future partner systems
- standards alignment is broader than team collaboration alone

`collab` remains a use-case label inside the broader integration domain.

## Boundary Model

### 1. Local Control Plane

Purpose:

- local dashboard
- local settings
- local automation control
- local diagnostics

Properties:

- loopback-only
- first-party UI/control only
- not the public integration plane

Current implementation:

- `/api`
- embedded frontend
- Tauri IPC

### 2. Integration Plane

Purpose:

- client-initiated outbound connectivity to an external system
- secure uplink of privacy-filtered summaries, context, and events
- secure downlink of prompts, suggestions, tasks, and policy-approved instructions

Properties:

- client initiates the session
- replay-safe
- capability-scoped
- auditable
- privacy-gated

This is the long-term external interoperability plane.

### 3. Agent Interop Plane

Purpose:

- agent/tool interoperability with systems that speak an agent standard

Properties:

- adapter layer, not the top-level domain model
- may expose MCP-compatible or A2A-compatible adapters later

## Standards Mapping

The integration domain should use standards where they fit, but no single standard covers the whole system.

### Authentication and Session Bootstrap

Preferred standards:

- OAuth 2.0 / OIDC
- Native app guidance: RFC 8252
- Device authorization flow where browser callback is not appropriate: RFC 8628
- DPoP for replay-safe token-bound sessions: RFC 9449
- Resource Indicators for resource-scoped access tokens: RFC 8707
- Protected Resource Metadata for client bootstrap/discovery: RFC 9728

Use for:

- device registration
- workspace/org-scoped session bootstrap
- token refresh and rotation

### Event Envelope

Preferred standard:

- CloudEvents
- CESQL-aware attribute naming and extension discipline

Use for:

- outbound insight packets
- policy/audit events sent to server
- inbound prompt/task/instruction envelopes

Rationale:

- stable event metadata shape
- routing-friendly
- vendor-neutral

### Async Contract Documentation

Preferred standard:

- AsyncAPI 3.1

Use for:

- bidirectional session contract
- event stream topics/channels
- inbox semantics
- ack/cursor semantics

Rationale:

- documents the event plane better than OpenAPI
- fits WebSocket, SSE, brokered, and streaming transports

### Agent/Tool Interoperability

Preferred standards:

- MCP for tool/resource/prompt interoperability
- A2A for future agent-to-agent task interoperability where needed

Use for:

- future external agent integrations
- tool/resource/prompt adapters
- optional peer-agent task delegation adapters

Important:

- MCP and A2A are adapter-level interoperability standards
- neither is the top-level sync/session/event model

### HTTP Contract Documentation

Preferred standard:

- OpenAPI

Use only for:

- local `/api`
- narrow bootstrap/admin endpoints if they remain HTTP

OpenAPI should not be treated as the primary modeling tool for the outbound integration plane.

## Architectural Shape

### Domain Layers

#### `oneshim-core`

Owns domain contracts and invariants:

- integration session contracts
- outbound packet contracts
- inbox message contracts
- privacy/policy rules for external egress
- capability scopes and authorization semantics

#### Adapter crates

Implement transports and serializers:

- HTTPS/WebSocket session adapter
- HTTPS request/response + SSE or long-poll adapter
- optional gRPC session adapter for tightly controlled environments
- CloudEvents serializer/profile
- AsyncAPI 3.1-generated or documented transport bindings
- MCP adapter if added later
- A2A adapter if added later

#### `src-tauri`

Composition root and orchestrator:

- session lifecycle
- reconnect policy
- token/material wiring
- runtime backpressure / queue control
- notification delivery to UI

#### `oneshim-web`

Local UI only:

- settings and status
- visibility into integration state
- not the main external integration server

## Domain Subsystems

### 1. `integration.session`

Responsibilities:

- device registration
- handshake
- token rotation
- channel establishment
- heartbeat
- replay window / clock-skew handling

Suggested ports:

- `IntegrationSessionPort`
- `IntegrationAuthPort`
- `IntegrationPresencePort`

### 2. `integration.sync`

Responsibilities:

- outbound insight packet queueing
- batching
- retry and backoff
- dedupe and idempotency
- server ack cursor tracking

Suggested ports:

- `InsightSyncPort`
- `IntegrationOutboxPort`
- `IntegrationAckPort`

### 3. `integration.inbox`

Responsibilities:

- inbound proactive prompts
- inbox item lifecycle
- acknowledgement
- dedupe
- TTL / expiry

Suggested ports:

- `IntegrationInboxPort`
- `ProactivePromptPort`

### 4. `integration.policy`

Responsibilities:

- privacy gating before external egress
- content minimization
- explicit consent checks
- audit logging

Suggested ports:

- `IntegrationEgressPolicyPort`
- `IntegrationAuditPort`

### 5. `integration.agent`

Responsibilities:

- external agent/tool interoperability
- MCP adapter registration
- agent-exposed resource/tool surfaces

Suggested ports:

- `AgentInteropPort`
- `McpAdapterPort`

## Transport Decision

The primary integration transport should be modeled as an outbound session, not an externally exposed inbound RPC server.

That means:

- client dials out
- server responds on the same trusted session
- proactive prompts arrive over the established channel

Preferred first transport candidates:

- WebSocket over HTTPS
- HTTPS request/response + SSE or long-poll hybrid

Optional controlled-environment adapter:

- gRPC bidirectional stream

The domain model must not depend on one transport.

## Current Priority Adjustment

At this point in the client, provider registration and provider-surface handling are no longer the primary architectural risk.

The remaining critical path is the integration domain itself:

- production-grade auth/bootstrap
- durable outbox/inbox and ack persistence
- live bidirectional delivery
- operations, audit, and failure visibility

That means the next iterations should not re-open provider/surface architecture unless a concrete new requirement forces it. The higher-value work is to harden the integration runtime around the boundaries already introduced.

Two temporary implementation shortcuts must not be mistaken for end-state architecture:

- env-token bootstrap auth
- in-memory-only session/outbox/inbox state

They are acceptable as foundation scaffolding, but not as completion criteria.

## Execution Priority

The remaining implementation should follow this order:

1. close production auth/bootstrap
2. close durable persistence and ack state
3. tighten CloudEvents and AsyncAPI delivery semantics around persisted cursors
4. bind the live WebSocket/SSE runtime
5. expose operations, audit, and failure surfaces

This ordering matters because live transport work without durable state or production auth would create a misleading sense of completion and force rework later.

## Data Model Principle

Never send raw sensitive source data by default.

The integration plane should send:

- summaries
- derived metrics
- policy-approved context packets
- explicit user-approved attachments only when allowed

This keeps the outbound model aligned with existing privacy and audit rules.

## Current Temporary HTTP Surface

The newly added `/integration/v1` surface is a transitional boundary, not the final architecture.

It exists to:

- keep internal `/api` loopback-only
- prevent saved-secret confused-deputy behavior
- provide a minimal reviewable integration foothold

Long-term:

- `/integration/v1` should remain narrow
- the main external flow should move to outbound session transports

## Canonical Contracts To Introduce

### `IntegrationEnvelope`

Common envelope for all outbound/inbound integration messages.

Minimum metadata:

- `envelope_id`
- `schema_version`
- `message_type`
- `timestamp`
- `nonce`
- `origin`
- `capability_scope`

### `InsightPacket`

Outbound privacy-filtered context payload.

Suggested fields:

- summary text
- derived tags
- time window
- source scope
- privacy classification
- audit reference id

### `ProactivePrompt`

Inbound actionable prompt/suggestion/task packet.

Suggested fields:

- prompt id
- category
- title
- body
- priority
- expires_at
- actions
- provenance

### `IntegrationCapabilityScope`

Required for least-privilege access control.

Examples:

- `insight:write`
- `prompt:read`
- `task:ack`
- `device:presence`

## Review Gates

Every implementation phase must end with a review gate.

### Gate A: Contract Review

Check:

- port ownership
- dependency direction
- envelope fields
- scope semantics

### Gate B: Threat Review

Check:

- replay protection
- credential handling
- privacy minimization
- fail-closed behavior

### Gate C: Runtime Review

Check:

- reconnect behavior
- queue growth
- duplicate delivery handling
- UI impact and diagnostics

### Gate D: Interop Review

Check:

- standards alignment
- AsyncAPI/OpenAPI sync
- CloudEvents compatibility
- adapter extensibility

## Explicit Non-Goals

These are not goals of the integration domain itself:

- exposing the full local dashboard/control plane publicly
- treating MCP as the only integration standard
- treating A2A as a replacement for the core integration domain
- bypassing local privacy/policy/audit gates for server sync
- assuming collaboration is the only future external use case

## Summary

The correct top-level model is `integration`, not `collab`.

The client should keep:

- local `/api` for first-party local control
- outbound session-based integration for external systems
- standards-based envelopes and contracts
- CloudEvents-compatible envelopes with CESQL-friendly attributes
- AsyncAPI 3.1-documented channel contracts
- optional MCP and A2A adapters as interoperability layers

This architecture is broad enough for collaboration, external solutions, and future agent interoperability without turning the local control plane into the public API.
