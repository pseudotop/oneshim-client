#!/usr/bin/env bash
set -euo pipefail
# Nightly mutation score check — target: ≥ 70% killed
# Usage: ./scripts/run-mutants.sh [crate-name]
# Default: oneshim-core

CRATE="${1:-oneshim-core}"
echo "=== cargo-mutants: $CRATE ==="
cargo mutants -p "$CRATE" --timeout 120 --json | tee mutants-report.json

if command -v jq >/dev/null 2>&1; then
  killed=$(jq '.outcomes.killed // 0' mutants-report.json)
  total=$(jq '.outcomes.total // 1' mutants-report.json)
  score=$((killed * 100 / total))
  echo "Mutation score: ${score}% (${killed}/${total})"
  if [ "$score" -lt 70 ]; then
    echo "::warning::Mutation score below threshold (70%)"
  fi
fi
