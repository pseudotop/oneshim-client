#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CARGO_CMD="$ROOT_DIR/scripts/cargo-cache.sh"

ASSETS_DIR="${ONESHIM_SMOKE_ASSETS_DIR:-dist}"
INSTALL_SCRIPT="${ONESHIM_INSTALL_SCRIPT:-scripts/install.sh}"
HOST="${ONESHIM_SMOKE_HOST:-127.0.0.1}"
PORT="${ONESHIM_SMOKE_PORT:-18090}"
RUN_UPDATER_TESTS="${ONESHIM_SMOKE_RUN_UPDATER_TESTS:-1}"
ASSET_NAME="${ONESHIM_SMOKE_ASSET_NAME:-}"
INSTALL_DIR="${ONESHIM_SMOKE_INSTALL_DIR:-}"

info() {
  printf '[SMOKE] %s\n' "$*"
}

fatal() {
  printf '[SMOKE][ERROR] %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Release reliability smoke test (macOS/Linux)

Usage:
  ./scripts/release-reliability-smoke.sh [options]

Options:
  --assets-dir <path>        Asset directory containing archive + .sha256
  --install-script <path>    Installer script path (default: scripts/install.sh)
  --asset-name <name>        Override archive file name (auto-detected by OS/arch if omitted)
  --install-dir <path>       Installer target directory (temp dir by default)
  --host <host>              Local HTTP host for serving artifacts
  --port <port>              Local HTTP port for serving artifacts
  --skip-updater-tests       Skip updater release reliability tests
  -h, --help                 Show help
EOF
}

detect_asset_name() {
  local os_name="$1"
  local arch_name="$2"

  case "$os_name" in
    Darwin)
      printf 'oneshim-macos-universal.tar.gz'
      ;;
    Linux)
      case "$arch_name" in
        x86_64 | amd64)
          printf 'oneshim-linux-x64.tar.gz'
          ;;
        *)
          fatal "Unsupported Linux architecture: $arch_name"
          ;;
      esac
      ;;
    *)
      fatal "Unsupported OS: $os_name"
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --assets-dir)
      [[ $# -ge 2 ]] || fatal "--assets-dir requires a value"
      ASSETS_DIR="$2"
      shift 2
      ;;
    --install-script)
      [[ $# -ge 2 ]] || fatal "--install-script requires a value"
      INSTALL_SCRIPT="$2"
      shift 2
      ;;
    --asset-name)
      [[ $# -ge 2 ]] || fatal "--asset-name requires a value"
      ASSET_NAME="$2"
      shift 2
      ;;
    --install-dir)
      [[ $# -ge 2 ]] || fatal "--install-dir requires a value"
      INSTALL_DIR="$2"
      shift 2
      ;;
    --host)
      [[ $# -ge 2 ]] || fatal "--host requires a value"
      HOST="$2"
      shift 2
      ;;
    --port)
      [[ $# -ge 2 ]] || fatal "--port requires a value"
      PORT="$2"
      shift 2
      ;;
    --skip-updater-tests)
      RUN_UPDATER_TESTS=0
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      fatal "Unknown option: $1"
      ;;
  esac
done

[[ -d "$ASSETS_DIR" ]] || fatal "Asset directory not found: $ASSETS_DIR"
[[ -f "$INSTALL_SCRIPT" ]] || fatal "Installer script not found: $INSTALL_SCRIPT"

if [[ -z "$ASSET_NAME" ]]; then
  ASSET_NAME="$(detect_asset_name "$(uname -s)" "$(uname -m)")"
fi

ARTIFACT_PATH="$ASSETS_DIR/$ASSET_NAME"
CHECKSUM_PATH="$ARTIFACT_PATH.sha256"
[[ -f "$ARTIFACT_PATH" ]] || fatal "Artifact missing: $ARTIFACT_PATH"
[[ -f "$CHECKSUM_PATH" ]] || fatal "Checksum missing: $CHECKSUM_PATH"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oneshim-release-smoke.XXXXXX")"
SERVER_LOG="$TMP_DIR/http.log"
if [[ -z "$INSTALL_DIR" ]]; then
  INSTALL_DIR="$TMP_DIR/bin"
fi

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]] && kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

PYTHON_CMD=""
if command -v python3 >/dev/null 2>&1; then
  PYTHON_CMD="python3"
elif command -v python >/dev/null 2>&1; then
  PYTHON_CMD="python"
else
  fatal "Python is required to host local release assets"
fi

info "Serving assets from $ASSETS_DIR on http://$HOST:$PORT"
(
  cd "$ASSETS_DIR"
  "$PYTHON_CMD" -m http.server "$PORT" --bind "$HOST" >"$SERVER_LOG" 2>&1
) &
SERVER_PID=$!
sleep 1
kill -0 "$SERVER_PID" >/dev/null 2>&1 || fatal "Failed to start local HTTP server"

BASE_URL="http://$HOST:$PORT"
info "Running installer against local base URL"
bash "$INSTALL_SCRIPT" \
  --install-dir "$INSTALL_DIR" \
  --base-url "$BASE_URL"

TARGET_BIN="$INSTALL_DIR/oneshim"
[[ -x "$TARGET_BIN" ]] || fatal "Installed binary not found: $TARGET_BIN"

info "Validating first-run command"
"$TARGET_BIN" --version >/dev/null

if [[ "$RUN_UPDATER_TESTS" == "1" ]]; then
  info "Running updater reliability regression tests"
  "$CARGO_CMD" test --manifest-path "$ROOT_DIR/Cargo.toml" -p oneshim-app release_reliability_ -- --nocapture
fi

info "Release reliability smoke completed"
