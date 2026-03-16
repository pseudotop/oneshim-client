# Integration Domain Implementation Plan

Date: 2026-03-16
Status: Draft
Related design: `docs/superpowers/specs/2026-03-16-integration-domain-architecture-design.md`

## Goal

Move external interoperability from an ad hoc local HTTP mindset to a reviewable, standard-driven integration domain.

This plan is staged so each step can be reviewed before the next one lands.

## Constraints

- Keep `/api` local-first and loopback-only
- Keep privacy, consent, and audit as mandatory outbound gates
- Preserve Hexagonal boundaries
- Prefer standard envelopes and transport documentation
- Avoid a large one-shot rewrite

## Phase 0: Boundary Lock-In

Status:

- mostly done

Deliverables:

- `/api` remains loopback-only
- `/integration/v1` is minimal and token-protected
- saved-secret confused-deputy path is blocked for integration model discovery

Review gate:

- confirm no internal control-plane endpoint is externally reachable without explicit future design

## Phase 1: Core Integration Contracts

Primary crates:

- `oneshim-core`
- `oneshim-api-contracts`

Deliverables:

- `IntegrationEnvelope`
- `InsightPacket`
- `ProactivePrompt`
- `IntegrationCapabilityScope`
- `IntegrationSessionState`
- `IntegrationAckCursor`

Ports to add:

- `IntegrationSessionPort`
- `InsightSyncPort`
- `IntegrationInboxPort`
- `IntegrationEgressPolicyPort`

Review gate:

- contract review
- threat review on envelope/replay fields

Parallelizable work:

- envelope and schema design
- scope taxonomy
- privacy classification taxonomy

## Phase 2: Outbound Session Runtime

Primary crates:

- `oneshim-network`
- `src-tauri`

Deliverables:

- transport-neutral session orchestrator
- reconnect policy
- heartbeat
- ack cursor persistence
- queue drain/backoff rules

Implementation note:

Pick a first transport adapter, but keep port contracts transport-neutral.

Likely first adapter:

- WebSocket or gRPC bidirectional stream

Review gate:

- runtime review
- reconnect/failure review

Parallelizable work:

- session runtime
- ack/outbox persistence
- token/session auth bootstrap

## Phase 3: Outbound Insight Sync

Primary crates:

- `oneshim-core`
- `src-tauri`
- `oneshim-network`

Deliverables:

- insight packet builder
- privacy-filtered summary generation
- batching and dedupe
- CloudEvents-compatible envelope serialization

Review gate:

- privacy/audit review
- payload minimization review

Parallelizable work:

- event classification
- packet schema
- CloudEvents mapping

## Phase 4: Inbound Prompt Inbox

Primary crates:

- `oneshim-core`
- `src-tauri`
- `oneshim-web` (status/UI only)

Deliverables:

- inbound prompt/task packet model
- inbox persistence
- notification pipeline
- ack / dismiss / expire lifecycle

Review gate:

- UX review
- duplicate/expiry review

Parallelizable work:

- inbox store
- proactive prompt DTOs
- notification presentation

## Phase 5: Standards Documentation Layer

Primary outputs:

- AsyncAPI spec for session channels
- CloudEvents mapping guide
- OpenAPI kept narrow for local and bootstrap HTTP surfaces

Deliverables:

- `docs/contracts/integration-asyncapi.yaml`
- `docs/contracts/integration-event-envelope.md`
- explicit mapping from domain DTOs to CloudEvents attributes

Review gate:

- interop review
- standards compliance review

Parallelizable work:

- AsyncAPI authoring
- CloudEvents field mapping
- review of naming/versioning

## Phase 6: Agent Interop Adapter

Primary crates:

- new adapter crate or `oneshim-network` adjunct
- `oneshim-core`

Deliverables:

- MCP-compatible adapter layer for selected resources/tools/prompts

Important:

- this is not the main integration transport
- it is an optional adapter over the integration domain

Review gate:

- interop review
- scope review

## Workstream Split For Parallelization

Once Phase 1 is approved, the work can be parallelized into these domain streams:

### Stream A: Session/Auth

Scope:

- bootstrap
- tokens
- reconnect
- replay fields

### Stream B: Insight Sync

Scope:

- packet schema
- CloudEvents mapping
- outbox/ack

### Stream C: Inbox/Prompt

Scope:

- inbound DTOs
- inbox lifecycle
- notifications

### Stream D: Policy/Audit

Scope:

- egress filters
- consent integration
- audit linkage

These streams should converge through the shared core contracts from Phase 1.

## Review Requirement Per Stage

Every phase must end with:

1. contract review
2. security/threat review
3. runtime/UX review where applicable
4. contract artifact verification if new specs were added

Do not start the next phase without explicitly closing the current review.

## Recommended Immediate Next Step

Implement Phase 1 only.

Specifically:

1. add core integration envelope types
2. add integration ports in `oneshim-core`
3. add narrow `oneshim-api-contracts` DTOs for status/bootstrap only
4. review before any transport runtime work

## Success Criteria

The integration domain is ready for parallel implementation when:

- ports exist in `oneshim-core`
- the envelope and scope model are approved
- privacy/audit hooks are part of the first contract, not bolted on later
- outbound session is clearly separate from local `/api`
