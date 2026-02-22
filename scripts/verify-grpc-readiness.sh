#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "${ROOT_DIR}"

echo "[grpc] Checking oneshim-network with grpc feature"
cargo check -p oneshim-network --features grpc

echo "[grpc] Running oneshim-network tests with grpc feature"
cargo test -p oneshim-network --features grpc

echo "[grpc] Checking oneshim-app wiring"
cargo check -p oneshim-app --features oneshim-network/grpc

echo "[grpc] Verifying committed generated proto files are up-to-date"
if ! git diff --quiet -- crates/oneshim-network/src/proto/generated; then
  echo "Generated proto files changed. Regenerate and commit updated files:" >&2
  git diff -- crates/oneshim-network/src/proto/generated >&2
  exit 1
fi

echo "[grpc] Readiness checks completed successfully"
