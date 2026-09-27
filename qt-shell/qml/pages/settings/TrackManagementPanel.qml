import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings → Timeline Tracks. QML port of TrackManagementSettings.vue.
// Omitted from this port: the "Live Preview" section — it renders via
// TimelineTrackRow.qml, which is Phase 5/editor scope and doesn't exist yet.
ColumnLayout {
    id: root
    width: parent.width
    spacing: 20

    property var s: JSON.parse(SettingsController.settingsJson || "{}")
    property var trackDefs: root.s.trackDefs || []

    Connections {
        target: SettingsController
        function onSettingsJsonChanged() { root.s = JSON.parse(SettingsController.settingsJson || "{}") }
    }

    function writeTracks(next) {
        SettingsController.setValue("trackDefs", JSON.stringify(next))
    }

    function updateTrack(index, patch) {
        const next = root.trackDefs.map((d, i) => i === index ? Object.assign({}, d, patch) : d)
        root.writeTracks(next)
    }

    function addTrack() {
        const idx = root.trackDefs.length
        const next = root.trackDefs.concat([{ id: "A" + idx, name: "Audio " + idx, color: Theme.textDim, icon: "game", visible: true }])
        root.writeTracks(next)
    }

    function removeTrack(index) {
        if (root.trackDefs.length <= 1) return
        root.writeTracks(root.trackDefs.filter((_, i) => i !== index))
    }

    // IconPicker.qml as a separate component file was found to hang the app at
    // startup for unknown reasons (Popup- and ComboBox-based versions both hung,
    // even with a bare empty Popup). The picker below is a plain inline Row
    // toggled by a property, not a Popup/ComboBox, to sidestep that landmine
    // while still giving a real "choose one of six" picker instead of the
    // blind cycle-on-click button this replaced.
    readonly property var iconIds: ["video", "game", "chat", "mic", "media", "overlay"]
    readonly property var trackIcons: ({
        video: "track-video", game: "track-game", chat: "headphones",
        mic: "track-mic", media: "track-media", overlay: "track-overlay"
    })
    property string openIconPickerFor: ""

    SettingsHeading { titleText: (I18n.language, I18n.t("settings.timelineTracks.title")) }

    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: tCol.implicitHeight + 40

        ColumnLayout {
            id: tCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 12

            RowLayout {
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.timelineTracks.trackList"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                InfoIcon { tooltipText: I18n.t("settings.timelineTracks.trackListHint") }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            Repeater {
                model: root.trackDefs
                ColumnLayout {
                    id: tCol2
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    spacing: 6

                RowLayout {
                    id: tRow
                    property var modelData: tCol2.modelData
                    property int index: tCol2.index
                    Layout.fillWidth: true
                    spacing: 8

                    Rectangle {
                        width: 28; height: 28
                        radius: Theme.radius
                        color: "transparent"
                        border.width: 1
                        border.color: Theme.border
                        Icon {
                            anchors.centerIn: parent
                            name: tRow.modelData.visible ? "eye" : "eye-off"; size: 14
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.updateTrack(tRow.index, { visible: !tRow.modelData.visible })
                        }
                    }

                    // Clickable swatch. Typing a hex was the only way to
                    // recolour a track, which is impractical for something
                    // you pick by eye; the field stays for exact values.
                    Rectangle {
                        id: swatch
                        width: 22; height: 22
                        radius: 4
                        color: tRow.modelData.color
                        border.width: 1
                        border.color: swatchArea.containsMouse || palette.visible
                                      ? Theme.text : Theme.border

                        MouseArea {
                            id: swatchArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: palette.visible ? palette.close() : palette.open()
                        }

                        Popup {
                            id: palette
                            y: swatch.height + 4
                            width: 5 * 26 + 12
                            implicitHeight: 2 * 26 + 12
                            padding: 6
                            background: Rectangle {
                                radius: Theme.radius
                                color: Theme.surface
                                border.width: 1
                                border.color: Theme.border
                            }
                            // Drawn from theme tokens rather than hex
                            // literals, so a retheme moves the palette too
                            // (and check-colors.sh stays happy).
                            readonly property var swatches: [
                                Theme.channelColor("Game"), Theme.channelColor("Chat"),
                                Theme.channelColor("Media"), Theme.channelColor("Aux"),
                                Theme.channelColor("Mic"),
                                Theme.accent, Theme.success, Theme.purple,
                                Theme.overdrive, Theme.textDim
                            ]
                            contentItem: Grid {
                                columns: 5
                                spacing: 4
                                Repeater {
                                    model: palette.swatches
                                    Rectangle {
                                        required property var modelData
                                        width: 22; height: 22
                                        radius: 4
                                        color: modelData
                                        border.width: 1
                                        border.color: pickArea.containsMouse
                                                      ? Theme.text : Theme.border
                                        MouseArea {
                                            id: pickArea
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                root.updateTrack(tRow.index,
                                                    { color: String(parent.color) })
                                                palette.close()
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    TextField {
                        id: colorField
                        Layout.preferredWidth: 80
                        text: tRow.modelData.color
                        color: Theme.text
                        font.pixelSize: 11
                        background: Rectangle {
                            radius: Theme.radius
                            color: Theme.bg
                            border.width: 1
                            border.color: colorField.activeFocus ? Theme.accent : Theme.border
                        }
                        onEditingFinished: root.updateTrack(tRow.index, { color: text })
                    }

                    TextField {
                        id: nameField
                        Layout.fillWidth: true
                        text: tRow.modelData.name
                        placeholderText: tRow.modelData.id
                        color: Theme.text
                        font.pixelSize: 12
                        maximumLength: 20
                        background: Rectangle {
                            radius: Theme.radius
                            color: Theme.bg
                            border.width: 1
                            border.color: nameField.activeFocus ? Theme.accent : Theme.border
                        }
                        onEditingFinished: root.updateTrack(tRow.index, { name: text })
                    }

                    Rectangle {
                        id: iconBtn
                        width: 32; height: 28
                        radius: Theme.radius
                        property bool pickerOpen: root.openIconPickerFor === tRow.modelData.id
                        color: pickerOpen ? Theme.accentAlpha(15) : Theme.bg
                        border.width: 1
                        border.color: pickerOpen ? Theme.accent : Theme.border
                        Icon {
                            anchors.centerIn: parent
                            name: root.trackIcons[tRow.modelData.icon] || "track-game"; size: 16
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.openIconPickerFor = iconBtn.pickerOpen ? "" : tRow.modelData.id
                        }
                    }

                    Rectangle {
                        width: 28; height: 28
                        radius: Theme.radius
                        property bool protectedTrack: tRow.modelData.id === "V1" || tRow.modelData.id === "O1"
                        color: "transparent"
                        opacity: protectedTrack ? 0.35 : 1.0
                        Icon {
                            anchors.centerIn: parent
                            name: "x"; size: 12
                            color: Theme.textDim
                        }
                        MouseArea {
                            anchors.fill: parent
                            enabled: !parent.protectedTrack
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.removeTrack(tRow.index)
                        }
                    }
                }

                // Inline icon-choice strip — a plain Row toggled by
                // openIconPickerFor, not a Popup/ComboBox (see the landmine
                // note above). Lets you pick a specific icon instead of
                // cycling blind through six options one click at a time.
                Row {
                    visible: root.openIconPickerFor === tCol2.modelData.id
                    Layout.leftMargin: 68
                    spacing: 6

                    Repeater {
                        model: root.iconIds
                        Rectangle {
                            id: iconChoice
                            required property string modelData
                            width: 30; height: 28
                            radius: Theme.radius
                            property bool isCurrent: tCol2.modelData.icon === modelData
                            color: isCurrent ? Theme.accentAlpha(20) : Theme.bg
                            border.width: 1
                            border.color: isCurrent ? Theme.accent : Theme.border
                            Icon {
                                anchors.centerIn: parent
                                name: root.trackIcons[iconChoice.modelData]
                                size: 15
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    root.updateTrack(tCol2.index, { icon: iconChoice.modelData })
                                    root.openIconPickerFor = ""
                                }
                            }
                        }
                    }
                }
                }
            }

            Rectangle {
                Layout.topMargin: 4
                width: 160; height: 30
                radius: Theme.radius
                color: Theme.bg
                border.width: 1
                border.color: Theme.border
                Text {
                    anchors.centerIn: parent
                    text: (I18n.language, I18n.t("settings.timelineTracks.addAudioTrack"))
                    color: Theme.textDim
                    font.pixelSize: 12
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.addTrack()
                }
            }
        }
    }

    Component.onCompleted: SettingsController.refresh()
}
