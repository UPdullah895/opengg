import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings → Notifications. QML port of NotificationsSettings.vue.
ColumnLayout {
    id: root
    width: parent.width
    spacing: 20

    property var s: JSON.parse(SettingsController.settingsJson || "{}")
    property string style: s.notificationStyle || "auto"
    property string position: s.notificationPosition || "top-right"
    property real duration: s.notificationDuration || 5

    Connections {
        target: SettingsController
        function onSettingsJsonChanged() { root.s = JSON.parse(SettingsController.settingsJson || "{}") }
    }

    function setStyle(v) { SettingsController.setValue("notificationStyle", JSON.stringify(v)) }

    readonly property var styleOptions: [
        { value: "auto", key: "styleAuto" },
        { value: "gsr-notify", key: "styleGsrNotify" },
        { value: "x11-overlay", key: "styleX11Overlay" },
        { value: "system", key: "styleSystem" },
        { value: "disabled", key: "styleDisabled" },
    ]

    readonly property var positionOptions: [
        { value: "top-right", key: "positionTopRight" },
        { value: "top-left", key: "positionTopLeft" },
        { value: "bottom-right", key: "positionBottomRight" },
        { value: "bottom-left", key: "positionBottomLeft" },
    ]

    SettingsHeading { titleText: (I18n.language, I18n.t("settings.notificationsPage.title")) }

    // ── Style card ──
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: styleCol.implicitHeight + 40

        ColumnLayout {
            id: styleCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 14

            RowLayout {
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.notificationsPage.style"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                InfoIcon { tooltipText: I18n.t("settings.notificationsPage.description") }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            Flow {
                Layout.fillWidth: true
                spacing: 10

                Repeater {
                    model: root.styleOptions
                    Rectangle {
                        required property var modelData
                        width: 110; height: 60
                        radius: Theme.radius
                        color: root.style === modelData.value ? Theme.accentAlpha(12) : Theme.bg
                        border.width: 1
                        border.color: root.style === modelData.value ? Theme.accent : Theme.border

                        Text {
                            anchors.centerIn: parent
                            text: I18n.t("settings.notificationsPage." + modelData.key)
                            color: root.style === modelData.value ? Theme.accent : Theme.textDim
                            font.pixelSize: 12
                            width: parent.width - 12
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                        }

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.setStyle(modelData.value)
                        }
                    }
                }
            }
        }
    }

    // ── Position + duration card ──
    Rectangle {
        visible: root.style !== "disabled"
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: posCol.implicitHeight + 40

        ColumnLayout {
            id: posCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 16

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.notificationsPage.position"))
                    color: Theme.textDim
                    font.pixelSize: 14
                    Layout.fillWidth: true
                }
                InfoIcon { tooltipText: I18n.t("settings.notificationsPage.positionDesc") }

                ComboBox {
                    id: posCombo
                    Layout.preferredWidth: 180
                    model: root.positionOptions.map(o => I18n.t("settings.notificationsPage." + o.key))
                    currentIndex: root.positionOptions.findIndex(o => o.value === root.position)
                    onActivated: (index) => SettingsController.setValue(
                        "notificationPosition", JSON.stringify(root.positionOptions[index].value))

                    contentItem: Text {
                        leftPadding: 12
                        text: posCombo.displayText
                        color: Theme.text
                        font.pixelSize: 13
                        verticalAlignment: Text.AlignVCenter
                    }
                    background: Rectangle {
                        implicitHeight: 34
                        radius: Theme.radius
                        color: Theme.bg
                        border.width: 1
                        border.color: posCombo.activeFocus ? Theme.accent : Theme.border
                    }
                }
            }

            RowLayout {
                visible: root.style === "x11-overlay"
                Layout.fillWidth: true
                spacing: 8

                Text {
                    text: (I18n.language, I18n.t("settings.notificationsPage.duration"))
                    color: Theme.textDim
                    font.pixelSize: 14
                }
                InfoIcon { tooltipText: I18n.t("settings.notificationsPage.durationDesc") }

                Slider {
                    id: durSlider
                    Layout.fillWidth: true
                    from: 1; to: 10; stepSize: 1
                    value: root.duration
                    onMoved: SettingsController.setValue("notificationDuration", JSON.stringify(value))
                }
                Text {
                    text: Math.round(durSlider.value) + "s"
                    color: Theme.text
                    font.pixelSize: 13
                    Layout.preferredWidth: 28
                }
            }
        }
    }

    Component.onCompleted: SettingsController.refresh()
}
