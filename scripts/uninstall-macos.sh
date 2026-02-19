#!/usr/bin/env bash
# ONESHIM macOS 제거 스크립트
# launchctl unload + plist 삭제 + 바이너리 삭제

set -euo pipefail

BINARY_NAME="oneshim"
INSTALL_DIR="/usr/local/bin"
PLIST_LABEL="com.oneshim.agent"
PLIST_PATH="$HOME/Library/LaunchAgents/$PLIST_LABEL.plist"

# 색상 출력
info()  { echo -e "\033[1;34m[INFO]\033[0m  $*"; }
ok()    { echo -e "\033[1;32m[OK]\033[0m    $*"; }

# LaunchAgent 해제
if [[ -f "$PLIST_PATH" ]]; then
    info "LaunchAgent 해제"
    launchctl unload "$PLIST_PATH" 2>/dev/null || true
    rm -f "$PLIST_PATH"
    ok "plist 삭제 완료"
else
    info "plist 파일 없음 (이미 제거됨)"
fi

# 바이너리 삭제
if [[ -f "$INSTALL_DIR/$BINARY_NAME" ]]; then
    info "바이너리 삭제: $INSTALL_DIR/$BINARY_NAME"
    sudo rm -f "$INSTALL_DIR/$BINARY_NAME"
    ok "바이너리 삭제 완료"
else
    info "바이너리 없음 (이미 제거됨)"
fi

ok "ONESHIM 제거 완료!"
