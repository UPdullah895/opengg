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
    exit 0
}

trap cleanup SIGINT SIGTERM EXIT

# ── Preflight Checks ────────────────────────────────────────────
check_deps() {
    local missing=()

    command -v cargo   >/dev/null 2>&1 || missing+=("cargo (install rustup)")
    command -v pactl   >/dev/null 2>&1 || missing+=("pactl (install pipewire-pulse)")
    # node/npm are NOT required to build or run OpenGG itself — only for
    # scaffolding third-party extensions via `make new-extension` (see
    # scripts/new-extension.sh), which degrades gracefully without them.

    if [ ${#missing[@]} -gt 0 ]; then
        loge "Missing required tools:"
        for dep in "${missing[@]}"; do
            echo -e "   ${RED}•${RESET} $dep"
        done
        exit 1
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

do_setup() {
    log "${BOLD}Running first-time setup...${RESET}"
    echo ""

    # Check Rust toolchain
    log "Checking Rust toolchain..."
    cd "$DAEMON_DIR"
    cargo check --quiet 2>/dev/null && logs "Daemon compiles OK" || logw "Daemon has compile issues — run 'cargo check' in daemon/"

    cd "$QT_SHELL_DIR"
    cargo check --quiet 2>/dev/null && logs "Qt6 frontend compiles OK" || logw "Qt6 frontend has compile issues — run 'cargo check' in qt-shell/"

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
    logs "${BOLD}Setup complete!${RESET} Run ${CYAN}./dev.sh${RESET} to start developing."
}

# ── Run Daemon ───────────────────────────────────────────────────
run_daemon() {
    log "Building daemon..."
    cd "$DAEMON_DIR"

    # Build in debug mode (faster compilation)
    cargo build 2>&1 | sed "s/^/$(echo -e "${DAEMON_PREFIX} ")/" &
    wait $!

    if [ $? -ne 0 ]; then
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
        echo "  examples        Install the bundled example extensions"
        echo ""
        ;;
esac
