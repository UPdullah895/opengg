import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Home — live audio channels from the openggd daemon (real D-Bus data).
Rectangle {
    id: page
    color: Theme.bg

    // Re-parses whenever the daemon pushes new JSON (NOTIFYing property).
    property var channels: AudioController.channelsJson
        ? JSON.parse(AudioController.channelsJson)
        : []

    Component.onCompleted: AudioController.refresh()

    // Light polling so external volume changes show up live.
    Timer {
        interval: 2000
        running: true
        repeat: true
        onTriggered: AudioController.refresh()
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: Math.min(parent.width, 760)
            x: 32
            y: 28
            spacing: 20

            Text {
                text: (I18n.language, I18n.t("nav.home"))
                color: Theme.text
                font.pixelSize: 26
                font.weight: Font.Bold
            }

            // ── Audio channels card ────────────────────────────────────────
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredWidth: 680
                radius: Theme.radius
                color: Theme.surface
                border.width: 1
                border.color: Theme.border
                implicitHeight: mixCol.implicitHeight + 40

                ColumnLayout {
                    id: mixCol
                    anchors.fill: parent
                    anchors.margins: 20
                    spacing: 16

                    RowLayout {
                        Layout.fillWidth: true
                        Text {
                            text: (I18n.language, I18n.t("nav.mixer"))
                            color: Theme.text
                            font.pixelSize: 18
                            font.weight: Font.DemiBold
                            Layout.fillWidth: true
                        }
                        // Live connection dot
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

                    // One row per channel
                    Repeater {
                        model: page.channels

                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 14

                            Text {
                                text: modelData.name
                                color: Theme.text
                                font.pixelSize: 14
                                Layout.preferredWidth: 70
                            }

                            // Mute toggle
                            Rectangle {
                                width: 32; height: 28
                                radius: Theme.radius
                                color: modelData.muted ? Qt.rgba(0.914, 0.271, 0.376, 0.15) : "transparent"
                                border.width: 1
                                border.color: modelData.muted ? Theme.accent : Theme.border
                                Text {
                                    anchors.centerIn: parent
                                    text: modelData.muted ? "🔇" : "🔊"
                                    font.pixelSize: 12
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: AudioController.setMute(modelData.name, !modelData.muted)
                                }
                            }

                            Slider {
                                id: chSlider
                                Layout.fillWidth: true
                                from: 0; to: 100
                                value: modelData.volume
                                onMoved: AudioController.setVolume(modelData.name, Math.round(value))

                                background: Rectangle {
                                    x: chSlider.leftPadding
                                    y: chSlider.topPadding + chSlider.availableHeight / 2 - height / 2
                                    width: chSlider.availableWidth
                                    height: 4
                                    radius: 2
                                    color: Theme.border
                                    Rectangle {
                                        width: parent.width * chSlider.visualPosition
                                        height: parent.height
                                        radius: 2
                                        color: Theme.accent
                                    }
                                }
                                handle: Rectangle {
                                    x: chSlider.leftPadding + chSlider.visualPosition * (chSlider.availableWidth - width)
                                    y: chSlider.topPadding + chSlider.availableHeight / 2 - height / 2
                                    width: 16; height: 16; radius: 8
                                    color: Theme.text
                                    border.width: 2
                                    border.color: Theme.accent
                                }
                            }

                            Text {
                                text: Math.round(modelData.volume) + "%"
                                color: Theme.textDim
                                font.pixelSize: 13
                                Layout.preferredWidth: 40
                                horizontalAlignment: Text.AlignRight
                            }
                        }
                    }

                    Text {
                        visible: page.channels.length === 0
                        text: "No channels — is openggd running?"
                        color: Theme.textDim
                        font.pixelSize: 13
                    }
                }
            }
        }
    }
}
