import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Export settings dialog for the clip editor — port of AdvancedEditor.vue's
// export modal. Filename, directory, target size, an Advanced Settings
// disclosure with the codec choice, and a summary of what will actually be
// produced. Shows progress while running and a done state afterwards, from
// which the result can be revealed in the file manager.
Rectangle {
    id: dlg

    /// Clip being exported: {filepath, title}.
    property var clip: null
    /// Trim window, in seconds.
    property real trimStart: 0
    property real trimEnd: 0
    readonly property real outLength: Math.max(0, dlg.trimEnd - dlg.trimStart)

    /// The editor's live track mix, by audio-track index: gain 0..1 and
    /// mute. Only applied when the user opts in ("Use my track mix").
    property var trackLevels: []
    property var trackMuted: []

    signal closed()

    property string filename: ""
    property string directory: ""
    /// 0 = keep original size; otherwise the target in MB.
    property real targetMb: 0
    property string codec: "copy"
    property bool advancedOpen: false

    // ── Effects (opengg_core::edit) ──
    property string look: "none"
    property real speed: 1.0
    property bool applyMix: false
    property bool mixTrack: false
    readonly property var lookIds: JSON.parse(EditorController.lookIds() || "[]")

    /// The SimpleEdit + size/codec payload for `exportEdited`.
    function editJson() {
        return JSON.stringify({
            look: dlg.look, speed: dlg.speed,
            applyMix: dlg.applyMix, levels: dlg.trackLevels, muted: dlg.trackMuted,
            mixTrack: dlg.mixTrack, targetMb: dlg.targetMb, codec: dlg.codec
        })
    }
    /// {active, text}: whether the effects route is needed, and its summary.
    readonly property var editInfo: JSON.parse(EditorController.describeEdit(dlg.editJson()) || "{}")
    readonly property bool effectsActive: dlg.editInfo.active === true

    /// Recomputed explicitly rather than bound: exportSummary reads
    /// mediaInfoJson INSIDE the invokable, and QML's dependency tracker does
    /// not see property reads that happen on the Rust side — the line stayed
    /// on its first (pre-probe) value showing a dash for the resolution.
    property string summary: ""

    function refreshSummary() {
        dlg.summary = EditorController.exportSummary(dlg.outLength, dlg.targetMb, dlg.codec)
    }

    onTargetMbChanged: dlg.refreshSummary()
    onCodecChanged: dlg.refreshSummary()
    onOutLengthChanged: dlg.refreshSummary()
    Connections {
        target: EditorController
        function onMediaInfoJsonChanged() { dlg.refreshSummary() }
    }

    readonly property bool running: EditorController.exportRunning
    readonly property string result: EditorController.exportResult
    readonly property string error: EditorController.exportError

    anchors.fill: parent
    color: Theme.scrim(70)
    visible: dlg.clip !== null

    onClipChanged: {
        if (!dlg.clip)
            return
        // Reset per-open so a previous run's result doesn't greet the next one.
        dlg.filename = dlg.clip.title || "clip"
        dlg.directory = EditorController.defaultExportDir()
        dlg.targetMb = 0
        dlg.codec = "copy"
        dlg.advancedOpen = false
        dlg.look = "none"
        dlg.speed = 1.0
        dlg.applyMix = false
        dlg.mixTrack = false
        dlg.refreshSummary()
    }

    // Effects need a re-encode; "keep original codec" would silently drop
    // them, so switch to H.264 the moment one is turned on.
    onEffectsActiveChanged: if (dlg.effectsActive && dlg.codec === "copy") dlg.codec = "h264"

    /// A small uppercase section label, like the other dialog headings.
    component SectionLabel: Text {
        color: Theme.textMuted
        font.pixelSize: 10
        font.weight: Font.Bold
        font.letterSpacing: 0.8
    }

    /// Absolute output path from the two fields, with the source's extension.
    function outputPath() {
        const ext = String(dlg.clip ? dlg.clip.filepath : "").split(".").pop()
        const safe = dlg.filename.trim().length > 0 ? dlg.filename.trim() : "clip"
        return dlg.directory.replace(/\/+$/, "") + "/" + safe + "." + ext
    }

    // Swallow clicks so they never reach the editor behind.
    MouseArea { anchors.fill: parent }

    Rectangle {
        anchors.centerIn: parent
        width: 456
        implicitHeight: col.implicitHeight + 40
        radius: Theme.radiusLg
        color: Theme.bg
        border.width: 1
        border.color: Theme.border

        ColumnLayout {
            id: col
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 20
            spacing: 14

            Text {
                text: dlg.result.length > 0 ? "Export Complete" : "Export Clip"
                color: Theme.text
                font.pixelSize: 18
                font.weight: Font.Bold
            }

            // ── Done state ────────────────────────────────────────────────
            ColumnLayout {
                visible: dlg.result.length > 0
                Layout.fillWidth: true
                spacing: 10

                Text {
                    Layout.fillWidth: true
                    text: dlg.result
                    color: Theme.textDim
                    font.pixelSize: 12
                    wrapMode: Text.WrapAnywhere
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    ClipsBarButton {
                        label: "Show in folder"
                        icon: "folder"
                        onTriggered: SystemController.revealInFolder(dlg.result)
                    }
                    ClipsBarButton {
                        label: "Copy path"
                        icon: "package"
                        onTriggered: SystemController.writeClipboard(dlg.result)
                    }
                    Item { Layout.fillWidth: true }
                }
            }

            // ── Form ──────────────────────────────────────────────────────
            ColumnLayout {
                visible: dlg.result.length === 0
                Layout.fillWidth: true
                spacing: 14

                ExportFieldRow {
                    Layout.fillWidth: true
                    label: "FILENAME"
                    value: dlg.filename
                    onEdited: (v) => dlg.filename = v
                }

                ExportFieldRow {
                    Layout.fillWidth: true
                    label: "DIRECTORY"
                    value: dlg.directory
                    onEdited: (v) => dlg.directory = v
                    // Opens the folder so the path can be checked; there is no
                    // native directory picker wired into this shell yet, so the
                    // field itself stays editable rather than read-only.
                    onBrowse: SystemController.revealInFolder(dlg.directory)
                }

                // Target size
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 7
                    Text {
                        text: (I18n.language, I18n.t("editor.targetSize").toUpperCase())
                        color: Theme.textMuted
                        font.pixelSize: 10
                        font.weight: Font.Bold
                        font.letterSpacing: 0.8
                    }
                    RowLayout {
                        spacing: 8
                        Repeater {
                            model: [
                                { label: I18n.t("editor.original").toUpperCase(), mb: 0 },
                                { label: "100MB",    mb: 100 },
                                { label: "50MB",     mb: 50 },
                                { label: "10MB",     mb: 10 }
                            ]
                            ChoiceChip {
                                required property var modelData
                                text: modelData.label
                                selected: dlg.targetMb === modelData.mb
                                onTriggered: {
                                    dlg.targetMb = modelData.mb
                                    // A size target always means a re-encode;
                                    // silently leaving "no re-encode" selected
                                    // would ignore the size the user picked.
                                    if (modelData.mb > 0 && dlg.codec === "copy")
                                        dlg.codec = "h264"
                                }
                            }
                        }
                    }
                }

                // Effects
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 7
                    SectionLabel { text: (I18n.language, I18n.t("editor.effects.look").toUpperCase()) }
                    Flow {
                        Layout.fillWidth: true
                        spacing: 6
                        Repeater {
                            model: dlg.lookIds
                            ChoiceChip {
                                required property string modelData
                                text: (I18n.language, I18n.t("editor.looks." + modelData))
                                selected: dlg.look === modelData
                                onTriggered: dlg.look = modelData
                            }
                        }
                    }
                    SectionLabel { text: (I18n.language, I18n.t("editor.effects.speed").toUpperCase()) }
                    RowLayout {
                        spacing: 6
                        Repeater {
                            model: [0.5, 0.75, 1, 1.5, 2]
                            ChoiceChip {
                                required property real modelData
                                text: modelData + "×"
                                selected: Math.abs(dlg.speed - modelData) < 0.001
                                onTriggered: dlg.speed = modelData
                            }
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        visible: dlg.trackLevels.length > 0
                        spacing: 6
                        Text {
                            text: (I18n.language, I18n.t("editor.effects.applyMix"))
                            color: Theme.text
                            font.pixelSize: 12
                        }
                        InfoIcon { tooltipText: I18n.t("editor.effects.applyMixHint") }
                        Item { Layout.fillWidth: true }
                        ToggleSwitch {
                            checked: dlg.applyMix
                            onToggled: (v) => dlg.applyMix = v
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        visible: dlg.trackLevels.length > 1
                        spacing: 6
                        Text {
                            text: (I18n.language, I18n.t("editor.effects.mixTrack"))
                            color: Theme.text
                            font.pixelSize: 12
                        }
                        InfoIcon { tooltipText: I18n.t("editor.effects.mixTrackHint") }
                        Item { Layout.fillWidth: true }
                        ToggleSwitch {
                            checked: dlg.mixTrack
                            onToggled: (v) => dlg.mixTrack = v
                        }
                    }
                }

                // Advanced
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    Item {
                        Layout.fillWidth: true
                        implicitHeight: 18
                        Row {
                            spacing: 6
                            Icon {
                                anchors.verticalCenter: parent.verticalCenter
                                name: dlg.advancedOpen ? "chevron-up" : "chevron-down"
                                size: 12
                                color: Theme.textDim
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: (I18n.language, I18n.t("editor.advancedSettings"))
                                color: Theme.textDim
                                font.pixelSize: 12
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: dlg.advancedOpen = !dlg.advancedOpen
                        }
                    }

                    Rectangle {
                        visible: dlg.advancedOpen
                        Layout.fillWidth: true
                        implicitHeight: advCol.implicitHeight + 24
                        radius: Theme.radius
                        color: "transparent"
                        border.width: 1
                        border.color: Theme.border

                        ColumnLayout {
                            id: advCol
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: 12
                            spacing: 8

                            Text {
                                text: (I18n.language, I18n.t("editor.codec").toUpperCase())
                                color: Theme.textMuted
                                font.pixelSize: 10
                                font.weight: Font.Bold
                                font.letterSpacing: 0.8
                            }
                            Flow {
                                Layout.fillWidth: true
                                spacing: 8
                                Repeater {
                                    model: [
                                        { label: "H.264", v: "h264" },
                                        { label: "H.265", v: "h265" },
                                        { label: "VP9",   v: "vp9" },
                                        { label: "AV1",   v: "av1" }
                                    ]
                                    ChoiceChip {
                                        required property var modelData
                                        text: modelData.label
                                        selected: dlg.codec === modelData.v
                                        onTriggered: dlg.codec = modelData.v
                                    }
                                }
                            }
                            ChoiceChip {
                                text: (I18n.language, I18n.t("editor.codecOriginal").toUpperCase())
                                selected: dlg.codec === "copy"
                                // Only meaningful at original size; a target
                                // size requires re-encoding to reach it.
                                enabled: dlg.targetMb === 0 && !dlg.effectsActive
                                onTriggered: dlg.codec = "copy"
                            }
                        }
                    }
                }

                // Summary
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 34
                    radius: Theme.radius
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.border
                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: 12
                        anchors.right: parent.right
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        text: dlg.effectsActive ? dlg.summary + " • " + dlg.editInfo.text : dlg.summary
                        color: Theme.textDim
                        font.pixelSize: 11
                        font.family: "monospace"
                        elide: Text.ElideRight
                    }
                }

                // Progress
                ColumnLayout {
                    visible: dlg.running
                    Layout.fillWidth: true
                    spacing: 4
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: 6
                        radius: 3
                        color: Theme.border
                        Rectangle {
                            width: parent.width * (EditorController.exportProgress / 100)
                            height: parent.height
                            radius: 3
                            color: Theme.accent
                        }
                    }
                    Text {
                        text: EditorController.exportStage + " · "
                              + Math.round(EditorController.exportProgress) + "%"
                        color: Theme.textMuted
                        font.pixelSize: 11
                    }
                }

                Text {
                    Layout.fillWidth: true
                    visible: dlg.error.length > 0
                    text: dlg.error
                    color: Theme.danger
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                }
            }

            // ── Actions ───────────────────────────────────────────────────
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 4
                spacing: 8

                Item { Layout.fillWidth: true }

                Rectangle {
                    Layout.preferredWidth: 78
                    Layout.preferredHeight: 32
                    radius: Theme.radius
                    color: cancelArea.containsMouse ? Theme.bgHover : Theme.surface
                    border.width: 1
                    border.color: Theme.border
                    Text {
                        anchors.centerIn: parent
                        text: dlg.result.length > 0 ? "Close" : "Cancel"
                        color: Theme.text
                        font.pixelSize: 13
                    }
                    MouseArea {
                        id: cancelArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            // Cancel stops an effects export in flight (the
                            // exporter removes its partial file); the plain
                            // exports run to completion as before.
                            if (dlg.running)
                                EditorController.cancelExport()
                            dlg.closed()
                        }
                    }
                }

                Rectangle {
                    visible: dlg.result.length === 0
                    Layout.preferredWidth: 104
                    Layout.preferredHeight: 32
                    radius: Theme.radius
                    color: dlg.running ? Theme.bgHover : Theme.accent
                    Text {
                        anchors.centerIn: parent
                        text: dlg.running ? "Exporting…" : "Export Clip"
                        color: dlg.running ? Theme.textDim : "#ffffff"
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                    }
                    MouseArea {
                        anchors.fill: parent
                        enabled: !dlg.running && dlg.outLength > 0
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            EditorController.saveTrim(dlg.clip.filepath,
                                                      dlg.trimStart, dlg.trimEnd)
                            if (dlg.effectsActive)
                                EditorController.exportEdited(dlg.clip.filepath,
                                                              dlg.trimStart, dlg.trimEnd,
                                                              dlg.outputPath(), dlg.editJson())
                            else
                                EditorController.exportClip(dlg.clip.filepath,
                                                            dlg.trimStart, dlg.trimEnd,
                                                            dlg.targetMb, dlg.outputPath(),
                                                            dlg.codec)
                        }
                    }
                }
            }
        }
    }
}
