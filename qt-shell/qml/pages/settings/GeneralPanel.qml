import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings → General. QML port of GeneralSettings.vue.
// Omitted from this port: the "Replay guided tour" card — GuidedTour.qml is
// Phase 4 scope (plan §4) and doesn't exist yet in qt-shell.
ColumnLayout {
    id: root
    spacing: 20

    property var s: JSON.parse(SettingsController.settingsJson || "{}")

    Connections {
        target: SettingsController
        function onSettingsJsonChanged() { root.s = JSON.parse(SettingsController.settingsJson || "{}") }
    }

    Text {
        text: (I18n.language, I18n.t("settings.general.title"))
        color: Theme.text
        font.pixelSize: 22
        font.weight: Font.Bold
    }

    // ── Theme File card ──
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredWidth: 680
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: themeCol.implicitHeight + 40

        ColumnLayout {
            id: themeCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 14

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.general.themeFile"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                InfoIcon { tooltipText: I18n.t("settings.general.themeHint") }
                Item { Layout.fillWidth: true }

                Text {
                    text: ThemeController.darkMode ? "🌙" : "☀️"
                    font.pixelSize: 16
                    MouseArea {
                        anchors.fill: parent
                        anchors.margins: -6
                        cursorShape: Qt.PointingHandCursor
                        onClicked: ThemeController.save(ThemeController.accent, !ThemeController.darkMode)
                    }
                }
                Icon {
                    name: "refresh-cw"; size: 14
                    color: Theme.textDim
                    MouseArea {
                        anchors.fill: parent
                        anchors.margins: -6
                        cursorShape: Qt.PointingHandCursor
                        onClicked: ThemeController.reload()
                    }
                }
            }

            RowLayout {
                spacing: 10
                Rectangle {
                    width: 34; height: 34
                    radius: Theme.radius
                    color: ThemeController.accent
                    border.width: 1
                    border.color: Theme.border
                }
                TextField {
                    id: accentField
                    Layout.preferredWidth: 140
                    text: ThemeController.accent
                    color: Theme.text
                    font.pixelSize: 13
                    background: Rectangle {
                        radius: Theme.radius
                        color: Theme.bg
                        border.width: 1
                        border.color: accentField.activeFocus ? Theme.accent : Theme.border
                    }
                    onEditingFinished: ThemeController.save(text, ThemeController.darkMode)
                }
            }
        }
    }

    // ── Clip Preferences card ──
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredWidth: 680
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: clipCol.implicitHeight + 40

        ColumnLayout {
            id: clipCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 14

            Text {
                text: (I18n.language, I18n.t("settings.clipPreferences.title"))
                color: Theme.text
                font.pixelSize: 16
                font.weight: Font.DemiBold
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 24

                ColumnLayout {
                    spacing: 6
                    Text {
                        text: (I18n.language, I18n.t("settings.clipSettings.defaultClick"))
                        color: Theme.textDim
                        font.pixelSize: 13
                    }
                    Row {
                        spacing: 6
                        Repeater {
                            model: [
                                { value: "preview", key: "defaultClickPreview" },
                                { value: "editor", key: "defaultClickEditor" },
                            ]
                            Rectangle {
                                required property var modelData
                                width: 90; height: 30
                                radius: Theme.radius
                                property bool active: (root.s.defaultClickAction || "preview") === modelData.value
                                color: active ? Qt.rgba(0.914, 0.271, 0.376, 0.12) : Theme.bg
                                border.width: 1
                                border.color: active ? Theme.accent : Theme.border
                                Text {
                                    anchors.centerIn: parent
                                    text: I18n.t("settings.clipSettings." + modelData.key)
                                    color: active ? Theme.accent : Theme.textDim
                                    font.pixelSize: 11
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: SettingsController.setValue("defaultClickAction", JSON.stringify(modelData.value))
                                }
                            }
                        }
                    }
                }

                ColumnLayout {
                    spacing: 6
                    RowLayout {
                        spacing: 6
                        Text {
                            text: (I18n.language, I18n.t("settings.general.dateFormat"))
                            color: Theme.textDim
                            font.pixelSize: 13
                        }
                        InfoIcon { tooltipText: I18n.t("settings.general.dateFormatHint") }
                    }
                    Row {
                        spacing: 6
                        Repeater {
                            model: [
                                { value: "YMD", label: "YYYY/MM/DD" },
                                { value: "YDM", label: "YYYY/DD/MM" },
                            ]
                            Rectangle {
                                required property var modelData
                                width: 100; height: 30
                                radius: Theme.radius
                                property bool active: (root.s.dateFormat || "YMD") === modelData.value
                                color: active ? Qt.rgba(0.914, 0.271, 0.376, 0.12) : Theme.bg
                                border.width: 1
                                border.color: active ? Theme.accent : Theme.border
                                Text {
                                    anchors.centerIn: parent
                                    text: modelData.label
                                    color: active ? Theme.accent : Theme.textDim
                                    font.pixelSize: 11
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: SettingsController.setValue("dateFormat", JSON.stringify(modelData.value))
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Daemon & Startup card ──
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredWidth: 680
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: daemonCol.implicitHeight + 40

        ColumnLayout {
            id: daemonCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 14

            Text {
                text: (I18n.language, I18n.t("settings.daemon.title"))
                color: Theme.text
                font.pixelSize: 16
                font.weight: Font.DemiBold
            }

            RowLayout {
                Layout.fillWidth: true
                RowLayout {
                    spacing: 6
                    Layout.fillWidth: true
                    Text {
                        text: (I18n.language, I18n.t("settings.daemon.runAtStartup"))
                        color: Theme.text
                        font.pixelSize: 13
                    }
                    InfoIcon { tooltipText: I18n.t("settings.daemon.runAtStartupTooltip") }
                }
                ToggleSwitch {
                    checked: SystemController.getAutostart()
                    onToggled: (v) => SystemController.setAutostart(v)
                }
            }

            RowLayout {
                Layout.fillWidth: true
                RowLayout {
                    spacing: 6
                    Layout.fillWidth: true
                    Text {
                        text: (I18n.language, I18n.t("settings.daemon.keepInBackground"))
                        color: Theme.text
                        font.pixelSize: 13
                    }
                    InfoIcon { tooltipText: I18n.t("settings.daemon.keepInBackgroundTooltip") }
                }
                ToggleSwitch {
                    checked: root.s.runInBackground || false
                    onToggled: (v) => SettingsController.setValue("runInBackground", JSON.stringify(v))
                }
            }
        }
    }

    // ── Diagnostics card ──
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredWidth: 680
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: diagCol.implicitHeight + 40

        ColumnLayout {
            id: diagCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 12

            RowLayout {
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.diagnostics.title"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                InfoIcon { tooltipText: I18n.t("settings.diagnostics.hint") }
            }

            Rectangle {
                width: 220; height: 34
                radius: Theme.radius
                color: Theme.accent
                Text {
                    anchors.centerIn: parent
                    text: (I18n.language, I18n.t("settings.diagnostics.openCrashLogs"))
                    color: "#ffffff"
                    font.pixelSize: 13
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: SystemController.openCrashLogsFolder()
                }
            }
        }
    }

    // ── Guided tour ──────────────────────────────────────────────────────
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredWidth: 680
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: tourCol.implicitHeight + 40

        ColumnLayout {
            id: tourCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 12

            RowLayout {
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.tour.title"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                InfoIcon { tooltipText: I18n.t("settings.tour.desc") }
            }

            Rectangle {
                id: replayTourBtn
                width: 160; height: 34
                radius: Theme.radius
                color: Theme.accent
                Text {
                    anchors.centerIn: parent
                    text: (I18n.language, I18n.t("settings.tour.replay"))
                    color: "#ffffff"
                    font.pixelSize: 13
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: TourController.start()
                }

                Component.onCompleted: TourController.registerTarget("settings-replay", replayTourBtn)
                Component.onDestruction: TourController.unregisterTarget("settings-replay")
            }
        }
    }

    Component.onCompleted: SettingsController.refresh()
}
