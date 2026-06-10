#!/usr/bin/env bash
# OpenGG — Extension Scaffold
#
# Creates a new extension directory under ~/.local/share/opengg/extensions/
# from the extension-template. Works for UI-only, daemon-only, or both.
#
# Usage:
#   ./packaging/create-extension.sh
#   ./packaging/create-extension.sh --id my-ext --name "My Ext" --type daemon

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TEMPLATE_DIR="$ROOT_DIR/extension-template"
EXT_BASE="${XDG_DATA_HOME:-$HOME/.local/share}/opengg/extensions"

# ── Colors ───────────────────────────────────────────────────────
GREEN='\033[0;32m'; CYAN='\033[0;36m'; YELLOW='\033[0;33m'; BOLD='\033[1m'; RESET='\033[0m'
ok()  { echo -e "${GREEN}✓${RESET} $*"; }
ask() { echo -e "${CYAN}?${RESET}  $*"; }
tip() { echo -e "${YELLOW}→${RESET} $*"; }

# ── Parse flags ──────────────────────────────────────────────────
EXT_ID=""; EXT_NAME=""; EXT_TYPE=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --id)   EXT_ID="$2";   shift 2 ;;
        --name) EXT_NAME="$2"; shift 2 ;;
        --type) EXT_TYPE="$2"; shift 2 ;;
        *) shift ;;
    esac
done

# ── Interactive prompts ───────────────────────────────────────────
echo ""
echo -e "${BOLD}OpenGG Extension Scaffold${RESET}"
echo ""

if [[ -z $EXT_ID ]]; then
    ask "Extension ID (kebab-case, e.g. my-audio-tool):"
    read -r EXT_ID
fi
EXT_ID="${EXT_ID// /-}"
EXT_ID="${EXT_ID,,}"

if [[ -z $EXT_NAME ]]; then
    ask "Display name (e.g. My Audio Tool):"
    read -r EXT_NAME
fi

ask "Short description (one sentence):"
read -r EXT_DESC

ask "Author name:"
read -r EXT_AUTHOR

if [[ -z $EXT_TYPE ]]; then
    echo ""
    echo "  Extension type:"
    echo "  1) UI only      — settings panel in OpenGG, uses window.opengg.invoke()"
    echo "  2) Daemon only  — background script/binary (PipeWire, D-Bus, anything)"
    echo "  3) Both         — UI panel + background process"
    ask "Choose [1/2/3]:"
    read -r choice
    case "$choice" in
        2) EXT_TYPE="daemon"   ;;
        3) EXT_TYPE="both"     ;;
        *) EXT_TYPE="ui"       ;;
    esac
fi

EXT_DIR="$EXT_BASE/$EXT_ID"
if [[ -d $EXT_DIR ]]; then
    echo -e "${YELLOW}⚠${RESET}  $EXT_DIR already exists — aborting to avoid overwriting."
    exit 1
fi

mkdir -p "$EXT_BASE"

# ── Scaffold ──────────────────────────────────────────────────────
echo ""

case "$EXT_TYPE" in
    daemon)
        # Daemon-only: copy sunshine as a template, strip its PipeWire logic
        mkdir -p "$EXT_DIR/bin" "$EXT_DIR/assets"

        # Minimal daemon template script
        DAEMON_FILE="$EXT_DIR/bin/$EXT_ID"
        cat > "$DAEMON_FILE" <<'SCRIPT'
#!/usr/bin/env bash
# OpenGG daemon extension — edit this script to add your background logic.
#
# This process is supervised by openggd:
#   • Crash / non-zero exit → restarts with exponential backoff (2s → 4s → … → 60s)
#   • Exit code 0           → clean finish, no restart
#   • SIGTERM               → graceful shutdown request
#
# You have full access to: pactl, pw-link, dbus-send, pw-cli, etc.
# For PipeWire events, subscribe via:  pactl subscribe
# For D-Bus calls, use:               dbus-send --session …

log() { printf '[extension] %s\n' "$*" >&2; }

log "Started"
trap 'log "Stopping"; exit 0' SIGTERM

# ── Your logic here ───────────────────────────────────────────────
# Example: watch for PipeWire events
# while IFS= read -r event; do
#   log "PipeWire event: $event"
# done < <(exec pactl subscribe 2>/dev/null)

# Exit 0 to stop without restart, or loop forever to keep running.
wait
SCRIPT
        chmod +x "$DAEMON_FILE"

        # Copy icon from sunshine or generate a placeholder
        cp "$ROOT_DIR/packaging/extensions/sunshine/assets/icon.svg" \
           "$EXT_DIR/assets/icon.svg" 2>/dev/null || \
           echo '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#555"/></svg>' \
           > "$EXT_DIR/assets/icon.svg"

        cat > "$EXT_DIR/manifest.json" <<JSON
{
  "id":          "$EXT_ID",
  "name":        "$EXT_NAME",
  "description": "$EXT_DESC",
  "version":     "0.1.0",
  "author":      "$EXT_AUTHOR",
  "icon":        "assets/icon.svg",
  "daemon":      "bin/$EXT_ID"
}
JSON
        ok "Daemon extension created at $EXT_DIR"
        echo ""
        tip "Edit $EXT_DIR/bin/$EXT_ID to add your logic"
        tip "Open OpenGG → Settings → Extensions and enable $EXT_NAME"
        ;;

    both|ui)
        # UI (or both): copy extension-template, patch names, optionally add daemon stub
        cp -r "$TEMPLATE_DIR/." "$EXT_DIR/"
        # Remove build artefacts
        rm -rf "$EXT_DIR/node_modules" "$EXT_DIR/dist" \
               "$EXT_DIR/package-lock.json" 2>/dev/null || true

        # Patch manifest.json
        GLOBAL_KEY="${EXT_ID//-/_}"
        if [[ $EXT_TYPE == "both" ]]; then
            mkdir -p "$EXT_DIR/bin"
            DAEMON_STUB="$EXT_DIR/bin/$EXT_ID"
            cat > "$DAEMON_STUB" <<'SCRIPT'
#!/usr/bin/env bash
log() { printf '[extension] %s\n' "$*" >&2; }
log "Started"
trap 'log "Stopping"; exit 0' SIGTERM
# Add your background logic here
wait
SCRIPT
            chmod +x "$DAEMON_STUB"
            cat > "$EXT_DIR/manifest.json" <<JSON
{
  "id":          "$EXT_ID",
  "name":        "$EXT_NAME",
  "description": "$EXT_DESC",
  "version":     "0.1.0",
  "author":      "$EXT_AUTHOR",
  "icon":        "assets/icon.svg",
  "daemon":      "bin/$EXT_ID",
  "main":        "dist/index.iife.js",
  "hasSettings": true
}
JSON
        else
            cat > "$EXT_DIR/manifest.json" <<JSON
{
  "id":          "$EXT_ID",
  "name":        "$EXT_NAME",
  "description": "$EXT_DESC",
  "version":     "0.1.0",
  "author":      "$EXT_AUTHOR",
  "icon":        "assets/icon.svg",
  "main":        "dist/index.iife.js",
  "hasSettings": true
}
JSON
        fi

        # Patch the global registration key in src/index.ts
        sed -i "s/__ext_my_extension/__ext_${GLOBAL_KEY}/g" "$EXT_DIR/src/index.ts" 2>/dev/null || true
        sed -i "s/my-extension/$EXT_ID/g"  "$EXT_DIR/package.json" 2>/dev/null || true

        ok "UI extension created at $EXT_DIR"
        echo ""
        tip "cd $EXT_DIR && npm install && npm run build"
        tip "Open OpenGG → Settings → Extensions → Refresh"
        ;;
esac

echo ""
tip "For AI-assisted extension creation: see $TEMPLATE_DIR/PROMPT.md"
echo ""
