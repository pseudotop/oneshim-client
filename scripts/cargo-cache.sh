#!/usr/bin/env bash
set -euo pipefail

if [ $# -eq 0 ]; then
  echo "Usage: ./scripts/cargo-cache.sh <cargo-args...>"
  exit 1
fi

if command -v sccache >/dev/null 2>&1; then
  export RUSTC_WRAPPER="$(command -v sccache)"
  export SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-10G}"
  sccache --start-server >/dev/null 2>&1 || true
fi

exec cargo "$@"
