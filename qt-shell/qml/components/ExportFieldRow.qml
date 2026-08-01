import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Labelled text field with an optional trailing folder button — the export
// dialog's FILENAME and DIRECTORY rows.
ColumnLayout {
    id: row

    property string label: ""
    property string value: ""

    signal edited(string value)
    /// Emitted by the folder button; omit the handler to hide the button.
    signal browse()

    spacing: 7

    Text {
        text: row.label
        color: Theme.textMuted
        font.pixelSize: 10
        font.weight: Font.Bold
        font.letterSpacing: 0.8
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 34
            radius: Theme.radius
            color: Theme.surface
            border.width: 1
            border.color: input.activeFocus ? Theme.accent : Theme.border

            TextField {
                id: input
                anchors.fill: parent
                leftPadding: 10
                rightPadding: 10
                verticalAlignment: TextInput.AlignVCenter
                color: Theme.text
                font.pixelSize: 13
                background: Item {}
                selectByMouse: true
                text: row.value
                onTextEdited: row.edited(input.text)
            }
        }

        Rectangle {
            Layout.preferredWidth: 34
            Layout.preferredHeight: 34
            radius: Theme.radius
            color: browseArea.containsMouse ? Theme.bgHover : Theme.surface
            border.width: 1
            border.color: Theme.border
            Icon {
                anchors.centerIn: parent
                name: "folder"; size: 14; color: Theme.textDim
            }
            MouseArea {
                id: browseArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: row.browse()
            }
        }
    }
}
