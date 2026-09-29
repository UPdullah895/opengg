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

    // ── Simple / advanced ─────────────────────────────────────────────────
    // Simple (the default) is a fixed layout from a preset matching a common
    // recording setup; only colours change. Advanced is the full editor:
    // rename, re-icon, add and remove tracks, reset to defaults. Simple
    // never rewrites the track list until a preset is picked, so switching
    // modes to look does not lose advanced edits.
    readonly property bool advanced: root.s.trackMode === "advanced"
    function setMode(mode) { SettingsController.setValue("trackMode", JSON.stringify(mode)) }

    /// A track name for display: the built-in role names ("Game", "Mic", …)
    /// and the empty Overlays/Video defaults are shown translated; anything
    /// the user typed is shown as-is. Track data keeps the English role name
    /// so it stays stable across language switches.
    function trackLabel(def) {
        const n = (def && def.name) || ""
        if (n.length === 0 || n === "Overlays" || n === "Video") {
            if (def && def.id === "O1") return I18n.t("editor.overlayTrack")
            if (def && def.id === "V1") return I18n.t("editor.videoTrack")
        }
        const k = "mixer.channels." + n.toLowerCase()
        const t = I18n.t(k)
        return t !== k ? t : (n || (def ? def.id : ""))
    }

    /// Audio roles a preset can contain: default icon and identity colour.
    function role(name) {
        const icons = { Game: "game", Chat: "chat", Mic: "mic", Media: "media", Aux: "media", Desktop: "game" }
        return { name: name, icon: icons[name] || "game",
                 color: String(Theme.channelColor(name === "Desktop" ? "Game" : name)) }
    }
    /// The role a capture source records, from OpenGG's recorder setup.
    function roleForSource(src) {
        const m = /^OpenGG_(\w+)/.exec(src || "")
        if (m) return m[1]
        if (/^(alsa_input|default_input)/.test(src || "")) return "Mic"
        return "Desktop"
    }
    /// Roles recorded by the replay buffer as configured in Capture & Sound.
    readonly property var recordedRoles: (root.s.captureTracks || [])
        .map(t => root.roleForSource(t.source))
    function presets() {
        var out = []
        if (root.recordedRoles.length > 0)
            out.push({ id: "auto", label: I18n.t("settings.timelineTracks.presetAuto"), roles: root.recordedRoles })
        const named = (roles) => roles.map(r => root.trackLabel({ name: r })).join(" · ")
        out.push({ id: "gcm",  roles: ["Game", "Chat", "Mic"] })
        out.push({ id: "gm",   roles: ["Game", "Mic"] })
        out.push({ id: "g",    roles: ["Game"] })
        out.push({ id: "gcmm", roles: ["Game", "Chat", "Media", "Mic"] })
        out.forEach(p => { if (!p.label) p.label = named(p.roles) })
        return out
    }
    /// The preset the current track list matches, or "" for a custom list.
    readonly property string activePreset: {
        I18n.language
        const audio = root.trackDefs.filter(d => /^A\d+$/.test(d.id)).map(d => d.name)
        const all = root.presets()
        for (var i = 0; i < all.length; i++)
            if (JSON.stringify(all[i].roles) === JSON.stringify(audio)) return all[i].id
        return ""
    }
    /// Rewrite the audio tracks from a preset, keeping the colour of any
    /// role the user already recoloured, and Overlays/Video untouched.
    function applyPreset(p) {
        const keep = root.trackDefs.filter(d => !/^A\d+$/.test(d.id))
        const oldColor = {}
        root.trackDefs.forEach(d => { if (/^A\d+$/.test(d.id)) oldColor[d.name] = d.color })
        const audio = p.roles.map((r, i) => {
            const base = root.role(r)
            return { id: "A" + (i + 1), name: base.name, icon: base.icon,
                     color: oldColor[r] || base.color, visible: true }
        })
        root.writeTracks(keep.concat(audio))
        SettingsController.setValue("trackPreset", JSON.stringify(p.id))
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
                Item { Layout.fillWidth: true }
                ChoiceChip {
                    text: (I18n.language, I18n.t("settings.timelineTracks.simple"))
                    selected: !root.advanced
                    onTriggered: root.setMode("simple")
                }
                ChoiceChip {
                    text: (I18n.language, I18n.t("settings.timelineTracks.advanced"))
                    selected: root.advanced
                    onTriggered: root.setMode("advanced")
                }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            // ── Simple mode ──────────────────────────────────────────────
            ColumnLayout {
                visible: !root.advanced
                Layout.fillWidth: true
                spacing: 10

                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.textDim
                    font.pixelSize: 12
                    text: (I18n.language, I18n.t("settings.timelineTracks.simpleHint"))
                }
                Flow {
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: (I18n.language, root.presets())
                        ChoiceChip {
                            required property var modelData
                            text: modelData.label
                            selected: root.activePreset === modelData.id
                            onTriggered: root.applyPreset(modelData)
                        }
                    }
                }
                Text {
                    visible: root.activePreset === ""
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.textMuted
                    font.pixelSize: 11
                    text: (I18n.language, I18n.t("settings.timelineTracks.customLayout"))
                }

                Repeater {
                    model: root.trackDefs
                    RowLayout {
                        id: sRow
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        spacing: 10
                        TrackColorSwatch {
                            current: sRow.modelData.color
                            onPicked: (c) => root.updateTrack(sRow.index, { color: c })
                        }
                        Icon {
                            name: root.trackIcons[sRow.modelData.icon] || "track-game"
                            size: 15
                            color: sRow.modelData.color
                        }
                        Text {
                            Layout.fillWidth: true
                            text: (I18n.language, root.trackLabel(sRow.modelData))
                            color: Theme.text
                            font.pixelSize: 13
                            elide: Text.ElideRight
                        }
                        Text {
                            text: sRow.modelData.id
                            color: Theme.textMuted
                            font.pixelSize: 11
                            font.family: "monospace"
                        }
                    }
                }

                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.textMuted
                    font.pixelSize: 11
                    text: (I18n.language, I18n.t("settings.timelineTracks.fallbackHint"))
                }
            }

            // ── Advanced mode ────────────────────────────────────────────
            ColumnLayout {
                visible: root.advanced
                Layout.fillWidth: true
                spacing: 12

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

                    // Clickable swatch; the hex field next to it stays for
                    // exact values.
                    TrackColorSwatch {
                        current: tRow.modelData.color
                        onPicked: (c) => root.updateTrack(tRow.index, { color: c })
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

            Rectangle {
                width: resetText.implicitWidth + 24; height: 30
                radius: Theme.radius
                color: resetArea.containsMouse ? Theme.bgHover : Theme.bg
                border.width: 1
                border.color: Theme.border
                Text {
                    id: resetText
                    anchors.centerIn: parent
                    text: (I18n.language, I18n.t("settings.timelineTracks.resetDefaults"))
                    color: Theme.textDim
                    font.pixelSize: 12
                }
                MouseArea {
                    id: resetArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: SettingsController.resetTrackDefs()
                }
            }
            }
        }
    }

    Component.onCompleted: SettingsController.refresh()
}
