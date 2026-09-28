#!/usr/bin/env bash
# Fail if qml/ contains a bare colour literal that should be a Theme token.
#
# This exists because the reverse happened: ThemeController originally exposed
# 7 of the ~18 tokens the Vue UI uses, so with no `danger`/`success`/`bgDeep`
# token to reach for, panel authors reached for a plausible hex literal instead.
# 84 accumulated, 24 of them semantically WRONG (#ef4444 where this project's
# --danger is #dc2626), plus frozen Qt.rgba accent tints that silently stopped
# following the user's chosen accent colour.
#
# Add a token in src/theme.rs rather than an entry to the allowlist below.
#
# Usage: qt-shell/tools/check-colors.sh   (also run by `make lint-qml`)

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

fail=0

# ── 1. Bare hex literals ──────────────────────────────────────────────────
# Allowlisted, with the reason each is legitimately not a theme token:
#   #ffffff / #fff  - foreground on an accent-filled control; the Vue UI
#                     hardcodes `color: #fff` in the same places.
#   #000000         - video letterboxing behind a VideoOutput, not a surface.
#   channel palette - Master/Game/Chat/Media/Aux/Mic identity colours, copied
#                     verbatim from MixerPage.vue:62-63 which hardcodes them too.
ALLOW_HEX='"#ffffff"|"#fff"|"#000000"'
#   dashboard cards - .card-icon.accent/.red/.green/.purple badge colours,
#                     likewise hardcoded in HomePage.vue's scoped CSS.
CHANNEL_LINE='Master:|Media:|mixer: "#3b82f6"'

hits=$(grep -rnE '"#[0-9a-fA-F]{3,8}"' qml/ \
        | grep -vE "$ALLOW_HEX" \
        | grep -vE "$CHANNEL_LINE" \
        | grep -v 'Icons.qml' || true)
if [[ -n "$hits" ]]; then
    echo "error: bare colour literal in qml/ — use a Theme token:" >&2
    echo "$hits" | sed 's/^/  /' >&2
    fail=1
fi

# ── 2. Frozen accent/semantic tints ───────────────────────────────────────
# Qt.rgba(...) with a literal RGB triple bakes in a specific colour, so the
# tint stops tracking the user's accent (or diverges from --danger/--success).
# Use Theme.accentAlpha(pct) / Theme.tint(Theme.danger, pct) / Theme.scrim(pct).
rgba=$(grep -rn 'Qt\.rgba(' qml/ | grep -v 'qml/Theme.qml' || true)
if [[ -n "$rgba" ]]; then
    echo "error: literal Qt.rgba() in qml/ — use Theme.accentAlpha/tint/scrim:" >&2
    echo "$rgba" | sed 's/^/  /' >&2
    fail=1
fi

# ── 3. Emoji used as iconography ──────────────────────────────────────────
# Emoji render in the system emoji font (wrong weight/metrics, often ignoring
# `color:`) and several codepoints have no coverage at all — they drew as tofu
# boxes. Use Icon { name: "..." }; add paths to Icons.qml.
# The ✓ and • below are deliberate: the Vue original uses them as text too.
# `grep -v '//'` drops comment lines: several comments legitimately name the
# emoji they replaced, to document why an icon is absent.
emoji=$(grep -rnP '[\x{1F300}-\x{1FAFF}\x{2600}-\x{27BF}\x{2B00}-\x{2BFF}\x{FE0F}]' qml/ \
        | grep -vE '✓|•' \
        | grep -vE ':[0-9]+: *(//|\*)' || true)
if [[ -n "$emoji" ]]; then
    echo "error: emoji used as an icon in qml/ — use Icon{} + Icons.qml:" >&2
    echo "$emoji" | sed 's/^/  /' >&2
    fail=1
fi

if (( fail )); then
    exit 1
fi
echo "check-colors: qml/ is clean (no bare literals, no frozen tints, no emoji icons)"
