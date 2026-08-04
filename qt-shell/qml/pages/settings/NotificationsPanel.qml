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
    SettingsCard {
        title: (I18n.language, I18n.t("settings.notificationsPage.style"))
        infoText: I18n.t("settings.notificationsPage.description")

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

    // ── Position + duration card ── previously a bare, unstyled "Position"
    // label (14px, not bold) next to a raw ComboBox — the only field card in
    // Settings without a proper section title, and the only place still
    // using a plain ComboBox instead of the SelectField + uppercase-label
    // pattern Clip Preferences established.
    SettingsCard {
        visible: root.style !== "disabled"
        title: (I18n.language, I18n.t("settings.notificationsPage.display"))

        RowLayout {
            Layout.fillWidth: true
            spacing: 24

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 6
                RowLayout {
                    spacing: 6
                    Text {
                        text: (I18n.language, I18n.t("settings.notificationsPage.position").toUpperCase())
                        color: Theme.textMuted
                        font.pixelSize: 11
                        font.weight: Font.Bold
                        font.letterSpacing: 0.5
                    }
                    InfoIcon { tooltipText: I18n.t("settings.notificationsPage.positionDesc") }
                }
                SelectField {
                    Layout.fillWidth: true
                    options: root.positionOptions.map(o => ({ value: o.value, label: I18n.t("settings.notificationsPage." + o.key) }))
                    value: root.position
                    onPicked: (v) => SettingsController.setValue("notificationPosition", JSON.stringify(v))
                }
            }

            ColumnLayout {
                visible: root.style === "x11-overlay"
                Layout.fillWidth: true
                spacing: 6
                RowLayout {
                    spacing: 6
                    Text {
                        text: (I18n.language, I18n.t("settings.notificationsPage.duration").toUpperCase())
                        color: Theme.textMuted
                        font.pixelSize: 11
                        font.weight: Font.Bold
                        font.letterSpacing: 0.5
                    }
                    InfoIcon { tooltipText: I18n.t("settings.notificationsPage.durationDesc") }
                }
                HSlider {
                    Layout.fillWidth: true
                    from: 1; to: 10
                    value: root.duration
                    suffix: "s"
                    onMoved: (v) => SettingsController.setValue("notificationDuration", JSON.stringify(Math.round(v)))
                }
            }
        }
    }

    Component.onCompleted: SettingsController.refresh()
}
