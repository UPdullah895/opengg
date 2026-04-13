#!/usr/bin/env bash
# reset-clip-data.sh
# Deletes thumbnails, waveforms, and the SQLite metadata cache so you get a
# fresh clips library for performance testing. Does NOT touch actual video files.
#
# IMPORTANT: Close the app before running this script — otherwise the app will
# immediately regenerate thumbnails while you're deleting them.

set -e

DATA_DIR="$HOME/.local/share/opengg"
THUMB_DIR="$DATA_DIR/thumbnails"
WAVE_DIR="$DATA_DIR/waveforms"
DB_FILE="$DATA_DIR/clips.db"
SETTINGS="$HOME/.config/opengg/ui-settings.json"

echo "=== OpenGG clip data reset ==="
echo ""

# Show what will be deleted
thumb_count=0
wave_count=0
[ -d "$THUMB_DIR" ] && thumb_count=$(find "$THUMB_DIR" -maxdepth 1 -name "*.jpg" | wc -l)
[ -d "$WAVE_DIR"  ] && wave_count=$(find "$WAVE_DIR"  -maxdepth 1 -name "*.png" | wc -l)

echo "Will delete:"
echo "  thumbnails    : $thumb_count file(s) in $THUMB_DIR"
echo "  waveforms     : $wave_count file(s) in $WAVE_DIR"
[ -f "$DB_FILE" ] && echo "  database      : $DB_FILE" || echo "  database      : not found"
echo ""
echo "Will clear:"
echo "  clip_directories in $SETTINGS"
echo "  (set to a non-existent path so the app starts with no clips loaded)"
echo "  Re-add your clips folder in Settings after launching."
echo ""
echo "Your actual video files will NOT be touched."
echo ""
read -rp "Close the app first. Continue? [y/N] " confirm
[[ "$confirm" =~ ^[Yy]$ ]] || { echo "Aborted."; exit 0; }

# Clear clip_directories in ui-settings.json so the app doesn't auto-load clips.
# An empty array [] still falls back to ~/Videos/OpenGG, so we set a sentinel path
# that doesn't exist — the app will find no clips until you re-add the directory.
if [ -f "$SETTINGS" ]; then
    python3 - "$SETTINGS" <<'EOF'
import json, sys
path = sys.argv[1]
with open(path) as f:
    data = json.load(f)
data.setdefault('settings', {})['clip_directories'] = ['/tmp/__opengg_reset__']
with open(path, 'w') as f:
    json.dump(data, f, indent=2)
EOF
    echo "  cleared clip_directories in settings"
else
    echo "  settings file not found, skipping"
fi

# Thumbnails
if [ -d "$THUMB_DIR" ]; then
    find "$THUMB_DIR" -maxdepth 1 -name "*.jpg" -delete
    echo "  deleted $thumb_count thumbnail(s)"
else
    echo "  thumbnails dir not found, skipping"
fi

# Waveforms
if [ -d "$WAVE_DIR" ]; then
    find "$WAVE_DIR" -maxdepth 1 -name "*.png" -delete
    echo "  deleted $wave_count waveform(s)"
else
    echo "  waveforms dir not found, skipping"
fi

# SQLite DB (clip_meta: custom names, favorites, game tags; trim_state: trim points)
if [ -f "$DB_FILE" ]; then
    rm -f "$DB_FILE"
    echo "  deleted database"
else
    echo "  database not found, skipping"
fi

echo ""
echo "Done. Launch the app, then go to Settings → Storage to add your clips folder."
