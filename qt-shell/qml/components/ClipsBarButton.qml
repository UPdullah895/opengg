import QtQuick
import com.opengg.app

// Small icon+label button used by the Clips page's selection action bar.
// Extracted because that bar needs four of them and an inline Rectangle +
// MouseArea + Row each time is most of the file.
Rectangle {
    id: btn

    property string label: ""
    property string icon: ""
    /// Renders in the danger colour and highlights red on hover.
    property bool danger: false

    signal triggered()

    implicitWidth: row.implicitWidth + 20
    implicitHeight: 30
    radius: Theme.radius
    color: area.containsMouse
           ? (btn.danger ? Theme.tint(Theme.danger, 16) : Theme.accentAlpha(18))
           : "transparent"
    border.width: 1
    border.color: btn.danger ? Theme.tint(Theme.danger, 40) : Theme.accentAlpha(35)

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 6

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: btn.icon
            size: 13
            color: btn.danger ? Theme.danger : Theme.accent
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: btn.label
            color: btn.danger ? Theme.danger : Theme.text
            font.pixelSize: 12
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: btn.triggered()
    }
}
