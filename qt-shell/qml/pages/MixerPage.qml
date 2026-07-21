import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Mixer — full channel-strip view over live daemon audio (reuses AudioController).
Rectangle {
    id: page
    color: Theme.bg

    property var channels: AudioController.channelsJson
        ? JSON.parse(AudioController.channelsJson)
        : []

    Component.onCompleted: AudioController.refresh()
    Timer { interval: 2000; running: true; repeat: true; onTriggered: AudioController.refresh() }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 28
        spacing: 20

        RowLayout {
            Layout.fillWidth: true
            Text {
                text: (I18n.language, I18n.t("nav.mixer"))
                color: Theme.text
                font.pixelSize: 26
                font.weight: Font.Bold
                Layout.fillWidth: true
            }
            Rectangle {
                width: 8; height: 8; radius: 4
                color: AudioController.connected ? "#22c55e" : "#ef4444"
                Layout.alignment: Qt.AlignVCenter
            }
            Text {
                text: AudioController.connected ? "daemon connected" : "daemon offline"
                color: Theme.textDim
                font.pixelSize: 12
            }
        }

        // Row of channel strips
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.maximumHeight: 420
            spacing: 16

            Repeater {
                model: page.channels

                // One channel strip
                Rectangle {
                    required property var modelData
                    Layout.preferredWidth: 130
                    Layout.fillHeight: true
                    radius: Theme.radius
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.border

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 14
                        spacing: 10

                        // Channel name
                        Text {
                            text: modelData.name
                            color: Theme.text
                            font.pixelSize: 15
                            font.weight: Font.DemiBold
                            Layout.alignment: Qt.AlignHCenter
                        }

                        // Volume %
                        Text {
                            text: Math.round(modelData.volume) + "%"
                            color: Theme.textDim
                            font.pixelSize: 12
                            Layout.alignment: Qt.AlignHCenter
                        }

                        // Vertical fader
                        Slider {
                            id: fader
                            orientation: Qt.Vertical
                            Layout.alignment: Qt.AlignHCenter
                            Layout.fillHeight: true
                            from: 0; to: 100
                            value: modelData.volume
                            onMoved: AudioController.setVolume(modelData.name, Math.round(value))

                            background: Rectangle {
                                x: fader.leftPadding + fader.availableWidth / 2 - width / 2
                                y: fader.topPadding
                                width: 6
                                height: fader.availableHeight
                                radius: 3
                                color: Theme.border
                                Rectangle {
                                    // vertical slider: visualPosition is 0 at the top (max),
                                    // so the filled level from the bottom is (1 - visualPosition).
                                    width: parent.width
                                    height: (1 - fader.visualPosition) * parent.height
                                    y: parent.height - height
                                    radius: 3
                                    color: Theme.accent
                                }
                            }
                            handle: Rectangle {
                                x: fader.leftPadding + fader.availableWidth / 2 - width / 2
                                y: fader.topPadding + fader.visualPosition * (fader.availableHeight - height)
                                width: 20; height: 20; radius: 10
                                color: Theme.text
                                border.width: 2
                                border.color: Theme.accent
                            }
                        }

                        // Mute
                        Rectangle {
                            Layout.alignment: Qt.AlignHCenter
                            width: 40; height: 30
                            radius: Theme.radius
                            color: modelData.muted ? Qt.rgba(0.914, 0.271, 0.376, 0.15) : "transparent"
                            border.width: 1
                            border.color: modelData.muted ? Theme.accent : Theme.border
                            Text {
                                anchors.centerIn: parent
                                text: modelData.muted ? "🔇" : "🔊"
                                font.pixelSize: 13
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: AudioController.setMute(modelData.name, !modelData.muted)
                            }
                        }

                        // Routed apps
                        Text {
                            text: modelData.apps && modelData.apps.length > 0
                                ? modelData.apps.map(a => a.name).join(", ")
                                : "—"
                            color: Theme.textDim
                            font.pixelSize: 10
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                        }
                    }
                }
            }

            Item { Layout.fillWidth: true } // push strips to the leading edge
        }
    }
}
