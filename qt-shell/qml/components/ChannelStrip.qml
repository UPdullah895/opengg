import QtQuick
import QtQuick.Controls
import com.opengg.app

// ChannelStrip — one mixer channel column. Extracted from an inline MixerPage
// block and rebuilt against ChannelStrip.vue, which the inline version had
// reduced to just a name, a percentage, a fader and a mute button.
//
// Restored here, following ChannelStrip.vue's template and scoped styles:
// the channel-coloured accent bar across the top, the tinted icon badge, the
// VU meter with its 0/-20/-40/-60 tick scale, the dB readout under the
// percentage, and the output-device selector beneath the mute button.
Rectangle {
    id: strip

    property string name: ""
    property real volume: 100
    property bool muted: false
    property color channelColor: Theme.accent
    property string iconName: "volume-2"
    /// dBFS from the VU stream; -60 reads as silence.
    property real vuDb: -60
    property int maxVolume: 100
    property var devices: []
    property string selectedDevice: ""

    signal volumeRequested(int value)
    signal muteToggled()
    signal deviceRequested(string device)

    radius: Theme.radiusLg
    color: Theme.surface
    border.width: 1
    border.color: Theme.border
    clip: true

    // 0 dB at the top of the meter, -60 at the bottom.
    readonly property real vuFraction: Math.max(0, Math.min(1, (vuDb + 60) / 60))

    // While dragging, the fill/thumb track the mouse directly rather than
    // `volume` (which only moves once AudioController.setVolume's D-Bus/
    // pactl round trip completes and channelsJson refreshes) — otherwise
    // every pixel of drag waits on that round trip and the fader visibly
    // lags or stutters behind the cursor.
    property bool dragging: false
    property real dragFraction: 0
    readonly property real fillFraction: strip.dragging
        ? strip.dragFraction
        : Math.max(0, Math.min(1, volume / strip.maxVolume))

    // ── Accent bar (ChannelStrip.vue's .accent-bar) ──────────────────────
    Rectangle {
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: 3
        color: strip.channelColor
    }

    Column {
        anchors.fill: parent
        anchors.topMargin: 16
        anchors.bottomMargin: 12
        spacing: 8

        // ── Header: tinted icon badge + name ─────────────────────────────
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 30; height: 30
            radius: Theme.radius
            // .icon-box uses `color + '18'` — ~9% alpha.
            color: Theme.tint(strip.channelColor, 9)
            Icon {
                anchors.centerIn: parent
                name: strip.iconName
                size: 16
                color: strip.channelColor
            }
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: strip.name.toUpperCase()
            color: Theme.text
            font.pixelSize: 12
            font.weight: Font.Bold
            font.letterSpacing: 0.5
        }

        // ── VU meter + fader ─────────────────────────────────────────────
        Item {
            id: faderRow
            width: parent.width
            height: strip.height - 168

            Row {
                anchors.centerIn: parent
                spacing: 10

                // VU: tick labels + level track
                Row {
                    spacing: 4
                    height: faderRow.height

                    Column {
                        height: parent.height
                        width: 16
                        Repeater {
                            model: ["0", "-20", "-40", "-60"]
                            Item {
                                width: 16
                                height: faderRow.height / 4
                                Text {
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    text: modelData
                                    color: Theme.textMuted
                                    font.pixelSize: 7
                                }
                            }
                        }
                    }

                    Rectangle {
                        width: 6
                        height: parent.height
                        radius: 3
                        color: Theme.bgDeep
                        Rectangle {
                            anchors.bottom: parent.bottom
                            width: parent.width
                            height: parent.height * strip.vuFraction
                            radius: 3
                            color: strip.channelColor
                        }
                    }
                }

                // Fader: track, channel-coloured fill, grip thumb
                Item {
                    width: 22
                    height: faderRow.height

                    Rectangle {
                        id: faderTrack
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 6
                        height: parent.height
                        radius: 3
                        color: Theme.bgDeep

                        Rectangle {
                            anchors.bottom: parent.bottom
                            width: parent.width
                            height: parent.height * strip.fillFraction
                            radius: 3
                            color: strip.muted ? Theme.textMuted : strip.channelColor
                        }
                    }

                    Rectangle {
                        id: thumb
                        width: 20; height: 12
                        radius: 3
                        color: strip.muted ? Theme.textMuted : strip.channelColor
                        border.width: 1
                        border.color: Theme.scrim(25)
                        anchors.horizontalCenter: parent.horizontalCenter
                        y: (faderTrack.height - height) * (1 - strip.fillFraction)

                        // .grip — the lighter bar across the thumb
                        Rectangle {
                            anchors.centerIn: parent
                            width: 10; height: 2
                            radius: 1
                            color: "#ffffff"
                            opacity: 0.75
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        // Widen the hit area either side of the 6px track.
                        anchors.margins: -8
                        cursorShape: Qt.PointingHandCursor
                        function apply(my) {
                            var f = 1 - Math.max(0, Math.min(1, (my + 8) / faderTrack.height))
                            strip.dragFraction = f
                            strip.volumeRequested(Math.round(f * strip.maxVolume))
                        }
                        onPressed: (m) => { strip.dragging = true; apply(m.y) }
                        onPositionChanged: (m) => { if (pressed) apply(m.y) }
                        onReleased: strip.dragging = false
                        onCanceled: strip.dragging = false
                    }
                }
            }
        }

        // ── Volume + dB readout ──────────────────────────────────────────
        Text {
            readonly property real displayVolume: strip.dragging
                ? strip.dragFraction * strip.maxVolume
                : strip.volume
            anchors.horizontalCenter: parent.horizontalCenter
            text: Math.round(displayVolume) + "%"
            color: strip.muted ? Theme.textMuted
                 : displayVolume > 100 ? Theme.overdrive : strip.channelColor
            font.pixelSize: 17
            font.weight: Font.Bold
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: strip.vuDb <= -60 ? "— dB" : strip.vuDb.toFixed(1) + " dB"
            color: Theme.textMuted
            font.pixelSize: 9
        }

        // ── Mute + device selector ───────────────────────────────────────
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 6

            Rectangle {
                width: 28; height: 26
                radius: Theme.radius
                color: strip.muted ? Theme.accentAlpha(15) : "transparent"
                border.width: 1
                border.color: strip.muted ? Theme.accent : Theme.border
                Icon {
                    anchors.centerIn: parent
                    name: strip.muted ? "volume-x" : "volume-1"
                    size: 13
                    color: strip.muted ? Theme.accent : Theme.textDim
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: strip.muteToggled()
                }
            }

            ComboBox {
                id: devBox
                visible: strip.devices.length > 0
                width: strip.width - 52
                height: 26
                model: strip.devices
                font.pixelSize: 9
                currentIndex: Math.max(0, strip.devices.indexOf(strip.selectedDevice))
                onActivated: strip.deviceRequested(String(strip.devices[currentIndex]))

                contentItem: Row {
                    leftPadding: 6
                    spacing: 4
                    Icon {
                        name: "headphones"
                        size: 11
                        color: Theme.textDim
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Text {
                        width: devBox.width - 34
                        text: devBox.displayText
                        color: Theme.textDim
                        font.pixelSize: 9
                        elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
                background: Rectangle {
                    radius: Theme.radius
                    color: Theme.bg
                    border.width: 1
                    border.color: Theme.border
                }
                delegate: ItemDelegate {
                    width: devBox.width
                    highlighted: devBox.highlightedIndex === index
                    contentItem: Text {
                        text: modelData
                        color: highlighted ? Theme.accent : Theme.text
                        font.pixelSize: 10
                        elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter
                    }
                    background: Rectangle { color: highlighted ? Theme.accentAlpha(10) : Theme.surface }
                }
            }
        }
    }
}
