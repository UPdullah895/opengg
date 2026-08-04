import QtQuick
import com.opengg.app

// Restyled pill toggle — QML port of ToggleSwitch.vue.
Item {
    id: root
    property bool checked: false
    signal toggled(bool value)

    implicitWidth: 40
    implicitHeight: 22

    // Matches ToggleSwitch.vue exactly: an accent-TINTED track (not a solid
    // fill) with an accent border when on, and a knob that's text-muted when
    // off / accent when on — never a flat white circle regardless of theme.
    Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: root.checked ? Theme.accentAlpha(20) : Theme.bgDeep
        border.width: 1
        border.color: root.checked ? Theme.accent : Theme.border
        Behavior on color { ColorAnimation { duration: 120 } }
        Behavior on border.color { ColorAnimation { duration: 120 } }
    }

    Rectangle {
        width: parent.height - 8
        height: parent.height - 8
        radius: height / 2
        color: root.checked ? Theme.accent : Theme.textMuted
        y: 3
        x: root.checked ? parent.width - width - 3 : 3
        Behavior on x { NumberAnimation { duration: 120; easing.type: Easing.OutQuad } }
        Behavior on color { ColorAnimation { duration: 120 } }
    }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            root.checked = !root.checked
            root.toggled(root.checked)
        }
    }
}
