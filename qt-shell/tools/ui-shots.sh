#!/usr/bin/env bash
# Capture every qt-shell page and settings panel to PNG, fully headless.
#
# Runs under QT_QPA_PLATFORM=offscreen, so this NEVER opens a window on the
# developer's desktop — no focus steal, no window flashing over a fullscreen
# game. That property is the whole point (see the UI-fidelity plan, Phase 0):
# the visual gap this harness exists to close accumulated precisely because UI
# work was being signed off on clean builds and empty logs instead of pixels.
#
# Usage:
#   qt-shell/tools/ui-shots.sh [OUTDIR]      # default: qt-shell/target/ui-shots
#   ONLY=clips qt-shell/tools/ui-shots.sh    # single page/panel, faster iteration
#   DELAY=1500 qt-shell/tools/ui-shots.sh    # longer settle for slow thumbnails

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

OUTDIR="${1:-target/ui-shots}"
DELAY="${DELAY:-900}"
ONLY="${ONLY:-}"
BIN="target/debug/opengg-qt"

if [[ ! -x "$BIN" ]]; then
    echo "error: $BIN not built — run 'cargo build' in qt-shell/ first" >&2
    exit 1
fi

mkdir -p "$OUTDIR"

# Top-level pages, and the settings sub-panels (keyed as settings:<panel>).
TARGETS=(
    home mixer clips devices
    settings:general settings:language settings:shortcuts
    settings:mixerRouting settings:captureSound settings:trackManagement
    settings:storage settings:notifications settings:extensions
    settings:store settings:about
    tour
)

shot() {
    local target="$1" page panel name args=()
    if [[ "$target" == tour ]]; then
        # The tour overlay is suppressed in normal capture runs (it would cover
        # whatever page we asked for), so it needs an explicit opt-in.
        page=home; panel=""; name=tour; args+=(--with-tour)
    elif [[ "$target" == settings:* ]]; then
        page=settings; panel="${target#settings:}"; name="settings-$panel"
    else
        page="$target"; panel=""; name="$target"
    fi

    args+=(--screenshot "$OUTDIR/$name.png" --page "$page" --delay "$DELAY")
    [[ -n "$panel" ]] && args+=(--panel "$panel")

    # 30s ceiling: a hung capture must not stall the whole sweep.
    local out
    out=$(timeout 30 env QT_QPA_PLATFORM=offscreen "$BIN" "${args[@]}" 2>&1)

    if [[ -s "$OUTDIR/$name.png" ]]; then
        printf '  ok   %-28s %s\n' "$name" "$(du -h "$OUTDIR/$name.png" | cut -f1)"
    else
        printf '  FAIL %-28s %s\n' "$name" "$(echo "$out" | grep -i screenshot | head -1)"
        return 1
    fi
}

echo "Capturing qt-shell UI → $OUTDIR (delay ${DELAY}ms)"
failed=0
for t in "${TARGETS[@]}"; do
    [[ -n "$ONLY" && "$t" != *"$ONLY"* ]] && continue
    shot "$t" || failed=$((failed + 1))
done

echo
if (( failed > 0 )); then
    echo "$failed capture(s) failed" >&2
    exit 1
fi
echo "All captures written to $OUTDIR"
