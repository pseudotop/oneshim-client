#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "[web-contract] Verifying handler contract boundary"

if rg -n "pub (struct|enum)" crates/oneshim-web/src/handlers >/tmp/web_contract_pub_types.txt; then
  echo "[web-contract] Public DTOs must not be defined in handlers. Move them to oneshim-api-contracts:" >&2
  cat /tmp/web_contract_pub_types.txt >&2
  exit 1
fi

if rg -n "#\[derive\([^\]]*(Serialize|Deserialize)" crates/oneshim-web/src/handlers >/tmp/web_contract_serde_derive.txt; then
  echo "[web-contract] Serde DTO derives must not live in handlers. Move types to oneshim-api-contracts:" >&2
  cat /tmp/web_contract_serde_derive.txt >&2
  exit 1
fi

if rg -n "use crate::handlers::" crates/oneshim-web/src/services >/tmp/web_contract_service_dep.txt; then
  echo "[web-contract] Services must not depend on handler-layer types. Use oneshim-api-contracts instead:" >&2
  cat /tmp/web_contract_service_dep.txt >&2
  exit 1
fi

echo "[web-contract] Boundary check passed"
