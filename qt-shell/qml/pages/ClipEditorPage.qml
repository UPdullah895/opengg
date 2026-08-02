import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtMultimedia
import com.opengg.app

// ClipEditorPage — the clip editor as a real page, replacing the TrimEditor
// modal overlay. Port of AdvancedEditor.vue's layout: preview + transport on
// the left, a media INFO panel on the right, and a multi-lane timeline
// (video + one lane per audio track) with draggable trim handles underneath.
//
// AUDIO LANES: the lanes are labelled with the clip's real track names, which
// only exist in the mp4 `name` tag (see core::media's parser). Every track
// plays SIMULTANEOUSLY through ClipAudioMixer's GStreamer pipeline, so each
// lane's speaker button mutes that one track in the live mix. Qt Multimedia
// only ever supplies the picture here — it can decode one audio stream at a
// time and cannot mix. Export is a lossless stream copy and keeps every track
// regardless of what is muted for monitoring.
Rectangle {
    id: page
    color: Theme.bg

    /// {filepath, title} of the clip being edited, or null.
    property var clip: null
    signal closed()

    readonly property string filepath: page.clip ? page.clip.filepath : ""
    readonly property var info: JSON.parse(EditorController.mediaInfoJson || "{}")
    readonly property var audioStreams: (page.info.streams || [])
        .filter(function (s) { return s.codec_type === "audio" })

    property real trimStart: 0
    property real trimEnd: 0
    readonly property real duration: EditorController.duration

    focus: visible

    // ── Audio ownership ───────────────────────────────────────────────────
    // ClipAudioMixer plays every audio track simultaneously with independent
    // gain, which is the whole point of a multi-track editor; Qt Multimedia is
    // muted down to the picture. Single-track clips fall back to Qt's audio.
    readonly property bool mixed: ClipAudioMixer.active
    /// Ownership token from ClipAudioMixer.load — see the note there.
    property var mixerToken: null
    /// Per-track mute flags. Reassigned wholesale so bindings re-evaluate.
    property var trackMuted: ({})
    /// Master mute. Applies to the mix when mixed, to Qt's output otherwise.
    property bool masterMuted: false
    /// Output level, owned by the page rather than read back off audioOut:
    /// while mixed, Qt's output is muted by design and its `volume` says
    /// nothing about what you can actually hear.
    property real masterVolume: 1.0

    function applyVolume() {
        if (page.mixed)
            ClipAudioMixer.setMasterVolume(page.masterMuted ? 0 : page.masterVolume)
        else
            audioOut.volume = page.masterVolume
    }
    /// Hides the info panel and timeline so the picture fills the page.
    property bool theaterMode: false
    /// Current game tag, seeded from the clip row and edited in the top bar.
    property string gameTag: ""
    /// Export settings dialog open?
    property bool exportOpen: false

    function toggleTrack(index) {
        if (!page.mixed)
            return
        var next = {}
        for (var k in page.trackMuted) next[k] = page.trackMuted[k]
        next[index] = !next[index]
        page.trackMuted = next
        ClipAudioMixer.setTrackVolume(index, next[index] ? 0.0 : 1.0)
    }

    onClipChanged: {
        if (!page.clip)
            return
        // Read straight off `clip` rather than through the `filepath` binding:
        // QML does not guarantee a dependent binding has re-evaluated by the
        // time the property's own onChanged handler runs, and loading with a
        // stale empty path left the whole INFO panel showing dashes.
        EditorController.loadClip(page.clip.filepath)
        page.trimStart = EditorController.trimStart
        page.trimEnd = EditorController.trimEnd
        page.gameTag = page.clip.game || ""
        page.trackMuted = ({})
        // false: the editor has no ClipVideoSurface yet (its picture still
        // comes from Qt Multimedia below) — see load()'s doc comment.
        page.mixerToken = ClipAudioMixer.load(page.clip.filepath, false)
        page.applyVolume()
        // The mix is started by onPlaybackStateChanged, not here — starting it
        // alongside mp.play() ran the audio ahead of the first frame.
        mp.play()
    }

    onVisibleChanged: {
        if (visible) {
            page.forceActiveFocus()
        } else {
            mp.stop()
            // Release the audio device; a live pipeline would keep playing
            // over the rest of the app after navigating away. Token-scoped so
            // it can't tear down a mix another view already owns.
            if (page.mixerToken !== null) {
                ClipAudioMixer.release(page.mixerToken)
                page.mixerToken = null
            }
        }
    }

    // Independent clocks drift; nudge the audio back to the video periodically.
    Timer {
        interval: 400
        running: page.visible && page.mixed
                 && mp.playbackState === MediaPlayer.PlayingState
        repeat: true
        onTriggered: {
            const apos = ClipAudioMixer.positionMs()
            if (apos >= 0 && Math.abs(apos - mp.position) > 120)
                ClipAudioMixer.seek(mp.position)
        }
    }

    function fmt(sec) {
        if (!sec || sec < 0) sec = 0
        var t = Math.floor(sec)
        var m = Math.floor(t / 60)
        var s = t % 60
        return m + ":" + (s < 10 ? "0" : "") + s + "." + Math.floor((sec % 1) * 10)
    }

    function togglePlay() {
        // Only the video is driven here; onPlaybackStateChanged mirrors the
        // resulting state onto the mixer.
        if (mp.playbackState === MediaPlayer.PlayingState) mp.pause()
        else mp.play()
    }
    function seekTo(ms) {
        mp.position = Math.max(0, Math.min(mp.duration, ms))
        if (page.mixed) ClipAudioMixer.seek(mp.position)
    }
    function skip(ms) {
        page.seekTo(mp.position + ms)
    }
    function resetTrim() {
        page.trimStart = 0
        page.trimEnd = page.duration
    }

    Keys.onPressed: (e) => {
        if (e.key === Qt.Key_Space)       { page.togglePlay(); e.accepted = true }
        else if (e.key === Qt.Key_Right)  { page.skip(5000);   e.accepted = true }
        else if (e.key === Qt.Key_Left)   { page.skip(-5000);  e.accepted = true }
        else if (e.key === Qt.Key_Escape) { page.closed();     e.accepted = true }
        else if (e.key === Qt.Key_Z && (e.modifiers & Qt.ControlModifier)) {
            page.resetTrim(); e.accepted = true
        }
    }

    MediaPlayer {
        id: mp
        source: page.filepath ? "file://" + page.filepath : ""
        videoOutput: videoOut
        audioOutput: AudioOutput {
            id: audioOut
            // Silence Qt's single-track decode while the mixer owns the sound,
            // or track 1 would be heard twice.
            // Muted whenever the mixer owns the sound (its track 1 would
            // otherwise double up), and whenever the user mutes the output.
            muted: page.mixed || page.masterMuted
            volume: page.masterVolume
        }
        onPlaybackRateChanged: if (page.mixed) ClipAudioMixer.setRate(mp.playbackRate)
        // The mix follows the video's ACTUAL state; mp.play() returns long
        // before the first frame is on screen.
        onPlaybackStateChanged: {
            if (!page.mixed)
                return
            if (mp.playbackState === MediaPlayer.PlayingState) {
                ClipAudioMixer.seek(mp.position)
                ClipAudioMixer.play()
            } else {
                ClipAudioMixer.pause()
            }
        }
        onMediaStatusChanged: {
            if (mediaStatus === MediaPlayer.EndOfMedia) {
                mp.pause()
                page.seekTo(Math.round(page.trimStart * 1000))
            }
        }
        // Playback is clamped to the trim window so the handles preview what
        // the export will actually produce.
        onPositionChanged: {
            if (page.trimEnd > 0 && mp.position > page.trimEnd * 1000 + 50)
                page.seekTo(Math.round(page.trimStart * 1000))
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ── Top bar ───────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 56
            color: Theme.surface
            border.width: 0

            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width; height: 1
                color: Theme.border
            }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 14
                spacing: 10

                Rectangle {
                    Layout.preferredWidth: 32
                    Layout.preferredHeight: 32
                    radius: Theme.radius
                    color: backArea.containsMouse ? Theme.bgHover : "transparent"
                    border.width: 1
                    border.color: Theme.border
                    Icon {
                        anchors.centerIn: parent
                        name: "chevron-down"
                        size: 14
                        color: Theme.text
                        rotation: 90     // no dedicated back glyph in the set
                    }
                    MouseArea {
                        id: backArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: page.closed()
                    }
                }

                // Name and game are editable here, as in the old editor's
                // header — the Clips grid's inline rename only covers the name.
                EditorField {
                    id: nameField
                    Layout.preferredWidth: 220
                    placeholder: "Clip name"
                    text: page.clip ? page.clip.title : ""
                    onCommitted: (v) => {
                        if (v.length > 0 && page.clip) {
                            ClipsController.setCustomName(page.clip.filepath, v)
                            page.clip = { filepath: page.clip.filepath, title: v }
                        }
                    }
                }
                EditorField {
                    id: gameField
                    Layout.preferredWidth: 170
                    placeholder: "Game…"
                    text: page.gameTag
                    onCommitted: (v) => {
                        if (page.clip) {
                            EditorController.setGameTag(page.clip.filepath, v)
                            page.gameTag = v
                            ClipsController.refresh()
                        }
                    }
                }

                Item { Layout.fillWidth: true }

                // Format badges
                Repeater {
                    model: [
                        page.info.video_codec || "",
                        page.info.width ? page.info.width + "×" + page.info.height : "",
                        page.info.fps ? Math.round(page.info.fps) + "fps" : ""
                    ].filter(function (t) { return !!t })

                    Rectangle {
                        required property string modelData
                        Layout.preferredHeight: 20
                        implicitWidth: badgeText.implicitWidth + 16
                        radius: 4
                        color: Theme.bgDeep
                        Text {
                            id: badgeText
                            anchors.centerIn: parent
                            text: modelData
                            color: Theme.textMuted
                            font.pixelSize: 11
                        }
                    }
                }

                // Export
                Rectangle {
                    Layout.preferredWidth: 118
                    Layout.preferredHeight: 32
                    radius: Theme.radius
                    color: EditorController.exportRunning ? Theme.bgHover : Theme.accent
                    Text {
                        anchors.centerIn: parent
                        text: EditorController.exportRunning
                              ? Math.round(EditorController.exportProgress) + "%"
                              : "Export Clip"
                        color: EditorController.exportRunning ? Theme.text : "#ffffff"
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                    }
                    MouseArea {
                        anchors.fill: parent
                        enabled: !EditorController.exportRunning && page.trimEnd > page.trimStart
                        cursorShape: Qt.PointingHandCursor
                        // Opens the settings dialog rather than exporting on the
                        // spot — filename, directory, size and codec all belong
                        // to the user, not to a default.
                        onClicked: page.exportOpen = true
                    }
                }
            }
        }

        // ── Preview + info ────────────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            // Video
            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                color: "#000000"
                clip: true

                VideoOutput {
                    id: videoOut
                    anchors.fill: parent
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: page.togglePlay()
                }

                Rectangle {
                    anchors.centerIn: parent
                    width: 60; height: 60; radius: 30
                    visible: mp.playbackState !== MediaPlayer.PlayingState
                    color: Theme.scrim(55)
                    Icon {
                        anchors.centerIn: parent
                        anchors.horizontalCenterOffset: 3
                        name: "play"; size: 24; color: "#ffffff"
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: page.togglePlay()
                    }
                }
            }

            // Info sidebar
            Rectangle {
                visible: !page.theaterMode
                Layout.preferredWidth: page.theaterMode ? 0 : 232
                Layout.fillHeight: true
                color: Theme.surface

                Rectangle {
                    anchors.left: parent.left
                    width: 1; height: parent.height
                    color: Theme.border
                }

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 14
                    spacing: 10

                    Text {
                        text: "INFO"
                        color: Theme.textMuted
                        font.pixelSize: 11
                        font.weight: Font.Bold
                        font.letterSpacing: 1
                    }

                    Text {
                        Layout.fillWidth: true
                        text: page.filepath
                        color: Theme.textMuted
                        font.pixelSize: 10
                        elide: Text.ElideMiddle
                    }

                    Repeater {
                        model: [
                            { k: "Duration",   v: page.duration > 0 ? page.duration.toFixed(1) + "s" : "—" },
                            { k: "Resolution", v: page.info.width ? page.info.width + "×" + page.info.height : "—" },
                            { k: "FPS",        v: page.info.fps ? page.info.fps.toFixed(1) : "—" },
                            { k: "Codec",      v: page.info.video_codec || "—" },
                            { k: "Audio",      v: page.audioStreams.length
                                                  ? page.audioStreams.length + " tracks" : "—" },
                            { k: "Trim",       v: page.fmt(page.trimStart) + " → " + page.fmt(page.trimEnd) },
                            { k: "Output",     v: Math.max(0, page.trimEnd - page.trimStart).toFixed(1) + "s" }
                        ]

                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 6
                            Text {
                                text: modelData.k
                                color: Theme.textMuted
                                font.pixelSize: 11
                            }
                            Item { Layout.fillWidth: true }
                            Text {
                                text: modelData.v
                                color: Theme.text
                                font.pixelSize: 11
                                font.weight: Font.DemiBold
                            }
                        }
                    }

                    Item { Layout.fillHeight: true }

                    Rectangle {
                        Layout.fillWidth: true
                        height: 1
                        color: Theme.border
                    }

                    Repeater {
                        model: [
                            { k: "Space",  v: "Play / Pause" },
                            { k: "← →",    v: "Skip ±5s" },
                            { k: "Ctrl+Z", v: "Reset trim" }
                        ]

                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 8
                            Rectangle {
                                Layout.preferredWidth: keyText.implicitWidth + 12
                                Layout.preferredHeight: 18
                                radius: 3
                                color: Theme.bgDeep
                                border.width: 1
                                border.color: Theme.border
                                Text {
                                    id: keyText
                                    anchors.centerIn: parent
                                    text: modelData.k
                                    color: Theme.textDim
                                    font.pixelSize: 10
                                }
                            }
                            Text {
                                text: modelData.v
                                color: Theme.textMuted
                                font.pixelSize: 10
                            }
                            Item { Layout.fillWidth: true }
                        }
                    }
                }
            }
        }

        // ── Transport ─────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            color: Theme.surface

            Rectangle { width: parent.width; height: 1; color: Theme.border }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 14
                spacing: 4

                EditorButton { icon: "rewind";       tooltip: "Back 5s";   onTriggered: page.skip(-5000) }
                EditorButton {
                    icon: mp.playbackState === MediaPlayer.PlayingState ? "pause" : "play"
                    tooltip: "Play / Pause"
                    onTriggered: page.togglePlay()
                }
                EditorButton { icon: "fast-forward"; tooltip: "Forward 5s"; onTriggered: page.skip(5000) }

                Text {
                    Layout.leftMargin: 8
                    text: page.fmt(mp.position / 1000) + "  /  " + page.fmt(page.duration)
                    color: Theme.textDim
                    font.pixelSize: 12
                }

                Item { Layout.fillWidth: true }

                EditorButton {
                    icon: masterMuted ? "volume-x" : "volume-2"
                    tooltip: masterMuted ? "Unmute" : "Mute"
                    onTriggered: {
                        page.masterMuted = !page.masterMuted
                        if (page.mixed)
                            page.applyVolume()
                        else
                            audioOut.muted = page.masterMuted
                    }
                }
                Slider {
                    id: vol
                    Layout.preferredWidth: 84
                    Layout.alignment: Qt.AlignVCenter
                    from: 0; to: 1
                    // Seeded once, then owned locally — a binding here would be
                    // severed by QQC2's own writes on the first drag anyway.
                    Component.onCompleted: value = page.masterVolume
                    onMoved: {
                        page.masterVolume = vol.value
                        if (vol.value > 0) page.masterMuted = false
                        page.applyVolume()
                    }
                    background: Rectangle {
                        x: vol.leftPadding
                        y: vol.topPadding + vol.availableHeight / 2 - height / 2
                        width: vol.availableWidth
                        height: 4
                        radius: 2
                        color: Theme.border
                        Rectangle {
                            width: vol.visualPosition * parent.width
                            height: parent.height
                            radius: 2
                            color: Theme.accent
                        }
                    }
                    handle: Rectangle {
                        x: vol.leftPadding + vol.visualPosition * (vol.availableWidth - width)
                        y: vol.topPadding + vol.availableHeight / 2 - height / 2
                        width: 12; height: 12; radius: 6
                        color: Theme.text
                        border.width: 2
                        border.color: Theme.accent
                    }
                }

                EditorButton {
                    icon: "camera"
                    tooltip: "Save this frame as an image"
                    onTriggered: {
                        if (page.clip)
                            EditorController.grabFrame(page.clip.filepath, mp.position / 1000)
                    }
                }
                EditorButton {
                    icon: page.theaterMode ? "minimize" : "maximize"
                    tooltip: page.theaterMode ? "Exit full view" : "Full view"
                    onTriggered: page.theaterMode = !page.theaterMode
                }
                EditorButton {
                    icon: "rotate-ccw"
                    tooltip: "Reset trim"
                    onTriggered: page.resetTrim()
                }
            }
        }

        // ── Timeline ──────────────────────────────────────────────────────
        Rectangle {
            id: timelinePane
            visible: !page.theaterMode
            Layout.fillWidth: true
            Layout.preferredHeight: page.theaterMode
                                    ? 0 : Math.min(196, 34 + lanes.implicitHeight + 16)
            color: Theme.bgDeep

            Rectangle { width: parent.width; height: 1; color: Theme.border }

            // Fixed-width label gutter, so every lane's track area starts at
            // the same x and the playhead/trim overlay can span them all.
            readonly property int gutter: 96
            readonly property real trackW: width - gutter - 24

            function xToTime(x) {
                if (page.duration <= 0 || timelinePane.trackW <= 0) return 0
                return Math.max(0, Math.min(page.duration,
                    (x / timelinePane.trackW) * page.duration))
            }
            function timeToX(t) {
                if (page.duration <= 0) return 0
                return (t / page.duration) * timelinePane.trackW
            }

            ColumnLayout {
                id: lanes
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.leftMargin: 12
                anchors.rightMargin: 12
                anchors.topMargin: 10
                spacing: 4

                // Video lane
                EditorTrackLane {
                    Layout.fillWidth: true
                    gutter: timelinePane.gutter
                    label: "Video"
                    accent: Theme.accent
                    icon: "track-video"
                    monitorable: false
                }

                // One lane per audio stream, labelled with its real track name.
                Repeater {
                    model: page.audioStreams

                    EditorTrackLane {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        gutter: timelinePane.gutter
                        label: modelData.title || ("Audio " + (index + 1))
                        accent: Theme.channelColor(modelData.title)
                        icon: "music"
                        // Every track plays at once; this mutes one of them
                        // rather than switching which single track is audible.
                        monitorable: page.mixed
                        monitoring: page.trackMuted[index] !== true
                        onMonitorToggled: page.toggleTrack(index)
                    }
                }
            }

            // ── Trim + playhead overlay ───────────────────────────────────
            // One overlay across every lane rather than per-lane handles, so a
            // trim edit visibly applies to the whole clip.
            Item {
                id: overlay
                x: timelinePane.gutter + 12
                y: 10
                width: timelinePane.trackW
                height: lanes.implicitHeight

                // Dim the regions the export will discard.
                Rectangle {
                    x: 0
                    width: Math.max(0, timelinePane.timeToX(page.trimStart))
                    height: parent.height
                    color: Theme.scrim(60)
                }
                Rectangle {
                    x: timelinePane.timeToX(page.trimEnd)
                    width: Math.max(0, parent.width - x)
                    height: parent.height
                    color: Theme.scrim(60)
                }

                // Seek by clicking anywhere on the timeline.
                MouseArea {
                    anchors.fill: parent
                    onClicked: (m) => page.seekTo(Math.round(timelinePane.xToTime(m.x) * 1000))
                }

                // Playhead
                Rectangle {
                    x: timelinePane.timeToX(mp.position / 1000) - width / 2
                    width: 2
                    height: parent.height
                    color: Theme.text
                }

                TrimHandle {
                    x: timelinePane.timeToX(page.trimStart) - width / 2
                    height: parent.height
                    onMovedTo: (px) => {
                        page.trimStart = Math.min(timelinePane.xToTime(px),
                                                  page.trimEnd - 0.1)
                    }
                }
                TrimHandle {
                    x: timelinePane.timeToX(page.trimEnd) - width / 2
                    height: parent.height
                    onMovedTo: (px) => {
                        page.trimEnd = Math.max(timelinePane.xToTime(px),
                                                page.trimStart + 0.1)
                    }
                }
            }
        }
    }

    // ── Export settings dialog ────────────────────────────────────────────
    ExportDialog {
        clip: page.exportOpen ? page.clip : null
        trimStart: page.trimStart
        trimEnd: page.trimEnd
        z: 100
        onClosed: page.exportOpen = false
    }

    // A finished frame grab is worth confirming — it lands in a directory the
    // user may not have open.
    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 90
        z: 90
        visible: EditorController.screenshotPath.length > 0 && shotToast.running
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.accent
        implicitWidth: shotText.implicitWidth + 28
        implicitHeight: 34
        Text {
            id: shotText
            anchors.centerIn: parent
            text: "Frame saved"
            color: Theme.text
            font.pixelSize: 12
        }
        Timer {
            id: shotToast
            interval: 2600
        }
        Connections {
            target: EditorController
            function onScreenshotPathChanged() {
                if (EditorController.screenshotPath.length > 0)
                    shotToast.restart()
            }
        }
    }
}
