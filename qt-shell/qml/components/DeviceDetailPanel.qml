import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// The selected device's detail/settings surface — Devices regression pass.
//
// Everything that used to be crammed into the list card lives here: DPI and
// polling-rate controls, headset live stats, the button-mapping entry point,
// and the raw identifiers behind an "Advanced" disclosure. This is also the
// surface Phase 5's button-mapping photo editor is meant to grow into, which
// is why it is a real component rather than more inlining into the list.
//
// `device` is one entry of DevicesPage's parsed device list, or null when
// nothing is selected.
Rectangle {
    id: panel

    property var device: null
    /// Raised when the user asks to open the button-mapping editor; the page
    /// owns the single shared editor instance, not this panel.
    signal configureButtonsRequested()

    property bool showAdvanced: false

    radius: Theme.radius
    color: Theme.surface
    border.width: 1
    border.color: Theme.border

    readonly property string heroSource: {
        if (!panel.device) return ""
        var p = DeviceController.imagePath(panel.device.vid, panel.device.pid,
                                           panel.device.deviceType)
        return p === "" ? "" : p + "?v=" + DeviceController.photoRevision
    }

    function hasCapability(cap) {
        return !!(panel.device && panel.device.capabilities
                  && panel.device.capabilities.indexOf(cap) !== -1)
    }
    function dpiOptions() {
        return ((panel.device && panel.device.dpiOptions) || []).map(d => ({ value: d, label: d + " DPI" }))
    }
    function pollingRateOptions() {
        return ((panel.device && panel.device.pollingRateOptions) || []).map(r => ({ value: r, label: r + " Hz" }))
    }

    // ── Empty state ─────────────────────────────────────────────────────────
    ColumnLayout {
        anchors.centerIn: parent
        width: parent.width - 80
        spacing: 10
        visible: panel.device === null

        Icon {
            Layout.alignment: Qt.AlignHCenter
            name: "mouse"
            size: 34
            color: Theme.textMuted
        }
        Text {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: "Select a device"
            color: Theme.textDim
            font.pixelSize: 15
            font.weight: Font.DemiBold
        }
        Text {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            text: "Choose a device from the list to see its settings."
            color: Theme.textMuted
            font.pixelSize: 12
        }
    }

    // ── Detail content ──────────────────────────────────────────────────────
    ScrollView {
        id: detailScroll
        anchors.fill: parent
        anchors.margins: 1
        contentWidth: availableWidth
        // Explicit, for the same reason HomePage/DevicesPage set it: QQC2
        // does not reliably infer contentHeight from a ColumnLayout child
        // placed at an explicit offset.
        contentHeight: body.implicitHeight + 48
        visible: panel.device !== null
        clip: true

        ColumnLayout {
            id: body
            width: detailScroll.availableWidth - 48
            x: 24
            y: 24
            spacing: 18

            // ── Header ──────────────────────────────────────────────────────
            RowLayout {
                Layout.fillWidth: true
                spacing: 16

                Item {
                    Layout.preferredWidth: 72
                    Layout.preferredHeight: 72
                    Layout.alignment: Qt.AlignTop

                    Image {
                        id: heroImage
                        anchors.fill: parent
                        fillMode: Image.PreserveAspectFit
                        cache: false
                        // Same empty-path guard as DeviceCard: a bare
                        // "?v=0" would be resolved against this .qml file
                        // and decoded as an image.
                        source: panel.heroSource
                        visible: source !== "" && status === Image.Ready
                    }
                    Icon {
                        anchors.centerIn: parent
                        name: panel.device && panel.device.deviceType === "headset"
                              ? "headphones" : "mouse"
                        size: 34
                        color: Theme.textDim
                        visible: !heroImage.visible
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 6

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        Text {
                            text: panel.device ? DeviceController.displayName(panel.device.name) : ""
                            color: Theme.text
                            font.pixelSize: 20
                            font.weight: Font.Bold
                            elide: Text.ElideRight
                            maximumLineCount: 1
                            Layout.fillWidth: true
                        }
                        ConnectionBadge {
                            connection: (panel.device && panel.device.connection) || ""
                            Layout.alignment: Qt.AlignVCenter
                        }
                    }

                    Text {
                        text: panel.device ? panel.device.deviceType : ""
                        color: Theme.textDim
                        font.pixelSize: 12
                        Layout.fillWidth: true
                    }
                }
            }

            // ── Headset live stats ──────────────────────────────────────────
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 10
                visible: panel.device && panel.device.deviceType === "headset"

                Text {
                    text: "Status"
                    color: Theme.textDim
                    font.pixelSize: 11
                    font.weight: Font.DemiBold
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 24

                    Row {
                        spacing: 6
                        visible: panel.device && panel.device.batteryLevel !== undefined
                        Icon {
                            name: "battery"
                            size: 15
                            color: Theme.textDim
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        Text {
                            text: panel.device && panel.device.batteryLevel >= 0
                                  ? panel.device.batteryLevel + "%" : "n/a"
                            color: Theme.text
                            font.pixelSize: 13
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }
                    Text {
                        visible: panel.device && panel.device.chatmix !== undefined
                        text: "chatmix " + (panel.device ? panel.device.chatmix : "")
                        color: Theme.text
                        font.pixelSize: 13
                    }
                }
            }

            // ── Mouse controls ──────────────────────────────────────────────
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 10
                visible: panel.hasCapability("dpi") || panel.hasCapability("polling_rate")

                Text {
                    text: "Sensor"
                    color: Theme.textDim
                    font.pixelSize: 11
                    font.weight: Font.DemiBold
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 20

                    ColumnLayout {
                        visible: panel.hasCapability("dpi")
                        spacing: 5
                        Text { text: "DPI"; color: Theme.textMuted; font.pixelSize: 11 }
                        SelectField {
                            Layout.preferredWidth: 160
                            options: panel.dpiOptions()
                            value: panel.device ? panel.device.dpi : null
                            onPicked: (v) => DeviceController.setDpi(panel.device.id, v)
                        }
                    }
                    ColumnLayout {
                        visible: panel.hasCapability("polling_rate")
                        spacing: 5
                        Text { text: "Polling Rate"; color: Theme.textMuted; font.pixelSize: 11 }
                        SelectField {
                            Layout.preferredWidth: 160
                            options: panel.pollingRateOptions()
                            value: panel.device ? panel.device.pollingRate : null
                            onPicked: (v) => DeviceController.setPollingRate(panel.device.id, v)
                        }
                    }
                    Item { Layout.fillWidth: true }
                }
            }

            // ── Buttons ─────────────────────────────────────────────────────
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 10
                visible: panel.hasCapability("buttons")

                Text {
                    text: "Buttons"
                    color: Theme.textDim
                    font.pixelSize: 11
                    font.weight: Font.DemiBold
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 12
                    ClipsBarButton {
                        label: "Configure Buttons"
                        icon: "gear"
                        onTriggered: panel.configureButtonsRequested()
                    }
                    Text {
                        text: panel.device && panel.device.buttonCount
                              ? panel.device.buttonCount + " buttons" : ""
                        color: Theme.textMuted
                        font.pixelSize: 12
                        Layout.alignment: Qt.AlignVCenter
                    }
                    Item { Layout.fillWidth: true }
                }
            }

            // ── Write-failure feedback, keyed to this device ─────────────────
            Text {
                Layout.fillWidth: true
                visible: panel.device
                         && DeviceController.lastErrorDeviceId === panel.device.id
                         && DeviceController.lastError !== ""
                text: DeviceController.lastError
                color: Theme.danger
                font.pixelSize: 12
                wrapMode: Text.WordWrap
            }

            // ── Advanced disclosure ─────────────────────────────────────────
            // The raw `usb:vid:pid` model string and the internal device id
            // are diagnostics, not user-facing information — they sit behind
            // this toggle rather than under every device by default.
            ColumnLayout {
                Layout.fillWidth: true
                Layout.topMargin: 4
                spacing: 8

                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 1
                    color: Theme.border
                }

                Item {
                    Layout.fillWidth: true
                    implicitHeight: 22

                    Row {
                        id: advancedToggle
                        spacing: 6
                        Icon {
                            name: panel.showAdvanced ? "chevron-up" : "chevron-down"
                            size: 12
                            color: Theme.textMuted
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        Text {
                            text: "Advanced"
                            color: Theme.textMuted
                            font.pixelSize: 11
                            font.weight: Font.DemiBold
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }
                    TapHandler { onTapped: panel.showAdvanced = !panel.showAdvanced }
                }

                GridLayout {
                    Layout.fillWidth: true
                    visible: panel.showAdvanced
                    columns: 2
                    columnSpacing: 16
                    rowSpacing: 6

                    Text { text: "Model"; color: Theme.textMuted; font.pixelSize: 11 }
                    Text {
                        text: panel.device ? panel.device.model : ""
                        color: Theme.textDim
                        font.pixelSize: 11
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Text { text: "Full name"; color: Theme.textMuted; font.pixelSize: 11 }
                    Text {
                        text: panel.device ? panel.device.name : ""
                        color: Theme.textDim
                        font.pixelSize: 11
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Text { text: "Device id"; color: Theme.textMuted; font.pixelSize: 11 }
                    Text {
                        text: panel.device ? panel.device.id : ""
                        color: Theme.textDim
                        font.pixelSize: 11
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }
}
