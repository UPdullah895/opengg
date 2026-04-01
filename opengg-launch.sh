#!/usr/bin/env bash
# OpenGG launcher — runs release binary if built, otherwise opens dev mode in Konsole

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_BIN="$SCRIPT_DIR/frontend/src-tauri/target/release/opengg"

if [ -f "$RELEASE_BIN" ]; then
    exec "$RELEASE_BIN"
else
    exec konsole --noclose -e bash -c "cd '$SCRIPT_DIR' && ./dev.sh; exec bash"
fi
