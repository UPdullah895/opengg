#!/usr/bin/env bash
# Sync packaging/aur/PKGBUILD into a local AUR git clone and regenerate its
# .SRCINFO, so the AUR listing can't drift from what's tracked in this repo.
# Usage: ./scripts/sync-aur.sh [path-to-aur-clone]
# Defaults to ~/aur/opengg-bin. Silently does nothing if that path doesn't
# exist, so it's safe to call from bump-version.sh on machines without a
# local AUR clone.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

AUR_DIR="${1:-$HOME/aur/opengg-bin}"

if [[ ! -d "$AUR_DIR/.git" ]]; then
  exit 0
fi

if ! command -v makepkg >/dev/null 2>&1; then
  echo "⚠ makepkg not found — can't regenerate .SRCINFO in $AUR_DIR. Install pacman-contrib/base-devel." >&2
  exit 1
fi

cp "$ROOT/packaging/aur/PKGBUILD" "$AUR_DIR/PKGBUILD"
(cd "$AUR_DIR" && makepkg --printsrcinfo > .SRCINFO)

if git -C "$AUR_DIR" diff --quiet; then
  echo "✓ $AUR_DIR already matches packaging/aur/PKGBUILD — nothing to publish."
else
  NEW_VER="$(grep -m1 '^pkgver=' "$AUR_DIR/PKGBUILD" | cut -d= -f2)"
  echo "✓ Synced PKGBUILD and regenerated .SRCINFO in $AUR_DIR"
  echo ""
  echo "Review and publish to AUR:"
  echo "  cd $AUR_DIR"
  echo "  git diff"
  echo "  git add -A && git commit -m \"update to v$NEW_VER\""
  echo "  git push"
fi
