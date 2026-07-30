pragma Singleton
import QtQuick
import com.opengg.app

// Thin read-only mirror of ThemeController (the mutable/persisted source —
// see qt-shell/src/theme.rs), kept as a stable `Theme.xxx` surface so every
// existing call site is unaffected by the switch to a live theme.json-backed
// controller.
//
// Token names map 1:1 onto the CSS custom properties in App.vue's :root, with
// the two legacy aliases noted below. NOTHING in qml/ should contain a bare hex
// color literal: if a value you need isn't here, add the token in theme.rs
// rather than inlining it. That rule exists because the reverse produced 84
// hardcoded literals across this shell, 24 of them semantically wrong.
QtObject {
    // ── Colors ────────────────────────────────────────────────────────────
    // Accent color for highlights and active states  (--accent)
    readonly property string accent: ThemeController.accent

    // Primary window background  (--bg-surface)
    // NOTE: named `bg`, not `surface` — the alias predates full token parity
    // and is load-bearing across every page. `Theme.surface` is --bg-card.
    readonly property string bg: ThemeController.bg

    // Card/panel background  (--bg-card)
    readonly property string surface: ThemeController.surface

    // Recessed/inset background, below the card layer  (--bg-deep)
    readonly property string bgDeep: ThemeController.bgDeep

    // Hover state background  (--bg-hover)
    readonly property string bgHover: ThemeController.bgHover

    // Text input background  (--bg-input)
    readonly property string bgInput: ThemeController.bgInput

    // Border color for dividers and edges  (--border)
    readonly property string border: ThemeController.border

    // Primary text  (--text)
    readonly property string text: ThemeController.text

    // Secondary text  (--text-sec)
    readonly property string textDim: ThemeController.textDim

    // Tertiary/muted text — dimmer than textDim  (--text-muted)
    // The Vue UI uses --text-sec and --text-muted about equally (134 vs 128
    // uses); collapsing both onto textDim is what flattened this shell's text
    // hierarchy, so pick deliberately between them.
    readonly property string textMuted: ThemeController.textMuted

    // Destructive actions  (--danger). Not #ef4444.
    readonly property string danger: ThemeController.danger

    // Success/ready states  (--success). Not #22c55e.
    readonly property string success: ThemeController.success

    // Secondary highlight  (--purple)
    readonly property string purple: ThemeController.purple

    // Fader tint past 100%. The Vue UI hardcodes this at ChannelStrip.vue:253
    // rather than using a CSS var; tokenised so this shell has no bare literal.
    readonly property string overdrive: ThemeController.overdrive

    // ── Layout ────────────────────────────────────────────────────────────
    readonly property int radius: ThemeController.radius          // --radius
    readonly property int radiusLg: ThemeController.radiusLg      // --radius-lg
    readonly property int clipsGridCols: ThemeController.clipsGridCols
    readonly property int titlebarH: ThemeController.titlebarH
    readonly property int sidebarW: ThemeController.sidebarW

    readonly property bool darkMode: ThemeController.darkMode

    // ── Derived helpers ───────────────────────────────────────────────────
    // Translucent accent, mirroring the Vue UI's
    //   color-mix(in srgb, var(--accent) N%, transparent)
    // which it uses in 101 places. ALWAYS use this instead of a literal
    // Qt.rgba(0.914, 0.271, 0.376, a): that hardcodes the *default* accent, so
    // tinted highlights silently stop following the user's chosen accent color.
    function accentAlpha(pct) {
        return Qt.alpha(Theme.accent, pct / 100)
    }

    // Translucent version of any token — the general form of accentAlpha, for
    // danger/success tints (Vue: color-mix(in srgb, var(--danger) 8%, transparent)).
    function tint(color, pct) {
        return Qt.alpha(color, pct / 100)
    }

    // Modal backdrop. The Vue UI uses plain black at varying alpha
    // (rgba(0,0,0,.4) … rgba(0,0,0,.85)) rather than a tinted scrim, so pass
    // the percentage the specific modal wants.
    function scrim(pct) {
        return Qt.rgba(0, 0, 0, pct / 100)
    }
}
