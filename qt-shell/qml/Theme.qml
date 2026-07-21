pragma Singleton
import QtQuick

QtObject {
    // Accent color for highlights and active states
    readonly property string accent: "#E94560"

    // Primary background color
    readonly property string bg: "#0f1117"

    // Surface/card background color
    readonly property string surface: "#171923"

    // Border color for dividers and edges
    readonly property string border: "#2a2d3a"

    // Primary text color
    readonly property string text: "#e2e8f0"

    // Dimmed/secondary text color
    readonly property string textDim: "#94a3b8"

    // Border radius constant
    readonly property int radius: 6
}
