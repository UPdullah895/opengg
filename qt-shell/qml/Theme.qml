pragma Singleton
import QtQuick
import com.opengg.app

// Thin read-only mirror of ThemeController (the mutable/persisted source —
// see qt-shell/src/theme.rs), kept as a stable `Theme.xxx` surface so every
// existing call site is unaffected by the switch to a live theme.json-backed
// controller.
QtObject {
    // Accent color for highlights and active states
    readonly property string accent: ThemeController.accent

    // Primary background color
    readonly property string bg: ThemeController.bg

    // Surface/card background color
    readonly property string surface: ThemeController.surface

    // Border color for dividers and edges
    readonly property string border: ThemeController.border

    // Primary text color
    readonly property string text: ThemeController.text

    // Dimmed/secondary text color
    readonly property string textDim: ThemeController.textDim

    // Border radius constant
    readonly property int radius: ThemeController.radius
}
