import QtQuick
import QtQuick.Controls
import com.opengg.app

// Compact recorder status + controls for the Clips toolbar — port of
// RecordingDropdown.vue. Shows the replay buffer's live state (idle /
// recording) and opens a small menu with start, save-clip and stop.
//
// All state and actions come from the existing RecordingController; this is
// purely a second, more convenient entry point to what the Home page's
// recording card already drives.
Item {
    id: rec

    property bool menuOpen: false

    implicitWidth: pill.width
    implicitHeight: 32

    Component.onCompleted: RecordingController.refresh()

    // The controller only refreshes on demand, so poll while the Clips page is
    // up — the recorder can also be started from the tray, the Home card or a
    // global shortcut, and the pill must not go stale when it is.
    Timer {
        interval: 2000
        running: rec.visible
        repeat: true
        onTriggered: RecordingController.refresh()
    }

    Rectangle {
        id: pill
        width: row.implicitWidth + 20
        height: 32
        radius: Theme.radius
        color: pillArea.containsMouse || rec.menuOpen ? Theme.bgHover : Theme.surface
        border.width: 1
        border.color: RecordingController.running ? Theme.danger : Theme.border

        Row {
            id: row
            anchors.centerIn: parent
            spacing: 7

            // Pulses while the buffer is live, so "recording" reads at a glance.
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 8; height: 8; radius: 4
                color: RecordingController.running ? Theme.danger : Theme.textMuted

                SequentialAnimation on opacity {
                    running: RecordingController.running
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.25; duration: 700; easing.type: Easing.InOutQuad }
                    NumberAnimation { to: 1.0;  duration: 700; easing.type: Easing.InOutQuad }
                }
                // Leaving the loop mid-fade would strand it dim.
                onOpacityChanged: if (!RecordingController.running) opacity = 1
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: RecordingController.running ? "Recording" : "Idle"
                color: RecordingController.running ? Theme.danger : Theme.textDim
                font.pixelSize: 12
                font.weight: Font.DemiBold
            }

            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: rec.menuOpen ? "chevron-up" : "chevron-down"
                size: 11
                color: Theme.textMuted
            }
        }

        MouseArea {
            id: pillArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: rec.menuOpen = !rec.menuOpen
        }
    }

    // ── Menu ──────────────────────────────────────────────────────────────
    // Plain positioned Rectangle rather than a QQC2 Popup, matching the
    // established choice everywhere else in this shell.
    Rectangle {
        id: menu
        visible: rec.menuOpen
        y: pill.height + 4
        width: 188
        height: menuCol.implicitHeight + 10
        z: 60
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border

        Column {
            id: menuCol
            anchors.centerIn: parent
            width: parent.width - 8
            spacing: 0

            Text {
                width: parent.width
                leftPadding: 8
                topPadding: 4
                bottomPadding: 4
                visible: RecordingController.error.length > 0
                text: RecordingController.error
                color: Theme.danger
                font.pixelSize: 11
                wrapMode: Text.WordWrap
            }

            Text {
                width: parent.width
                leftPadding: 8
                topPadding: 2
                bottomPadding: 6
                text: RecordingController.statusText
                color: Theme.textMuted
                font.pixelSize: 11
                elide: Text.ElideRight
            }

            Rectangle { width: parent.width; height: 1; color: Theme.border }

            Repeater {
                model: [
                    { key: "start", icon: "record", label: "Start replay buffer",
                      show: !RecordingController.running },
                    { key: "save",  icon: "video",  label: "Save clip",
                      show: RecordingController.running },
                    { key: "stop",  icon: "square", label: "Stop",
                      show: RecordingController.running, danger: true }
                ]

                Rectangle {
                    required property var modelData
                    width: menuCol.width
                    height: modelData.show ? 30 : 0
                    visible: modelData.show
                    radius: 4
                    color: itemArea.containsMouse
                           ? (modelData.danger ? Theme.tint(Theme.danger, 12) : Theme.accentAlpha(12))
                           : "transparent"

                    Row {
                        anchors.left: parent.left
                        anchors.leftMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8

                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: modelData.icon
                            size: 13
                            color: modelData.danger ? Theme.danger
                                 : itemArea.containsMouse ? Theme.accent : Theme.textDim
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData.label
                            color: modelData.danger ? Theme.danger : Theme.text
                            font.pixelSize: 12
                        }
                    }

                    MouseArea {
                        id: itemArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            switch (modelData.key) {
                            case "start": RecordingController.start(); break
                            case "save":  RecordingController.save(); break
                            case "stop":  RecordingController.stop(); break
                            }
                            rec.menuOpen = false
                        }
                    }
                }
            }
        }
    }
}
