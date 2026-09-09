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

# Top-level pages, and per-page sub-panels (keyed as <page>:<panel> — the
# same generic ScreenshotController.panel property MixerPage.qml already
# reuses for its own tab selection, not a settings-only concept).
TARGETS=(
    home mixer clips devices
    settings:general settings:language settings:shortcuts
    settings:mixerRouting settings:captureSound settings:trackManagement
    settings:storage settings:notifications settings:extensions
    settings:store settings:about
    devices:buttons
    devices:list-many devices:grid-many devices:carousel-many
    tour
)

shot() {
    local target="$1" page panel name args=()
    if [[ "$target" == tour ]]; then
        # The tour overlay is suppressed in normal capture runs (it would cover
        # whatever page we asked for), so it needs an explicit opt-in.
        page=home; panel=""; name=tour; args+=(--with-tour)
    elif [[ "$target" == *:* ]]; then
        page="${target%%:*}"; panel="${target#*:}"; name="${page}-${panel}"
    else
        page="$target"; panel=""; name="$target"
    fi

    args+=(--screenshot "$OUTDIR/$name.png" --page "$page" --delay "$DELAY")
    [[ -n "$panel" ]] && args+=(--panel "$panel")

    # 30s ceiling: a hung capture must not stall the whole sweep.
    #
    # QT_FORCE_STDERR_LOGGING=1 is REQUIRED, not cosmetic: this Qt build has
    # journald support, so without it every qWarning/console.log/QML TypeError
    # goes to the journal and stderr looks perfectly clean. That is how a live
    # `TypeError: Cannot read property 'length' of undefined` in DevicesPage.qml
    # went unnoticed through an entire session of "verified, empty log" claims.
    # (QT_LOGGING_TO_CONSOLE is the old name for this and now warns.)
    local out
    out=$(timeout 30 env QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 \
        "$BIN" "${args[@]}" 2>&1)

    # Anything QML complains about — TypeErrors, unresolved properties, binding
    # loops, property shadowing — is reported per-target so it cannot hide
    # behind a successfully written PNG.
    local warns
    warns=$(echo "$out" | grep -E \
        "TypeError|ReferenceError|Unable to assign|is not a|binding loop|overrides a member|QML .* (Error|Warning)" \
        | sort -u)

    if [[ -s "$OUTDIR/$name.png" ]]; then
        if [[ -n "$warns" ]]; then
            printf '  WARN %-28s %s\n' "$name" "$(du -h "$OUTDIR/$name.png" | cut -f1)"
            echo "$warns" | sed 's/^/         /'
            return 2
        fi
        printf '  ok   %-28s %s\n' "$name" "$(du -h "$OUTDIR/$name.png" | cut -f1)"
    else
        printf '  FAIL %-28s %s\n' "$name" "$(echo "$out" | grep -i screenshot | head -1)"
        return 1
    fi
}

echo "Capturing qt-shell UI → $OUTDIR (delay ${DELAY}ms)"
failed=0
warned=0
for t in "${TARGETS[@]}"; do
    [[ -n "$ONLY" && "$t" != *"$ONLY"* ]] && continue
    shot "$t"
    case $? in
        1) failed=$((failed + 1)) ;;
        2) warned=$((warned + 1)) ;;
    esac
done

echo
(( failed > 0 )) && echo "$failed capture(s) failed" >&2
(( warned > 0 )) && echo "$warned capture(s) produced QML warnings" >&2
if (( failed > 0 || warned > 0 )); then
    exit 1
fi
echo "All captures written to $OUTDIR (no QML warnings)"
