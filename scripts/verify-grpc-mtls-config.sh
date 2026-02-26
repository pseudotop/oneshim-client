#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "${ROOT_DIR}"

echo "[grpc-mtls] Validating TLS/mTLS configuration invariants"
cargo test -p oneshim-network --features grpc test_tls_
cargo test -p oneshim-network --features grpc test_mtls_

echo "[grpc-mtls] TLS/mTLS validation checks completed"
