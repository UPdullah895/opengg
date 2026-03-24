#!/usr/bin/env bash
#
# OpenGG — Unified Development Script
#
# Usage:
#   ./dev.sh          Run daemon + frontend (full stack)
#   ./dev.sh daemon   Run daemon only
#   ./dev.sh ui       Run frontend only
#   ./dev.sh build    Build everything for release
#   ./dev.sh setup    First-time setup (install deps, create dirs)
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
DAEMON_DIR="$ROOT_DIR/daemon"
FRONTEND_DIR="$ROOT_DIR/frontend"

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
TAURI_PREFIX="${BLUE}[tauri]${RESET}"
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
    command -v node    >/dev/null 2>&1 || missing+=("node (install nodejs)")
    command -v npm     >/dev/null 2>&1 || missing+=("npm (install nodejs)")
    command -v pactl   >/dev/null 2>&1 || missing+=("pactl (install pipewire-pulse)")

    if [ ${#missing[@]} -gt 0 ]; then
        loge "Missing required tools:"
        for dep in "${missing[@]}"; do
            echo -e "   ${RED}•${RESET} $dep"
        done
        exit 1
    fi
}

# ── Setup ────────────────────────────────────────────────────────
do_setup() {
    log "${BOLD}Running first-time setup...${RESET}"
    echo ""

    # Install frontend npm deps
    log "Installing frontend dependencies..."
    cd "$FRONTEND_DIR"
    npm install
    logs "Frontend deps installed"

    # Check Rust toolchain
    log "Checking Rust toolchain..."
    cd "$DAEMON_DIR"
    cargo check --quiet 2>/dev/null && logs "Daemon compiles OK" || logw "Daemon has compile issues — run 'cargo check' in daemon/"

    # Create data dirs
    mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/opengg"
    mkdir -p "${XDG_DATA_HOME:-$HOME/.local/share}/opengg/thumbnails"
    mkdir -p "${XDG_DATA_HOME:-$HOME/.local/share}/opengg/waveforms"
    mkdir -p "$HOME/Videos/OpenGG"
    logs "Data directories created"

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

# ── Run Frontend ─────────────────────────────────────────────────
run_frontend() {
    cd "$FRONTEND_DIR"

    # Ensure node_modules exists
    if [ ! -d "node_modules" ]; then
        log "Installing frontend dependencies (first run)..."
        npm install 2>&1 | tail -3
    fi

    log "Starting Tauri dev server..."
    # Use npx to ensure local binaries are found
    npx tauri dev 2>&1 | sed -u "s/^/$(echo -e "${TAURI_PREFIX} ")/" &
    PIDS+=($!)
    logs "Tauri dev server starting (PID ${PIDS[-1]})"
}

# ── Build Release ────────────────────────────────────────────────
do_build() {
    log "${BOLD}Building release...${RESET}"
    echo ""

    log "Building daemon (release)..."
    cd "$DAEMON_DIR"
    cargo build --release 2>&1 | sed "s/^/$(echo -e "${DAEMON_PREFIX} ")/"
    logs "Daemon: $DAEMON_DIR/target/release/openggd"

    log "Building frontend (release)..."
    cd "$FRONTEND_DIR"
    [ -d "node_modules" ] || npm install
    npx tauri build 2>&1 | sed "s/^/$(echo -e "${TAURI_PREFIX} ")/"
    logs "Frontend built"

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

case "${1:-all}" in
    setup)
        do_setup
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
        log " Daemon: ${DIM}http://localhost:9473 (D-Bus)${RESET}"
        log " Tauri:  ${DIM}http://localhost:1420 (Vite)${RESET}"
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
        echo "  (none)    Run daemon + frontend (full stack)"
        echo "  daemon    Run daemon only"
        echo "  ui        Run frontend only"
        echo "  build     Build everything for release"
        echo "  setup     First-time setup"
        echo ""
        ;;
esac
