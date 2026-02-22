# Standalone Integrity Baseline

This document defines the mandatory integrity baseline for ONESHIM Client in standalone mode.

The goal is to keep the standalone trust model strict today, while making future server and third-party integrations additive rather than disruptive.

## Security Objectives

- Fail closed when integrity checks fail.
- Keep update/install trust anchored in cryptographic verification.
- Produce machine-verifiable supply-chain evidence for each release.
- Keep boundary contracts stable for future remote integrations.

## Mandatory Controls

### 1) Update Integrity

- `update.require_signature_verification` MUST stay enabled when updates are enabled.
- `update.signature_public_key` MUST be a valid Ed25519 public key (32-byte decoded payload).
- Update artifacts MUST pass both SHA-256 and Ed25519 verification before extraction/install.
- Signature/checksum sidecars (`.sig`, `.sha256`) MUST be generated and published with every release artifact.

### 2) Supply-Chain Integrity

- RustSec scan: `cargo audit`
- Dependency policy: `cargo deny check licenses advisories sources bans`
- Vet policy: `cargo vet check`
- SBOM: `cargo cyclonedx --workspace`
- Provenance attestation: GitHub artifact attestation on release artifacts

### 3) Runtime Boundary Rules

- Web handlers MUST not access SQLite internals directly (`conn_ref` forbidden in handlers).
- Data access for web handlers MUST pass through storage adapter APIs.
- Integrity-sensitive behavior MUST fail closed (startup/check/update stage), not warn-and-continue.

### 4) Documentation and Auditability

- Integrity policy changes MUST update this baseline and `docs/security/integrity-runbook.md`.
- Security process and disclosure policy remain in `SECURITY.md`.

## Local Verification Command

```bash
./scripts/verify-integrity.sh
```

This command verifies integrity policy tests, signature verification tests, supply-chain checks, and SBOM generation.

## Future Integration Readiness (Server / Third-Party)

The following are not required for standalone runtime now, but are required design constraints from this phase onward:

- Signed request envelope fields reserved in contracts: `nonce`, `timestamp`, `key_id`, `sig`
- Replay-protection-ready protocol semantics
- Capability-scoped third-party integration contracts (least privilege by default)
- Root/online key separation and documented key-rotation process
