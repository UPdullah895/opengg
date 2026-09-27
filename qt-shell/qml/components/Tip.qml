import QtQuick
import QtQuick.Controls
import com.opengg.app

// Themed hover tooltip — a drop-in for a bare QQC2 `ToolTip`, which renders
// in the Basic style's own palette (pale box, system font, square corners)
// and so was the one piece of chrome in the shell that ignored the theme
// entirely. Same API: set `text` and `visible`.
ToolTip {
    id: tip

    delay: 400
    padding: 0
    // Enough to clear the pointer without drifting away from the control.
    y: -tip.implicitHeight - 6

    background: Rectangle {
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
    }

    contentItem: Text {
        text: tip.text
        color: Theme.text
        font.pixelSize: 11
        leftPadding: 8
        rightPadding: 8
        topPadding: 5
        bottomPadding: 5
        // Long hints (the settings panels have several) wrap instead of
        // running off the side of the window.
        wrapMode: Text.WordWrap
        width: Math.min(implicitWidth, 260)
    }
}
