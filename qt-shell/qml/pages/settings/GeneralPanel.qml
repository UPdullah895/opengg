import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.opengg.app

// Settings → General. QML port of GeneralSettings.vue.
// Omitted from this port: the "Replay guided tour" card — GuidedTour.qml is
// Phase 4 scope (plan §4) and doesn't exist yet in qt-shell.
ColumnLayout {
    id: root
    width: parent.width
    spacing: 20

    property var s: JSON.parse(SettingsController.settingsJson || "{}")

    Connections {
        target: SettingsController
        function onSettingsJsonChanged() { root.s = JSON.parse(SettingsController.settingsJson || "{}") }
    }

    SettingsHeading { titleText: (I18n.language, I18n.t("settings.general.title")) }

    // ── Theme File card ──
    SettingsCard {
        title: (I18n.language, I18n.t("settings.general.themeFile"))
        infoText: I18n.t("settings.general.themeHint")

        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            // Click opens the native colour picker — a hex TextField alone
            // meant every colour change was a "look up the hex code first"
            // round trip.
            Rectangle {
                id: accentSwatch
                width: 34; height: 34
                radius: Theme.radius
                color: ThemeController.accent
                border.width: 1
                border.color: swatchArea.containsMouse ? Theme.accent : Theme.border

                MouseArea {
                    id: swatchArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: colorDialog.open()
                }
            }
            ColorDialog {
                id: colorDialog
                selectedColor: ThemeController.accent
                onAccepted: ThemeController.save(selectedColor.toString(), ThemeController.darkMode)
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

            Item { Layout.fillWidth: true }

            // Dark/light toggle and reload, as buttons rather than bare
            // floating icons — a bare Icon on the card background didn't
            // read as clickable at all next to every other control here,
            // which is all pill- or button-shaped.
            Rectangle {
                width: 30; height: 30
                radius: Theme.radius
                color: darkModeArea.containsMouse ? Theme.bgHover : Theme.bgInput
                border.width: 1
                border.color: Theme.border
                Icon {
                    anchors.centerIn: parent
                    name: ThemeController.darkMode ? "moon" : "sun"; size: 15
                    color: Theme.textDim
                }
                MouseArea {
                    id: darkModeArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: ThemeController.save(ThemeController.accent, !ThemeController.darkMode)
                }
            }
            Rectangle {
                width: 30; height: 30
                radius: Theme.radius
                color: reloadArea.containsMouse ? Theme.bgHover : Theme.bgInput
                border.width: 1
                border.color: Theme.border
                Icon {
                    anchors.centerIn: parent
                    name: "refresh-cw"; size: 14
                    color: Theme.textDim
                }
                MouseArea {
                    id: reloadArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    // reload() just re-applies whatever's already on disk —
                    // a no-op once a custom accent had been saved. reset()
                    // actually discards the saved override.
                    onClicked: ThemeController.reset()
                }
            }
        }
    }

    // ── Clip Preferences card ──
    SettingsCard {
        title: (I18n.language, I18n.t("settings.clipPreferences.title"))

        RowLayout {
            Layout.fillWidth: true
            spacing: 24

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 6
                Text {
                    text: (I18n.language, I18n.t("settings.clipSettings.defaultClick").toUpperCase())
                    color: Theme.textMuted
                    font.pixelSize: 11
                    font.weight: Font.Bold
                    font.letterSpacing: 0.5
                }
                SelectField {
                    Layout.fillWidth: true
                    options: [
                        { value: "preview", label: I18n.t("settings.clipSettings.defaultClickPreview") },
                        { value: "editor", label: I18n.t("settings.clipSettings.defaultClickEditor") },
                    ]
                    value: root.s.defaultClickAction || "preview"
                    onPicked: (v) => SettingsController.setValue("defaultClickAction", JSON.stringify(v))
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 6
                RowLayout {
                    spacing: 6
                    Text {
                        text: (I18n.language, I18n.t("settings.general.dateFormat").toUpperCase())
                        color: Theme.textMuted
                        font.pixelSize: 11
                        font.weight: Font.Bold
                        font.letterSpacing: 0.5
                    }
                    InfoIcon { tooltipText: I18n.t("settings.general.dateFormatHint") }
                }
                SelectField {
                    Layout.fillWidth: true
                    options: [
                        { value: "YMD", label: "YYYY/MM/DD" },
                        { value: "YDM", label: "YYYY/DD/MM" },
                    ]
                    value: root.s.dateFormat || "YMD"
                    onPicked: (v) => SettingsController.setValue("dateFormat", JSON.stringify(v))
                }
            }
        }
    }

    // ── Daemon & Startup card ──
    SettingsCard {
        title: (I18n.language, I18n.t("settings.daemon.title"))

        RowLayout {
            // A nested RowLayout wrapping the title+icon, marked
            // Layout.fillWidth here, does NOT stretch to fill the outer
            // row the way an Item or ColumnLayout spacer does — leaving
            // the toggle clustered right next to the title instead of
            // pushed to the card's right edge like every other toggle row
            // in this app (Ear Blast Protection, GraphicEQ's Enabled).
            // Use a dedicated fillWidth spacer instead of a fillWidth
            // wrapper Layout.
            Layout.fillWidth: true
            spacing: 6
            Text {
                text: (I18n.language, I18n.t("settings.daemon.runAtStartup"))
                color: Theme.text
                font.pixelSize: 13
            }
            InfoIcon { tooltipText: I18n.t("settings.daemon.runAtStartupTooltip") }
            Item { Layout.fillWidth: true }
            ToggleSwitch {
                checked: SystemController.getAutostart()
                onToggled: (v) => SystemController.setAutostart(v)
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            Text {
                text: (I18n.language, I18n.t("settings.daemon.keepInBackground"))
                color: Theme.text
                font.pixelSize: 13
            }
            InfoIcon { tooltipText: I18n.t("settings.daemon.keepInBackgroundTooltip") }
            Item { Layout.fillWidth: true }
            ToggleSwitch {
                checked: root.s.runInBackground || false
                onToggled: (v) => SettingsController.setValue("runInBackground", JSON.stringify(v))
            }
        }
    }

    // ── Diagnostics card ──
    SettingsCard {
        title: (I18n.language, I18n.t("settings.diagnostics.title"))
        infoText: I18n.t("settings.diagnostics.hint")

        Rectangle {
            width: outlinedRow.implicitWidth + 28; height: 34
            radius: Theme.radius
            color: crashLogsArea.containsMouse ? Theme.tint(Theme.danger, 12) : "transparent"
            border.width: 1
            border.color: Theme.danger
            Row {
                id: outlinedRow
                anchors.centerIn: parent
                spacing: 8
                Icon { anchors.verticalCenter: parent.verticalCenter; name: "folder"; size: 14; color: Theme.danger }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: (I18n.language, I18n.t("settings.diagnostics.openCrashLogs"))
                    color: Theme.danger
                    font.pixelSize: 13
                }
            }
            MouseArea {
                id: crashLogsArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: SystemController.openCrashLogsFolder()
            }
        }
    }

    // ── Guided tour ──────────────────────────────────────────────────────
    SettingsCard {
        title: (I18n.language, I18n.t("settings.tour.title"))
        infoText: I18n.t("settings.tour.desc")

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

    Component.onCompleted: SettingsController.refresh()
}
