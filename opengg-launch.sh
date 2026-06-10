#!/usr/bin/env bash
# OpenGG launcher — runs release binary if built, otherwise opens dev mode in Konsole

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="$HOME/.local/share/opengg/opengg.log"

# Search for a release binary in multiple locations (priority order)
find_release_bin() {
    local candidates=(
        "$SCRIPT_DIR/frontend/src-tauri/target/release/opengg"
        "$HOME/.local/bin/opengg"
        "/usr/bin/opengg"
    )
    for candidate in "${candidates[@]}"; do
        if [ -f "$candidate" ] && [ -x "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

RELEASE_BIN="$(find_release_bin)"

if [ -n "$RELEASE_BIN" ]; then
    # Single-instance guard
    if pgrep -x "opengg" > /dev/null 2>&1; then
        exit 0
    fi
    mkdir -p "$(dirname "$LOG_FILE")"

    BIN_MTIME="$(stat -c '%y' "$RELEASE_BIN" 2>/dev/null || echo 'unknown')"
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] Launching: $RELEASE_BIN (modified $BIN_MTIME)" >> "$LOG_FILE"

    # When launched from autostart, give the session bus a moment to settle
    if [ -n "$OPENGG_AUTOSTART" ]; then
        sleep 1
    fi

    setsid "$RELEASE_BIN" >> "$LOG_FILE" 2>&1 &
    disown
    exit 0
else
    exec konsole --noclose -e bash -c "cd '$SCRIPT_DIR' && ./dev.sh; exec bash"
fi
