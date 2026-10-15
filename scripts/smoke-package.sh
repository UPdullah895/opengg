#!/usr/bin/env bash
# Install a SHIPPED OpenGG package in a clean distro container and launch it.
#
#   scripts/smoke-package.sh <debian|fedora|suse|arch> <package.deb|package.rpm|tarball.tar.gz> [outdir]
#
# Why this exists: until now every .deb/.rpm/tarball was built and uploaded
# without ever being installed or started. A missing runtime dependency (a
# qml6-module-* package, pipewire-utils on Fedora, a Qt floor above what the
# distro has) was only discovered by a user, as a window that never appears.
# Run this in a FRESH container of the target distro — the build image has
# every -dev and runtime package installed and would hide exactly that.
#
# Must run as root (installs the package). Checks, in order:
#   1. the package installs with the distro's own manager (arch: the
#      PKGBUILD's depends=() are installed, then the tarball is unpacked)
#   2. no shared library of /usr/bin/opengg or /usr/bin/openggd is unresolved
#   3. the external tools the daemon/UI shell out to are present
#   4. the shipped binary, run as a NON-root user on a fresh $HOME, renders the
#      home page offscreen, exits 0 and leaves a non-empty PNG
#   5. nothing in its output says a QML module / type / symbol version is
#      missing (the three ways a package that "installed fine" does not run)
#
# Environment: PKGBUILD (arch only; default packaging/aur/PKGBUILD next to this
# script's repo). The screenshot and log land in [outdir] (default /tmp/smoke-out).
set -uo pipefail

family="${1:-}"; pkg="${2:-}"; out="${3:-/tmp/smoke-out}"
if [ -z "$family" ] || [ -z "$pkg" ] || [ ! -f "$pkg" ]; then
    echo "usage: $0 <debian|fedora|suse|arch> <package-file> [outdir]" >&2
    exit 2
fi
pkg="$(readlink -f "$pkg")"
out="$(readlink -m "$out")"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PKGBUILD="${PKGBUILD:-$ROOT/packaging/aur/PKGBUILD}"
USER_NAME=smoke

fail=0
bad() { echo "::error::$*"; fail=1; }
step() { printf '\n== %s\n' "$*"; }

# ── 1. install ─────────────────────────────────────────────────────────────
step "install $(basename "$pkg")"
case "$family" in
    debian)
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y "$pkg" || { bad "apt-get could not install $pkg"; exit 1; }
        ;;
    fedora)
        dnf install -y "$pkg" || { bad "dnf could not install $pkg"; exit 1; }
        ;;
    suse)
        zypper --non-interactive --gpg-auto-import-keys refresh
        # Unsigned: CI builds the rpm itself and has no signing key.
        zypper --non-interactive install --allow-unsigned-rpm "$pkg" \
            || { bad "zypper could not install $pkg"; exit 1; }
        ;;
    arch)
        # The tarball is what the opengg-bin AUR package installs, so the
        # dependency list under test is the PKGBUILD's, parsed here rather than
        # copied (a copy would drift, which is the failure this guards).
        [ -f "$PKGBUILD" ] || { bad "PKGBUILD not found: $PKGBUILD"; exit 1; }
        mapfile -t deps < <(bash -c 'source "$1" >/dev/null 2>&1; printf "%s\n" "${depends[@]}"' _ "$PKGBUILD")
        [ "${#deps[@]}" -gt 0 ] || { bad "no depends=() parsed from $PKGBUILD"; exit 1; }
        echo "PKGBUILD depends: ${deps[*]}"
        pacman -Syu --noconfirm --needed "${deps[@]}" \
            || { bad "pacman could not satisfy the PKGBUILD depends"; exit 1; }
        tar -xzf "$pkg" -C / --strip-components=1 || { bad "cannot unpack $pkg"; exit 1; }
        ;;
    *) echo "unknown family: $family" >&2; exit 2 ;;
esac

# ── 2. every shared library resolves ───────────────────────────────────────
step "ldd"
for b in /usr/bin/opengg /usr/bin/openggd; do
    [ -x "$b" ] || { bad "$b was not installed by the package"; continue; }
    missing="$(ldd "$b" 2>&1 | grep 'not found' || true)"
    if [ -n "$missing" ]; then
        bad "$b has unresolved libraries (a missing or too-old dependency):"
        echo "$missing"
    else
        echo "ok  $b"
    fi
done

# ── 3. tools the daemon / UI shell out to ──────────────────────────────────
step "runtime tools"
for t in pactl pw-link pw-cli pw-dump pw-metadata ffmpeg ffprobe; do
    if command -v "$t" >/dev/null 2>&1; then
        echo "ok  $t -> $(command -v "$t")"
    else
        bad "$t is not on PATH after installing the package (declare the package that ships it)"
    fi
done

# ── test tooling (installed AFTER the checks above so it cannot mask them) ──
step "test tooling"
# dbus-run-session for the session bus, and one font so the uploaded screenshot
# is readable (a container has none, and every glyph renders as a box; real
# desktops always have fonts, so this is not a dependency under test).
case "$family" in
    debian) apt-get install -y --no-install-recommends dbus fonts-dejavu-core ;;
    # util-linux-core: the fedora image ships neither runuser nor su.
    fedora) dnf install -y dbus-daemon dejavu-sans-fonts util-linux-core ;;
    suse)   zypper --non-interactive install --no-recommends dbus-1-daemon dbus-1-tools dejavu-fonts ;;
    arch)   pacman -S --noconfirm --needed dbus ttf-dejavu ;;
esac
command -v dbus-run-session >/dev/null 2>&1 || { bad "dbus-run-session unavailable: cannot run the launch test"; exit 1; }

# ── 4. launch the shipped binary as an ordinary user ───────────────────────
step "launch as non-root, fresh \$HOME"
useradd -m "$USER_NAME" 2>/dev/null || true
mkdir -p "$out"
chown "$USER_NAME" "$out"
chmod 755 "$out"
rundir="$(mktemp -d)"
chown "$USER_NAME" "$rundir"
chmod 700 "$rundir"

as_user() {
    if command -v runuser >/dev/null 2>&1; then
        runuser -u "$USER_NAME" -- "$@"
    else
        su -s /bin/sh "$USER_NAME" -c "$(printf '%q ' "$@")"
    fi
}

log="$out/launch.log"
# QT_FORCE_STDERR_LOGGING: Qt builds with journald support send QML warnings to
# the journal otherwise, and this log would look clean no matter what.
as_user env HOME="/home/$USER_NAME" XDG_RUNTIME_DIR="$rundir" \
    QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 \
    timeout 90 dbus-run-session -- opengg --screenshot "$out/home.png" --page home --delay 1500 \
    >"$log" 2>&1
rc=$?
echo "exit status: $rc"
sed 's/^/  | /' "$log" | tail -n 60

[ "$rc" -eq 0 ] || bad "opengg exited with status $rc (127 = loader failure, 124 = hung)"

# ── 5. outcome ─────────────────────────────────────────────────────────────
step "verdict"
if [ ! -s "$out/home.png" ]; then
    bad "no screenshot written"
elif [ "$(head -c 8 "$out/home.png" | od -An -tx1 | tr -d ' \n')" != 89504e470d0a1a0a ]; then
    bad "$out/home.png is not a PNG"
else
    echo "ok  screenshot: $(wc -c <"$out/home.png") bytes"
fi

# The three ways a package "installs fine" yet does not run, plus the loader
# and platform-plugin failures that precede them.
patterns='module .* is not installed|is not a type|version .* not found|cannot open shared object file|Could not load the Qt platform plugin|Cannot load library|qml6glsink unavailable'
hits="$(grep -E "$patterns" "$log" || true)"
if [ -n "$hits" ]; then
    bad "launch output reports a missing dependency:"
    echo "$hits"
fi

if [ "$fail" -ne 0 ]; then
    echo "SMOKE FAILED"
    exit 1
fi
echo "SMOKE OK"
