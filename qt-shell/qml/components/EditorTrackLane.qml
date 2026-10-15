import QtQuick
import QtQuick.Controls
import com.opengg.app

// One row of the clip editor's timeline: a fixed-width label gutter followed
// by the track's coloured lane. Audio lanes draw their real waveform on top of
// the tinted bar once `peaks` arrives (EditorController.requestWaveform decodes
// it off-thread); until then, and for the video lane, the bar alone conveys
// track identity, extent and monitoring state.
Item {
    id: lane

    property int gutter: 96
    property string label: ""
    property string icon: ""
    property color accent: Theme.accent
    /// Audio lanes can be monitored (Qt Multimedia plays one track at a time);
    /// the video lane cannot.
    property bool monitorable: false
    property bool monitoring: false

    /// Per-track gain, 0..1. Only meaningful when `monitorable`.
    property real volume: 1.0

    /// Amplitude peaks in 0..1, one per time slice, or empty for no waveform.
    property var peaks: []

    signal monitorToggled()
    /// User dragged this track's gain. Named *Requested rather than
    /// volumeChanged: a `volume` property already owns that signal name.
    signal volumeRequested(real value)

    implicitHeight: 30

    Row {
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: lane.gutter
        spacing: 6

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: lane.icon
            size: 12
            color: lane.accent
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: lane.gutter - 46
            text: lane.label
            color: Theme.text
            font.pixelSize: 11
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
        // Speaker: opens a gain slider rather than hard-muting on click.
        // Muting outright on a single click made the button a trap — there
        // was no way to simply turn a track down.
        Rectangle {
            id: monBtn
            anchors.verticalCenter: parent.verticalCenter
            visible: lane.monitorable
            width: 20; height: 20; radius: 4
            color: monArea.containsMouse || volPop.visible
                   ? Theme.bgHover : "transparent"
            Icon {
                anchors.centerIn: parent
                name: !lane.monitoring || lane.volume <= 0.001 ? "volume-x"
                    : lane.volume < 0.5 ? "volume-1" : "volume-2"
                size: 12
                color: lane.monitoring ? lane.accent : Theme.textMuted
            }
            MouseArea {
                id: monArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: volPop.toggle()
            }

            // FlyoutPopup, so it opens ABOVE the speaker when there is no
            // room below. The lanes sit at the very bottom of the editor, so
            // a fader that always dropped downward ran off the window and
            // could not be dragged at all.
            FlyoutPopup {
                id: volPop
                anchorItem: monBtn
                centerOnAnchor: true
                width: 40
                implicitHeight: volCol.implicitHeight + 12
                padding: 6

                // Leaving the button AND the popover closes it shortly after,
                // so it behaves like a hover flyout without snapping shut the
                // instant the pointer crosses the gap between the two.
                HoverHandler { id: popHover }
                Timer {
                    id: leaveTimer
                    interval: 700
                    onTriggered: if (!popHover.hovered && !monArea.containsMouse
                                     && !laneVol.pressed) volPop.close()
                }
                Connections {
                    target: popHover
                    function onHoveredChanged() { if (!popHover.hovered) leaveTimer.restart() }
                }
                Connections {
                    target: monArea
                    function onContainsMouseChanged() {
                        if (!monArea.containsMouse) leaveTimer.restart()
                    }
                }

                // Vertical, bottom-to-top: a fader reads as a level, and it
                // keeps the flyout narrow enough to sit over the lane rather
                // than stretching across it. The mute toggle sits at the foot
                // of the fader, where the track bottoms out.
                contentItem: Column {
                    id: volCol
                    spacing: 4

                    VSlider {
                        id: laneVol
                        anchors.horizontalCenter: parent.horizontalCenter
                        trackHeight: 90
                        from: 0; to: 100
                        suffix: ""
                        sliderColor: lane.accent
                        value: Math.round(lane.volume * 100)
                        onMoved: (v) => lane.volumeRequested(v / 100)
                    }
                    Icon {
                        anchors.horizontalCenter: parent.horizontalCenter
                        name: lane.monitoring ? "volume-2" : "volume-x"
                        size: 13
                        color: lane.monitoring ? lane.accent : Theme.textMuted
                        MouseArea {
                            anchors.fill: parent
                            anchors.margins: -4
                            cursorShape: Qt.PointingHandCursor
                            onClicked: lane.monitorToggled()
                        }
                    }
                }
            }
        }
    }

    Rectangle {
        anchors.left: parent.left
        anchors.leftMargin: lane.gutter
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        // Grows with the lane, so dragging the timeline taller gives the
        // waveform more room instead of just adding empty space.
        height: Math.max(22, lane.height - 8)
        radius: 4
        color: Theme.tint(lane.accent, lane.monitorable && !lane.monitoring ? 10 : 28)
        border.width: 1
        border.color: Theme.tint(lane.accent, 55)

        Canvas {
            id: wave
            anchors.fill: parent
            anchors.margins: 2
            visible: lane.peaks && lane.peaks.length > 0
            // Muted tracks keep their waveform but recede, matching how the
            // bar itself dims — the shape is still useful for seeking.
            opacity: lane.monitorable && !lane.monitoring ? 0.45 : 1.0

            onPaint: {
                const ctx = wave.getContext("2d")
                ctx.reset()
                const src = lane.peaks || []
                if (src.length === 0 || wave.width <= 0 || wave.height <= 0)
                    return

                // One column per pixel at most: with 400+ peaks in a ~700px
                // lane the extra peaks would just overdraw each other, so
                // buckets are collapsed to their maximum instead.
                const cols = Math.max(1, Math.min(src.length, Math.floor(wave.width)))
                const per = src.length / cols
                const colW = wave.width / cols
                const mid = wave.height / 2

                ctx.fillStyle = lane.accent
                for (let c = 0; c < cols; c++) {
                    let peak = 0
                    const end = Math.min(src.length, Math.round((c + 1) * per))
                    for (let i = Math.round(c * per); i < end; i++)
                        peak = Math.max(peak, src[i])
                    // A 1px floor keeps silence visible as a centre line
                    // rather than a gap in the track.
                    const h = Math.max(1, peak * wave.height)
                    ctx.fillRect(c * colW, mid - h / 2, Math.max(1, colW), h)
                }
            }

            onWidthChanged: wave.requestPaint()
            onHeightChanged: wave.requestPaint()
        }

        // `peaks` is a plain var, so the Canvas has to be told to repaint when
        // its contents are swapped in — a binding alone would not redraw.
        Connections {
            target: lane
            function onPeaksChanged() { wave.requestPaint() }
            function onAccentChanged() { wave.requestPaint() }
        }
    }
}
