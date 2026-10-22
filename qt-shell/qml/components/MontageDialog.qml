import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Montage builder — the Moments "Create Montage" flow. Opened from the Clips
// page's selection bar with the selected clips in selection order; the list
// here can be reordered before rendering. Each clip contributes its saved
// trim window (set in the editor), so the reel is built from the moments the
// user already cut down rather than the raw captures.
//
// All rendering happens in core::montage via MontageController; this file
// only gathers choices into one JSON spec.
Rectangle {
    id: dlg

    /// Clips to stitch, in order: [{filepath, title}]. null = closed.
    /// Input only — reordering works on `order`, because assigning to
    /// `clips` here would sever the caller's binding and the dialog could
    /// never be reopened.
    property var clips: null
    property var order: []
    signal closed()

    property string transition: "crossfade"
    property real transitionSecs: 0.6
    /// Key into `formats` below.
    property string format: "source"
    property bool fadeInOut: true
    property string musicPath: ""
    property real musicVolume: 0.35
    property bool duck: true
    property string codec: "h264"
    property string output: ""

    readonly property var formats: ({
        "source":   { w: 0,    h: 0,    fit: "letterbox", label: "SOURCE" },
        "1080p":    { w: 1920, h: 1080, fit: "letterbox", label: "1080P" },
        "720p":     { w: 1280, h: 720,  fit: "letterbox", label: "720P" },
        "vertical": { w: 1080, h: 1920, fit: "blur",      label: "VERTICAL 9:16" }
    })

    /// filepath -> probe row ({duration, start, end, audioTracks}).
    readonly property var probe: {
        var out = {}
        try {
            const rows = JSON.parse(MontageController.clipsJson || "[]")
            for (var i = 0; i < rows.length; i++)
                out[rows[i].path] = rows[i]
        } catch (e) {}
        return out
    }

    function segmentLength(fp) {
        const p = dlg.probe[fp]
        return p ? Math.max(0, p.end - p.start) : 0
    }

    /// Mirrors core::montage::plan — overlap clamped to half the shortest
    /// segment — so the estimate matches the file that comes out.
    readonly property real totalSecs: {
        if (dlg.order.length === 0)
            return 0
        var sum = 0
        var shortest = 1e9
        for (var i = 0; i < dlg.order.length; i++) {
            const d = dlg.segmentLength(dlg.order[i].filepath)
            sum += d
            shortest = Math.min(shortest, d)
        }
        if (dlg.transition === "cut" || dlg.order.length < 2)
            return sum
        const t = Math.min(dlg.transitionSecs, shortest / 2)
        return Math.max(0, sum - (dlg.order.length - 1) * t)
    }
    readonly property bool probed: MontageController.clipsJson.length > 0

    readonly property bool running: MontageController.running
    readonly property string result: MontageController.result
    readonly property string error: MontageController.error

    function fmtTime(s) {
        const t = Math.round(s)
        const m = Math.floor(t / 60)
        const r = t % 60
        return m + ":" + (r < 10 ? "0" : "") + r
    }

    function move(i, delta) {
        const j = i + delta
        if (j < 0 || j >= dlg.order.length)
            return
        var next = dlg.order.slice()
        const tmp = next[i]
        next[i] = next[j]
        next[j] = tmp
        dlg.order = next
    }

    function removeAt(i) {
        var next = dlg.order.slice()
        next.splice(i, 1)
        dlg.order = next
    }

    function specJson() {
        const f = dlg.formats[dlg.format]
        var spec = {
            clips: dlg.order.map(function (c) { return { path: c.filepath } }),
            transition: dlg.transition,
            transition_secs: dlg.transitionSecs,
            width: f.w,
            height: f.h,
            fit: f.fit,
            fps: 60,
            fade_in_out: dlg.fadeInOut,
            codec: dlg.codec,
            output: dlg.output.trim()
        }
        if (dlg.musicPath.trim().length > 0)
            spec.music = { path: dlg.musicPath.trim(), volume: dlg.musicVolume, duck: dlg.duck }
        return JSON.stringify(spec)
    }

    anchors.fill: parent
    color: Theme.scrim(70)
    visible: dlg.clips !== null

    // Probe once per open, not per reorder: the set of clips only ever
    // shrinks while the dialog is up, and every row stays in `probe`.
    onClipsChanged: {
        if (!dlg.clips)
            return
        dlg.order = dlg.clips.slice()
        MontageController.reset()
        MontageController.probeClips(dlg.order.map(function (c) { return c.filepath }))
        dlg.output = dlg.order.length > 0
                ? MontageController.defaultOutput(dlg.order[0].filepath) : ""
    }

    Connections {
        target: MontageController
        function onPickedMusicChanged() { dlg.musicPath = MontageController.pickedMusic }
    }

    // Swallow clicks so they never reach the grid behind.
    MouseArea { anchors.fill: parent }

    Rectangle {
        anchors.centerIn: parent
        width: Math.min(520, parent.width - 32)
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

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Icon { name: "film"; size: 18; color: Theme.accent }
                Text {
                    Layout.fillWidth: true
                    text: dlg.result.length > 0 ? "Montage Ready" : "Create Montage"
                    color: Theme.text
                    font.pixelSize: 18
                    font.weight: Font.Bold
                }
                Text {
                    visible: dlg.result.length === 0
                    text: dlg.probed ? dlg.fmtTime(dlg.totalSecs) : "…"
                    color: Theme.textDim
                    font.pixelSize: 13
                    font.family: "monospace"
                }
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
                        icon: "copy"
                        onTriggered: SystemController.writeClipboard(dlg.result)
                    }
                    Item { Layout.fillWidth: true }
                }
            }

            // ── Form ──────────────────────────────────────────────────────
            ColumnLayout {
                visible: dlg.result.length === 0
                enabled: !dlg.running
                Layout.fillWidth: true
                spacing: 14

                // Clip order
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 7
                    Text {
                        text: "CLIPS · IN PLAYBACK ORDER"
                        color: Theme.textMuted
                        font.pixelSize: 10
                        font.weight: Font.Bold
                        font.letterSpacing: 0.8
                    }
                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.min(dlg.order.length, 5) * 34 + 2
                        radius: Theme.radius
                        color: Theme.surface
                        border.width: 1
                        border.color: Theme.border

                        ListView {
                            id: clipList
                            anchors.fill: parent
                            anchors.margins: 1
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: dlg.order

                            delegate: Rectangle {
                                id: rowItem
                                required property var modelData
                                required property int index
                                width: ListView.view.width
                                height: 34
                                color: rowHover.hovered ? Theme.bgHover : "transparent"

                                HoverHandler { id: rowHover }

                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 10
                                    anchors.rightMargin: 6
                                    spacing: 8

                                    Text {
                                        Layout.preferredWidth: 18
                                        text: String(rowItem.index + 1)
                                        color: Theme.textMuted
                                        font.pixelSize: 11
                                        font.family: "monospace"
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: rowItem.modelData.title
                                        color: Theme.text
                                        font.pixelSize: 12
                                        elide: Text.ElideRight
                                    }
                                    Text {
                                        readonly property var p: dlg.probe[rowItem.modelData.filepath]
                                        text: !p ? "…"
                                              : dlg.fmtTime(dlg.segmentLength(rowItem.modelData.filepath))
                                                + (p.end - p.start < p.duration - 0.05 ? " trimmed" : "")
                                        color: Theme.textDim
                                        font.pixelSize: 11
                                        font.family: "monospace"
                                    }
                                    Repeater {
                                        model: [
                                            { icon: "chevron-up",   act: -1, en: rowItem.index > 0 },
                                            { icon: "chevron-down", act: 1,  en: rowItem.index < dlg.order.length - 1 },
                                            { icon: "x",            act: 0,  en: dlg.order.length > 1 }
                                        ]
                                        Rectangle {
                                            required property var modelData
                                            Layout.preferredWidth: 22
                                            Layout.preferredHeight: 22
                                            radius: Theme.radius
                                            opacity: modelData.en ? 1 : 0.3
                                            color: btnArea.containsMouse && modelData.en ? Theme.bgHover : "transparent"
                                            Icon {
                                                anchors.centerIn: parent
                                                name: parent.modelData.icon
                                                size: 12
                                                color: parent.modelData.act === 0 && btnArea.containsMouse
                                                       ? Theme.danger : Theme.textDim
                                            }
                                            MouseArea {
                                                id: btnArea
                                                anchors.fill: parent
                                                hoverEnabled: true
                                                enabled: parent.modelData.en
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: parent.modelData.act === 0
                                                           ? dlg.removeAt(rowItem.index)
                                                           : dlg.move(rowItem.index, parent.modelData.act)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Transition
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 7
                    Text {
                        text: "TRANSITION"
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
                                { label: "CUT",       v: "cut" },
                                { label: "CROSSFADE", v: "crossfade" },
                                { label: "DIP TO BLACK", v: "fadeblack" }
                            ]
                            ChoiceChip {
                                required property var modelData
                                text: modelData.label
                                selected: dlg.transition === modelData.v
                                onTriggered: dlg.transition = modelData.v
                            }
                        }
                        Item { width: 8; height: 1 }
                        Repeater {
                            model: [0.3, 0.6, 1.0]
                            ChoiceChip {
                                required property var modelData
                                text: modelData.toFixed(1) + "S"
                                enabled: dlg.transition !== "cut"
                                selected: dlg.transition !== "cut" && dlg.transitionSecs === modelData
                                onTriggered: dlg.transitionSecs = modelData
                            }
                        }
                    }
                }

                // Format
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 7
                    Text {
                        text: "FORMAT"
                        color: Theme.textMuted
                        font.pixelSize: 10
                        font.weight: Font.Bold
                        font.letterSpacing: 0.8
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: 8
                        Repeater {
                            model: ["source", "1080p", "720p", "vertical"]
                            ChoiceChip {
                                required property var modelData
                                text: dlg.formats[modelData].label
                                selected: dlg.format === modelData
                                onTriggered: dlg.format = modelData
                            }
                        }
                        Item { width: 8; height: 1 }
                        Repeater {
                            model: [{ label: "H.264", v: "h264" }, { label: "H.265", v: "h265" }]
                            ChoiceChip {
                                required property var modelData
                                text: modelData.label
                                selected: dlg.codec === modelData.v
                                onTriggered: dlg.codec = modelData.v
                            }
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        ToggleSwitch {
                            checked: dlg.fadeInOut
                            onToggled: (v) => dlg.fadeInOut = v
                        }
                        Text {
                            Layout.fillWidth: true
                            text: "Fade in from and out to black"
                            color: Theme.textDim
                            font.pixelSize: 12
                        }
                    }
                }

                // Music
                ExportFieldRow {
                    Layout.fillWidth: true
                    label: "MUSIC (OPTIONAL)"
                    value: dlg.musicPath
                    onEdited: (v) => dlg.musicPath = v
                    onBrowse: {
                        if (!MontageController.pickMusic())
                            console.log("MontageDialog: no native file picker; type a path instead")
                    }
                }
                RowLayout {
                    visible: dlg.musicPath.trim().length > 0
                    Layout.fillWidth: true
                    spacing: 8
                    Repeater {
                        model: [
                            { label: "QUIET", v: 0.2 },
                            { label: "MEDIUM", v: 0.35 },
                            { label: "LOUD", v: 0.6 }
                        ]
                        ChoiceChip {
                            required property var modelData
                            text: modelData.label
                            selected: dlg.musicVolume === modelData.v
                            onTriggered: dlg.musicVolume = modelData.v
                        }
                    }
                    Item { Layout.fillWidth: true }
                    ToggleSwitch {
                        checked: dlg.duck
                        onToggled: (v) => dlg.duck = v
                    }
                    Text {
                        text: "Duck under game audio"
                        color: Theme.textDim
                        font.pixelSize: 12
                    }
                }

                ExportFieldRow {
                    Layout.fillWidth: true
                    label: "SAVE AS"
                    value: dlg.output
                    onEdited: (v) => dlg.output = v
                    onBrowse: SystemController.revealInFolder(dlg.output)
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
                            width: parent.width * (MontageController.progress / 100)
                            height: parent.height
                            radius: 3
                            color: Theme.accent
                        }
                    }
                    Text {
                        text: MontageController.stage + " · "
                              + Math.round(MontageController.progress) + "%"
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
                        text: dlg.result.length > 0 ? "Close" : (dlg.running ? "Stop" : "Cancel")
                        color: Theme.text
                        font.pixelSize: 13
                    }
                    MouseArea {
                        id: cancelArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            if (dlg.running)
                                MontageController.cancel()
                            else
                                dlg.closed()
                        }
                    }
                }

                Rectangle {
                    visible: dlg.result.length === 0
                    Layout.preferredWidth: 124
                    Layout.preferredHeight: 32
                    radius: Theme.radius
                    readonly property bool ready: !dlg.running
                                                  && dlg.order.length > 0
                    color: ready ? Theme.accent : Theme.bgHover
                    Text {
                        anchors.centerIn: parent
                        text: dlg.running ? "Rendering…" : "Create Montage"
                        color: parent.ready ? "#ffffff" : Theme.textDim
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                    }
                    MouseArea {
                        anchors.fill: parent
                        enabled: parent.ready
                        cursorShape: Qt.PointingHandCursor
                        onClicked: MontageController.build(dlg.specJson())
                    }
                }
            }
        }
    }
}
