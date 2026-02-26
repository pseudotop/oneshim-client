#!/usr/bin/env bash
set -euo pipefail

MANIFEST_PATH="docs/contracts/http-interface-manifest.v1.json"
ROUTES_PATH="crates/oneshim-web/src/routes.rs"

if [[ ! -f "$MANIFEST_PATH" ]]; then
  echo "[http-interface-manifest] missing file: $MANIFEST_PATH" >&2
  exit 1
fi

if [[ ! -f "$ROUTES_PATH" ]]; then
  echo "[http-interface-manifest] missing routes file: $ROUTES_PATH" >&2
  exit 1
fi

if ! jq -e '
  .schema_version
  and .document_version
  and .updated_at
  and (.updated_at | type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$"))
  and .source
  and .groups
  and (.groups | type == "array" and length > 0)
  and (
    [.groups[].operations[] |
      (.path | startswith("/api/"))
      and (.method | IN("GET", "POST", "PUT", "DELETE"))
    ] | all
  )
' "$MANIFEST_PATH" >/dev/null; then
  echo "[http-interface-manifest] schema validation failed: $MANIFEST_PATH" >&2
  exit 1
fi

if ! jq -e '
  [.groups[].operations[] | "\(.method) \(.path)"] as $ops
  | ($ops | length) == ($ops | unique | length)
' "$MANIFEST_PATH" >/dev/null; then
  echo "[http-interface-manifest] duplicate method/path entries detected" >&2
  exit 1
fi

mapfile -t route_paths < <(
  {
    if command -v rg >/dev/null 2>&1; then
      rg -o '"\/[^"]+"' "$ROUTES_PATH" || true
    else
      grep -oE '"\/[^"]+"' "$ROUTES_PATH" || true
    fi
  } | tr -d '"' | sort -u
)

mapfile -t manifest_paths < <(
  jq -r '.groups[].operations[].path' "$MANIFEST_PATH" | sed 's#^/api##' | sort -u
)

if [[ ${#route_paths[@]} -eq 0 ]]; then
  echo "[http-interface-manifest] no routes were discovered in $ROUTES_PATH" >&2
  exit 1
fi

missing_paths="$(comm -23 <(printf '%s\n' "${route_paths[@]}" | sort -u) <(printf '%s\n' "${manifest_paths[@]}" | sort -u) || true)"
extra_paths="$(comm -13 <(printf '%s\n' "${route_paths[@]}" | sort -u) <(printf '%s\n' "${manifest_paths[@]}" | sort -u) || true)"

if [[ -n "$missing_paths" ]]; then
  echo "[http-interface-manifest] missing routes in manifest:" >&2
  echo "$missing_paths" >&2
  exit 1
fi

if [[ -n "$extra_paths" ]]; then
  echo "[http-interface-manifest] manifest contains unknown routes:" >&2
  echo "$extra_paths" >&2
  exit 1
fi

echo "[http-interface-manifest] validation passed"
