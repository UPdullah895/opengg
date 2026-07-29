import QtQuick
import QtQuick.Controls
import QtMultimedia
import com.opengg.app

// TrimEditor — Phase 5 "trim-only first" slice (plan's documented fallback:
// full multi-track editing stays in the Tauri app until the GStreamer
// multi-track pipeline work lands; PoC confirmed Qt Multimedia only decodes
// a clip's first audio track). Single-track preview (peer to VideoPlayer.qml,
// not a modification of it) + a two-handle trim range + lossless ffmpeg
// stream-copy export via EditorController.
Rectangle {
    id: root
    color: "#cc0b0d13"
    property string source: ""
    property string filepath: ""
    property string title: ""
    signal closed()

    focus: visible
    Keys.onEscapePressed: root.closed()

    property string draggingHandle: ""   // "", "start", "end"
    readonly property real minGapSec: 0.2

    function fmt(sec) {
        if (!sec || sec < 0)
            sec = 0
        var t = Math.floor(sec)
        var m = Math.floor(t / 60)
        var s = t % 60
        return m + ":" + (s < 10 ? "0" : "") + s
    }

    function pxToSec(px, trackWidth) {
        var dur = Math.max(EditorController.duration, 0.001)
        return Math.max(0, Math.min(dur, (px / trackWidth) * dur))
    }

    onVisibleChanged: {
        if (visible && filepath) {
            EditorController.loadClip(filepath)
        } else if (!visible) {
            mp.pause()
        }
    }

    MouseArea {
        anchors.fill: parent
        onClicked: root.closed()
    }

    MediaPlayer {
        id: mp
        source: root.source
        videoOutput: videoOut
        audioOutput: AudioOutput { id: audioOut }
        onMediaStatusChanged: {
            if (mediaStatus === MediaPlayer.EndOfMedia) {
                mp.pause()
                mp.position = Math.round(EditorController.trimStart * 1000)
            }
        }
        onPositionChanged: {
            if (playbackState === MediaPlayer.PlayingState
                    && position >= EditorController.trimEnd * 1000) {
                mp.pause()
                mp.position = Math.round(EditorController.trimStart * 1000)
            }
        }
    }
    Component.onDestruction: mp.stop()

    Rectangle {
        id: panel
        anchors.centerIn: parent
        width: Math.min(parent.width - 64, 1200)
        height: Math.min(parent.height - 64, width * 0.68)
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border

        MouseArea { anchors.fill: parent }

        Column {
            anchors.fill: parent
            anchors.margins: 14
            spacing: 12

            // Title bar
            Item {
                width: parent.width
                height: 24
                Text {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 40
                    text: root.title
                    color: Theme.text
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
                Rectangle {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: 26; height: 26; radius: 13
                    color: closeArea.containsMouse ? Theme.accent : "transparent"
                    Text {
                        anchors.centerIn: parent
                        text: "✕"
                        color: Theme.text
                        font.pixelSize: 14
                    }
                    MouseArea {
                        id: closeArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.closed()
                    }
                }
            }

            // Video surface
            Rectangle {
                width: parent.width
                height: parent.height - 92 - timelineCol.height - 12
                radius: Theme.radius
                color: "#000000"
                clip: true

                VideoOutput {
                    id: videoOut
                    anchors.fill: parent
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: mp.playbackState === MediaPlayer.PlayingState
                               ? mp.pause() : mp.play()
                }

                Text {
                    anchors.centerIn: parent
                    visible: mp.mediaStatus === MediaPlayer.LoadingMedia
                             || mp.mediaStatus === MediaPlayer.BufferingMedia
                    text: "Loading…"
                    color: Theme.textDim
                    font.pixelSize: 14
                }
            }

            // Play control + time readout
            Row {
                width: parent.width
                height: 36
                spacing: 12

                Rectangle {
                    width: 36; height: 36; radius: 18
                    color: Theme.accent
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        anchors.centerIn: parent
                        text: mp.playbackState === MediaPlayer.PlayingState ? "⏸" : "▶"
                        color: "#ffffff"
                        font.pixelSize: 14
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            if (mp.playbackState === MediaPlayer.PlayingState) {
                                mp.pause()
                            } else {
                                if (mp.position < EditorController.trimStart * 1000
                                        || mp.position >= EditorController.trimEnd * 1000) {
                                    mp.position = Math.round(EditorController.trimStart * 1000)
                                }
                                mp.play()
                            }
                        }
                    }
                }

                Text {
                    text: root.fmt(mp.position / 1000) + " / " + root.fmt(EditorController.duration)
                    color: Theme.textDim
                    font.pixelSize: 12
                    anchors.verticalCenter: parent.verticalCenter
                }

                Item { width: parent.width - 340; height: 1 }

                Text {
                    text: "Trim: " + root.fmt(EditorController.trimStart) + " – " + root.fmt(EditorController.trimEnd)
                          + "  (" + root.fmt(EditorController.trimEnd - EditorController.trimStart) + ")"
                    color: Theme.text
                    font.pixelSize: 12
                    anchors.verticalCenter: parent.verticalCenter
                }
            }

            // Trim timeline with two handles
            Column {
                id: timelineCol
                width: parent.width
                spacing: 6

                Item {
                    id: track
                    width: parent.width
                    height: 28

                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width
                        height: 6
                        radius: 3
                        color: Theme.border
                    }

                    // Selected range highlight
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        height: 6
                        radius: 3
                        color: Theme.accent
                        x: EditorController.duration > 0
                           ? (EditorController.trimStart / EditorController.duration) * track.width : 0
                        width: EditorController.duration > 0
                               ? ((EditorController.trimEnd - EditorController.trimStart) / EditorController.duration) * track.width : 0
                    }

                    // Playhead
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 2
                        height: 20
                        color: Theme.text
                        visible: EditorController.duration > 0
                        x: EditorController.duration > 0
                           ? (mp.position / 1000 / EditorController.duration) * track.width : 0
                    }

                    // Handle hit area — determines nearest handle on press, drags it.
                    MouseArea {
                        anchors.fill: parent
                        preventStealing: true
                        onPressed: (mouse) => {
                            var t = root.pxToSec(mouse.x, track.width)
                            var dStart = Math.abs(t - EditorController.trimStart)
                            var dEnd = Math.abs(t - EditorController.trimEnd)
                            root.draggingHandle = dStart <= dEnd ? "start" : "end"
                        }
                        onPositionChanged: (mouse) => {
                            if (!root.draggingHandle)
                                return
                            var t = root.pxToSec(mouse.x, track.width)
                            if (root.draggingHandle === "start") {
                                EditorController.trimStart = Math.min(t, EditorController.trimEnd - root.minGapSec)
                            } else {
                                EditorController.trimEnd = Math.max(t, EditorController.trimStart + root.minGapSec)
                            }
                        }
                        onReleased: {
                            if (root.draggingHandle && root.filepath) {
                                EditorController.saveTrim(root.filepath, EditorController.trimStart, EditorController.trimEnd)
                            }
                            root.draggingHandle = ""
                        }
                    }

                    // Handle knobs (visual only — the MouseArea above does the dragging)
                    Rectangle {
                        width: 10; height: 20; radius: 3
                        color: "#ffffff"
                        border.width: 1
                        border.color: Theme.accent
                        anchors.verticalCenter: parent.verticalCenter
                        x: (EditorController.duration > 0
                            ? (EditorController.trimStart / EditorController.duration) * track.width : 0) - width / 2
                    }
                    Rectangle {
                        width: 10; height: 20; radius: 3
                        color: "#ffffff"
                        border.width: 1
                        border.color: Theme.accent
                        anchors.verticalCenter: parent.verticalCenter
                        x: (EditorController.duration > 0
                            ? (EditorController.trimEnd / EditorController.duration) * track.width : 0) - width / 2
                    }
                }

                // Export row
                Row {
                    width: parent.width
                    spacing: 12

                    Rectangle {
                        width: 120; height: 32; radius: Theme.radius
                        color: EditorController.exportRunning ? Theme.border : (exportArea.containsMouse ? Theme.accent : Theme.border)
                        Text {
                            anchors.centerIn: parent
                            text: EditorController.exportRunning ? "Exporting…" : "Export Trim"
                            color: Theme.text
                            font.pixelSize: 12
                        }
                        MouseArea {
                            id: exportArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            enabled: !EditorController.exportRunning
                            onClicked: EditorController.exportTrim(root.filepath, EditorController.trimStart, EditorController.trimEnd)
                        }
                    }

                    Text {
                        visible: EditorController.exportRunning
                        text: Math.round(Math.max(EditorController.exportProgress, 0)) + "%"
                        color: Theme.textDim
                        font.pixelSize: 12
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        visible: !EditorController.exportRunning && EditorController.exportResult.length > 0
                        text: "Saved: " + EditorController.exportResult
                        color: Theme.text
                        font.pixelSize: 12
                        elide: Text.ElideMiddle
                        width: parent.width - 300
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        visible: !EditorController.exportRunning && EditorController.exportError.length > 0
                        text: EditorController.exportError
                        color: "#ef4444"
                        font.pixelSize: 12
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }
        }
    }
}
