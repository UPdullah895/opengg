pragma Singleton
import QtQuick

// Icon path registry — SVG path data for every icon the shell draws.
//
// Geometry is COPIED from the Vue UI, never redrawn: extracted mechanically
// from the 193 inline <svg> elements in frontend/src/**/*.vue plus the 16
// named ICON_* constants in frontend/src/assets/deviceAssets.ts. SVG
// primitives (line/rect/circle/polyline/polygon) were converted to equivalent
// path data because QML's PathSvg only understands a `d` string. Each entry
// notes where it came from so it can be re-checked against the original.
//
// Why this exists: the shell previously drew icons as emoji/Unicode glyphs.
// That is not a cheaper icon, it is a different one — wrong weight, wrong
// metrics, distro-dependent, often ignoring `color:` — and several codepoints
// had no font coverage at all and rendered as tofu boxes (□), most visibly the
// mixer's mute buttons. See the UI-fidelity plan, D5.
//
// Multiple subpaths are joined into one `d` string: SVG path data allows
// several `M` subpaths, and Shape/Repeater cannot be combined (ShapePath is
// not an Item, so a Repeater cannot produce them).
QtObject {
    // `sw` is the source stroke-width; `filled` icons paint rather than stroke.
    readonly property var defs: ({
        // ── Audio (deviceAssets.ts + ChannelStrip.vue) ──
        // ICON_VOLUME / ChannelStrip muted state: speaker + X
        "volume-x": { d: "M 11 5L 6 9L 2 9L 2 15L 6 15L 11 19L 11 5ZM 23 9L 17 15M 17 9L 23 15", sw: 2.5 },
        // ChannelStrip unmuted state: speaker + one wave
        "volume-1": { d: "M 11 5L 6 9L 2 9L 2 15L 6 15L 11 19L 11 5ZM 15.54 8.46a 5 5 0 0 1 0 7.07", sw: 2.5 },
        // ICON_AUDIO: speaker + two waves
        "volume-2": { d: "M 11 5L 6 9L 2 9L 2 15L 6 15L 11 19L 11 5ZM 15.54 8.46a 5 5 0 0 1 0 7.07 M 19.07 4.93a 10 10 0 0 1 0 14.14", sw: 2 },
        // ChannelStrip.vue / DeviceCard.vue
        "headphones": { d: "M 3 18v -6a 9 9 0 0 1 18 0 v 6M 21 19a 2 2 0 0 1 -2 2 h -1a 2 2 0 0 1 -2 -2 v -3a 2 2 0 0 1 2 -2 h 3zM 3 19a 2 2 0 0 0 2 2 h 1a 2 2 0 0 0 2 -2 v -3a 2 2 0 0 0 -2 -2 H 3z", sw: 2 },
        "mic": { d: "M 12 1a 3 3 0 0 0 -3 3 v 8a 3 3 0 0 0 6 0 V 4a 3 3 0 0 0 -3 -3 zM 19 10v 2a 7 7 0 0 1 -14 0 v -2", sw: 2 },

        // ── Chevrons (ChannelStrip.vue dev-chev) ──
        "chevron-down": { d: "M 6 9l 6 6 6 -6", sw: 2 },
        // Mirror of chevron-down (the Vue UI only ships the down variant).
        "chevron-up": { d: "M 18 15l -6 -6 -6 6", sw: 2 },

        // ── Window controls (Titlebar.vue) ──
        "minus": { d: "M 5 12L 19 12", sw: 2 },
        "square": { d: "M 6 4L 18 4A 2 2 0 0 1 20 6 L 20 18A 2 2 0 0 1 18 20 L 6 20A 2 2 0 0 1 4 18 L 4 6A 2 2 0 0 1 6 4 Z", sw: 2 },
        "x": { d: "M 6 6L 18 18M 18 6L 6 18", sw: 2 },

        // ── Actions ──
        // ICON_DELETE (deviceAssets.ts)
        "trash": { d: "M 3 6L 5 6L 21 6M 19 6l -1 14a 2 2 0 0 1 -2 2 H 8a 2 2 0 0 1 -2 -2 L 5 6M 10 11v 6M 14 11v 6M 9 6V 4a 1 1 0 0 1 1 -1 h 4a 1 1 0 0 1 1 1 v 2", sw: 2 },
        // ClipsPage.vue rename
        "edit": { d: "M 11 4H 4a 2 2 0 0 0 -2 2 v 14a 2 2 0 0 0 2 2 h 14a 2 2 0 0 0 2 -2 v -7M 18.5 2.5a 2.121 2.121 0 0 1 3 3 L 12 15l -4 1 1 -4 9.5 -9.5z", sw: 2 },
        // ClipCard.vue trimmed badge (filled)
        "scissors": { d: "M 9.64 7.64a 2.5 2.5 0 1 1 -3.54 -3.54 2.5 2.5 0 0 1 3.54 3.54 Zm 0 8.72a 2.5 2.5 0 1 1 -3.54 3.54 2.5 2.5 0 0 1 3.54 -3.54 ZM 14.59 12l 6.2 6.2 -1.41 1.41L 12 12.41l -7.38 7.2 -1.4 -1.42L 9.41 12 3.22 5.8l 1.4 -1.41L 12 11.59l 7.38 -7.2 1.41 1.42z", sw: 0, filled: true },
        "refresh-cw": { d: "M 23 4L 23 10L 17 10M 20.49 15a 9 9 0 1 1 -2.12 -9.36 L 23 10", sw: 2 },
        "rotate-ccw": { d: "M 1 4L 1 10L 7 10M 3.51 15a 9 9 0 1 0 2.13 -9.36 L 1 10", sw: 2 },
        "plus": { d: "M 12 5v 14M 5 12h 14", sw: 2 },
        "check": { d: "M 20 6L 9 17L 4 12", sw: 2.5 },
        // ClipCard.vue kebab (filled dots)
        "more-vertical": { d: "M 10.5 5A 1.5 1.5 0 0 1 13.5 5 A 1.5 1.5 0 0 1 10.5 5 ZM 10.5 12A 1.5 1.5 0 0 1 13.5 12 A 1.5 1.5 0 0 1 10.5 12 ZM 10.5 19A 1.5 1.5 0 0 1 13.5 19 A 1.5 1.5 0 0 1 10.5 19 Z", sw: 0, filled: true },

        // ── Media (ClipEditor.vue / CustomVideoPlayer.vue) ──
        "play": { d: "M 5 3L 19 12L 5 21L 5 3Z", sw: 0, filled: true },
        "pause": { d: "M 6 4L 10 4L 10 20L 6 20ZM 14 4L 18 4L 18 20L 14 20Z", sw: 0, filled: true },

        // ── Status / meta ──
        // ICON_ALERT
        "alert-triangle": { d: "M 10.29 3.86L 1.82 18a 2 2 0 0 0 1.71 3 h 16.94a 2 2 0 0 0 1.71 -3 L 13.71 3.86a 2 2 0 0 0 -3.42 0 zM 12 9L 12 13M 12 17L 12.01 17", sw: 2 },
        "info": { d: "M 2 12A 10 10 0 0 1 22 12 A 10 10 0 0 1 2 12 ZM 12 8L 12 12M 12 16L 12.01 16", sw: 2 },
        // MixerRoutingSettings.vue create-virtual-audio
        "check-square": { d: "M 9 11L 12 14L 22 4M 21 12v 7a 2 2 0 0 1 -2 2 H 5a 2 2 0 0 1 -2 -2 V 5a 2 2 0 0 1 2 -2 h 11", sw: 2 },
        // StoreSettings.vue
        "package": { d: "M 6 2 3 6v 14a 2 2 0 0 0 2 2 h 14a 2 2 0 0 0 2 -2 V 6l -3 -4zM 3 6L 21 6M 11 15A 1 1 0 0 1 13 15 A 1 1 0 0 1 11 15 Z", sw: 2 },
        // ICON_BOLT — also the mixer's Overdrive indicator
        "zap": { d: "M 13 2L 3 14L 12 14L 11 22L 21 10L 12 10L 13 2Z", sw: 2 },
        // ICON_BATTERY
        "battery": { d: "M 4 7L 18 7A 2 2 0 0 1 20 9 L 20 16A 2 2 0 0 1 18 18 L 4 18A 2 2 0 0 1 2 16 L 2 9A 2 2 0 0 1 4 7 ZM 22 11v 3", sw: 2 },
        // ICON_FOLDER
        "folder": { d: "M 22 19a 2 2 0 0 1 -2 2 H 4a 2 2 0 0 1 -2 -2 V 5a 2 2 0 0 1 2 -2 h 5l 2 3h 9a 2 2 0 0 1 2 2 z", sw: 2 },
        // ICON_SEARCH
        "search": { d: "M 3 11A 8 8 0 0 1 19 11 A 8 8 0 0 1 3 11 ZM 21 21L 16.65 16.65", sw: 2 },
        // ICON_FILM — ClipCard.vue's missing-thumbnail placeholder
        "film": { d: "M 4.18 2L 19.82 2A 2.18 2.18 0 0 1 22 4.18 L 22 19.82A 2.18 2.18 0 0 1 19.82 22 L 4.18 22A 2.18 2.18 0 0 1 2 19.82 L 2 4.18A 2.18 2.18 0 0 1 4.18 2 ZM 7 2L 7 22M 17 2L 17 22M 2 12L 22 12M 2 7L 7 7M 2 17L 7 17M 17 17L 22 17M 17 7L 22 7", sw: 2 },
        // ICON_GAMEPAD
        "gamepad": { d: "M 6 12L 10 12M 8 10L 8 14M 15 13L 15.01 13M 18 11L 18.01 11M 4 6L 20 6A 2 2 0 0 1 22 8 L 22 16A 2 2 0 0 1 20 18 L 4 18A 2 2 0 0 1 2 16 L 2 8A 2 2 0 0 1 4 6 Z", sw: 2 },

        // ── Appearance toggle (GeneralSettings.vue:113-114) ──
        "sun": { d: "M 7 12A 5 5 0 0 1 17 12 A 5 5 0 0 1 7 12 ZM 12 1L 12 3M 12 21L 12 23M 4.22 4.22L 5.64 5.64M 18.36 18.36L 19.78 19.78M 1 12L 3 12M 21 12L 23 12M 4.22 19.78L 5.64 18.36M 18.36 5.64L 19.78 4.22", sw: 2 },
        "moon": { d: "M 21 12.79A 9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79 z", sw: 2 },

        // ── Track visibility (TrackManagementSettings.vue:46-52) ──
        "eye": { d: "M 1 12s 4 -8 11 -8 11 8 11 8 -4 8 -11 8 -11 -8 -11 -8zM 9 12A 3 3 0 0 1 15 12 A 3 3 0 0 1 9 12 Z", sw: 2 },
        "eye-off": { d: "M 17.94 17.94A 10.07 10.07 0 0 1 12 20 c -7 0 -11 -8 -11 -8a 18.45 18.45 0 0 1 5.06 -5.94 M 9.9 4.24A 9.12 9.12 0 0 1 12 4 c 7 0 11 8 11 8a 18.5 18.5 0 0 1 -2.16 3.19 m -6.72 -1.07a 3 3 0 1 1 -4.24 -4.24 M 1 1L 23 23", sw: 2 },

        // ── Timeline track types (IconPicker.vue ICONS) ──
        "track-video": { d: "M 15 10l 4.553 -2.276A 1 1 0 0 1 21 8.618 v 6.764a 1 1 0 0 1 -1.447 .894 L 15 14M 3 6h 10a 2 2 0 0 1 2 2 v 8a 2 2 0 0 1 -2 2 H 3a 2 2 0 0 1 -2 -2 V 8a 2 2 0 0 1 2 -2 z", sw: 2 },
        "track-game": { d: "M 6 11h 4m -2 -2v 4m 7 -1h .01M 18 11h .01M 2 6a 2 2 0 0 1 2 -2 h 16a 2 2 0 0 1 2 2 v 10a 4 4 0 0 1 -4 4 H 6a 4 4 0 0 1 -4 -4 V 6z", sw: 2 },
        "track-mic": { d: "M 12 1a 3 3 0 0 0 -3 3 v 8a 3 3 0 0 0 6 0 V 4a 3 3 0 0 0 -3 -3 zM 19 10v 2a 7 7 0 0 1 -14 0 v -2M 12 19v 3M 8 23h 8", sw: 2 },
        "track-media": { d: "M 9 18V 5l 12 -2v 13M 9 19c 0 1.1 -1.34 2 -3 2s -3 -.9 -3 -2 1.34 -2 3 -2 3 .9 3 2zm 12 -3c 0 1.1 -1.34 2 -3 2s -3 -.9 -3 -2 1.34 -2 3 -2 3 .9 3 2z", sw: 2 },
        "track-overlay": { d: "M 12 2L 2 7l 10 5 10 -5 -10 -5zM 2 17l 10 5 10 -5M 2 12l 10 5 10 -5", sw: 2 },

        // DeviceCard.vue's non-headset branch (it only distinguishes headset vs
        // everything else, so there is no separate keyboard glyph to port).
        "mouse": { d: "M 12 2A 7 7 0 0 1 19 9 L 19 15A 7 7 0 0 1 5 15 L 5 9A 7 7 0 0 1 12 2 ZM 12 2L 12 10", sw: 2 },
        // ClipCard.vue favourite (filled when active)
        "heart": { d: "M 20.84 4.61a 5.5 5.5 0 0 0 -7.78 0 L 12 5.67l -1.06 -1.06a 5.5 5.5 0 0 0 -7.78 7.78 l 1.06 1.06L 12 21.23l 7.78 -7.78 1.06 -1.06a 5.5 5.5 0 0 0 0 -7.78 z", sw: 2 }
    })

    function path(name) {
        var i = defs[name]
        if (!i) {
            console.warn("Icons: unknown icon name '" + name + "'")
            return ""
        }
        return i.d
    }
    function isFilled(name) {
        var i = defs[name]
        return !!(i && i.filled)
    }
    function strokeWidth(name) {
        var i = defs[name]
        return (i && i.sw !== undefined) ? i.sw : 2
    }
}
