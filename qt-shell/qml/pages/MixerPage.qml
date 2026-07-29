import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Mixer — full channel-strip view over live daemon audio (reuses AudioController).
Rectangle {
    id: page
    color: Theme.bg

    property var channels: AudioController.channelsJson
        ? JSON.parse(AudioController.channelsJson)
        : []
    property var vuLevels: AudioController.vuLevelsJson
        ? JSON.parse(AudioController.vuLevelsJson)
        : {}

    // Current volume (0..150) for a named channel, or 100 if not found yet
    // (matches ChatMix's usage: `audio.channelMap['Game']?.volume ?? 100`).
    function volumeFor(name) {
        for (const ch of page.channels) {
            if (ch.name === name) return ch.volume
        }
        return 100
    }

    function vuDb(name) {
        return name in page.vuLevels ? page.vuLevels[name] : -60
    }
    // 0..1 fill fraction from a -60..0 dB range, matching ChannelStrip.vue.
    function vuFrac(name) {
        const db = page.vuDb(name)
        return Math.max(0, Math.min(1, (db + 60) / 60))
    }
    function vuColor(name, baseColor) {
        const db = page.vuDb(name)
        return db > -3 ? "#ef4444" : db > -12 ? "#f59e0b" : baseColor
    }

    property string activeTab: "mixer"
    // Overdrive — expands fader range from 100% to 150%. Client-side UI
    // state only (matches MixerPage.vue's overdriveEnabled ref: not
    // persisted). Disabling it clamps any channel currently above 100%
    // back down, mirroring clampChannelsTo100().
    property bool overdriveEnabled: false
    readonly property var mixerChannelNames: ["Master", "Game", "Chat", "Media", "Aux", "Mic"]
    onOverdriveEnabledChanged: {
        if (overdriveEnabled) return
        for (const modelData of page.channels) {
            if (page.mixerChannelNames.includes(modelData.name) && modelData.volume > 100)
                AudioController.setVolume(modelData.name, 100)
        }
    }
    readonly property var tabs: [
        { id: "mixer", label: "Mixer" },
        { id: "game", label: "Game" },
        { id: "chat", label: "Chat" },
        { id: "media", label: "Media" },
        { id: "aux", label: "Aux" },
        { id: "mic", label: "Mic" },
    ]
    readonly property var channelColors: ({
        Master: "#94A3B8", Game: "#E94560", Chat: "#3B82F6",
        Media: "#10B981", Aux: "#A855F7", Mic: "#F59E0B",
    })

    Component.onCompleted: AudioController.refresh()
    Timer { interval: 2000; running: true; repeat: true; onTriggered: AudioController.refresh() }

    // The VU stream spawns a real pw-cat subprocess per channel — only run
    // it while this page is actually the visible one. StackLayout sets
    // `visible: false` on inactive children, so this is a cheap, correct
    // gate without reaching into Main.qml's currentPage.
    onVisibleChanged: {
        if (visible) AudioController.startVuStream()
        else AudioController.stopVuStream()
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 28
        spacing: 20

        RowLayout {
            Layout.fillWidth: true
            Text {
                text: (I18n.language, I18n.t("nav.mixer"))
                color: Theme.text
                font.pixelSize: 26
                font.weight: Font.Bold
                Layout.fillWidth: true
            }
            Rectangle {
                width: 8; height: 8; radius: 4
                color: AudioController.connected ? "#22c55e" : "#ef4444"
                Layout.alignment: Qt.AlignVCenter
            }
            Text {
                text: AudioController.connected ? "daemon connected" : "daemon offline"
                color: Theme.textDim
                font.pixelSize: 12
            }
        }

        // Tab bar — Mixer (fader strips) + one tab per EQ/DSP channel.
        RowLayout {
            Layout.fillWidth: false
            spacing: 4

            Repeater {
                model: page.tabs
                Rectangle {
                    id: tabBtn
                    required property var modelData
                    property bool isActive: page.activeTab === modelData.id
                    width: tabLabel.implicitWidth + 24
                    height: 30
                    radius: Theme.radius
                    color: isActive ? Theme.surface : "transparent"

                    Text {
                        id: tabLabel
                        anchors.centerIn: parent
                        text: tabBtn.modelData.label
                        color: tabBtn.isActive ? Theme.text : Theme.textDim
                        font.pixelSize: 12
                        font.weight: tabBtn.isActive ? Font.DemiBold : Font.Normal
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: page.activeTab = tabBtn.modelData.id
                    }
                }
            }

            // Overdrive — unlocks faders beyond 100% (up to 150%).
            Rectangle {
                visible: page.activeTab === "mixer"
                width: 30; height: 30
                radius: Theme.radius
                color: page.overdriveEnabled ? Qt.rgba(0.961, 0.620, 0.043, 0.15) : "transparent"
                border.width: 1
                border.color: page.overdriveEnabled ? "#f59e0b" : Theme.border

                Text {
                    anchors.centerIn: parent
                    text: "⚡"
                    font.pixelSize: 14
                }
                MouseArea {
                    id: overdriveArea
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    hoverEnabled: true
                    onClicked: page.overdriveEnabled = !page.overdriveEnabled
                }
                ToolTip {
                    visible: overdriveArea.containsMouse
                    text: page.overdriveEnabled
                        ? "Overdrive ON — faders go to 150%"
                        : "Enable Overdrive (faders up to 150%)"
                    delay: 300
                }
            }
        }

        // Row of channel strips
        RowLayout {
            visible: page.activeTab === "mixer"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.maximumHeight: 420
            spacing: 16

            Repeater {
                model: page.channels

                // One channel strip
                Rectangle {
                    required property var modelData
                    Layout.preferredWidth: 130
                    Layout.fillHeight: true
                    radius: Theme.radius
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.border

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 14
                        spacing: 10

                        // Channel name
                        Text {
                            text: modelData.name
                            color: Theme.text
                            font.pixelSize: 15
                            font.weight: Font.DemiBold
                            Layout.alignment: Qt.AlignHCenter
                        }

                        // Volume % (orange once overdrive pushes it past 100%)
                        Text {
                            text: Math.round(modelData.volume) + "%"
                            color: modelData.volume > 100 ? "#f59e0b" : Theme.textDim
                            font.pixelSize: 12
                            Layout.alignment: Qt.AlignHCenter
                        }

                        // Vertical fader + VU meter
                        RowLayout {
                            Layout.alignment: Qt.AlignHCenter
                            Layout.fillHeight: true
                            spacing: 8

                            Slider {
                                id: fader
                                orientation: Qt.Vertical
                                Layout.fillHeight: true
                                from: 0; to: page.overdriveEnabled ? 150 : 100
                                value: modelData.volume
                                onMoved: AudioController.setVolume(modelData.name, Math.round(value))

                                background: Rectangle {
                                    x: fader.leftPadding + fader.availableWidth / 2 - width / 2
                                    y: fader.topPadding
                                    width: 6
                                    height: fader.availableHeight
                                    radius: 3
                                    color: Theme.border
                                    Rectangle {
                                        // vertical slider: visualPosition is 0 at the top (max),
                                        // so the filled level from the bottom is (1 - visualPosition).
                                        width: parent.width
                                        height: (1 - fader.visualPosition) * parent.height
                                        y: parent.height - height
                                        radius: 3
                                        color: modelData.volume > 100 ? "#f59e0b" : Theme.accent
                                    }
                                }
                                handle: Rectangle {
                                    x: fader.leftPadding + fader.availableWidth / 2 - width / 2
                                    y: fader.topPadding + fader.visualPosition * (fader.availableHeight - height)
                                    width: 20; height: 20; radius: 10
                                    color: Theme.text
                                    border.width: 2
                                    border.color: modelData.volume > 100 ? "#f59e0b" : Theme.accent
                                }
                            }

                            // Live VU bar — fills from the bottom, -60..0 dB.
                            Rectangle {
                                Layout.fillHeight: true
                                Layout.topMargin: fader.topPadding
                                Layout.bottomMargin: fader.bottomPadding
                                width: 5
                                radius: 3
                                color: Theme.bg
                                border.width: 1
                                border.color: Theme.border

                                Rectangle {
                                    width: parent.width
                                    height: page.vuFrac(modelData.name) * parent.height
                                    y: parent.height - height
                                    radius: 3
                                    color: page.vuColor(modelData.name, Theme.accent)
                                    visible: AudioController.vuRunning
                                }
                            }
                        }

                        // dB readout
                        Text {
                            text: AudioController.vuRunning
                                ? (page.vuDb(modelData.name) <= -59.9 ? "—" : page.vuDb(modelData.name).toFixed(1) + " dB")
                                : ""
                            color: Theme.textDim
                            font.pixelSize: 9
                            Layout.alignment: Qt.AlignHCenter
                        }

                        // Mute
                        Rectangle {
                            Layout.alignment: Qt.AlignHCenter
                            width: 40; height: 30
                            radius: Theme.radius
                            color: modelData.muted ? Qt.rgba(0.914, 0.271, 0.376, 0.15) : "transparent"
                            border.width: 1
                            border.color: modelData.muted ? Theme.accent : Theme.border
                            Text {
                                anchors.centerIn: parent
                                text: modelData.muted ? "🔇" : "🔊"
                                font.pixelSize: 13
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: AudioController.setMute(modelData.name, !modelData.muted)
                            }
                        }

                        // Routed apps
                        Text {
                            text: modelData.apps && modelData.apps.length > 0
                                ? modelData.apps.map(a => a.name).join(", ")
                                : "—"
                            color: Theme.textDim
                            font.pixelSize: 10
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                        }
                    }
                }
            }

            Item { Layout.fillWidth: true } // push strips to the leading edge
        }

        // ChatMix — Game/Chat balance slider. Balance is derived from the
        // actual channel volumes (not a separate stored ref), matching
        // ChatMix.vue's two-way-sync design: dragging it calls setVolume on
        // both channels, and it re-centers correctly if either volume is
        // changed by any other route (fader drag, Ear Blast ducking, etc).
        Rectangle {
            id: chatMixBar
            visible: page.activeTab === "mixer" && AudioController.virtualAudioReady
            Layout.fillWidth: true
            Layout.preferredWidth: 480
            radius: Theme.radius
            color: Theme.surface
            border.width: 1
            border.color: Theme.border
            implicitHeight: chatMixCol.implicitHeight + 24

            readonly property real gameVol: page.volumeFor("Game")
            readonly property real chatVol: page.volumeFor("Chat")
            readonly property real balance: Math.max(-100, Math.min(100, chatVol - gameVol))
            readonly property int gameLevel: Math.round(Math.max(0, 100 - Math.max(0, balance)))
            readonly property int chatLevel: Math.round(Math.max(0, 100 - Math.max(0, -balance)))

            ColumnLayout {
                id: chatMixCol
                anchors.fill: parent
                anchors.margins: 12
                spacing: 8

                RowLayout {
                    Layout.fillWidth: true
                    Text {
                        text: (I18n.language, I18n.t("chatMix.title"))
                        color: Theme.textDim
                        font.pixelSize: 11
                        font.weight: Font.Bold
                        Layout.fillWidth: true
                    }
                    Rectangle {
                        width: 20; height: 20; radius: Theme.radius
                        color: "transparent"
                        Text { anchors.centerIn: parent; text: "↺"; color: Theme.textDim; font.pixelSize: 13 }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                AudioController.setVolume("Game", 100)
                                AudioController.setVolume("Chat", 100)
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 12

                    Text {
                        text: (I18n.language, I18n.t("chatMix.game")) + "  " + chatMixBar.gameLevel + "%"
                        color: page.channelColors.Game
                        font.pixelSize: 11
                        font.weight: Font.DemiBold
                    }

                    Slider {
                        id: chatMixSlider
                        Layout.fillWidth: true
                        from: -100; to: 100
                        value: chatMixBar.balance
                        onMoved: {
                            const g = Math.round(Math.max(0, 100 - Math.max(0, value)))
                            const c = Math.round(Math.max(0, 100 - Math.max(0, -value)))
                            AudioController.setVolume("Game", g)
                            AudioController.setVolume("Chat", c)
                        }

                        background: Rectangle {
                            x: chatMixSlider.leftPadding
                            y: chatMixSlider.topPadding + chatMixSlider.availableHeight / 2 - height / 2
                            width: chatMixSlider.availableWidth
                            height: 6
                            radius: 3
                            gradient: Gradient {
                                orientation: Gradient.Horizontal
                                GradientStop { position: 0.0; color: page.channelColors.Game }
                                GradientStop { position: 0.5; color: Theme.border }
                                GradientStop { position: 1.0; color: page.channelColors.Chat }
                            }
                        }
                        handle: Rectangle {
                            x: chatMixSlider.leftPadding + chatMixSlider.visualPosition * (chatMixSlider.availableWidth - width)
                            y: chatMixSlider.topPadding + chatMixSlider.availableHeight / 2 - height / 2
                            width: 18; height: 18; radius: 9
                            color: Theme.text
                            border.width: 2
                            border.color: Theme.accent
                        }
                    }

                    Text {
                        text: chatMixBar.chatLevel + "%  " + (I18n.language, I18n.t("chatMix.chat"))
                        color: page.channelColors.Chat
                        font.pixelSize: 11
                        font.weight: Font.DemiBold
                    }
                }
            }
        }

        // Per-channel EQ/DSP tab content. Each panel is a static child kept
        // alive for the whole session (not a Loader) so its jalv engine and
        // band/toggle state survive switching tabs — only visibility toggles.
        ScrollView {
            visible: page.activeTab !== "mixer"
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth

            ColumnLayout {
                width: parent.width
                spacing: 20

                GraphicEQ {
                    Layout.fillWidth: true
                    visible: page.activeTab === "game"
                    channel: "Game"
                    accentColor: page.channelColors.Game
                }
                GraphicEQ {
                    Layout.fillWidth: true
                    visible: page.activeTab === "media"
                    channel: "Media"
                    accentColor: page.channelColors.Media
                }
                GraphicEQ {
                    Layout.fillWidth: true
                    visible: page.activeTab === "aux"
                    channel: "Aux"
                    accentColor: page.channelColors.Aux
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    visible: page.activeTab === "chat"
                    spacing: 24
                    GraphicEQ { Layout.fillWidth: true; channel: "Chat"; accentColor: page.channelColors.Chat }
                    DspControls { Layout.fillWidth: true; channel: "Chat"; accentColor: page.channelColors.Chat }
                }
                DspControls {
                    Layout.fillWidth: true
                    visible: page.activeTab === "mic"
                    channel: "Mic"
                    accentColor: page.channelColors.Mic
                }
            }
        }
    }
}
