#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ARTIFACT_DIR="${ROOT_DIR}/artifacts/integrity"

mkdir -p "${ARTIFACT_DIR}"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf "Missing required command: %s\n" "$1" >&2
    exit 1
  fi
}

require_cmd cargo

if ! command -v cargo-audit >/dev/null 2>&1; then
  printf "cargo-audit is required. Install: cargo install cargo-audit --locked\n" >&2
  exit 1
fi

if ! command -v cargo-deny >/dev/null 2>&1; then
  printf "cargo-deny is required. Install: cargo install cargo-deny --locked\n" >&2
  exit 1
fi

if ! command -v cargo-vet >/dev/null 2>&1; then
  printf "cargo-vet is required. Install: cargo install cargo-vet --locked\n" >&2
  exit 1
fi

if ! command -v cargo-cyclonedx >/dev/null 2>&1; then
  printf "cargo-cyclonedx is required. Install: cargo install cargo-cyclonedx --locked\n" >&2
  exit 1
fi

cd "${ROOT_DIR}"

cargo test -p oneshim-core update_integrity_policy
cargo test -p oneshim-app integrity_guard
cargo test -p oneshim-app verify_signature_
cargo audit
cargo deny check licenses advisories sources bans
cargo vet check
cargo cyclonedx --workspace --format json --output-file "${ARTIFACT_DIR}/sbom.cdx.json"

printf "Integrity verification completed.\n"
