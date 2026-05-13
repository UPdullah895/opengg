#!/bin/bash
# Fetch device images from libratbag/piper's SVG collection.
# Usage: ./scripts/fetch_device_assets.sh
# Requires: jq, inkscape or rsvg-convert (for SVG→PNG), curl
set -euo pipefail

ASSETS_DIR="frontend/src/assets/devices"
DB="frontend/src/assets/device_database.json"
PIPER_BASE="https://raw.githubusercontent.com/libratbag/piper/master/data/svgs"

mkdir -p "$ASSETS_DIR"

if ! command -v jq &>/dev/null; then
  echo "Error: jq is required. Install with: sudo pacman -S jq" >&2
  exit 1
fi

convert_svg() {
  local src="$1" dst="$2"
  if command -v rsvg-convert &>/dev/null; then
    rsvg-convert -w 300 -h 200 "$src" -o "$dst"
  elif command -v inkscape &>/dev/null; then
    inkscape --export-width=300 --export-height=200 --export-filename="$dst" "$src" 2>/dev/null
  else
    echo "  Warning: no SVG converter found (install librsvg or inkscape). Skipping PNG conversion." >&2
    return 1
  fi
}

jq -r 'to_entries[] | "\(.key) \(.value.image)"' "$DB" | while IFS=' ' read -r vpid imgfile; do
  dest="$ASSETS_DIR/$imgfile"
  [ -f "$dest" ] && echo "  Skip $imgfile (already exists)" && continue

  # libratbag SVG key uses lowercase vid:pid without underscore
  svg_key="${vpid/_/:}"
  svg_url="$PIPER_BASE/${svg_key}.svg"
  tmp_svg="/tmp/opengg_device_${vpid}.svg"

  echo "Fetching $vpid → $imgfile ..."
  if curl -fsL "$svg_url" -o "$tmp_svg" 2>/dev/null; then
    if convert_svg "$tmp_svg" "$dest"; then
      echo "  Done: $imgfile"
    else
      echo "  SVG saved at $tmp_svg — convert manually to $dest"
    fi
    rm -f "$tmp_svg"
  else
    echo "  Not found in piper collection: $vpid (skipping)"
  fi
done

echo ""
echo "Done. Place any missing PNGs in $ASSETS_DIR using {vid}_{pid}.png naming."
echo "VID/PID reference: $DB"
