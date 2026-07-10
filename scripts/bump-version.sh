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

# Note: the app's own version is read at runtime via Tauri's getVersion()
# (see frontend/src/composables/useAppVersion.ts) — there is no static
# "About page version" string in the locale files to bump. Changelog
# entries are historical and NOT bumped either. Don't add a blind
# "version": "v..." sed here — settings.store.browse.version in both
# locales is an unrelated per-extension display template ("v{version}")
# and a prior version of this script clobbered it by mistake.

# ── AUR packaging ──
sed -i "s/^pkgver=.*/pkgver=$NEW_VERSION/" "$ROOT/packaging/aur/PKGBUILD"
if command -v makepkg >/dev/null 2>&1; then
  (cd "$ROOT/packaging/aur" && makepkg --printsrcinfo > .SRCINFO)
else
  echo "⚠ makepkg not found — packaging/aur/.SRCINFO NOT regenerated, do it manually before releasing."
fi

# ── package-lock.json ──
cd "$ROOT/frontend"
npm install --package-lock-only 2>/dev/null || true

echo "✓ Version bumped to $NEW_VERSION"
echo ""
echo "Files modified:"
git -C "$ROOT" diff --name-only 2>/dev/null || true

# ── Local AUR clone (opt-in, only if it exists on this machine) ──
"$SCRIPT_DIR/sync-aur.sh" || true
