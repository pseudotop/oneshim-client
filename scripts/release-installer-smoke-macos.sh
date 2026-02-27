#!/usr/bin/env bash

set -euo pipefail

ASSETS_DIR="${ONESHIM_SMOKE_INSTALLERS_DIR:-smoke-installers}"
DMG_NAME="${ONESHIM_SMOKE_DMG_NAME:-oneshim-macos-universal.dmg}"
PKG_NAME="${ONESHIM_SMOKE_PKG_NAME:-oneshim-macos-universal.pkg}"
APP_NAME="${ONESHIM_SMOKE_APP_NAME:-ONESHIM.app}"

info() {
  printf '[INSTALLER-SMOKE] %s\n' "$*"
}

fatal() {
  printf '[INSTALLER-SMOKE][ERROR] %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
macOS installer smoke test for release artifacts

Usage:
  ./scripts/release-installer-smoke-macos.sh [options]

Options:
  --assets-dir <path>  Directory containing DMG/PKG artifacts
  --dmg-name <name>    DMG file name (default: oneshim-macos-universal.dmg)
  --pkg-name <name>    PKG file name (default: oneshim-macos-universal.pkg)
  --app-name <name>    Installed app bundle name (default: ONESHIM.app)
  -h, --help           Show help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --assets-dir)
      [[ $# -ge 2 ]] || fatal "--assets-dir requires a value"
      ASSETS_DIR="$2"
      shift 2
      ;;
    --dmg-name)
      [[ $# -ge 2 ]] || fatal "--dmg-name requires a value"
      DMG_NAME="$2"
      shift 2
      ;;
    --pkg-name)
      [[ $# -ge 2 ]] || fatal "--pkg-name requires a value"
      PKG_NAME="$2"
      shift 2
      ;;
    --app-name)
      [[ $# -ge 2 ]] || fatal "--app-name requires a value"
      APP_NAME="$2"
      shift 2
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

resolve_asset() {
  local file_name="$1"
  local direct_path="$ASSETS_DIR/$file_name"
  if [[ -f "$direct_path" ]]; then
    printf '%s' "$direct_path"
    return
  fi

  local found_path
  found_path="$(find "$ASSETS_DIR" -type f -name "$file_name" | head -n1 || true)"
  if [[ -z "$found_path" ]]; then
    fatal "Asset not found: $file_name"
  fi
  printf '%s' "$found_path"
}

run_as_root() {
  if command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    "$@"
  fi
}

DMG_PATH="$(resolve_asset "$DMG_NAME")"
PKG_PATH="$(resolve_asset "$PKG_NAME")"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oneshim-installer-smoke.XXXXXX")"
MOUNT_DIR="$TMP_DIR/mount"
APP_INSTALL_PATH="/Applications/$APP_NAME"
APP_BINARY_PATH="$APP_INSTALL_PATH/Contents/MacOS/oneshim"
APP_BACKUP_PATH="$TMP_DIR/${APP_NAME}.backup"
MOUNTED=0
APP_WAS_PRESENT=0

cleanup() {
  if [[ "$MOUNTED" == "1" ]]; then
    hdiutil detach "$MOUNT_DIR" -quiet || true
  fi

  if [[ -d "$APP_INSTALL_PATH" ]]; then
    run_as_root rm -rf "$APP_INSTALL_PATH" || true
  fi

  if [[ "$APP_WAS_PRESENT" == "1" && -d "$APP_BACKUP_PATH" ]]; then
    run_as_root mv "$APP_BACKUP_PATH" "$APP_INSTALL_PATH" || true
  fi

  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

info "Using DMG: $DMG_PATH"
info "Using PKG: $PKG_PATH"

if [[ -d "$APP_INSTALL_PATH" ]]; then
  info "Backing up existing app at $APP_INSTALL_PATH"
  run_as_root mv "$APP_INSTALL_PATH" "$APP_BACKUP_PATH"
  APP_WAS_PRESENT=1
fi

mkdir -p "$MOUNT_DIR"
info "Mounting DMG"
hdiutil attach "$DMG_PATH" -nobrowse -readonly -mountpoint "$MOUNT_DIR" -quiet
MOUNTED=1

DMG_APP_PATH="$MOUNT_DIR/$APP_NAME"
DMG_BINARY_PATH="$DMG_APP_PATH/Contents/MacOS/oneshim"
[[ -d "$DMG_APP_PATH" ]] || fatal "App bundle missing in DMG: $DMG_APP_PATH"
[[ -x "$DMG_BINARY_PATH" ]] || fatal "Binary missing in DMG app bundle: $DMG_BINARY_PATH"

info "Validating binary from mounted DMG"
"$DMG_BINARY_PATH" --version >/dev/null

info "Installing PKG"
run_as_root installer -pkg "$PKG_PATH" -target / >/dev/null
[[ -x "$APP_BINARY_PATH" ]] || fatal "Installed app binary missing: $APP_BINARY_PATH"

info "Validating binary from PKG installation"
"$APP_BINARY_PATH" --version >/dev/null

info "macOS installer smoke completed"
