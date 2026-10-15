import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// A command the user has to run themselves, with a Copy button next to it.
//
// Commands used to be shown as plain Text, which cannot be selected, so
// users had to retype them — and for a long `sudo usermod …` line that is
// exactly where typos creep in. The text is selectable too, for anyone who
// wants only part of it.
RowLayout {
    id: cmd

    property string text: ""
    /// Font size for the command itself.
    property int pixelSize: 11

    spacing: 6

    Rectangle {
        Layout.fillWidth: true
        implicitHeight: cmdEdit.implicitHeight + 12
        radius: Theme.radius
        color: Theme.bg
        border.width: 1
        border.color: Theme.border

        TextEdit {
            id: cmdEdit
            anchors.fill: parent
            anchors.margins: 6
            // Commands are LTR even inside an Arabic sentence.
            LayoutMirroring.enabled: false
            horizontalAlignment: TextEdit.AlignLeft
            text: cmd.text
            readOnly: true
            selectByMouse: true
            wrapMode: TextEdit.WrapAnywhere
            color: Theme.text
            selectionColor: Theme.accentAlpha(45)
            selectedTextColor: Theme.text
            font.pixelSize: cmd.pixelSize
            font.family: "monospace"
        }
    }

    Rectangle {
        id: copyBtn
        property bool copied: false
        Layout.alignment: Qt.AlignTop
        Layout.preferredWidth: Math.max(56, copyLabel.implicitWidth + 16)
        Layout.preferredHeight: 24
        radius: Theme.radius
        color: copyArea.containsMouse ? Theme.bgHover : Theme.surface
        border.width: 1
        border.color: copyBtn.copied ? Theme.success : Theme.border

        Row {
            anchors.centerIn: parent
            spacing: 4
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: copyBtn.copied ? "check" : "copy"
                size: 11
                color: copyBtn.copied ? Theme.success : Theme.textDim
            }
            Text {
                id: copyLabel
                anchors.verticalCenter: parent.verticalCenter
                text: copyBtn.copied ? (I18n.language, I18n.t("settings.captureGsr.copied"))
                                     : (I18n.language, I18n.t("common.copy"))
                color: copyBtn.copied ? Theme.success : Theme.textDim
                font.pixelSize: 10
            }
        }
        MouseArea {
            id: copyArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                SystemController.writeClipboard(cmd.text)
                copyBtn.copied = true
                copiedTimer.restart()
            }
        }
        Timer { id: copiedTimer; interval: 1500; onTriggered: copyBtn.copied = false }
    }
}
