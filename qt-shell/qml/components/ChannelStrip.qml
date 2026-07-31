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

    // Anti-snap: keep showing the dragged value for a moment after release so
    // a refresh that was already in flight when the drag ended can't yank the
    // fader back to a pre-drag value. MixerPage.vue's store does the same with
    // its 3s "ignore polling after user interaction" window.
    Timer { id: settleTimer; interval: 600; onTriggered: strip.dragging = false }
    function settle() { settleTimer.restart() }

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
                        id: faderArea
                        anchors.fill: parent
                        // Widen the hit area either side of the 6px track.
                        anchors.margins: -8
                        cursorShape: Qt.PointingHandCursor

                        // Where the grab started, so the drag is *relative*.
                        // Previously a press jumped the value to the pressed
                        // position, so grabbing the thumb anywhere but its
                        // exact centre made the fader visibly jump before it
                        // started following the mouse. Now pressing parks the
                        // value where it already is and only movement changes
                        // it — you grab the fader and carry it, wherever on
                        // the track you happened to take hold of it.
                        property real grabY: 0
                        property real grabFraction: 0

                        onPressed: (m) => {
                            // Read fillFraction BEFORE setting `dragging` —
                            // that flag is what makes fillFraction switch from
                            // the volume-derived value to dragFraction, so
                            // flipping it first made this capture the stale
                            // dragFraction (0 on a fresh strip) and slam the
                            // fader to silence the instant you touched it.
                            faderArea.grabY = m.y
                            faderArea.grabFraction = strip.fillFraction
                            strip.dragFraction = faderArea.grabFraction
                            strip.dragging = true
                        }
                        onPositionChanged: (m) => {
                            if (!faderArea.pressed || faderTrack.height <= 0)
                                return
                            var f = faderArea.grabFraction
                                  - (m.y - faderArea.grabY) / faderTrack.height
                            f = Math.max(0, Math.min(1, f))
                            strip.dragFraction = f
                            strip.volumeRequested(Math.round(f * strip.maxVolume))
                        }
                        onReleased: strip.settle()
                        onCanceled: strip.settle()
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
                // `strip.devices` is now a list of {value, label} objects —
                // value is the real pactl node.name (what setChannelDevice
                // needs), label is the friendly description (what the user
                // should read). textRole/valueRole make displayText,
                // indexOfValue() and currentValue all resolve through
                // `label`/`value` automatically instead of stringifying the
                // whole object.
                textRole: "label"
                valueRole: "value"
                model: strip.devices
                font.pixelSize: 9
                // Previously `strip.selectedDevice` was always "" (MixerPage
                // never passed the channel's actual current device), so this
                // fell through to index 0 no matter what was really
                // selected — the dropdown could show one device while a
                // different one was actually in use, and there was no way to
                // tell from the UI. MixerPage now supplies the real value
                // via AudioController.channelDevicesJson.
                currentIndex: strip.selectedDevice ? devBox.indexOfValue(strip.selectedDevice) : -1
                // Reads the model directly by the index the signal hands us,
                // rather than `devBox.currentValue` — confirmed live that
                // `currentValue` can still read the pre-click value at the
                // moment `activated` fires, silently sending the OLD device
                // back to setChannelDevice (which then no-ops, since
                // core::set_channel_device treats "already linked to this
                // device" as nothing to do — so nothing even errors, the
                // dropdown just visually shows the new pick while the real
                // PipeWire routing never moves).
                onActivated: (index) => strip.deviceRequested(strip.devices[index].value)

                // The default popup sizes itself to the ComboBox's own
                // width, which is only ~80px on a 132px-wide strip — far too
                // narrow for "Arctis Nova 7 Analog Stereo". Widened
                // independently of the trigger.
                popup.width: Math.max(220, devBox.width)

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
                    width: devBox.popup.width
                    height: 32
                    highlighted: devBox.highlightedIndex === index
                    contentItem: Text {
                        text: modelData.label
                        color: highlighted ? Theme.accent : Theme.text
                        font.pixelSize: 11
                        elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter
                    }
                    background: Rectangle { color: highlighted ? Theme.accentAlpha(10) : Theme.surface }
                }
            }
        }
    }
}
