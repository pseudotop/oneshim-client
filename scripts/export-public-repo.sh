#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  ./scripts/export-public-repo.sh <destination-dir> [source-ref]

Examples:
  ./scripts/export-public-repo.sh /tmp/oneshim-client-public
  ./scripts/export-public-repo.sh /tmp/oneshim-client-public codex/release-web-gates-qa-connected-hardening

Behavior:
  1. Exports a clean snapshot of <source-ref> (default: HEAD).
  2. Applies exclusion rules from scripts/public-repo-exclude.txt.
  3. Initializes a fresh Git history in <destination-dir> with one initial commit.

This script does not push to any remote.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

DEST_DIR="${1:-}"
SOURCE_REF="${2:-HEAD}"

if [[ -z "$DEST_DIR" ]]; then
  usage
  exit 1
fi

if [[ -e "$DEST_DIR" ]]; then
  echo "error: destination already exists: $DEST_DIR" >&2
  exit 1
fi

REPO_ROOT="$(git rev-parse --show-toplevel)"
EXCLUDE_FILE="$REPO_ROOT/scripts/public-repo-exclude.txt"

mkdir -p "$DEST_DIR"

echo "==> Exporting snapshot from ref: $SOURCE_REF"
git -C "$REPO_ROOT" archive "$SOURCE_REF" | tar -xf - -C "$DEST_DIR"

if [[ -f "$EXCLUDE_FILE" ]]; then
  echo "==> Applying exclude rules from: scripts/public-repo-exclude.txt"
  while IFS= read -r rule; do
    [[ -z "$rule" || "$rule" =~ ^# ]] && continue
    rm -rf "$DEST_DIR/$rule"
  done < "$EXCLUDE_FILE"
fi

echo "==> Initializing fresh Git history"
git -C "$DEST_DIR" init -b main >/dev/null
git -C "$DEST_DIR" add -A
git -C "$DEST_DIR" commit -m "chore: bootstrap public repository history" >/dev/null

echo "==> Done"
echo "Public repo path: $DEST_DIR"
echo "Next:"
echo "  cd $DEST_DIR"
echo "  git log --oneline --decorate -n 1"
echo "  git remote add origin <public-repo-url>"
echo "  git push -u origin main"
