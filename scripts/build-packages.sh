#!/usr/bin/env bash
# Build a native .deb / .rpm from ALREADY-BUILT release binaries, using nfpm.
#
#   scripts/build-packages.sh <target>
#
# Targets (each must be built inside that distro's own container so the binary
# links the distro's Qt — a binary from one distro does not load on another):
#   debian13    -> opengg_<ver>_debian13_amd64.deb
#   ubuntu2604  -> opengg_<ver>_ubuntu2604_amd64.deb
#   fedora44    -> opengg-<ver>-1.fc44.x86_64.rpm
#   tumbleweed  -> opengg-<ver>-1.tumbleweed.x86_64.rpm
#
# Environment:
#   VERSION          package version; default: $GITHUB_REF_NAME when it is a
#                    tag (v stripped), else `git describe`, else
#                    qt-shell/Cargo.toml
#   OUT_DIR          where the package is written (default: ./dist)
#   QT_SHELL_BIN     default qt-shell/target/release/opengg-qt
#   DAEMON_BIN       default daemon/target/release/openggd
#   NFPM             path to an nfpm binary; default: `nfpm` on PATH, else the
#                    pinned release is downloaded (sha256-verified) to $NFPM_CACHE
#   NFPM_CACHE       default ~/.cache/opengg-nfpm
set -euo pipefail

NFPM_VERSION=2.48.0
NFPM_SHA256=6d7f9a28b2f397934403950fcbba83f598b08d02edbe79a0815b3b4897341750

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NFPM_DIR="$ROOT/packaging/nfpm"
target="${1:-}"

case "$target" in
    debian13|ubuntu2604) fmt=deb; release=1 ;;
    fedora44)            fmt=rpm; release=1.fc44 ;;
    tumbleweed)          fmt=rpm; release=1.tumbleweed ;;
    *) echo "usage: $0 <debian13|ubuntu2604|fedora44|tumbleweed>" >&2; exit 2 ;;
esac

# ── version ────────────────────────────────────────────────────────────────
VERSION="${VERSION:-}"
# GITHUB_REF_NAME is "58/merge" on a pull_request: only believe it for a tag.
if [ -z "$VERSION" ] && [ "${GITHUB_REF_TYPE:-}" = tag ] && [ -n "${GITHUB_REF_NAME:-}" ]; then VERSION="$GITHUB_REF_NAME"; fi
if [ -z "$VERSION" ]; then VERSION="$(git -C "$ROOT" describe --tags --abbrev=0 2>/dev/null || true)"; fi
if [ -z "$VERSION" ]; then VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/qt-shell/Cargo.toml" | head -n1)"; fi
VERSION="${VERSION#v}"
[ -n "$VERSION" ] || { echo "cannot determine version" >&2; exit 1; }

QT_SHELL_BIN="${QT_SHELL_BIN:-$ROOT/qt-shell/target/release/opengg-qt}"
DAEMON_BIN="${DAEMON_BIN:-$ROOT/daemon/target/release/openggd}"
for f in "$QT_SHELL_BIN" "$DAEMON_BIN"; do
    [ -x "$f" ] || { echo "missing binary: $f (build the release crates first)" >&2; exit 1; }
done
OUT_DIR="$(mkdir -p "${OUT_DIR:-$ROOT/dist}" && cd "${OUT_DIR:-$ROOT/dist}" && pwd)"

# ── nfpm (pinned, checksum-verified) ───────────────────────────────────────
ensure_nfpm() {
    if [ -n "${NFPM:-}" ]; then return; fi
    if command -v nfpm >/dev/null 2>&1; then NFPM="$(command -v nfpm)"; return; fi
    local cache="${NFPM_CACHE:-$HOME/.cache/opengg-nfpm}" tgz
    NFPM="$cache/nfpm-$NFPM_VERSION"
    if [ ! -x "$NFPM" ]; then
        mkdir -p "$cache"
        tgz="$(mktemp -d)/nfpm.tar.gz"
        curl -fsSL -o "$tgz" \
            "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_Linux_x86_64.tar.gz"
        echo "$NFPM_SHA256  $tgz" | sha256sum -c - >&2
        tar -xzf "$tgz" -C "$(dirname "$tgz")" nfpm
        install -m755 "$(dirname "$tgz")/nfpm" "$NFPM"
    fi
}
ensure_nfpm

# ── stage: same layout as the release tarball ──────────────────────────────
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
STAGE="$WORK/stage"

install -Dm755 "$QT_SHELL_BIN" "$STAGE/usr/bin/opengg"
install -Dm755 "$DAEMON_BIN"   "$STAGE/usr/bin/openggd"
# Distro packages ship stripped binaries (lintian/rpmlint both flag otherwise).
if command -v strip >/dev/null 2>&1; then
    strip --strip-unneeded "$STAGE/usr/bin/opengg" "$STAGE/usr/bin/openggd" || true
fi
install -Dm644 "$ROOT/packaging/99-opengg.rules"            "$STAGE/usr/lib/udev/rules.d/99-opengg.rules"
install -Dm644 "$ROOT/packaging/org.opengg.Daemon.service"  "$STAGE/usr/share/dbus-1/services/org.opengg.Daemon.service"
install -Dm644 "$ROOT/packaging/openggd.service"            "$STAGE/usr/lib/systemd/user/openggd.service"
install -Dm644 "$ROOT/packaging/opengg.desktop"             "$STAGE/usr/share/applications/opengg.desktop"
# The UI reads its translations from here; without them every string renders
# as its raw key ("nav.home").
for f in "$ROOT"/qt-shell/locales/*.json; do
    install -Dm644 "$f" "$STAGE/usr/share/opengg/locales/$(basename "$f")"
done
install -Dm644 "$ROOT/packaging/icons/256x256.png"          "$STAGE/usr/share/icons/hicolor/256x256/apps/opengg.png"
install -Dm644 "$ROOT/LICENSE"                              "$STAGE/usr/share/doc/opengg/copyright"
# Minimal Debian changelog (policy requires one; real release notes live on the
# GitHub release page).
printf 'opengg (%s-%s) unstable; urgency=medium\n\n  * Release %s. See https://github.com/UPdullah895/opengg/releases\n\n -- UPdullah895 <abdullahbomozh@gmail.com>  %s\n' \
    "$VERSION" "$release" "$VERSION" "$(date -R)" | gzip -9n > "$STAGE/usr/share/doc/opengg/changelog.Debian.gz"

# ── config: base + per-target dependency lists ─────────────────────────────
CONF="$WORK/nfpm.yaml"
{
    sed -e "s|@VERSION@|$VERSION|g" -e "s|@RELEASE@|$release|g" \
        -e "s|@STAGE@|$STAGE|g" -e "s|@NFPM_DIR@|$NFPM_DIR|g" "$NFPM_DIR/nfpm.base.yaml"
    cat "$NFPM_DIR/targets/$target.yaml"
} > "$CONF"

case "$target" in
    debian13|ubuntu2604) out="$OUT_DIR/opengg_${VERSION}_${target}_amd64.deb" ;;
    *)                   out="$OUT_DIR/opengg-${VERSION}-${release}.x86_64.rpm" ;;
esac
"$NFPM" package --config "$CONF" --packager "$fmt" --target "$out"
echo "built $out"
