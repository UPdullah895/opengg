import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Devices — live device list from the openggd daemon (real D-Bus data, read-only).
Rectangle {
    id: page
    color: Theme.bg

    property var devices: DeviceController.devicesJson
        ? JSON.parse(DeviceController.devicesJson)
        : []

    Component.onCompleted: {
        DeviceController.refresh()
        TourController.registerTarget("devices-list", page)
    }
    Component.onDestruction: TourController.unregisterTarget("devices-list")
    Timer { interval: 3000; running: true; repeat: true; onTriggered: DeviceController.refresh() }

    function iconFor(type) {
        return type === "mouse" ? "🖱️" : type === "headset" ? "🎧"
             : type === "keyboard" ? "⌨️" : "🎮"
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: Math.min(parent.width, 760)
            x: 32
            y: 28
            spacing: 20

            RowLayout {
                Layout.fillWidth: true
                Text {
                    text: (I18n.language, I18n.t("nav.devices"))
                    color: Theme.text
                    font.pixelSize: 26
                    font.weight: Font.Bold
                    Layout.fillWidth: true
                }
                Rectangle {
                    width: 8; height: 8; radius: 4
                    color: DeviceController.connected ? "#22c55e" : "#ef4444"
                    Layout.alignment: Qt.AlignVCenter
                }
                Text {
                    text: DeviceController.connected ? "daemon connected" : "daemon offline"
                    color: Theme.textDim
                    font.pixelSize: 12
                }
            }

            Repeater {
                model: page.devices

                // Device card
                Rectangle {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredWidth: 680
                    radius: Theme.radius
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.border
                    implicitHeight: card.implicitHeight + 32

                    RowLayout {
                        id: card
                        anchors.fill: parent
                        anchors.margins: 16
                        spacing: 16

                        Text {
                            text: page.iconFor(modelData.deviceType)
                            font.pixelSize: 30
                            Layout.alignment: Qt.AlignTop
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 6

                            Text {
                                text: modelData.name
                                color: Theme.text
                                font.pixelSize: 16
                                font.weight: Font.DemiBold
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                            }
                            Text {
                                text: modelData.deviceType + " · " + modelData.model
                                color: Theme.textDim
                                font.pixelSize: 12
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                            }

                            // Headset-specific live stats
                            RowLayout {
                                spacing: 18
                                visible: modelData.deviceType === "headset"

                                Text {
                                    visible: modelData.batteryLevel !== undefined
                                    text: "🔋 " + (modelData.batteryLevel >= 0
                                        ? modelData.batteryLevel + "%"
                                        : "n/a")
                                    color: Theme.textDim
                                    font.pixelSize: 12
                                }
                                Text {
                                    visible: modelData.chatmix !== undefined
                                    text: "🎚 chatmix " + modelData.chatmix
                                    color: Theme.textDim
                                    font.pixelSize: 12
                                }
                                Text {
                                    // `visible: false` does NOT stop a `text`
                                    // binding from evaluating, so the undefined
                                    // guard has to live in the expression too.
                                    visible: modelData.capabilities !== undefined
                                    text: (modelData.capabilities ? modelData.capabilities.length : 0)
                                          + " capabilities"
                                    color: Theme.textDim
                                    font.pixelSize: 12
                                }
                            }
                        }
                    }
                }
            }

            Text {
                visible: page.devices.length === 0
                text: "No devices detected."
                color: Theme.textDim
                font.pixelSize: 13
            }
        }
    }
}
