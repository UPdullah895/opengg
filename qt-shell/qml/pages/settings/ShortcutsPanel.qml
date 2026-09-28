import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings → Shortcuts. QML port of ShortcutsSettings.vue.
ColumnLayout {
    id: root
    width: parent.width
    spacing: 20
    focus: recordingKey !== ""

    property var s: JSON.parse(SettingsController.settingsJson || "{}")
    property string recordingKey: ""

    Connections {
        target: SettingsController
        function onSettingsJsonChanged() { root.s = JSON.parse(SettingsController.settingsJson || "{}") }
    }

    /// Built-in bindings, for the Reset button's enabled state.
    readonly property var defaults:
        JSON.parse(SettingsController.defaultShortcutsJson() || "{}")

    /// True when at least one binding differs from the built-in default.
    readonly property bool customised: {
        const cur = root.s.shortcuts || ({})
        for (var k in root.defaults)
            if ((cur[k] || "") !== (root.defaults[k] || "")) return true
        return false
    }

    /// Actions nothing listens for yet. The bindings are stored and
    /// editable, but nothing dispatches them, so pressing these does
    /// nothing. Flagged rather than hidden: the settings are real, the
    /// wiring is not.
    ///
    /// `saveReplay` and `toggleRecording` left this list once the daemon's
    /// evdev listener became real (daemon/src/replay/hotkey.rs) and
    /// `GlobalHotkeys.qml` started acting on its signals. `screenshot` is
    /// carried end to end but still has nothing to run — no part of this
    /// project takes a desktop screenshot. The rest are editor-local
    /// shortcuts with no dispatcher.
    readonly property var inactive: [
        "screenshot", "toggleEarBlast",
        "splitClip", "exportClip", "toggleMic", "undo", "redo",
    ]

    /// The daemon's global listener, for the two bindings that depend on it.
    readonly property bool globalActive: HotkeyController.active
    readonly property var global: ["saveReplay", "toggleRecording"]

    readonly property var actions: [
        "saveReplay", "toggleRecording", "screenshot", "toggleEarBlast",
        "splitClip", "exportClip", "toggleMic", "undo", "redo",
    ]

    function startRecord(key) {
        root.recordingKey = key
        root.forceActiveFocus()
    }
    function cancelRecord() { root.recordingKey = "" }

    function comboFromEvent(event) {
        const parts = []
        if (event.modifiers & Qt.ControlModifier) parts.push("Ctrl")
        if (event.modifiers & Qt.ShiftModifier) parts.push("Shift")
        if (event.modifiers & Qt.AltModifier) parts.push("Alt")
        if (event.modifiers & Qt.MetaModifier) parts.push("Meta")

        const k = event.key
        let main = ""
        if (k === Qt.Key_Control || k === Qt.Key_Shift || k === Qt.Key_Alt || k === Qt.Key_Meta) {
            return null // modifier-only, keep waiting
        } else if (k >= Qt.Key_A && k <= Qt.Key_Z) {
            main = String.fromCharCode(k)
        } else if (k >= Qt.Key_0 && k <= Qt.Key_9) {
            main = String.fromCharCode(k)
        } else if (k >= Qt.Key_F1 && k <= Qt.Key_F12) {
            main = "F" + (k - Qt.Key_F1 + 1)
        } else if (event.text && event.text.length === 1) {
            main = event.text.toUpperCase()
        } else {
            return null
        }
        parts.push(main)
        return parts.join("+")
    }

    Keys.onPressed: (event) => {
        if (!root.recordingKey) return
        event.accepted = true
        if (event.key === Qt.Key_Escape) { root.cancelRecord(); return }
        const combo = root.comboFromEvent(event)
        if (combo) {
            SettingsController.setValue("shortcuts." + root.recordingKey, JSON.stringify(combo))
            root.recordingKey = ""
        }
    }

    SettingsHeading { titleText: (I18n.language, I18n.t("settings.shortcuts.title")) }

    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: scCol.implicitHeight + 40

        ColumnLayout {
            id: scCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 12

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                // Same nested-RowLayout-fillWidth bug as the shortcut rows
                // below: a wrapper Layout marked fillWidth doesn't stretch,
                // so "Reset to Defaults" was landing right next to the
                // title instead of the card's far-right edge. Dedicated
                // spacer instead.
                Text {
                    text: (I18n.language, I18n.t("settings.shortcuts.title"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                InfoIcon { tooltipText: I18n.t("settings.shortcuts.hint") }
                Item { Layout.fillWidth: true }
                Rectangle {
                    id: resetBtn
                    width: 130; height: 28
                    radius: Theme.radius
                    // Nothing to reset when every binding is already the
                    // default — the button used to look live regardless.
                    enabled: root.customised
                    opacity: enabled ? 1 : 0.4
                    color: Theme.bg
                    border.width: 1
                    border.color: Theme.border
                    Text {
                        anchors.centerIn: parent
                        text: (I18n.language, I18n.t("settings.shortcuts.resetToDefaults"))
                        color: Theme.textDim
                        font.pixelSize: 11
                    }
                    MouseArea {
                        anchors.fill: parent
                        enabled: resetBtn.enabled
                        cursorShape: Qt.PointingHandCursor
                        onClicked: SettingsController.resetShortcuts()
                    }
                }
            }

            /// Why the global bindings are inert, when they are. Without
            /// this the two badges below say the keys do not work but not
            /// what to do about it, and the cause is almost always one
            /// fixable thing: the user is not in the `input` group.
            Rectangle {
                visible: !root.globalActive
                Layout.fillWidth: true
                radius: Theme.radius
                color: Theme.tint(Theme.overdrive, 12)
                border.width: 1
                border.color: Theme.tint(Theme.overdrive, 40)
                implicitHeight: offNote.implicitHeight + 20
                Text {
                    id: offNote
                    anchors.fill: parent
                    anchors.margins: 10
                    wrapMode: Text.WordWrap
                    color: Theme.textDim
                    font.pixelSize: 11
                    text: "Global shortcuts are not running — "
                          + (HotkeyController.error
                             || "the daemon has not confirmed a keyboard listener")
                          + ". Reading keys system-wide needs membership of "
                          + "the 'input' group: run  sudo usermod -aG input $USER "
                          + " in a terminal, then log out and back in."
                }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            Repeater {
                model: root.actions
                ColumnLayout {
                    id: actionCol
                    required property string modelData
                    required property int index
                    Layout.fillWidth: true
                    spacing: 0

                    RowLayout {
                        Layout.fillWidth: true
                        // Originally 4px/no divider (rows ran together);
                        // then overcorrected to 10px/10px, which made each
                        // row far taller than the reference design's
                        // compact list. 5px/5px + the divider below is the
                        // middle ground: separated rows without the
                        // padding bloat.
                        Layout.topMargin: 5
                        Layout.bottomMargin: 5
                        spacing: 6

                        // A nested RowLayout wrapping the label+icon, marked
                        // Layout.fillWidth, does NOT stretch to claim the
                        // row's remaining space the way an Item spacer does
                        // (same root cause as GeneralPanel's Start on
                        // Boot/Minimize to Tray toggles) — it left the key
                        // box sitting right next to the label instead of
                        // right-aligned to the card edge like the reference
                        // design. Use a dedicated fillWidth spacer instead.
                        Text {
                            text: I18n.t("settings.shortcuts.actions." + actionCol.modelData)
                            color: Theme.text
                            font.pixelSize: 13
                        }
                        InfoIcon { tooltipText: I18n.t("settings.shortcuts.hints." + actionCol.modelData) }
                        /// A global binding the daemon is not listening
                        /// for. Distinct from "not wired up": the wiring
                        /// exists, this machine just cannot read the
                        /// keyboard — almost always a missing `input` group.
                        Rectangle {
                            visible: !root.globalActive
                                     && root.global.indexOf(actionCol.modelData) >= 0
                            width: offText.implicitWidth + 12
                            height: 16
                            radius: 8
                            color: Theme.tint(Theme.overdrive, 18)
                            border.width: 1
                            border.color: Theme.tint(Theme.overdrive, 45)
                            Text {
                                id: offText
                                anchors.centerIn: parent
                                text: "listener off"
                                color: Theme.overdrive
                                font.pixelSize: 9
                                font.weight: Font.DemiBold
                            }
                        }
                        Rectangle {
                            visible: root.inactive.indexOf(actionCol.modelData) >= 0
                            width: badgeText.implicitWidth + 12
                            height: 16
                            radius: 8
                            color: Theme.tint(Theme.overdrive, 18)
                            border.width: 1
                            border.color: Theme.tint(Theme.overdrive, 45)
                            Text {
                                id: badgeText
                                anchors.centerIn: parent
                                text: "not wired up"
                                color: Theme.overdrive
                                font.pixelSize: 9
                                font.weight: Font.DemiBold
                            }
                        }
                        Item { Layout.fillWidth: true }

                        Rectangle {
                            id: keyBox
                            width: 140; height: 30
                            radius: Theme.radius
                            property bool isRecordingThis: root.recordingKey === actionCol.modelData
                            color: isRecordingThis ? Theme.accentAlpha(15) : Theme.bg
                            border.width: 1
                            border.color: isRecordingThis ? Theme.accent : Theme.border

                            Text {
                                anchors.centerIn: parent
                                text: keyBox.isRecordingThis
                                    ? I18n.t("settings.shortcuts.recording")
                                    : ((root.s.shortcuts && root.s.shortcuts[actionCol.modelData]) || "—")
                                color: keyBox.isRecordingThis ? Theme.accent : Theme.text
                                font.pixelSize: 12
                                elide: Text.ElideRight
                                width: parent.width - 12
                                horizontalAlignment: Text.AlignHCenter
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.recordingKey === actionCol.modelData ? root.cancelRecord() : root.startRecord(actionCol.modelData)
                            }
                        }
                    }

                    Rectangle {
                        visible: actionCol.index < root.actions.length - 1
                        Layout.fillWidth: true
                        height: 1
                        color: Theme.border
                    }
                }
            }
        }
    }

    Component.onCompleted: SettingsController.refresh()
}
