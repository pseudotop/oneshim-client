#!/usr/bin/env bash
# pre-release-check.sh — Run before tagging a release
# Validates version consistency + CHANGELOG entry
# Usage: ./scripts/pre-release-check.sh [version]
#   version: e.g. "0.3.5" (without v prefix). If omitted, reads from Cargo.toml.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

errors=0
warnings=0

pass() { echo -e "  ${GREEN}✓${NC} $1"; }
fail() { echo -e "  ${RED}✗${NC} $1"; errors=$((errors + 1)); }
warn() { echo -e "  ${YELLOW}!${NC} $1"; warnings=$((warnings + 1)); }

# --- Resolve version ---
CARGO_VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')

if [ -n "${1:-}" ]; then
  VERSION="$1"
else
  VERSION="$CARGO_VERSION"
fi

echo "=== Pre-Release Check: v${VERSION} ==="
echo ""

# --- 1. Cargo.toml version ---
echo "[Version Sync]"
if [ "$CARGO_VERSION" = "$VERSION" ]; then
  pass "Cargo.toml workspace version: $CARGO_VERSION"
else
  fail "Cargo.toml version ($CARGO_VERSION) != target ($VERSION)"
fi

# --- 2. package.json version ---
PKG_JSON="crates/oneshim-web/frontend/package.json"
if [ -f "$PKG_JSON" ]; then
  PKG_VERSION=$(python3 -c "import json; print(json.load(open('$PKG_JSON'))['version'])")
  if [ "$PKG_VERSION" = "$VERSION" ]; then
    pass "package.json version: $PKG_VERSION"
  else
    fail "package.json version ($PKG_VERSION) != target ($VERSION)"
    echo "       Fix: Update version in $PKG_JSON to \"$VERSION\""
  fi
fi

# --- 3. src-tauri Cargo.toml inherits workspace version ---
TAURI_CARGO="src-tauri/Cargo.toml"
if [ -f "$TAURI_CARGO" ]; then
  if grep -q 'version.workspace = true' "$TAURI_CARGO"; then
    pass "src-tauri/Cargo.toml inherits workspace version"
  else
    TAURI_VER=$(grep -m1 '^version' "$TAURI_CARGO" | sed 's/.*"\(.*\)"/\1/')
    if [ "$TAURI_VER" = "$VERSION" ]; then
      pass "src-tauri/Cargo.toml version: $TAURI_VER"
    else
      fail "src-tauri/Cargo.toml version ($TAURI_VER) != target ($VERSION)"
    fi
  fi
fi

echo ""

# --- 4. CHANGELOG.md ---
echo "[CHANGELOG]"
if [ -f "CHANGELOG.md" ]; then
  if grep -q "## \[$VERSION\]" CHANGELOG.md; then
    pass "CHANGELOG.md has entry for [$VERSION]"
    # Check if it has a date
    if grep -q "## \[$VERSION\] - " CHANGELOG.md; then
      pass "CHANGELOG entry has a date"
    else
      warn "CHANGELOG entry for [$VERSION] has no date"
    fi
    # Check if section has content (not just the header)
    SECTION_LINE=$(grep -n "## \[$VERSION\]" CHANGELOG.md | head -1 | cut -d: -f1)
    NEXT_SECTION_LINE=$(awk -v start="$((SECTION_LINE + 1))" 'NR > start && /^## \[/ { print NR; exit }' CHANGELOG.md)
    if [ -n "$NEXT_SECTION_LINE" ]; then
      CONTENT_LINES=$(sed -n "$((SECTION_LINE + 1)),$((NEXT_SECTION_LINE - 1))p" CHANGELOG.md | grep -c '[^[:space:]]' || true)
    else
      CONTENT_LINES=$(sed -n "$((SECTION_LINE + 1)),\$p" CHANGELOG.md | grep -c '[^[:space:]]' || true)
    fi
    if [ "$CONTENT_LINES" -gt 2 ]; then
      pass "CHANGELOG section has content ($CONTENT_LINES non-empty lines)"
    else
      warn "CHANGELOG section looks sparse ($CONTENT_LINES non-empty lines)"
    fi
  else
    fail "CHANGELOG.md missing entry for [$VERSION]"
    echo "       Fix: Add '## [$VERSION] - $(date +%Y-%m-%d)' section to CHANGELOG.md"
  fi
else
  fail "CHANGELOG.md not found"
fi

echo ""

# --- 5. Git status ---
echo "[Git Status]"
if [ -z "$(git status --porcelain -- CHANGELOG.md)" ]; then
  pass "CHANGELOG.md is committed"
else
  fail "CHANGELOG.md has uncommitted changes — commit before tagging"
fi

if [ -z "$(git diff --cached --name-only)" ]; then
  pass "No staged changes"
else
  warn "Staged changes exist — commit before tagging"
fi

echo ""

# --- 6. Tag check ---
echo "[Tag]"
if git tag -l "v$VERSION" | grep -q "v$VERSION"; then
  warn "Tag v$VERSION already exists (re-tagging will require: git tag -d v$VERSION && git push origin :refs/tags/v$VERSION)"
else
  pass "Tag v$VERSION does not exist yet"
fi

echo ""

# --- 7. Run config-sync check if available ---
if [ -x "scripts/check-config-sync.sh" ]; then
  echo "[Config Sync]"
  if scripts/check-config-sync.sh > /dev/null 2>&1; then
    pass "Config sync check passed"
  else
    fail "Config sync check failed — run: ./scripts/check-config-sync.sh --fix"
  fi
  echo ""
fi

# --- Summary ---
echo "=== Summary ==="
if [ "$errors" -gt 0 ]; then
  echo -e "${RED}$errors error(s)${NC}, $warnings warning(s) — fix errors before tagging"
  exit 1
elif [ "$warnings" -gt 0 ]; then
  echo -e "${GREEN}0 errors${NC}, ${YELLOW}$warnings warning(s)${NC} — OK to proceed"
  exit 0
else
  echo -e "${GREEN}All checks passed${NC} — ready to tag v$VERSION"
  exit 0
fi
