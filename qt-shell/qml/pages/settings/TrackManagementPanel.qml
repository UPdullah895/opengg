import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings → Timeline Tracks. QML port of TrackManagementSettings.vue.
// Omitted from this port: the "Live Preview" section — it renders via
// TimelineTrackRow.qml, which is Phase 5/editor scope and doesn't exist yet.
ColumnLayout {
    id: root
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
    // even with a bare empty Popup). Inlined here as a plain cycling button instead.
    readonly property var iconIds: ["video", "game", "chat", "mic", "media", "overlay"]
    readonly property var trackIcons: ({
        video: "track-video", game: "track-game", chat: "headphones",
        mic: "track-mic", media: "track-media", overlay: "track-overlay"
    })
    function nextIcon(current) {
        const i = root.iconIds.indexOf(current)
        return root.iconIds[(i + 1) % root.iconIds.length]
    }

    SettingsHeading { titleText: (I18n.language, I18n.t("settings.timelineTracks.title")) }

    Rectangle {
        Layout.fillWidth: true
        Layout.preferredWidth: 680
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

            Repeater {
                model: root.trackDefs
                RowLayout {
                    id: tRow
                    required property var modelData
                    required property int index
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

                    Rectangle {
                        width: 22; height: 22
                        radius: 4
                        color: tRow.modelData.color
                        border.width: 1
                        border.color: Theme.border
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
                        width: 32; height: 28
                        radius: Theme.radius
                        color: Theme.bg
                        border.width: 1
                        border.color: Theme.border
                        Icon {
                            anchors.centerIn: parent
                            name: root.trackIcons[tRow.modelData.icon] || "track-game"; size: 16
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.updateTrack(tRow.index, { icon: root.nextIcon(tRow.modelData.icon) })
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
