#!/usr/bin/env bash

set -euo pipefail

INSTALL_DIR="${ONESHIM_INSTALL_DIR:-$HOME/.local/bin}"
BINARY_NAME="oneshim"

usage() {
  cat <<'EOF'
ONESHIM uninstall script (macOS/Linux)

Usage:
  ./scripts/uninstall.sh [options]

Options:
  --install-dir <path>   Installation directory. Default: ~/.local/bin
  -h, --help             Show help

Environment:
  ONESHIM_INSTALL_DIR
EOF
}

info() {
  printf '[INFO] %s\n' "$*"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --install-dir)
      [[ $# -ge 2 ]] || { printf '[ERROR] --install-dir requires a value\n' >&2; exit 1; }
      INSTALL_DIR="$2"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      printf '[ERROR] Unknown option: %s (use --help)\n' "$1" >&2
      exit 1
      ;;
  esac
done

TARGET_PATH="$INSTALL_DIR/$BINARY_NAME"

if [[ -f "$TARGET_PATH" ]]; then
  rm -f "$TARGET_PATH"
  info "Removed $TARGET_PATH"
else
  info "No installed binary found at $TARGET_PATH"
fi

if [[ -d "$INSTALL_DIR" && -z "$(ls -A "$INSTALL_DIR")" ]]; then
  rmdir "$INSTALL_DIR"
  info "Removed empty directory $INSTALL_DIR"
fi

info "Uninstall complete"
