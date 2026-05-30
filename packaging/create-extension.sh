#!/usr/bin/env bash
# create-extension.sh — scaffold a new OpenGG extension
set -euo pipefail

EXT_BASE="${XDG_DATA_HOME:-$HOME/.local/share}/opengg/extensions"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEMPLATE_DIR="$SCRIPT_DIR/../extension-template"
SUNSHINE_DAEMON="$SCRIPT_DIR/extensions/sunshine/daemon"

echo "=== OpenGG Extension Scaffold ==="
echo

# ── Gather inputs ──────────────────────────────────────────────────────────────

read -rp "Extension name (display name): " EXT_NAME
if [[ -z "$EXT_NAME" ]]; then
    echo "Error: name is required." >&2
    exit 1
fi

# Default id: lowercase + replace spaces with hyphens
DEFAULT_ID="$(echo "$EXT_NAME" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]/-/g' | sed 's/--*/-/g' | sed 's/^-//;s/-$//')"
read -rp "Extension id [$DEFAULT_ID]: " EXT_ID
EXT_ID="${EXT_ID:-$DEFAULT_ID}"

read -rp "Description (one line): " EXT_DESC

echo
echo "Extension type:"
echo "  1) daemon   — background executable only (e.g. PipeWire automation)"
echo "  2) frontend — IIFE JavaScript bundle only (e.g. UI panel)"
echo "  3) both     — daemon + frontend bundle"
read -rp "Type [1/2/3]: " TYPE_CHOICE

case "$TYPE_CHOICE" in
    1) EXT_TYPE="daemon" ;;
    2) EXT_TYPE="frontend" ;;
    3) EXT_TYPE="both" ;;
    *) echo "Error: choose 1, 2, or 3." >&2; exit 1 ;;
esac

# ── Create directory ───────────────────────────────────────────────────────────

TARGET="$EXT_BASE/$EXT_ID"
if [[ -e "$TARGET" ]]; then
    echo "Error: $TARGET already exists." >&2
    exit 1
fi
mkdir -p "$TARGET"

# ── Build manifest ─────────────────────────────────────────────────────────────

MANIFEST="$TARGET/manifest.json"
{
    echo "{"
    echo "  \"id\":          \"$EXT_ID\","
    echo "  \"name\":        \"$EXT_NAME\","
    echo "  \"description\": \"$EXT_DESC\","
    echo "  \"version\":     \"0.1.0\""
    if [[ "$EXT_TYPE" == "daemon" || "$EXT_TYPE" == "both" ]]; then
        sed -i '$ s/$/,/' "$MANIFEST" 2>/dev/null || true
        echo "  \"daemon\":      \"daemon\""
    fi
    if [[ "$EXT_TYPE" == "frontend" || "$EXT_TYPE" == "both" ]]; then
        sed -i '$ s/$/,/' "$MANIFEST" 2>/dev/null || true
        echo "  \"main\":        \"dist/index.iife.js\","
        echo "  \"hasSettings\": false"
    fi
    echo "}"
} > "$MANIFEST"

# Rewrite manifest properly (avoid trailing comma issues with the above)
if [[ "$EXT_TYPE" == "daemon" ]]; then
    cat > "$MANIFEST" <<MANIFEST_EOF
{
  "id":          "$EXT_ID",
  "name":        "$EXT_NAME",
  "description": "$EXT_DESC",
  "version":     "0.1.0",
  "daemon":      "daemon"
}
MANIFEST_EOF
elif [[ "$EXT_TYPE" == "frontend" ]]; then
    cat > "$MANIFEST" <<MANIFEST_EOF
{
  "id":          "$EXT_ID",
  "name":        "$EXT_NAME",
  "description": "$EXT_DESC",
  "version":     "0.1.0",
  "main":        "dist/index.iife.js",
  "hasSettings": false
}
MANIFEST_EOF
else
    cat > "$MANIFEST" <<MANIFEST_EOF
{
  "id":          "$EXT_ID",
  "name":        "$EXT_NAME",
  "description": "$EXT_DESC",
  "version":     "0.1.0",
  "daemon":      "daemon",
  "main":        "dist/index.iife.js",
  "hasSettings": false
}
MANIFEST_EOF
fi

# ── Daemon scaffold ────────────────────────────────────────────────────────────

if [[ "$EXT_TYPE" == "daemon" || "$EXT_TYPE" == "both" ]]; then
    if [[ -f "$SUNSHINE_DAEMON" ]]; then
        cp "$SUNSHINE_DAEMON" "$TARGET/daemon"
        # Replace sunshine-specific comments with generic ones
        sed -i "1,5s/.*//" "$TARGET/daemon"
        {
            echo "#!/usr/bin/env bash"
            echo "# $EXT_NAME — OpenGG daemon extension"
            echo "# Replace this template with your extension logic."
            echo "# This script is supervised by openggd and restarted if it exits."
            echo ""
            tail -n +6 "$SUNSHINE_DAEMON"
        } > "$TARGET/daemon.tmp" && mv "$TARGET/daemon.tmp" "$TARGET/daemon"
    else
        cat > "$TARGET/daemon" <<'DAEMON_EOF'
#!/usr/bin/env bash
# OpenGG daemon extension template
# Replace this with your extension logic.
# openggd supervises this script and restarts it automatically if it exits.

set -euo pipefail

log() { echo "[extension] $*"; }

log "Starting…"

while true; do
    # TODO: your extension logic here
    # Example: react to PipeWire events, call D-Bus, trigger RGB effects…
    sleep 10
done
DAEMON_EOF
    fi
    chmod +x "$TARGET/daemon"
fi

# ── Frontend scaffold ──────────────────────────────────────────────────────────

if [[ "$EXT_TYPE" == "frontend" || "$EXT_TYPE" == "both" ]]; then
    if [[ -d "$TEMPLATE_DIR" ]]; then
        cp -r "$TEMPLATE_DIR/." "$TARGET/"
        # Update the copied manifest with the user's values
        cat > "$TARGET/manifest.json" <<MANIFEST_EOF
{
  "id":          "$EXT_ID",
  "name":        "$EXT_NAME",
  "description": "$EXT_DESC",
  "version":     "0.1.0",
  "main":        "dist/index.iife.js",
  "hasSettings": true
}
MANIFEST_EOF
    else
        mkdir -p "$TARGET/src" "$TARGET/locales"
        cat > "$TARGET/locales/en.json" <<'LOCALE_EOF'
{
  "settingsTitle": "Extension Settings"
}
LOCALE_EOF
        cat > "$TARGET/src/index.ts" <<'TS_EOF'
import { defineComponent, h } from 'vue'

const SettingsPanel = defineComponent({
  name: 'ExtSettings',
  setup() {
    return () => h('div', { style: 'padding:16px' }, 'Extension loaded!')
  },
})

// Replace MY_EXT with your extension id (dashes → underscores)
;(window as any).__ext_MY_EXT = { settingsComponent: SettingsPanel }
TS_EOF
    fi
fi

# ── Done ───────────────────────────────────────────────────────────────────────

echo
echo "Created: $TARGET"
echo
echo "Next steps:"
if [[ "$EXT_TYPE" == "daemon" || "$EXT_TYPE" == "both" ]]; then
    echo "  • Edit $TARGET/daemon with your extension logic"
    echo "  • Restart openggd to load the extension (it will supervise the daemon)"
fi
if [[ "$EXT_TYPE" == "frontend" || "$EXT_TYPE" == "both" ]]; then
    echo "  • cd $TARGET && npm install && npm run build"
    echo "  • Open Settings → Extensions in OpenGG to enable it"
fi
echo
echo "Open Settings → Extensions to see your new extension."
