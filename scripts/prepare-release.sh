#!/usr/bin/env bash
# prepare-release.sh — Prepare a new release with version consistency checks
#
# Usage:
#   ./scripts/prepare-release.sh 0.0.2
#   ./scripts/prepare-release.sh 0.1.0
#
# This script:
#   1. Updates Cargo.toml workspace version
#   2. Runs cargo check to update Cargo.lock
#   3. Validates CHANGELOG.md has an entry for the version
#   4. Commits, tags, and optionally pushes

set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <version>"
  echo "Example: $0 0.0.2"
  exit 1
fi

VERSION="$1"
TAG="v${VERSION}"

# Validate semver format
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "Error: Version must be in semver format (e.g., 0.0.2)"
  exit 1
fi

# Check we're on main
BRANCH=$(git branch --show-current)
if [ "$BRANCH" != "main" ]; then
  echo "Error: Must be on main branch (currently on $BRANCH)"
  exit 1
fi

# Check working tree is clean
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "Error: Working tree is not clean. Commit or stash changes first."
  exit 1
fi

# Check tag doesn't already exist
if git tag -l "$TAG" | grep -q "$TAG"; then
  echo "Error: Tag $TAG already exists"
  exit 1
fi

echo "=== Preparing release $TAG ==="

# 1. Update Cargo.toml version
CURRENT_VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
echo "Updating Cargo.toml: $CURRENT_VERSION -> $VERSION"
sed -i.bak "s/^version = \"$CURRENT_VERSION\"/version = \"$VERSION\"/" Cargo.toml
rm -f Cargo.toml.bak

# 2. Update Cargo.lock
echo "Updating Cargo.lock..."
cargo update --workspace 2>/dev/null || cargo generate-lockfile 2>/dev/null || true

# 3. Validate CHANGELOG.md
if ! grep -q "## \[$VERSION\]" CHANGELOG.md; then
  echo ""
  echo "Error: CHANGELOG.md is missing entry for [$VERSION]"
  echo ""
  echo "Add the following to CHANGELOG.md before the [Unreleased] link:"
  echo ""
  echo "  ## [$VERSION] - $(date +%Y-%m-%d)"
  echo ""
  echo "  ### Added"
  echo "  - ..."
  echo ""
  echo "  ### Changed"
  echo "  - ..."
  echo ""
  echo "  ### Fixed"
  echo "  - ..."
  echo ""
  # Revert Cargo.toml
  git checkout Cargo.toml Cargo.lock 2>/dev/null || true
  exit 1
fi

# 4. Update CHANGELOG.md [Unreleased] link
sed -i.bak "s|\[Unreleased\]: .*|[Unreleased]: https://github.com/pseudotop/oneshim-client/compare/$TAG...HEAD|" CHANGELOG.md
# Add version comparison link if not present
if ! grep -q "\[$VERSION\]:" CHANGELOG.md; then
  echo "[$VERSION]: https://github.com/pseudotop/oneshim-client/releases/tag/$TAG" >> CHANGELOG.md
fi
rm -f CHANGELOG.md.bak

# 5. Stage and commit
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "release: $TAG"

# 6. Create tag
git tag "$TAG"

echo ""
echo "=== Release $TAG prepared ==="
echo ""
echo "Review the commit:"
echo "  git log --oneline -1"
echo "  git diff HEAD~1"
echo ""
echo "Push to trigger CI/CD release pipeline:"
echo "  git push origin main --tags"
echo ""
echo "Or abort:"
echo "  git tag -d $TAG && git reset --soft HEAD~1"
