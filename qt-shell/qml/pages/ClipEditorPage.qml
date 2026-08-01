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
// only exist in the mp4 `name` tag (see core::media's parser). Qt Multimedia
// decodes one audio stream at a time, so the lane's speaker button selects
// which track the PREVIEW plays rather than mixing — a true mixdown needs a
// hand-built GStreamer pipeline. Export is a lossless stream copy and keeps
// every track regardless of what the preview is monitoring.
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
        mp.play()
    }

    onVisibleChanged: {
        if (visible) page.forceActiveFocus()
        else mp.stop()
    }

    function fmt(sec) {
        if (!sec || sec < 0) sec = 0
        var t = Math.floor(sec)
        var m = Math.floor(t / 60)
        var s = t % 60
        return m + ":" + (s < 10 ? "0" : "") + s + "." + Math.floor((sec % 1) * 10)
    }

    function togglePlay() {
        if (mp.playbackState === MediaPlayer.PlayingState) mp.pause()
        else mp.play()
    }
    function skip(ms) {
        mp.position = Math.max(0, Math.min(mp.duration, mp.position + ms))
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
        audioOutput: AudioOutput { id: audioOut }
        onMediaStatusChanged: {
            if (mediaStatus === MediaPlayer.EndOfMedia) {
                mp.pause()
                mp.position = Math.round(page.trimStart * 1000)
            }
        }
        // Playback is clamped to the trim window so the handles preview what
        // the export will actually produce.
        onPositionChanged: {
            if (page.trimEnd > 0 && mp.position > page.trimEnd * 1000 + 50)
                mp.position = Math.round(page.trimStart * 1000)
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

                Text {
                    text: page.clip ? page.clip.title : ""
                    color: Theme.text
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                    Layout.maximumWidth: 320
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
                        onClicked: {
                            EditorController.saveTrim(page.filepath, page.trimStart, page.trimEnd)
                            EditorController.exportTrim(page.filepath, page.trimStart, page.trimEnd)
                        }
                    }
                }
            }
        }

        // ── Export result / error banner ──────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: visible ? 32 : 0
            visible: EditorController.exportError.length > 0
                     || EditorController.exportResult.length > 0
            color: EditorController.exportError.length > 0
                   ? Theme.tint(Theme.danger, 12) : Theme.accentAlpha(12)
            Text {
                anchors.left: parent.left
                anchors.leftMargin: 14
                anchors.verticalCenter: parent.verticalCenter
                text: EditorController.exportError.length > 0
                      ? "Export failed: " + EditorController.exportError
                      : "Exported to " + EditorController.exportResult
                color: EditorController.exportError.length > 0 ? Theme.danger : Theme.text
                font.pixelSize: 12
                elide: Text.ElideMiddle
                width: parent.width - 28
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
                Layout.preferredWidth: 232
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
                    icon: audioOut.muted ? "volume-x" : "volume-2"
                    tooltip: audioOut.muted ? "Unmute" : "Mute"
                    onTriggered: audioOut.muted = !audioOut.muted
                }
                Slider {
                    id: vol
                    Layout.preferredWidth: 84
                    Layout.alignment: Qt.AlignVCenter
                    from: 0; to: 1
                    value: audioOut.muted ? 0 : audioOut.volume
                    onMoved: {
                        audioOut.volume = vol.value
                        if (vol.value > 0) audioOut.muted = false
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
                    icon: "rotate-ccw"
                    tooltip: "Reset trim"
                    onTriggered: page.resetTrim()
                }
            }
        }

        // ── Timeline ──────────────────────────────────────────────────────
        Rectangle {
            id: timelinePane
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(196, 34 + lanes.implicitHeight + 16)
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
                        monitorable: true
                        monitoring: mp.activeAudioTrack === index
                        onMonitorToggled: mp.activeAudioTrack = index
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
                    onClicked: (m) => mp.position = Math.round(timelinePane.xToTime(m.x) * 1000)
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
                    onDragged: (dx) => {
                        const t = timelinePane.xToTime(timelinePane.timeToX(page.trimStart) + dx)
                        page.trimStart = Math.min(t, page.trimEnd - 0.1)
                    }
                }
                TrimHandle {
                    x: timelinePane.timeToX(page.trimEnd) - width / 2
                    height: parent.height
                    onDragged: (dx) => {
                        const t = timelinePane.xToTime(timelinePane.timeToX(page.trimEnd) + dx)
                        page.trimEnd = Math.max(t, page.trimStart + 0.1)
                    }
                }
            }
        }
    }
}
