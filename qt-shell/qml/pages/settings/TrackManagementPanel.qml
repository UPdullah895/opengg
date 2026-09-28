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

    // The editor draws its picture and overlay lanes from these; see
    // REQUIRED_TRACK_IDS in core/src/settings/mod.rs, which also restores
    // them on load.
    readonly property var requiredIds: ["O1", "V1"]

    function addTrack() {
        // Lowest free A<n>. Numbering by list length handed out ids that
        // were already taken ("A5" twice once any track had been removed),
        // and the editor finds audio lane n by id "A<n>".
        let n = 1
        while (root.trackDefs.some(d => d.id === "A" + n)) n++
        const next = root.trackDefs.concat([{ id: "A" + n, name: "Audio " + n, color: Theme.textDim, icon: "game", visible: true }])
        root.writeTracks(next)
    }

    function removeTrack(index) {
        const def = root.trackDefs[index]
        if (!def || root.requiredIds.indexOf(def.id) !== -1) return
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
                        border.color: swatchArea.containsMouse || colorFlyout.visible
                                      ? Theme.text : Theme.border

                        MouseArea {
                            id: swatchArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: colorFlyout.toggle()
                        }

                        // Presets, then a hue strip and a shade grid built
                        // from it — ten fixed swatches could not express a
                        // colour the theme happened not to ship. Every colour
                        // comes from Theme.hsv() rather than a literal, so
                        // check-colors.sh still holds.
                        //
                        // Deliberately NOT `id: palette`: every Qt 6 Item has
                        // a `palette` property, and inside the cell delegates
                        // below it shadowed the id — each click threw
                        // "choose is not a function" and the hue strip wrote
                        // into the cell's own QQuickPalette, so no colour
                        // could ever be picked.
                        FlyoutPopup {
                            id: colorFlyout
                            anchorItem: swatch
                            width: 6 * 24 + 16
                            implicitHeight: pickerCol.implicitHeight + 16

                            /// Hue of the shade grid, 0..1. Seeded from the
                            /// track's current colour each time it opens so
                            /// the grid starts somewhere recognisable.
                            property real hue: 0
                            /// The track's colour, for marking the matching
                            /// cell so the picker shows what is selected.
                            readonly property color current: tRow.modelData.color
                            onOpened: colorFlyout.hue = Theme.toHsv(tRow.modelData.color).h

                            function choose(c) {
                                root.updateTrack(tRow.index, { color: String(c) })
                                colorFlyout.close()
                            }

                            contentItem: Column {
                                id: pickerCol
                                spacing: 6

                                Text {
                                    text: "Presets"
                                    color: Theme.textMuted
                                    font.pixelSize: 10
                                    font.weight: Font.ExtraBold
                                    font.letterSpacing: 1.2
                                }
                                Grid {
                                    columns: 6
                                    spacing: 4
                                    Repeater {
                                        model: [
                                            Theme.channelColor("Game"), Theme.channelColor("Chat"),
                                            Theme.channelColor("Media"), Theme.channelColor("Aux"),
                                            Theme.channelColor("Mic"), Theme.accent,
                                            Theme.success, Theme.purple,
                                            Theme.overdrive, Theme.textDim,
                                            Theme.text, Theme.textMuted
                                        ]
                                        Rectangle {
                                            required property var modelData
                                            width: 20; height: 20
                                            radius: 4
                                            color: modelData
                                            border.width: Qt.colorEqual(color, colorFlyout.current) ? 2 : 1
                                            border.color: presetArea.containsMouse
                                                          || Qt.colorEqual(color, colorFlyout.current)
                                                          ? Theme.text : Theme.border
                                            MouseArea {
                                                id: presetArea
                                                anchors.fill: parent
                                                hoverEnabled: true
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: colorFlyout.choose(parent.color)
                                            }
                                        }
                                    }
                                }

                                Rectangle {
                                    width: pickerCol.width; height: 1
                                    color: Theme.border
                                }

                                Text {
                                    text: "Hue"
                                    color: Theme.textMuted
                                    font.pixelSize: 10
                                    font.weight: Font.ExtraBold
                                    font.letterSpacing: 1.2
                                }
                                // A strip of discrete hue cells rather than a
                                // gradient: QML gradients cannot be built
                                // from tokens, and 30 cells already reads as
                                // continuous at this size.
                                Row {
                                    id: hueStrip
                                    readonly property int cells: 30
                                    readonly property real cellW:
                                        (pickerCol.width) / hueStrip.cells
                                    Repeater {
                                        model: hueStrip.cells
                                        Rectangle {
                                            required property int index
                                            width: hueStrip.cellW
                                            height: 16
                                            color: Theme.hsv(index / hueStrip.cells, 0.85, 0.95)
                                            // Marks which hue the grid below
                                            // is currently showing.
                                            Rectangle {
                                                anchors.fill: parent
                                                color: "transparent"
                                                border.width: 2
                                                border.color: Theme.text
                                                visible: Math.round(colorFlyout.hue * hueStrip.cells)
                                                         % hueStrip.cells === parent.index
                                            }
                                            MouseArea {
                                                anchors.fill: parent
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: colorFlyout.hue = parent.index / hueStrip.cells
                                            }
                                        }
                                    }
                                }

                                // Saturation across, brightness down.
                                Grid {
                                    columns: 6
                                    spacing: 4
                                    Repeater {
                                        model: 24
                                        Rectangle {
                                            required property int index
                                            readonly property real sat:
                                                0.25 + (index % 6) * 0.15
                                            readonly property real val:
                                                1.0 - Math.floor(index / 6) * 0.22
                                            width: 20; height: 20
                                            radius: 4
                                            color: Theme.hsv(colorFlyout.hue, sat, val)
                                            border.width: Qt.colorEqual(color, colorFlyout.current) ? 2 : 1
                                            border.color: shadeArea.containsMouse
                                                          || Qt.colorEqual(color, colorFlyout.current)
                                                          ? Theme.text : Theme.border
                                            MouseArea {
                                                id: shadeArea
                                                anchors.fill: parent
                                                hoverEnabled: true
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: colorFlyout.choose(parent.color)
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
                        property bool pickerOpen: iconPop.visible
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
                            onClicked: iconPop.toggle()
                        }

                        // A popup over the button, like the colour swatch's
                        // — the strip that used to unfold underneath the row
                        // pushed every track below it down, so the list
                        // jumped around as you edited.
                        FlyoutPopup {
                            id: iconPop
                            anchorItem: iconBtn
                            centerOnAnchor: true
                            width: 3 * 34 + 16
                            implicitHeight: 2 * 32 + 16
                            contentItem: Grid {
                                columns: 3
                                spacing: 4
                                Repeater {
                                    model: root.iconIds
                                    Rectangle {
                                        id: iconChoice
                                        required property string modelData
                                        width: 30; height: 28
                                        radius: Theme.radius
                                        readonly property bool isCurrent:
                                            tRow.modelData.icon === iconChoice.modelData
                                        color: isCurrent ? Theme.accentAlpha(20) : Theme.bg
                                        border.width: 1
                                        border.color: isCurrent ? Theme.accent
                                                    : choiceArea.containsMouse ? Theme.text
                                                    : Theme.border
                                        Icon {
                                            anchors.centerIn: parent
                                            name: root.trackIcons[iconChoice.modelData]
                                            size: 15
                                        }
                                        MouseArea {
                                            id: choiceArea
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                root.updateTrack(tRow.index,
                                                                 { icon: iconChoice.modelData })
                                                iconPop.close()
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Rectangle {
                        width: 28; height: 28
                        radius: Theme.radius
                        property bool protectedTrack: root.requiredIds.indexOf(tRow.modelData.id) !== -1
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
