import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Compact recorder status + controls for the Clips toolbar — port of
// RecordingDropdown.vue. Shows the replay buffer's live state (idle /
// recording) and opens a small menu with start, save-clip and stop.
//
// All state and actions come from the existing RecordingController; this is
// purely a second, more convenient entry point to what the Home page's
// recording card already drives.
Item {
    id: rec

    /// Read-only view of the flyout, for the pill's hover/chevron state.
    readonly property bool menuOpen: menu.visible

    // A page switch hides this control but not a Popup it owns, which lives
    // in the window overlay — close it explicitly so it cannot outlive the
    // Clips page.
    onVisibleChanged: if (!rec.visible) menu.close()

    implicitWidth: pill.width
    implicitHeight: 32

    // ── Quick-settings state, same shape as the Home dashboard's recorder
    // popover (HomePage.qml's recorderPopoverComp) — duplicated locally
    // rather than shared, matching this shell's existing per-page pattern for
    // small option lists (see ChannelStrip.qml's device selector).
    property var settingsObj: JSON.parse(SettingsController.settingsJson || "{}")
    Connections {
        target: SettingsController
        function onSettingsJsonChanged() { rec.settingsObj = JSON.parse(SettingsController.settingsJson || "{}") }
    }
    function setSetting(key, value) {
        SettingsController.setValue(key, JSON.stringify(value))
        if (rec.settingsObj.gsrEnabled) RecordingController.restart()
    }
    // Each is a function, not a `readonly property var` array literal: QML's
    // automatic dependency tracking only follows PROPERTY reads made while a
    // binding evaluates, not invokable calls like I18n.t() — an array
    // literal built once from I18n.t() results never re-evaluates on a live
    // language switch (see MixerPage.qml's `tabs`/`tabLabel()` for the same
    // landmine). The `(I18n.language, ...)` pattern at each call site below
    // is what actually re-triggers this.
    function gsrQualityOptions() {
        return [
            { value: "cbr", label: I18n.t("settings.captureGsr.qualityCbr") },
            { value: "medium", label: I18n.t("settings.captureGsr.qualityMedium") },
            { value: "high", label: I18n.t("settings.captureGsr.qualityHigh") },
            { value: "very_high", label: I18n.t("settings.captureGsr.qualityVeryHigh") },
            { value: "ultra", label: I18n.t("settings.captureGsr.qualityUltra") }
        ]
    }
    function gsrFpsOptions() {
        return [30, 60, 120].map(v => ({ value: v, label: I18n.t("dashboard.gsrFps." + v) }))
    }
    function gsrReplayOptions() {
        return [15, 30, 60, 90, 120].map(v => ({ value: v, label: I18n.t("dashboard.gsrReplay." + v) }))
    }
    function gsrTargetOptions() {
        return [
            { value: "screen", label: I18n.t("dashboard.gsrTarget.screen") },
            { value: "focused", label: I18n.t("dashboard.gsrTarget.focused") }
        ]
    }

    Component.onCompleted: RecordingController.refresh()

    // The controller only refreshes on demand, so poll while the Clips page is
    // up — the recorder can also be started from the tray, the Home card or a
    // global shortcut, and the pill must not go stale when it is.
    Timer {
        interval: 2000
        running: rec.visible
        repeat: true
        onTriggered: RecordingController.refresh()
    }

    Rectangle {
        id: pill
        width: row.implicitWidth + 20
        height: 32
        radius: Theme.radius
        color: pillArea.containsMouse || rec.menuOpen ? Theme.bgHover : Theme.surface
        border.width: 1
        border.color: RecordingController.running ? Theme.danger : Theme.border

        Row {
            id: row
            anchors.centerIn: parent
            spacing: 7

            // Pulses while the buffer is live, so "recording" reads at a glance.
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 8; height: 8; radius: 4
                color: RecordingController.running ? Theme.danger : Theme.textMuted

                SequentialAnimation on opacity {
                    running: RecordingController.running
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.25; duration: 700; easing.type: Easing.InOutQuad }
                    NumberAnimation { to: 1.0;  duration: 700; easing.type: Easing.InOutQuad }
                }
                // Leaving the loop mid-fade would strand it dim.
                onOpacityChanged: if (!RecordingController.running) opacity = 1
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: RecordingController.running
                      ? (I18n.language, I18n.t("recording.recording"))
                      : (I18n.language, I18n.t("recording.idle"))
                color: RecordingController.running ? Theme.danger : Theme.textDim
                font.pixelSize: 12
                font.weight: Font.DemiBold
            }

            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: rec.menuOpen ? "chevron-up" : "chevron-down"
                size: 11
                color: Theme.textMuted
            }
        }

        MouseArea {
            id: pillArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: menu.toggle()
        }
    }

    // ── Menu ──────────────────────────────────────────────────────────────
    // A FlyoutPopup, not the plain Rectangle this used to be: that one only
    // closed when the pill was clicked again, so it stayed open over whatever
    // the user clicked or navigated to next. The popup closes on any press
    // outside it and on Escape.
    FlyoutPopup {
        id: menu
        anchorItem: pill
        width: 260
        implicitHeight: menuCol.implicitHeight + 16

        contentItem: ColumnLayout {
            id: menuCol
            spacing: 8

            ErrorText {
                Layout.fillWidth: true
                text: RecordingController.error
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                Rectangle {
                    Layout.alignment: Qt.AlignVCenter
                    width: 7; height: 7; radius: 3.5
                    color: RecordingController.running ? Theme.danger : Theme.textMuted
                }
                Text {
                    Layout.fillWidth: true
                    text: RecordingController.statusText
                    color: Theme.textDim
                    font.pixelSize: 12
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
            }

            // Start/Stop + Save Clip, side by side (Save Clip only usable
            // once the buffer is actually running).
            RowLayout {
                Layout.fillWidth: true
                spacing: 6

                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 30
                    radius: Theme.radius
                    color: startArea.containsMouse ? Theme.bgHover : Theme.bgInput
                    border.width: 1
                    border.color: RecordingController.running ? Theme.danger : Theme.border
                    Text {
                        anchors.centerIn: parent
                        text: RecordingController.running
                              ? (I18n.language, I18n.t("dashboard.stop"))
                              : (I18n.language, I18n.t("dashboard.startReplay"))
                        color: RecordingController.running ? Theme.danger : Theme.text
                        font.pixelSize: 12
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                    }
                    MouseArea {
                        id: startArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: RecordingController.running ? RecordingController.stop() : RecordingController.start()
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 30
                    radius: Theme.radius
                    opacity: RecordingController.running ? 1 : 0.5
                    color: saveArea.containsMouse && RecordingController.running
                           ? Theme.tint(Theme.danger, 15) : "transparent"
                    border.width: 1
                    border.color: Theme.danger
                    Text {
                        anchors.centerIn: parent
                        text: (I18n.language, I18n.t("recording.saveClip"))
                        color: Theme.danger
                        font.pixelSize: 12
                        font.weight: Font.DemiBold
                    }
                    MouseArea {
                        id: saveArea
                        anchors.fill: parent
                        hoverEnabled: true
                        enabled: RecordingController.running
                        cursorShape: Qt.PointingHandCursor
                        onClicked: { RecordingController.save(); menu.close() }
                    }
                }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            Text {
                text: (I18n.language, I18n.t("recording.quickSettings").toUpperCase())
                color: Theme.textMuted
                font.pixelSize: 10
                font.weight: Font.Bold
                font.letterSpacing: 0.6
            }

            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: 8
                rowSpacing: 7

                Text { text: (I18n.language, I18n.t("settings.captureGsr.quality")); color: Theme.textDim; font.pixelSize: 12 }
                SelectField {
                    Layout.fillWidth: true
                    options: (I18n.language, rec.gsrQualityOptions())
                    value: rec.settingsObj.gsrQuality
                    onPicked: (v) => rec.setSetting("gsrQuality", v)
                }
                Text { text: (I18n.language, I18n.t("settings.captureGsr.fps")); color: Theme.textDim; font.pixelSize: 12 }
                SelectField {
                    Layout.fillWidth: true
                    options: (I18n.language, rec.gsrFpsOptions())
                    value: rec.settingsObj.gsrFps
                    onPicked: (v) => rec.setSetting("gsrFps", v)
                }
                Text { text: (I18n.language, I18n.t("recording.buffer")); color: Theme.textDim; font.pixelSize: 12 }
                SelectField {
                    Layout.fillWidth: true
                    options: (I18n.language, rec.gsrReplayOptions())
                    value: rec.settingsObj.gsrReplaySecs
                    onPicked: (v) => rec.setSetting("gsrReplaySecs", v)
                }
                Text { text: (I18n.language, I18n.t("recording.target")); color: Theme.textDim; font.pixelSize: 12 }
                SelectField {
                    Layout.fillWidth: true
                    options: (I18n.language, rec.gsrTargetOptions())
                    value: rec.settingsObj.gsrMonitorTarget
                    onPicked: (v) => rec.setSetting("gsrMonitorTarget", v)
                }
            }
        }
    }
}
