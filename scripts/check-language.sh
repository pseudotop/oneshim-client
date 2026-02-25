#!/usr/bin/env bash
set -euo pipefail

if [[ $# -gt 0 ]]; then
  case "$1" in
    non-english|i18n|all)
      cargo run -p oneshim-lint --bin language-check -- "$@"
      exit $?
      ;;
  esac
fi

cargo run -p oneshim-lint --bin language-check -- all "$@"
