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
    // Every live audio-producing app (routed or not), used to build the
    // "route an app to a channel" UI — derived client-side the same way
    // audio.ts's `channelMap` computed groups `allApps` by `channel`,
    // rather than trusting channels_json's own per-channel apps field.
    property var apps: AudioController.appsJson
        ? JSON.parse(AudioController.appsJson)
        : []
    readonly property var routeTargets: ["Master", "Game", "Chat", "Media", "Aux"]

    function appsForChannel(name) {
        if (name === "Master")
            return page.apps.filter(a => !a.locked && (!a.channel || a.channel === "" || a.channel === "Master"))
        return page.apps.filter(a => a.channel === name)
    }

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
        return db > -3 ? Theme.danger : db > -12 ? Theme.overdrive : baseColor
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
    // MixerPage.vue renders SIX strips: Master + the five daemon channels.
    // Master is not returned by the daemon — audio.ts synthesizes it (volume
    // defaults to 100, unmuted), so this does the same.
    readonly property var stripChannels: {
        var out = []
        var byName = {}
        for (var i = 0; i < page.channels.length; i++)
            byName[page.channels[i].name] = page.channels[i]
        var master = byName["Master"] || { name: "Master", volume: 100, muted: false }
        out.push(master)
        for (var j = 0; j < page.mixerChannelNames.length; j++) {
            var n = page.mixerChannelNames[j]
            if (n !== "Master" && byName[n])
                out.push(byName[n])
        }
        return out
    }

    readonly property var channelIcons: ({
        Master: "volume-2", Game: "gamepad", Chat: "headphones",
        Media: "play", Aux: "music", Mic: "mic"
    })

    property var outputDevices: JSON.parse(AudioController.outputDevicesJson || "[]")
    property var inputDevices: (JSON.parse(AudioController.inputDevicesJson || "[]"))
        .map(function (d) { return d.label || d.value || d })

    readonly property var channelColors: ({
        Master: "#94A3B8", Game: "#E94560", Chat: "#3B82F6",
        Media: "#10B981", Aux: "#A855F7", Mic: "#F59E0B",
    })

    Component.onCompleted: {
        AudioController.refresh()
        AudioController.refreshDevices()
    }
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
                // MixerPage.vue's heading is "Audio Mixer", not the nav label.
            text: (I18n.language, I18n.t("dashboard.audioMixer"))
                color: Theme.text
                font.pixelSize: 26
                font.weight: Font.Bold
                Layout.fillWidth: true
            }
            Rectangle {
                width: 8; height: 8; radius: 4
                color: AudioController.connected ? Theme.success : Theme.danger
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
                color: page.overdriveEnabled ? Theme.tint(Theme.overdrive, 15) : "transparent"
                border.width: 1
                border.color: page.overdriveEnabled ? Theme.overdrive : Theme.border

                Icon {
                    anchors.centerIn: parent
                    name: "zap"; size: 13
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
            id: stripsRow
            visible: page.activeTab === "mixer"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.maximumHeight: 420
            spacing: 16

            Component.onCompleted: TourController.registerTarget("mixer-channels", stripsRow)
            Component.onDestruction: TourController.unregisterTarget("mixer-channels")

            Repeater {
                model: page.stripChannels

                ChannelStrip {
                    required property var modelData
                    Layout.preferredWidth: 132
                    Layout.fillHeight: true

                    name: modelData.name
                    volume: modelData.volume
                    muted: !!modelData.muted
                    channelColor: page.channelColors[modelData.name] || Theme.accent
                    iconName: page.channelIcons[modelData.name] || "volume-2"
                    vuDb: page.vuDb(modelData.name)
                    maxVolume: page.overdriveEnabled ? 150 : 100
                    devices: modelData.name === "Mic" ? page.inputDevices : page.outputDevices
                    selectedDevice: ""

                    onVolumeRequested: (v) => AudioController.setVolume(modelData.name, v)
                    onMuteToggled: AudioController.setMute(modelData.name, !modelData.muted)
                    onDeviceRequested: (d) => AudioController.setChannelDevice(modelData.name, d)
                }
            }
            Item { Layout.fillWidth: true } // push strips to the leading edge
        }

        // App routing — click an app pill to expand an inline channel
        // picker. Deliberately NOT a port of DropZone.vue's custom
        // pointer-drag-with-ghost-element system (a from-scratch, high-risk
        // rebuild for comparatively little functional gain over click-to-
        // route); this reuses only proven primitives (Rectangle/MouseArea/
        // Flow), consistent with this migration's "boring primitive over
        // fragile fidelity" pattern elsewhere (GraphicEQ's discrete sliders
        // instead of an SVG bezier curve, the card-scoped confirm overlay
        // instead of a QQC2 Popup).
        Rectangle {
            id: routingCard
            visible: page.activeTab === "mixer" && page.apps.length > 0
            Layout.fillWidth: true
            radius: Theme.radius
            color: Theme.surface
            border.width: 1
            border.color: Theme.border
            implicitHeight: routingCol.implicitHeight + 24

            // Id of the app pill whose channel picker is currently open, or
            // -1 if none. Only one open at a time.
            property int expandedAppId: -1

            ColumnLayout {
                id: routingCol
                anchors.fill: parent
                anchors.margins: 12
                spacing: 10

                Text {
                    text: "APP ROUTING"
                    color: Theme.textDim
                    font.pixelSize: 11
                    font.weight: Font.Bold
                }

                Flow {
                    Layout.fillWidth: true
                    spacing: 8

                    Repeater {
                        model: page.apps.filter(a => !a.locked)

                        ColumnLayout {
                            id: appDelegate
                            required property var modelData
                            spacing: 4

                            Rectangle {
                                id: pill
                                readonly property bool expanded: routingCard.expandedAppId === appDelegate.modelData.id
                                implicitWidth: pillRow.implicitWidth + 16
                                implicitHeight: 26
                                radius: 13
                                color: pill.expanded ? Theme.accent : Theme.bg
                                border.width: 1
                                border.color: Theme.border

                                RowLayout {
                                    id: pillRow
                                    anchors.centerIn: parent
                                    spacing: 6
                                    Rectangle {
                                        width: 6; height: 6; radius: 3
                                        color: page.channelColors[appDelegate.modelData.channel || "Master"] || Theme.textDim
                                    }
                                    Text {
                                        text: appDelegate.modelData.name + " · " + (appDelegate.modelData.channel || "Master")
                                        color: pill.expanded ? "#fff" : Theme.text
                                        font.pixelSize: 11
                                    }
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: routingCard.expandedAppId = pill.expanded ? -1 : appDelegate.modelData.id
                                }
                            }

                            RowLayout {
                                visible: pill.expanded
                                spacing: 4
                                Repeater {
                                    model: page.routeTargets
                                    Rectangle {
                                        id: targetBtn
                                        required property string modelData
                                        readonly property bool isCurrent: (appDelegate.modelData.channel || "Master") === modelData
                                        implicitWidth: targetText.implicitWidth + 14
                                        implicitHeight: 22
                                        radius: Theme.radius
                                        color: targetBtn.isCurrent ? Theme.accent : "transparent"
                                        border.width: 1
                                        border.color: Theme.border
                                        Text {
                                            id: targetText
                                            anchors.centerIn: parent
                                            text: targetBtn.modelData
                                            color: targetBtn.isCurrent ? "#fff" : Theme.textDim
                                            font.pixelSize: 10
                                        }
                                        MouseArea {
                                            anchors.fill: parent
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                if (targetBtn.modelData === "Master")
                                                    AudioController.unrouteApp(appDelegate.modelData.id, appDelegate.modelData.binary)
                                                else
                                                    AudioController.routeApp(appDelegate.modelData.id, targetBtn.modelData, appDelegate.modelData.binary)
                                                routingCard.expandedAppId = -1
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
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
                        Icon { anchors.centerIn: parent; name: "rotate-ccw"; size: 13; color: Theme.textDim}
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
            // See HomePage.qml's ScrollView for why this is explicit — QQC2's
            // automatic contentHeight inference doesn't reliably track a
            // ColumnLayout's implicitHeight, which left the EQ/DSP tabs
            // unscrollable on shorter windows.
            contentHeight: eqDspCol.implicitHeight

            ColumnLayout {
                id: eqDspCol
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
