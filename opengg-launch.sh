#!/usr/bin/env bash
# OpenGG launcher — runs release binary if built, otherwise opens dev mode in Konsole

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_BIN="$SCRIPT_DIR/frontend/src-tauri/target/release/opengg"
LOG_FILE="$HOME/.local/share/opengg/opengg.log"

if [ -f "$RELEASE_BIN" ]; then
    # Single-instance guard
    if pgrep -x "opengg" > /dev/null 2>&1; then
        exit 0
    fi
    mkdir -p "$(dirname "$LOG_FILE")"
    setsid "$RELEASE_BIN" >> "$LOG_FILE" 2>&1 &
    disown
    exit 0
else
    exec konsole --noclose -e bash -c "cd '$SCRIPT_DIR' && ./dev.sh; exec bash"
fi
