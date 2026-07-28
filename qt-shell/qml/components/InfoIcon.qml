import QtQuick
import QtQuick.Controls
import com.opengg.app

// Small "info" glyph with a hover tooltip — QML port of InfoIcon.vue.
Item {
    id: root
    property string tooltipText: ""
    implicitWidth: 14
    implicitHeight: 14

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: "transparent"
        border.width: 1
        border.color: Theme.textDim

        Text {
            anchors.centerIn: parent
            text: "i"
            font.pixelSize: 9
            font.italic: true
            font.weight: Font.DemiBold
            color: Theme.textDim
        }
    }

    MouseArea {
        id: hoverArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
    }

    ToolTip {
        visible: hoverArea.containsMouse && root.tooltipText.length > 0
        text: root.tooltipText
        delay: 300
    }
}
