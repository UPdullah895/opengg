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

# Everything the PKGBUILD names locally has to travel with it. `install=` is
# the one that bit before: only PKGBUILD was copied, so edits to
# opengg.install never reached the AUR and makepkg failed on a clone that
# lacked the file.
cp "$ROOT/packaging/aur/PKGBUILD" "$AUR_DIR/PKGBUILD"
install_file="$(sed -n "s/^install=['\"]\{0,1\}\([^'\"]*\)['\"]\{0,1\}$/\1/p" "$ROOT/packaging/aur/PKGBUILD" | head -n1)"
if [[ -n "$install_file" ]]; then
  if [[ ! -f "$ROOT/packaging/aur/$install_file" ]]; then
    echo "✗ PKGBUILD says install=$install_file but packaging/aur/$install_file does not exist." >&2
    exit 1
  fi
  cp "$ROOT/packaging/aur/$install_file" "$AUR_DIR/$install_file"
fi

# The repo's PKGBUILD keeps sha256sums=('SKIP') because the tarball does not
# exist until the release workflow has built it. By the time someone syncs the
# AUR it does, and the workflow publishes SHA256SUMS next to it: use that so
# the AUR package is integrity-checked. Falls back to SKIP (with a warning)
# when the release or its SHA256SUMS is not reachable.
pkgver="$(sed -n 's/^pkgver=//p' "$AUR_DIR/PKGBUILD" | head -n1)"
tarball="opengg-v${pkgver}-linux-x86_64.tar.gz"
sums_url="https://github.com/UPdullah895/opengg/releases/download/v${pkgver}/SHA256SUMS"
if sum="$(curl -fsSL --max-time 30 "$sums_url" 2>/dev/null | awk -v f="$tarball" '$2 == f {print $1}')" && [[ "$sum" =~ ^[0-9a-f]{64}$ ]]; then
  sed -i "s/^sha256sums=.*/sha256sums=('$sum')/" "$AUR_DIR/PKGBUILD"
  echo "✓ sha256sums filled from the v${pkgver} release (${sum:0:12}…)"
else
  echo "⚠ could not read $tarball's checksum from $sums_url — AUR PKGBUILD keeps sha256sums=('SKIP')." >&2
fi
(cd "$AUR_DIR" && makepkg --printsrcinfo > .SRCINFO)

# `status --porcelain`, not `diff --quiet`: a newly copied (untracked) file such
# as opengg.install is a change worth publishing too.
if [[ -z "$(git -C "$AUR_DIR" status --porcelain)" ]]; then
  echo "✓ $AUR_DIR already matches packaging/aur — nothing to publish."
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
