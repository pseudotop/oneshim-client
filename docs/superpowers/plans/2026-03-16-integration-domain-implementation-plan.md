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

## Current Assessment

- AI provider registration and provider-surface handling are no longer the critical path
- The remaining architectural risk is concentrated in the integration domain
- The current runtime is best described as "foundation plus bootstrap wiring", not as a finished external interoperability runtime
- Any remaining phase wording that implies durable auth, persistence, or live delivery is already complete should be treated as stale and corrected below

## Phase 0: Boundary Lock-In

Status:

- done

Deliverables:

- `/api` remains loopback-only
- `/integration/v1` is minimal and token-protected
- saved-secret confused-deputy path is blocked for integration model discovery

Review gate:

- confirm no internal control-plane endpoint is externally reachable without explicit future design

## Phase 1: Core Integration Contracts

Status:

- foundation done
- review closed

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

## Phase 2: Standards-Profiled Auth Bootstrap

Status:

- standards profile selected
- first bootstrap adapter foundation done
- current env-token bootstrap is temporary
- real OIDC / device bootstrap and real DPoP are not started

Primary crates:

- `oneshim-network`
- `src-tauri`

Deliverables:

- OIDC / OAuth bootstrap profile for native app and device-flow cases
- resource-scoped token handling
- DPoP-capable request proof adapter
- token refresh / rotation policy
- explicit distinction between development bootstrap and production bootstrap

Review gate:

- auth review
- threat review
- standards review against the selected profile

Parallelizable work:

- bootstrap discovery/profile
- OIDC/device auth wiring
- request proof adapter

## Phase 3: Durable Session Persistence

Status:

- not started

Primary crates:

- `oneshim-core`
- `oneshim-network`
- `src-tauri`

Deliverables:

- durable outbox adapter
- durable inbox adapter
- durable ack cursor persistence
- reconnect-safe session state persistence
- queue drain/backoff rules tied to persisted state

Implementation note:

Do not treat in-memory stores or in-memory session state as phase completion. This phase is complete only when restart-safe behavior is present.

Review gate:

- durability review
- failure/restart review

Parallelizable work:

- outbox persistence
- inbox persistence
- ack/session state persistence

## Phase 4: Live Session Channel

Status:

- transport-neutral foundation done
- live bidirectional channel binding not started

Primary crates:

- `oneshim-network`
- `src-tauri`

Deliverables:

- live WebSocket channel over HTTPS
- SSE or long-poll fallback binding
- reconnect policy
- heartbeat
- queue drain/backpressure rules connected to the live channel

Implementation note:

Do not start live-channel runtime work until Phase 2 and Phase 3 review gates are explicitly closed.

Likely first adapter:

- WebSocket over HTTPS

Fallback adapter:

- HTTPS request/response + SSE or long-poll hybrid

Optional controlled-environment adapter:

- gRPC bidirectional stream

Review gate:

- runtime review
- reconnect/failure review
- transport review

Parallelizable work:

- session runtime
- SSE/long-poll fallback
- presence/heartbeat handling

## Phase 5: Outbound Insight Sync

Status:

- foundation done
- CloudEvents mapping started
- delivery is not yet backed by durable runtime state

Primary crates:

- `oneshim-core`
- `src-tauri`
- `oneshim-network`

Deliverables:

- insight packet builder
- privacy-filtered summary generation
- batching and dedupe
- CloudEvents-compatible envelope serialization
- CESQL-friendly event attribute profile

Review gate:

- privacy/audit review
- payload minimization review
- delivery semantics review

Parallelizable work:

- event classification
- packet schema
- CloudEvents mapping

## Phase 6: Inbound Prompt Inbox

Status:

- foundation done
- transport binding started
- durable persistence and UI delivery refinements not started

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
- durable lifecycle review

Parallelizable work:

- inbox store
- proactive prompt DTOs
- notification presentation

## Phase 7: Operations, Policy, and Audit Surface

Status:

- policy/audit foundation done
- operational visibility not started

Primary crates:

- `oneshim-core`
- `src-tauri`
- `oneshim-web`

Deliverables:

- integration status surface
- audit visibility
- policy decision visibility
- retry/failure diagnostics
- operator-safe runtime controls

Review gate:

- operations review
- audit/privacy review

Parallelizable work:

- status UX
- audit surfacing
- failure diagnostics

## Phase 8: Standards Documentation Layer

Status:

- first contract drafts started
- verification and naming review not started

Primary outputs:

- AsyncAPI 3.1 spec for session channels
- CloudEvents mapping guide
- CloudEvents profile guidance for CESQL-friendly attributes
- OpenAPI kept narrow for local and bootstrap HTTP surfaces

Deliverables:

- `docs/contracts/integration-asyncapi.yaml`
- `docs/contracts/integration-event-envelope.md`
- `docs/contracts/integration-cloudevents-profile.md`
- explicit mapping from domain DTOs to CloudEvents attributes

Review gate:

- interop review
- standards compliance review

Parallelizable work:

- AsyncAPI authoring
- CloudEvents field mapping
- review of naming/versioning

## Phase 9: Agent Interop Adapter

Primary crates:

- new adapter crate or `oneshim-network` adjunct
- `oneshim-core`

Deliverables:

- MCP-compatible adapter layer for selected resources/tools/prompts
- optional A2A adapter layer for external agent task interoperability

Important:

- this is not the main integration transport
- these are optional adapters over the integration domain

Review gate:

- interop review
- scope review

## Workstream Split For Parallelization

Parallelization should not start immediately after Phase 1.

The correct threshold is:

- Phase 1 contract review closed
- Phase 2 auth/bootstrap review closed
- Phase 3 durability review closed

Only after that should the runtime-heavy streams land in parallel.

Once those gates are closed, the work can be parallelized into these domain streams:

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

Re-order the remaining work so auth and persistence are closed before live-channel expansion.

Specifically:

1. replace temporary env-token bootstrap with the real OIDC / device bootstrap slice
2. define durable outbox / inbox / ack persistence boundaries
3. tighten AsyncAPI 3.1 and CloudEvents delivery semantics around persisted cursors and redelivery
4. only then bind the live WebSocket and SSE/long-poll runtime
5. review before expanding background loops or external operator surfaces

## Success Criteria

The integration domain is ready for parallel implementation when:

- ports exist in `oneshim-core`
- the envelope and scope model are approved
- privacy/audit hooks are part of the first contract, not bolted on later
- outbound session is clearly separate from local `/api`
- auth/bootstrap is not using temporary bootstrap material
- durable persistence exists for outbox, inbox, and ack cursors
