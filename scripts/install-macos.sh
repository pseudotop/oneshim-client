#!/usr/bin/env bash
# ONESHIM macOS 설치 스크립트
# 바이너리 복사 + LaunchAgent plist 설치 + launchctl load

set -euo pipefail

BINARY_NAME="oneshim"
INSTALL_DIR="/usr/local/bin"
PLIST_LABEL="com.oneshim.agent"
PLIST_DIR="$HOME/Library/LaunchAgents"
PLIST_PATH="$PLIST_DIR/$PLIST_LABEL.plist"

# 색상 출력
info()  { echo -e "\033[1;34m[INFO]\033[0m  $*"; }
ok()    { echo -e "\033[1;32m[OK]\033[0m    $*"; }
error() { echo -e "\033[1;31m[ERROR]\033[0m $*" >&2; }

# 바이너리 찾기
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY_PATH="$REPO_ROOT/target/release/$BINARY_NAME"

if [[ ! -f "$BINARY_PATH" ]]; then
    error "릴리즈 바이너리가 없습니다: $BINARY_PATH"
    info "먼저 빌드하세요: cargo build --release -p oneshim-app"
    exit 1
fi

# 바이너리 복사
info "바이너리 복사: $BINARY_PATH → $INSTALL_DIR/$BINARY_NAME"
sudo cp "$BINARY_PATH" "$INSTALL_DIR/$BINARY_NAME"
sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
ok "바이너리 설치 완료"

# LaunchAgents 디렉토리 생성
mkdir -p "$PLIST_DIR"

# plist 생성
info "LaunchAgent plist 생성: $PLIST_PATH"
cat > "$PLIST_PATH" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$PLIST_LABEL</string>
    <key>ProgramArguments</key>
    <array>
        <string>$INSTALL_DIR/$BINARY_NAME</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>StandardOutPath</key>
    <string>/tmp/oneshim-client.out.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/oneshim-client.err.log</string>
</dict>
</plist>
EOF
ok "plist 생성 완료"

# launchctl load
info "LaunchAgent 등록"
launchctl load "$PLIST_PATH" 2>/dev/null || true
ok "ONESHIM 설치 완료! 다음 로그인 시 자동 시작됩니다."
