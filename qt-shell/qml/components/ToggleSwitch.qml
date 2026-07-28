import QtQuick
import com.opengg.app

// Restyled pill toggle — QML port of ToggleSwitch.vue.
Item {
    id: root
    property bool checked: false
    signal toggled(bool value)

    implicitWidth: 40
    implicitHeight: 22

    Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: root.checked ? Theme.accent : Theme.border
        Behavior on color { ColorAnimation { duration: 120 } }
    }

    Rectangle {
        width: parent.height - 4
        height: parent.height - 4
        radius: height / 2
        color: "#ffffff"
        y: 2
        x: root.checked ? parent.width - width - 2 : 2
        Behavior on x { NumberAnimation { duration: 120; easing.type: Easing.OutQuad } }
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
