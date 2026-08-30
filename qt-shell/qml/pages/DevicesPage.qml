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

    // DeviceCard.vue only branches headset vs. everything else, so there is
    // no separate keyboard/gamepad glyph in the original to port.
    function iconFor(type) {
        return type === "headset" ? "headphones" : "mouse"
    }

    // Capability-gated, not device-type-gated (Devices Phase 2, roadmap §2's
    // rule): checks "does this device report a dpi/polling_rate capability"
    // rather than "is this a mouse" — a mouse ratbagd can't currently read
    // resolution data for has no capabilities at all and correctly shows no
    // control, and a future non-mouse device reporting the same capability
    // would get the same control with no new branch needed here.
    function hasCapability(modelData, cap) {
        return !!(modelData.capabilities && modelData.capabilities.indexOf(cap) !== -1)
    }
    function dpiOptionsFor(modelData) {
        return (modelData.dpiOptions || []).map(d => ({ value: d, label: d + " DPI" }))
    }
    function pollingRateOptionsFor(modelData) {
        return (modelData.pollingRateOptions || []).map(r => ({ value: r, label: r + " Hz" }))
    }

    ScrollView {
        id: devicesScroll
        anchors.fill: parent
        contentWidth: availableWidth
        // See HomePage.qml's ScrollView for why this is explicit: QQC2's
        // automatic contentHeight inference doesn't reliably track a
        // ColumnLayout child positioned with an explicit x/y offset, so the
        // page could look like it has nothing to scroll even when the
        // device list overflows the window.
        contentHeight: devicesCol.implicitHeight + devicesCol.y * 2

        ColumnLayout {
            id: devicesCol
            width: Math.min(parent.width, 760)
            x: 32
            y: 28
            spacing: 20

            RowLayout {
                Layout.fillWidth: true
                Text {
                    horizontalAlignment: Text.AlignLeft
                    text: (I18n.language, I18n.t("nav.devices"))
                    color: Theme.text
                    font.pixelSize: 26
                    font.weight: Font.Bold
                    Layout.fillWidth: true
                }
                Rectangle {
                    width: 8; height: 8; radius: 4
                    color: DeviceController.connected ? Theme.success : Theme.danger
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

                        // Device image — Devices roadmap Phase 3: a bundled
                        // generic silhouette resolved via
                        // opengg_core::device_assets (no real per-model
                        // photo tier yet, see that module's doc comment for
                        // why). Falls back to the small line-icon glyph
                        // (unchanged from before this phase) whenever no
                        // image resolves, so a card is never blank.
                        Item {
                            Layout.alignment: Qt.AlignTop
                            Layout.preferredWidth: 56
                            Layout.preferredHeight: 56

                            Image {
                                id: deviceImage
                                anchors.fill: parent
                                fillMode: Image.PreserveAspectFit
                                source: DeviceController.imagePath(
                                            modelData.vid, modelData.pid, modelData.deviceType)
                                visible: source !== "" && status === Image.Ready
                            }
                            Icon {
                                anchors.centerIn: parent
                                name: page.iconFor(modelData.deviceType)
                                size: 26
                                color: Theme.textDim
                                visible: !deviceImage.visible
                            }
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

                                // Battery: icon + level, mirroring DeviceCard.vue's
                                // ICON_BATTERY + "<n>%" pairing.
                                Row {
                                    visible: modelData.batteryLevel !== undefined
                                    spacing: 5
                                    Icon {
                                        name: "battery"
                                        size: 14
                                        color: Theme.textDim
                                        anchors.verticalCenter: parent.verticalCenter
                                    }
                                    Text {
                                        text: modelData.batteryLevel >= 0
                                              ? modelData.batteryLevel + "%"
                                              : "n/a"
                                        color: Theme.textDim
                                        font.pixelSize: 12
                                        anchors.verticalCenter: parent.verticalCenter
                                    }
                                }
                                Text {
                                    // No icon here: DeviceCard.vue renders chatmix
                                    // as plain text, so the 🎚 glyph was invented.
                                    visible: modelData.chatmix !== undefined
                                    text: "chatmix " + modelData.chatmix
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

                            // Mouse controls — capability-gated (see
                            // page.hasCapability), not deviceType-gated.
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.topMargin: 4
                                spacing: 16
                                visible: page.hasCapability(modelData, "dpi")
                                         || page.hasCapability(modelData, "polling_rate")

                                ColumnLayout {
                                    visible: page.hasCapability(modelData, "dpi")
                                    spacing: 4
                                    Text { text: "DPI"; color: Theme.textDim; font.pixelSize: 11 }
                                    SelectField {
                                        Layout.preferredWidth: 140
                                        options: page.dpiOptionsFor(modelData)
                                        value: modelData.dpi
                                        onPicked: (v) => DeviceController.setDpi(modelData.id, v)
                                    }
                                }
                                ColumnLayout {
                                    visible: page.hasCapability(modelData, "polling_rate")
                                    spacing: 4
                                    Text { text: "Polling Rate"; color: Theme.textDim; font.pixelSize: 11 }
                                    SelectField {
                                        Layout.preferredWidth: 140
                                        options: page.pollingRateOptionsFor(modelData)
                                        value: modelData.pollingRate
                                        onPicked: (v) => DeviceController.setPollingRate(modelData.id, v)
                                    }
                                }
                            }

                            // Write-failure feedback for this specific card —
                            // keyed by device id so two cards mid-write at
                            // once don't show each other's error.
                            Text {
                                visible: DeviceController.lastErrorDeviceId === modelData.id
                                         && DeviceController.lastError !== ""
                                text: DeviceController.lastError
                                color: Theme.danger
                                font.pixelSize: 11
                                Layout.fillWidth: true
                                wrapMode: Text.WordWrap
                            }
                        }
                    }
                }
            }

            Text {
                visible: page.devices.length === 0
                text: (I18n.language, I18n.t("devices.noDevices"))
                color: Theme.textDim
                font.pixelSize: 13
            }
        }
    }
    // Above the ScrollView, not inside it — see WheelScroller.qml.
    Item {
        anchors.fill: devicesScroll
        WheelScroller { anchors.fill: parent; flick: devicesScroll.contentItem }
    }

}
