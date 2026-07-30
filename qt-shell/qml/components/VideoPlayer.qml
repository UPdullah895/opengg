import QtQuick
import QtQuick.Controls
import QtMultimedia
import com.opengg.app

// VideoPlayer — a modal overlay clip player over the Clips grid. Qt Multimedia
// MediaPlayer + VideoOutput on the GStreamer backend (PoC-validated, Wayland-
// native). Single audio track (library preview); the multi-track editor mixer
// is a Phase 5 concern. Includes the end-of-media reset (PoC gap #6).
Rectangle {
    id: root
    color: "#cc0b0d13"           // dim backdrop
    property string source: ""
    property string title: ""
    signal closed()

    focus: visible
    Keys.onEscapePressed: root.closed()

    function fmt(ms) {
        if (!ms || ms < 0)
            ms = 0
        var t = Math.floor(ms / 1000)
        var m = Math.floor(t / 60)
        var s = t % 60
        return m + ":" + (s < 10 ? "0" : "") + s
    }

    // Click on the backdrop (outside the panel) closes the player.
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
            // PoC gap #6: Qt Multimedia leaves the pipeline at EndOfMedia; reset
            // to a paused first frame so the clip can be replayed.
            if (mediaStatus === MediaPlayer.EndOfMedia) {
                mp.pause()
                mp.position = 0
            }
        }
        Component.onCompleted: play()
    }
    Component.onDestruction: mp.stop()

    // ── Player panel ──────────────────────────────────────────────────────
    Rectangle {
        id: panel
        anchors.centerIn: parent
        width: Math.min(parent.width - 64, 1200)
        height: Math.min(parent.height - 64, width * 0.62)
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border

        // Absorb clicks so they don't fall through to the backdrop.
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
                    Icon {
                        anchors.centerIn: parent
                        name: "x"; size: 14
                        color: Theme.text
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
                height: parent.height - 92
                radius: Theme.radius
                color: "#000000"
                clip: true

                VideoOutput {
                    id: videoOut
                    anchors.fill: parent
                }

                // Click video to toggle play/pause.
                MouseArea {
                    anchors.fill: parent
                    onClicked: mp.playbackState === MediaPlayer.PlayingState
                               ? mp.pause() : mp.play()
                }

                // Buffering / loading hint.
                Text {
                    anchors.centerIn: parent
                    visible: mp.mediaStatus === MediaPlayer.LoadingMedia
                             || mp.mediaStatus === MediaPlayer.BufferingMedia
                    text: "Loading…"
                    color: Theme.textDim
                    font.pixelSize: 14
                }
            }

            // Control bar
            Row {
                width: parent.width
                height: 36
                spacing: 12

                Rectangle {
                    width: 36; height: 36; radius: 18
                    color: Theme.accent
                    anchors.verticalCenter: parent.verticalCenter
                    Icon {
                        anchors.centerIn: parent
                        name: mp.playbackState === MediaPlayer.PlayingState ? "pause" : "play"
                        size: 14
                        color: "#ffffff"
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: mp.playbackState === MediaPlayer.PlayingState
                                   ? mp.pause() : mp.play()
                    }
                }

                Text {
                    text: root.fmt(mp.position)
                    color: Theme.textDim
                    font.pixelSize: 12
                    anchors.verticalCenter: parent.verticalCenter
                }

                Slider {
                    id: seek
                    width: parent.width - 220
                    anchors.verticalCenter: parent.verticalCenter
                    from: 0
                    to: Math.max(mp.duration, 1)
                    value: pressed ? value : mp.position
                    onMoved: mp.position = Math.round(value)

                    background: Rectangle {
                        x: seek.leftPadding
                        y: seek.topPadding + seek.availableHeight / 2 - height / 2
                        width: seek.availableWidth
                        height: 4
                        radius: 2
                        color: Theme.border
                        Rectangle {
                            width: seek.visualPosition * parent.width
                            height: parent.height
                            radius: 2
                            color: Theme.accent
                        }
                    }
                    handle: Rectangle {
                        x: seek.leftPadding + seek.visualPosition * (seek.availableWidth - width)
                        y: seek.topPadding + seek.availableHeight / 2 - height / 2
                        width: 14; height: 14; radius: 7
                        color: Theme.accent
                    }
                }

                Text {
                    text: root.fmt(mp.duration)
                    color: Theme.textDim
                    font.pixelSize: 12
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }
    }
}
