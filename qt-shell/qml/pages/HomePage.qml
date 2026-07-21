import QtQuick
import QtQuick.Layouts
import com.opengg.app

Rectangle {
    color: Theme.bg

    ColumnLayout {
        anchors.centerIn: parent
        spacing: 12

        Text {
            text: "Home"
            color: Theme.text
            font.pixelSize: 28
            font.weight: Font.Bold
            Layout.alignment: Qt.AlignHCenter
        }

        Text {
            text: "placeholder — Phase 1+"
            color: Theme.textDim
            font.pixelSize: 14
            Layout.alignment: Qt.AlignHCenter
        }
    }
}
