import QtQuick
import com.opengg.app

// Radio-style pill used by the export dialog's target-size and codec rows.
// Selection is owned by the caller; this only reports taps.
Rectangle {
    id: chip

    property string text: ""
    property bool selected: false

    signal triggered()

    // `enabled` is Item's own property — redeclaring it shadows the base and
    // Qt warns about it. Inheriting it also disables the MouseArea for free.

    implicitWidth: label.implicitWidth + 24
    implicitHeight: 30
    radius: Theme.radius
    opacity: chip.enabled ? 1 : 0.4
    color: chip.selected ? Theme.accent
         : area.containsMouse && chip.enabled ? Theme.bgHover
         : Theme.surface
    border.width: 1
    border.color: chip.selected ? Theme.accent : Theme.border

    Text {
        id: label
        anchors.centerIn: parent
        text: chip.text
        color: chip.selected ? "#ffffff" : Theme.text
        font.pixelSize: 11
        font.weight: Font.DemiBold
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: chip.triggered()
    }
}
