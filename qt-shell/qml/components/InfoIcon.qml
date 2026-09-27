import QtQuick
import com.opengg.app

// Small info glyph with a hover tooltip — QML port of InfoIcon.vue.
//
// Uses the "info" icon from the registry (circle + bar + dot, the exact
// path the Vue component shipped) rather than drawing a circle and setting
// an italic "i" inside it: at 14px that read as a smudge, not as a hint
// marker. Brightens to the accent on hover, as the original did.
Item {
    id: root
    property string tooltipText: ""
    implicitWidth: 14
    implicitHeight: 14

    Icon {
        anchors.fill: parent
        name: "info"
        size: 14
        color: hoverArea.containsMouse ? Theme.accent : Theme.textMuted
    }

    MouseArea {
        id: hoverArea
        anchors.fill: parent
        hoverEnabled: true
        // "help", not a pointing hand: there is nothing to click here.
        cursorShape: Qt.WhatsThisCursor
    }

    Tip {
        visible: hoverArea.containsMouse && root.tooltipText.length > 0
        text: root.tooltipText
    }
}
