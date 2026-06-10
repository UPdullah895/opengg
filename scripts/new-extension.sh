#!/usr/bin/env bash
# new-extension.sh — scaffold a new OpenGG extension from extension-template/
#
# Usage:
#   scripts/new-extension.sh <name> [target-dir]
#   make new-extension NAME=<name>
#
# <name> must be kebab-case (a-z, 0-9, hyphen). The new extension is created in
# the OpenGG extensions directory by default so it appears in Settings →
# Extensions → Refresh immediately. Pass a [target-dir] to create it elsewhere.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE_DIR="$ROOT_DIR/extension-template"

NAME="${1:-}"
EXT_BASE="${2:-${XDG_DATA_HOME:-$HOME/.local/share}/opengg/extensions}"

if [[ -z "$NAME" ]]; then
    echo "error: extension name required" >&2
    echo "usage: scripts/new-extension.sh <name>   (or: make new-extension NAME=<name>)" >&2
    exit 1
fi
if ! [[ "$NAME" =~ ^[a-z][a-z0-9-]*$ ]]; then
    echo "error: '$NAME' must be kebab-case — lowercase letters, digits, hyphens (start with a letter)" >&2
    exit 1
fi
if [[ ! -d "$TEMPLATE_DIR" ]]; then
    echo "error: template not found at $TEMPLATE_DIR" >&2
    exit 1
fi

TARGET="$EXT_BASE/$NAME"
if [[ -e "$TARGET" ]]; then
    echo "error: $TARGET already exists — choose another name or remove it first" >&2
    exit 1
fi

# Derived identifiers
UNDERSCORED="${NAME//-/_}"                       # global key suffix: window.__ext_<UNDERSCORED>
# Title Case display name: "my-cool-ext" → "My Cool Ext"
DISPLAY="$(echo "$NAME" | tr '-' ' ' | awk '{ for (i=1;i<=NF;i++) $i=toupper(substr($i,1,1)) substr($i,2) } 1')"

echo "→ Creating extension '$NAME' at $TARGET"
mkdir -p "$EXT_BASE"
cp -r "$TEMPLATE_DIR" "$TARGET"
# Drop template-only cruft that shouldn't ship in a new extension
rm -rf "$TARGET/node_modules" "$TARGET/AGENTS.md" "$TARGET/PROMPT.md"

# manifest.json — set id + name
sed -i \
    -e "s/\"id\":\([[:space:]]*\)\"my-extension\"/\"id\":\1\"$NAME\"/" \
    -e "s/\"name\":\([[:space:]]*\)\"My Extension\"/\"name\":\1\"$DISPLAY\"/" \
    "$TARGET/manifest.json"

# package.json — set name
sed -i "s/\"name\":\([[:space:]]*\)\"my-extension\"/\"name\":\1\"$NAME\"/" "$TARGET/package.json"

# Registration global key in source + prebuilt bundle: my_extension → <UNDERSCORED>
for f in "$TARGET/src/index.ts" "$TARGET/dist/index.iife.js"; do
    [[ -f "$f" ]] && sed -i "s/__ext_my_extension/__ext_$UNDERSCORED/g" "$f"
done

# Build the UI bundle if npm is available (otherwise the prebuilt stub is used)
if command -v npm >/dev/null 2>&1; then
    echo "→ Installing dependencies and building (npm)…"
    ( cd "$TARGET" && npm install --silent && npm run build >/dev/null )
    echo "✓ Built $TARGET/dist/index.iife.js"
else
    echo "! npm not found — kept the prebuilt dist/index.iife.js stub (edit src/ and run 'npm run build' later)"
fi

cat <<EOF

✓ Extension '$NAME' created.

Next steps:
  1. Edit  $TARGET/src/Settings.vue   (your UI)
  2. Build $TARGET                     → cd "$TARGET" && npm run build
  3. Open OpenGG → Settings → Extensions → Refresh

Docs: extension-template/AGENTS.md (full contract) · PROMPT.md (build with AI)
EOF
