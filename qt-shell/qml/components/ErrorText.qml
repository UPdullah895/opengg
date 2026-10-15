import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// An error message the user can select and copy.
//
// Errors were plain Text, so the only way to report one was a screenshot —
// and text in a screenshot cannot be pasted into a search box or an issue.
// Hidden while empty, so it can sit in a layout unconditionally.
RowLayout {
    id: err

    property string text: ""
    property color color: Theme.danger
    property int pixelSize: 11

    visible: err.text.length > 0
    spacing: 6

    TextEdit {
        Layout.fillWidth: true
        text: err.text
        readOnly: true
        selectByMouse: true
        wrapMode: TextEdit.Wrap
        color: err.color
        selectionColor: Theme.accentAlpha(45)
        selectedTextColor: Theme.text
        font.pixelSize: err.pixelSize
    }

    Rectangle {
        id: copyBtn
        property bool copied: false
        Layout.alignment: Qt.AlignTop
        Layout.preferredWidth: 22
        Layout.preferredHeight: 22
        radius: Theme.radius
        color: copyArea.containsMouse ? Theme.bgHover : "transparent"
        Icon {
            anchors.centerIn: parent
            name: copyBtn.copied ? "check" : "copy"
            size: 12
            color: copyBtn.copied ? Theme.success : Theme.textDim
        }
        MouseArea {
            id: copyArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                SystemController.writeClipboard(err.text)
                copyBtn.copied = true
                copiedTimer.restart()
            }
        }
        Tip {
            visible: copyArea.containsMouse
            text: (I18n.language, I18n.t("common.copy"))
        }
        Timer { id: copiedTimer; interval: 1500; onTriggered: copyBtn.copied = false }
    }
}
