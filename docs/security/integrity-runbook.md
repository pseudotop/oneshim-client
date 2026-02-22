# Integrity Runbook

This runbook describes how to operate ONESHIM Client integrity controls in day-to-day development and release workflows.

## 1. Pre-merge (PR) Checklist

Run locally before opening or updating a PR:

```bash
cargo check --workspace
cargo test -p oneshim-web
./scripts/verify-integrity.sh
```

Expected output:

- Integrity policy tests pass (`oneshim-core`)
- Signature verification tests pass (`oneshim-app`)
- Supply-chain gates pass (`audit`, `deny`, `vet`)
- SBOM generated at `artifacts/integrity/sbom.cdx.json`

## 2. CI Gates

The following workflows enforce integrity in CI:

- `CI` (`.github/workflows/ci.yml`): lint, tests, cross-platform build sanity
- `Security & Compliance` (`.github/workflows/security-compliance.yml`): supply-chain checks + SBOM
- `Integrity Gates` (`.github/workflows/integrity-gates.yml`): policy + signature + supply-chain checks

PRs must not bypass these workflows.

## 3. Release Procedure

Release workflow (`.github/workflows/release.yml`) performs:

1. Artifact build (platform matrix)
2. SHA-256 sidecar generation (`.sha256`)
3. Ed25519 signing (`.sig`)
4. Provenance attestation for release artifacts
5. GitHub release publishing

Release artifacts are considered valid only when checksum + signature + provenance are all present.

## 4. Key Management Basics

- Store update signing private key only in GitHub Actions secrets.
- Never commit private key material.
- Keep public key in client config default and update policy checks.
- For key rotation:
  - Publish new public key in a release that still validates with the old key path.
  - Rotate private key in CI secret.
  - Document effective date and rollback plan.

### Local Rehearsal

```bash
./scripts/rehearse-key-rotation.sh
```

Use generated artifacts in `artifacts/integrity/key-rotation/` to verify both old/new signatures before production cutover.

## 4.1 Signed Policy Bundle Startup Gate

When using signed runtime policy bundles, set in config:

```json
{
  "update": {
    "min_allowed_version": "0.0.1"
  },
  "integrity": {
    "enabled": true,
    "require_signed_policy_bundle": true,
    "policy_file_path": "./runtime-policy.json",
    "policy_signature_path": "./runtime-policy.json.sig",
    "policy_public_key": "<base64-ed25519-public-key>"
  }
}
```

Startup will fail closed if bundle verification fails.

## 5. Incident Handling

If any integrity gate fails in CI:

1. Treat as release blocker.
2. Identify failing layer (policy / signature / supply chain / SBOM / provenance).
3. Fix root cause and rerun full integrity script.
4. Record impact and remediation in PR notes.

For vulnerability disclosure and response timelines, follow `SECURITY.md`.

## 6. Future Integration Constraints

Even in standalone mode, keep these ready for future server/third-party integrations:

- Signed envelope fields in transport contracts (`nonce`, `timestamp`, `key_id`, `sig`)
- Replay-safe semantics
- Capability-scoped third-party access model
- Fail-closed default for any trust decision
