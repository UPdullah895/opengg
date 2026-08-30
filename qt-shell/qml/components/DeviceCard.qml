import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// One device's card — image, name/model, headset live stats, mouse DPI/
// polling-rate controls, write-error feedback. Extracted out of
// DevicesPage.qml (Devices roadmap Phase 4) so the same card content can be
// reused unchanged across the List/Grid/Carousel view modes; callers set
// their own Layout.* sizing per view, this component owns none of its own.
Rectangle {
    id: root
    required property var modelData

    radius: Theme.radius
    color: Theme.surface
    border.width: 1
    border.color: Theme.border
    implicitHeight: card.implicitHeight + 32

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
    function hasCapability(cap) {
        return !!(root.modelData.capabilities && root.modelData.capabilities.indexOf(cap) !== -1)
    }
    function dpiOptions() {
        return (root.modelData.dpiOptions || []).map(d => ({ value: d, label: d + " DPI" }))
    }
    function pollingRateOptions() {
        return (root.modelData.pollingRateOptions || []).map(r => ({ value: r, label: r + " Hz" }))
    }

    RowLayout {
        id: card
        anchors.fill: parent
        anchors.margins: 16
        spacing: 16

        // Device image — Devices roadmap Phase 3: a bundled generic
        // silhouette resolved via opengg_core::device_assets (no real
        // per-model photo tier yet, see that module's doc comment for why).
        // Falls back to the small line-icon glyph whenever no image
        // resolves, so a card is never blank.
        Item {
            Layout.alignment: Qt.AlignTop
            Layout.preferredWidth: 56
            Layout.preferredHeight: 56

            Image {
                id: deviceImage
                anchors.fill: parent
                fillMode: Image.PreserveAspectFit
                source: DeviceController.imagePath(
                            root.modelData.vid, root.modelData.pid, root.modelData.deviceType)
                visible: source !== "" && status === Image.Ready
            }
            Icon {
                anchors.centerIn: parent
                name: root.iconFor(root.modelData.deviceType)
                size: 26
                color: Theme.textDim
                visible: !deviceImage.visible
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6

            Text {
                text: root.modelData.name
                color: Theme.text
                font.pixelSize: 16
                font.weight: Font.DemiBold
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
            Text {
                text: root.modelData.deviceType + " · " + root.modelData.model
                color: Theme.textDim
                font.pixelSize: 12
                Layout.fillWidth: true
                elide: Text.ElideRight
            }

            // Headset-specific live stats
            RowLayout {
                spacing: 18
                visible: root.modelData.deviceType === "headset"

                // Battery: icon + level, mirroring DeviceCard.vue's
                // ICON_BATTERY + "<n>%" pairing.
                Row {
                    visible: root.modelData.batteryLevel !== undefined
                    spacing: 5
                    Icon {
                        name: "battery"
                        size: 14
                        color: Theme.textDim
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Text {
                        text: root.modelData.batteryLevel >= 0
                              ? root.modelData.batteryLevel + "%"
                              : "n/a"
                        color: Theme.textDim
                        font.pixelSize: 12
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
                Text {
                    // No icon here: DeviceCard.vue renders chatmix as plain
                    // text, so the 🎚 glyph was invented.
                    visible: root.modelData.chatmix !== undefined
                    text: "chatmix " + root.modelData.chatmix
                    color: Theme.textDim
                    font.pixelSize: 12
                }
                Text {
                    // `visible: false` does NOT stop a `text` binding from
                    // evaluating, so the undefined guard has to live in the
                    // expression too.
                    visible: root.modelData.capabilities !== undefined
                    text: (root.modelData.capabilities ? root.modelData.capabilities.length : 0)
                          + " capabilities"
                    color: Theme.textDim
                    font.pixelSize: 12
                }
            }

            // Mouse controls — capability-gated (see root.hasCapability),
            // not deviceType-gated.
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 4
                spacing: 16
                visible: root.hasCapability("dpi") || root.hasCapability("polling_rate")

                ColumnLayout {
                    visible: root.hasCapability("dpi")
                    spacing: 4
                    Text { text: "DPI"; color: Theme.textDim; font.pixelSize: 11 }
                    SelectField {
                        Layout.preferredWidth: 140
                        options: root.dpiOptions()
                        value: root.modelData.dpi
                        onPicked: (v) => DeviceController.setDpi(root.modelData.id, v)
                    }
                }
                ColumnLayout {
                    visible: root.hasCapability("polling_rate")
                    spacing: 4
                    Text { text: "Polling Rate"; color: Theme.textDim; font.pixelSize: 11 }
                    SelectField {
                        Layout.preferredWidth: 140
                        options: root.pollingRateOptions()
                        value: root.modelData.pollingRate
                        onPicked: (v) => DeviceController.setPollingRate(root.modelData.id, v)
                    }
                }
            }

            // Write-failure feedback for this specific card — keyed by
            // device id so two cards mid-write at once don't show each
            // other's error.
            Text {
                visible: DeviceController.lastErrorDeviceId === root.modelData.id
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
