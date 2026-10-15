#!/usr/bin/env bash
#
# OpenGG — Unified Development Script
#
# Usage:
#   ./dev.sh          Run daemon + frontend (full stack)
#   ./dev.sh daemon   Run daemon only
#   ./dev.sh ui       Run frontend only
#   ./dev.sh build    Build everything for release
#   ./dev.sh setup    First-time setup (create dirs, group membership)
#   ./dev.sh deps     Check build prerequisites only (prints an install command)
#   ./dev.sh examples Install the bundled example extensions (opt-in)
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
DAEMON_DIR="$ROOT_DIR/daemon"
QT_SHELL_DIR="$ROOT_DIR/qt-shell"

# ── Colors ───────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
BOLD='\033[1m'
DIM='\033[2m'
RESET='\033[0m'

# Colored prefixes for log streams
DAEMON_PREFIX="${RED}[daemon]${RESET}"
DEV_PREFIX="${GREEN}[dev]${RESET}"

# ── Helpers ──────────────────────────────────────────────────────
log()  { echo -e "${DEV_PREFIX} $*"; }
logw() { echo -e "${DEV_PREFIX} ${YELLOW}⚠ $*${RESET}"; }
loge() { echo -e "${DEV_PREFIX} ${RED}✗ $*${RESET}"; }
logs() { echo -e "${DEV_PREFIX} ${GREEN}✓ $*${RESET}"; }

# Track child PIDs for clean shutdown
PIDS=()

cleanup() {
    # Keep the real exit status: a failed preflight or build must not look
    # like success to the caller (CI, scripts, `./dev.sh && ...`).
    local rc=$?
    echo ""
    log "Shutting down..."

    # Kill all tracked child processes
    for pid in "${PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill -TERM "$pid" 2>/dev/null || true
        fi
    done

    # Wait briefly, then force-kill stragglers
    sleep 1
    for pid in "${PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
        fi
    done

    # Kill any orphaned openggd processes from this session
    pkill -f "target/debug/openggd" 2>/dev/null || true
    pkill -f "target/release/openggd" 2>/dev/null || true

    logs "All processes terminated"
    exit "$rc"
}

# Ctrl+C / SIGTERM are the normal way to stop the dev stack, so they exit 0;
# the EXIT trap then sees that status. Anything else keeps its own.
trap 'exit 0' SIGINT SIGTERM
trap cleanup EXIT

# ── Preflight Checks ────────────────────────────────────────────
# Distro family from /etc/os-release (ID, then ID_LIKE): arch, fedora, debian
# or suse. Anything else is "unknown" and gets a generic tool list.
detect_family() {
    local ids
    ids=" $( . /etc/os-release 2>/dev/null; echo "${ID:-} ${ID_LIKE:-}" ) "
    case "$ids" in
        *" arch "*|*" cachyos "*|*" manjaro "*) echo arch ;;
        *" fedora "*|*" rhel "*|*" centos "*)   echo fedora ;;
        *" debian "*|*" ubuntu "*)              echo debian ;;
        *suse*)                                 echo suse ;;
        *)                                      echo unknown ;;
    esac
}

# Package(s) that provide a prerequisite, per distro family. Names verified by
# building and launching OpenGG in clean containers (see README "Requirements").
pkg_for() {
    local family="$1" key="$2"
    case "$family:$key" in
        arch:cxx) echo base-devel ;;       fedora:cxx) echo gcc gcc-c++ make ;;
        debian:cxx) echo build-essential ;; suse:cxx) echo gcc gcc-c++ make ;;
        *:cmake) echo cmake ;;
        arch:clang) echo clang ;;          fedora:clang) echo clang clang-devel ;;
        debian:clang) echo clang libclang-dev ;; suse:clang) echo clang ;;
        arch:pkgconf) echo pkgconf ;;      debian:pkgconf) echo pkg-config ;;
        fedora:pkgconf|suse:pkgconf) echo pkgconf-pkg-config ;;
        arch:qtbase) echo qt6-base ;;      fedora:qtbase) echo qt6-qtbase-devel ;;
        debian:qtbase) echo qt6-base-dev ;; suse:qtbase) echo qt6-base-devel ;;
        arch:qtdecl) echo qt6-declarative ;; fedora:qtdecl) echo qt6-qtdeclarative-devel ;;
        debian:qtdecl) echo qt6-declarative-dev ;; suse:qtdecl) echo qt6-declarative-devel ;;
        arch:qtmm) echo qt6-multimedia ;;  fedora:qtmm) echo qt6-qtmultimedia-devel ;;
        debian:qtmm) echo qt6-multimedia-dev ;; suse:qtmm) echo qt6-multimedia-devel ;;
        arch:qtshader) echo qt6-shadertools ;; fedora:qtshader) echo qt6-qtshadertools-devel ;;
        debian:qtshader) echo qt6-shadertools-dev ;; suse:qtshader) echo qt6-shadertools-devel ;;
        arch:gst) echo gstreamer ;;        fedora:gst) echo gstreamer1-devel ;;
        debian:gst) echo libgstreamer1.0-dev ;; suse:gst) echo gstreamer-devel ;;
        arch:gstbase) echo gst-plugins-base ;; fedora:gstbase) echo gstreamer1-plugins-base-devel ;;
        debian:gstbase) echo libgstreamer-plugins-base1.0-dev ;; suse:gstbase) echo gstreamer-plugins-base-devel ;;
        arch:sqlite) echo sqlite ;;        fedora:sqlite) echo sqlite-devel ;;
        debian:sqlite) echo libsqlite3-dev ;; suse:sqlite) echo sqlite3-devel ;;
        arch:gl) echo libglvnd ;;          fedora:gl) echo mesa-libGL-devel ;;
        debian:gl) echo libgl1-mesa-dev ;; suse:gl) echo Mesa-libGL-devel ;;
        arch:pactl) echo pipewire-pulse ;; fedora:pactl) echo pipewire-pulseaudio pulseaudio-utils ;;
        debian:pactl) echo pipewire-pulse pulseaudio-utils ;; suse:pactl) echo pipewire-pulseaudio pulseaudio-utils ;;
        arch:qml6glsink) echo gst-plugin-qml6 ;; fedora:qml6glsink) echo gstreamer1-plugins-good-qt6 ;;
        debian:qml6glsink) echo gstreamer1.0-qt6 ;; suse:qml6glsink) echo gstreamer-plugins-good-qtqml6 ;;
    esac
}

install_cmd() {
    case "$1" in
        arch)   echo "sudo pacman -S --needed" ;;
        fedora) echo "sudo dnf install" ;;
        debian) echo "sudo apt install" ;;
        suse)   echo "sudo zypper install" ;;
    esac
}

# Oldest Qt the QML (QtQuick.Controls / Qt.labs / Multimedia APIs) loads on.
# Ubuntu 24.04 (6.4) and Debian 12 (6.4) build fine and then fail at QML load
# with "MixerPage is not a type", so reject them up front.
MIN_QT="6.7"

check_deps() {
    local family pkgs=() hints=() need_pc=0
    family="$(detect_family)"

    need() { # need <key>…  — queue the packages for a missing prerequisite
        local k p
        for k in "$@"; do
            for p in $(pkg_for "$family" "$k"); do
                case " ${pkgs[*]:-} " in *" $p "*) ;; *) pkgs+=("$p") ;; esac
            done
        done
    }

    # Rust is installed with rustup on every distro (distro rustc is often too
    # old for the 2021+ edition features in use).
    command -v cargo >/dev/null 2>&1 \
        || hints+=("cargo — install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh")

    command -v c++ >/dev/null 2>&1 || command -v g++ >/dev/null 2>&1 || command -v clang++ >/dev/null 2>&1 \
        && command -v make >/dev/null 2>&1 || need cxx
    command -v cmake >/dev/null 2>&1 || need cmake
    # bindgen (via cxx-qt) loads libclang at build time.
    command -v clang >/dev/null 2>&1 || ls /usr/lib*/libclang*.so* /usr/lib/*/libclang*.so* /usr/lib/llvm*/lib/libclang*.so* >/dev/null 2>&1 \
        || need clang

    if command -v pkg-config >/dev/null 2>&1; then
        need_pc=1
    else
        need pkgconf
    fi

    # pc_has <module> <package-key>: queue the package if pkg-config can't find
    # the module (or if pkg-config itself is missing, so can't tell).
    pc_has() {
        if [ "$need_pc" = 1 ] && pkg-config --exists "$1" 2>/dev/null; then return 0; fi
        need "$2"
    }
    pc_has Qt6Core        qtbase
    pc_has Qt6Qml         qtdecl
    pc_has Qt6Quick       qtdecl
    pc_has Qt6Multimedia  qtmm
    pc_has Qt6ShaderTools qtshader
    pc_has gstreamer-1.0      gst
    pc_has gstreamer-base-1.0 gstbase
    pc_has sqlite3        sqlite
    pc_has gl             gl

    command -v pactl >/dev/null 2>&1 || need pactl

    # Qt version gate — only decidable when pkg-config can see Qt6Core.
    local qt_ver="" qt_old=0
    if [ "$need_pc" = 1 ] && qt_ver="$(pkg-config --modversion Qt6Core 2>/dev/null)" && [ -n "$qt_ver" ]; then
        if [ "$(printf '%s\n%s\n' "$MIN_QT" "$qt_ver" | sort -V | head -n1)" != "$MIN_QT" ]; then
            qt_old=1
        fi
    fi

    if [ ${#hints[@]} -eq 0 ] && [ ${#pkgs[@]} -eq 0 ] && [ "$qt_old" = 0 ]; then
        check_runtime "$family"
        return 0
    fi

    if [ ${#hints[@]} -gt 0 ] || [ ${#pkgs[@]} -gt 0 ]; then
        loge "Missing build prerequisites (detected distro family: ${family}):"
        local h
        for h in "${hints[@]:-}"; do [ -n "$h" ] && echo -e "   ${RED}•${RESET} $h"; done
        if [ ${#pkgs[@]} -gt 0 ]; then
            if [ "$family" = unknown ]; then
                echo -e "   ${RED}•${RESET} unrecognised distro — install: a C++ compiler, make, cmake, clang/libclang,"
                echo -e "     pkg-config, Qt ${MIN_QT}+ (base, declarative, multimedia, shadertools) dev packages,"
                echo -e "     GStreamer + gst-plugins-base dev packages, SQLite dev, OpenGL dev, pactl (PipeWire)"
            else
                echo ""
                log "Install everything missing with:"
                echo -e "   ${CYAN}$(install_cmd "$family") ${pkgs[*]}${RESET}"
            fi
        fi
    fi

    if [ "$qt_old" = 1 ]; then
        echo ""
        loge "Qt ${qt_ver} is too old: OpenGG needs Qt ${MIN_QT} or newer."
        echo -e "   It would compile but fail when loading the UI (\"MixerPage is not a type\")."
        echo -e "   Supported: Arch/CachyOS, Fedora, Debian 13+, Ubuntu 26.04+, openSUSE Tumbleweed."
        echo -e "   Ubuntu 24.04 / Debian 12 ship Qt 6.4 — upgrade the distro or use another install channel."
    fi
    trap - EXIT   # nothing was started; skip the "shutting down" cleanup
    exit 1
}

# Runtime packages per family, beyond what the -dev packages pull in. On
# Debian/Ubuntu every QML module is its own package and a missing one stops
# the main window loading ("module ... is not installed"); openSUSE splits
# QtMultimedia's QML imports out the same way. Verified in clean containers.
runtime_pkgs() {
    case "$1" in
        arch)   echo gst-plugin-qml6 gst-plugins-good gst-plugins-bad pipewire-pulse ;;
        fedora) echo gstreamer1-plugins-good-qt6 gstreamer1-plugins-good gstreamer1-plugins-bad-free \
                     gstreamer1-plugin-libav pipewire-pulseaudio qt6-qtwayland ;;
        debian) echo qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts \
                     qml6-module-qtquick-window qml6-module-qtquick-templates qml6-module-qtquick-dialogs \
                     qml6-module-qtquick-shapes qml6-module-qtmultimedia qml6-module-qt-labs-platform \
                     qml6-module-qtqml-workerscript gstreamer1.0-qt6 gstreamer1.0-plugins-good \
                     gstreamer1.0-plugins-bad gstreamer1.0-libav qt6-wayland pipewire-pulse ;;
        suse)   echo qt6-multimedia-imports gstreamer-plugins-good-qtqml6 gstreamer-plugins-good \
                     gstreamer-plugins-bad gstreamer-plugins-libav pipewire-pulseaudio ;;
    esac
}

pkg_installed() {
    case "$1" in
        arch)        pacman -Q "$2" >/dev/null 2>&1 ;;
        debian)      dpkg-query -W -f='${Status}' "$2" 2>/dev/null | grep -q "ok installed" ;;
        fedora|suse) rpm -q --whatprovides "$2" >/dev/null 2>&1 ;;
        *)           return 0 ;;
    esac
}

# Runtime-only pieces: not needed to compile, but without them the app
# either fails to open its window (QML modules) or loses clip video/audio.
# Warnings only — never fails the check, since the build itself is fine.
check_runtime() {
    local family="$1" rt=() p
    for p in $(runtime_pkgs "$family"); do
        pkg_installed "$family" "$p" || rt+=("$p")
    done
    command -v ffmpeg >/dev/null 2>&1 || rt+=(ffmpeg)
    if [ ${#rt[@]} -gt 0 ] && [ "$family" != unknown ]; then
        logw "Runtime packages not installed (the window, clip video or audio need them):"
        echo -e "       ${CYAN}$(install_cmd "$family") ${rt[*]}${RESET}"
    fi
    if command -v pactl >/dev/null 2>&1 && ! pactl info >/dev/null 2>&1; then
        logw "pactl can't reach an audio server — the mixer needs PipeWire's pulse server running:"
        echo -e "       ${CYAN}systemctl --user enable --now pipewire.socket pipewire-pulse.socket wireplumber.service${RESET}"
    fi
}

# ── Setup ────────────────────────────────────────────────────────
# ── Group membership ─────────────────────────────────────────────
# Global hotkeys read /dev/input/eventN directly (see
# daemon/src/replay/hotkey.rs) — the only unprivileged way to see a keypress
# while another window has focus, on X11 and Wayland alike. That needs the
# `input` group.
#
# This is the one privileged step in the project and it runs exactly once,
# here. Nothing at runtime uses sudo: the daemon stays unprivileged and gets
# everything it needs from group membership.
ensure_input_group() {
    if ! getent group input >/dev/null 2>&1; then
        logw "No 'input' group on this system — global hotkeys will not work"
        return 0
    fi

    if id -nG "$USER" | tr ' ' '\n' | grep -qx input; then
        logs "Already in the 'input' group"
        return 0
    fi

    if ! command -v sudo >/dev/null 2>&1; then
        logw "Not in the 'input' group and sudo is unavailable."
        logw "Global hotkeys need it — add yourself with:"
        echo -e "       ${CYAN}usermod -aG input $USER${RESET}  (as root)"
        return 0
    fi

    log "Adding ${BOLD}$USER${RESET} to the 'input' group (needed for global hotkeys)."
    log "${DIM}This is the only step that asks for your password.${RESET}"
    # Never abort setup over this: the rest of the project works without
    # hotkeys, and the user may reasonably decline the prompt.
    if sudo usermod -aG input "$USER"; then
        logs "Added to the 'input' group"
        logw "Log out and back in for it to take effect (a new terminal is not enough)."
        logw "To test without logging out: ${CYAN}newgrp input${RESET}, then run the daemon from that shell."
    else
        logw "Could not add you to the 'input' group — global hotkeys will not fire."
        logw "Run it yourself later with:"
        echo -e "       ${CYAN}sudo usermod -aG input $USER${RESET}"
    fi
}

# cargo check one crate; on failure show the tail of cargo's output instead of
# swallowing it (a missing system library surfaces here as a build-script error).
check_crate() {
    local name="$1" dir="$2" out
    if out="$(cd "$dir" && cargo check --quiet 2>&1)"; then
        logs "$name compiles OK"
    else
        loge "$name does not compile — last lines of 'cargo check':"
        echo "$out" | tail -n 25 | sed 's/^/       /'
        return 1
    fi
}

do_setup() {
    log "${BOLD}Running first-time setup...${RESET}"
    echo ""

    # Check Rust toolchain
    log "Checking Rust toolchain..."
    local failed=0
    check_crate "Daemon" "$DAEMON_DIR" || failed=1
    check_crate "Qt6 frontend" "$QT_SHELL_DIR" || failed=1

    # Create data dirs
    mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/opengg"
    mkdir -p "${XDG_DATA_HOME:-$HOME/.local/share}/opengg/thumbnails"
    mkdir -p "${XDG_DATA_HOME:-$HOME/.local/share}/opengg/waveforms"
    mkdir -p "${XDG_DATA_HOME:-$HOME/.local/share}/opengg/extensions"
    mkdir -p "$HOME/Videos/OpenGG"
    logs "Data directories created"

    # Group membership for global hotkeys (may prompt for a password).
    ensure_input_group

    # Example extensions are NOT installed any more. Setup used to copy every
    # folder in packaging/extensions/ into the user's own extensions
    # directory, so each one (Sunshine, …) showed up as if the user had
    # installed it. Extensions are something a user adds; `./dev.sh examples`
    # installs these on request. Copies an earlier setup left behind are
    # removed only while they are byte-identical to the bundled example, so
    # anything a user has edited stays put.
    remove_untouched_examples
    # Generate opengg.desktop from template
    if [ -f "$ROOT_DIR/opengg.desktop.template" ]; then
        sed "s|OPENGG_DIR|$ROOT_DIR|g" "$ROOT_DIR/opengg.desktop.template" > "$ROOT_DIR/opengg.desktop"
        chmod +x "$ROOT_DIR/opengg-launch.sh"
        logs "opengg.desktop generated"
    fi

    echo ""
    if [ "$failed" = 1 ]; then
        loge "Setup finished, but the build check failed (output above). Fix that, then re-run ${CYAN}./dev.sh setup${RESET}."
        return 1
    fi
    logs "${BOLD}Setup complete!${RESET} Run ${CYAN}./dev.sh${RESET} to start developing."
}

# ── Run Daemon ───────────────────────────────────────────────────
run_daemon() {
    log "Building daemon..."
    cd "$DAEMON_DIR"

    # Build in debug mode (faster compilation)
    # Run it in the foreground inside `if !`: pipefail then reports cargo's
    # status. Backgrounding the pipeline and `wait $!` returned sed's status
    # (the last stage), so a failed build was reported as success and the
    # stale target/debug/openggd from the previous build was started.
    if ! cargo build 2>&1 | sed -u "s/^/$(echo -e "${DAEMON_PREFIX} ")/"; then
        loge "Daemon build failed"
        return 1
    fi
    logs "Daemon built"

    log "Starting daemon (${DIM}RUST_LOG=info${RESET})..."
    RUST_LOG="${RUST_LOG:-info}" "$DAEMON_DIR/target/debug/openggd" 2>&1 | \
        sed -u "s/^/$(echo -e "${DAEMON_PREFIX} ")/" &
    PIDS+=($!)
    logs "Daemon running (PID ${PIDS[-1]})"
}

# ── Run Frontend (Qt6/QML) ──────────────────────────────────────
run_frontend() {
    log "Building Qt6/QML frontend..."
    cd "$QT_SHELL_DIR"

    # Build in debug mode (faster compilation)
    if ! cargo build 2>&1 | sed -u "s/^/$(echo -e "${DEV_PREFIX} [qt-shell] ")/"; then
        loge "Qt6 frontend build failed"
        return 1
    fi
    logs "Qt6 frontend built"

    log "Starting Qt6/QML frontend..."
    QT_FORCE_STDERR_LOGGING=1 "$QT_SHELL_DIR/target/debug/opengg-qt" 2>&1 | \
        sed -u "s/^/$(echo -e "[qt-shell] ")/" &
    PIDS+=($!)
    logs "Qt6 frontend running (PID ${PIDS[-1]})"
}

# ── Build Release ────────────────────────────────────────────────
do_build() {
    log "${BOLD}Building release...${RESET}"
    echo ""

    log "Building daemon (release)..."
    cd "$DAEMON_DIR"
    cargo build --release 2>&1 | sed "s/^/$(echo -e "${DAEMON_PREFIX} ")/"
    logs "Daemon: $DAEMON_DIR/target/release/openggd"

    log "Building Qt6/QML frontend (release)..."
    cd "$QT_SHELL_DIR"
    cargo build --release 2>&1 | sed "s/^/$(echo -e "${DEV_PREFIX} [qt-shell] ")/"
    logs "Qt6 frontend: $QT_SHELL_DIR/target/release/opengg-qt"

    echo ""
    logs "${BOLD}Release build complete!${RESET}"
}

# ── Main ─────────────────────────────────────────────────────────

echo ""
echo -e "${BOLD}${RED}  ╔═══════════════════════════════╗${RESET}"
echo -e "${BOLD}${RED}  ║${RESET}  ${BOLD}OpenGG${RESET} ${DIM}Development Server${RESET}   ${BOLD}${RED}║${RESET}"
echo -e "${BOLD}${RED}  ╚═══════════════════════════════╝${RESET}"
echo ""

check_deps

# ── Example extensions ───────────────────────────────────────────
EXT_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/opengg/extensions"

remove_untouched_examples() {
    [ -d "$ROOT_DIR/packaging/extensions" ] || return 0
    for ext in "$ROOT_DIR/packaging/extensions"/*/; do
        [ -d "$ext" ] || continue
        name=$(basename "$ext")
        if [ -d "$EXT_DIR/$name" ] && diff -rq "$ext" "$EXT_DIR/$name" >/dev/null 2>&1; then
            rm -rf "${EXT_DIR:?}/$name"
            logs "Removed example extension installed by an earlier setup: $name"
        fi
    done
}

install_examples() {
    mkdir -p "$EXT_DIR"
    for ext in "$ROOT_DIR/packaging/extensions"/*/; do
        [ -d "$ext" ] || continue
        name=$(basename "$ext")
        if [ -d "$EXT_DIR/$name" ]; then
            logw "Already installed, left alone: $name"
            continue
        fi
        cp -r "$ext" "$EXT_DIR/$name"
        # Keep daemon executables executable after the copy.
        find "$EXT_DIR/$name/bin" -type f -exec chmod +x {} \; 2>/dev/null || true
        logs "Installed example extension: $name"
    done
}

case "${1:-all}" in
    setup)
        do_setup
        ;;
    deps)
        trap - EXIT   # nothing was started; skip the "shutting down" cleanup
        logs "All build prerequisites found"
        ;;
    examples)
        install_examples
        ;;
    daemon|d)
        run_daemon
        log "Press ${BOLD}Ctrl+C${RESET} to stop"
        wait
        ;;
    ui|frontend|f)
        run_frontend
        log "Press ${BOLD}Ctrl+C${RESET} to stop"
        wait
        ;;
    build|release)
        do_build
        ;;
    all|"")
        # Run daemon first, give it a moment, then start frontend
        run_daemon
        sleep 2
        run_frontend

        echo ""
        log "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        log " ${BOLD}Both services running${RESET}"
        log " Daemon: ${DIM}D-Bus session bus${RESET}"
        log " Qt6:    ${DIM}Qt6/QML frontend (native)${RESET}"
        log " Press ${BOLD}Ctrl+C${RESET} to stop everything"
        log "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo ""

        # Wait for any child to exit
        wait
        ;;
    *)
        echo "Usage: ./dev.sh [command]"
        echo ""
        echo "Commands:"
        echo "  (none)          Run daemon + frontend (full stack)"
        echo "  daemon          Run daemon only"
        echo "  ui              Run Qt6/QML frontend only"
        echo "  build           Build everything for release"
        echo "  setup           First-time setup"
        echo "  deps            Check build prerequisites (prints the install command)"
        echo "  examples        Install the bundled example extensions"
        echo ""
        ;;
esac
