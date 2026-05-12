#!/usr/bin/env bash
# Bump the version across all OpenGG source files.
# Usage: ./scripts/bump-version.sh 0.1.2
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

NEW_VERSION="${1:-}"
if [[ -z "$NEW_VERSION" ]]; then
  echo "Usage: $0 <version>"
  echo "Example: $0 0.1.2"
  exit 1
fi

# Strip leading 'v' if present
NEW_VERSION="${NEW_VERSION#v}"

echo "Bumping OpenGG to version $NEW_VERSION"

# ── Rust crates ──
sed -i "s/^version = \"[^\"]*\"/version = \"$NEW_VERSION\"/" "$ROOT/daemon/Cargo.toml"
sed -i "s/^version = \"[^\"]*\"/version = \"$NEW_VERSION\"/" "$ROOT/frontend/src-tauri/Cargo.toml"

# ── NPM ──
sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$NEW_VERSION\"/" "$ROOT/frontend/package.json"

# ── Tauri config ──
sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$NEW_VERSION\"/" "$ROOT/frontend/src-tauri/tauri.conf.json"

# ── Extension template ──
sed -i "s/\"version\":\s*\"[^\"]*\"/\"version\": \"$NEW_VERSION\"/" "$ROOT/extension-template/manifest.json"

# ── Locale files (About page version string) ──
# Note: changelog entries are historical and NOT bumped
sed -i "s/\"version\": \"v[^\"]*\"/\"version\": \"v$NEW_VERSION\"/" "$ROOT/frontend/src/locales/en.json"
# Arabic locale: "الإصدار X.X.X" — replace the version number at the end
sed -i "s/\"version\": \"الإصدار [0-9]\+\.[0-9]\+\.[0-9]\+\"/\"version\": \"الإصدار $NEW_VERSION\"/" "$ROOT/frontend/src/locales/ar.json"

# ── package-lock.json ──
cd "$ROOT/frontend"
npm install --package-lock-only 2>/dev/null || true

echo "✓ Version bumped to $NEW_VERSION"
echo ""
echo "Files modified:"
git -C "$ROOT" diff --name-only 2>/dev/null || true
