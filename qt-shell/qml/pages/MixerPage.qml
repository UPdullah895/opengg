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

    function appsForChannel(name) {
        if (name === "Master")
            return page.apps.filter(a => !a.locked && (!a.channel || a.channel === "" || a.channel === "Master"))
        return page.apps.filter(a => !a.locked && a.channel === name)
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
    // Below this, six full-label tabs plus the Overdrive/gear buttons don't
    // reliably fit `page.width` (page.width is the content area alone —
    // Theme.sidebarW is already subtracted by the StackLayout it lives in).
    // Past that point tabs collapse to icon-only rather than letting the
    // RowLayout's plain `width:` bindings overflow the window with no
    // elision, which is how "Mixer" was rendering as "Mixe…" with no
    // ellipsis at narrow widths — the label Text had no `elide` at all.
    readonly property bool compactTabs: page.width < 620
    readonly property var mixerChannelNames: ["Master", "Game", "Chat", "Media", "Aux", "Mic"]
    // Overdrive — expands fader range from 100% to 150%. Client-side UI
    // state only (matches MixerPage.vue's overdriveEnabled ref: not
    // persisted), but lives on AudioController rather than as a page-local
    // property: the Home dashboard's Quick Mixer needs the same answer to
    // "is 150% currently allowed", and previously had no way to know,
    // letting it push a channel to 150% with Overdrive off. Disabling it
    // clamps any channel currently above 100% back down, mirroring
    // clampChannelsTo100().
    Connections {
        target: AudioController
        function onOverdriveEnabledChanged() {
            if (AudioController.overdriveEnabled) return
            for (const modelData of page.channels) {
                if (page.mixerChannelNames.includes(modelData.name) && modelData.volume > 100)
                    AudioController.setVolume(modelData.name, 100)
            }
        }
    }
    readonly property var tabs: [
        { id: "mixer", label: "Mixer", icon: "sliders" },
        { id: "game", label: "Game", icon: "gamepad" },
        { id: "chat", label: "Chat", icon: "headphones" },
        { id: "media", label: "Media", icon: "play" },
        { id: "aux", label: "Aux", icon: "music" },
        { id: "mic", label: "Mic", icon: "mic" },
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

    // Both lists are {value, label} objects — value is the real pactl
    // node.name ChannelStrip's ComboBox needs to send back via
    // setChannelDevice, label is what's shown. inputDevices used to collapse
    // this down to just the label string, which meant Mic's selector could
    // only ever send back display text instead of a real device name.
    property var outputDevices: JSON.parse(AudioController.outputDevicesJson || "[]")
    property var inputDevices: JSON.parse(AudioController.inputDevicesJson || "[]")
    property var channelDevices: JSON.parse(AudioController.channelDevicesJson || "{}")

    readonly property var channelColors: ({
        Master: "#94A3B8", Game: "#E94560", Chat: "#3B82F6",
        Media: "#10B981", Aux: "#A855F7", Mic: "#F59E0B",
    })

    // ── App-box layout: one global gear (below) controls every channel's
    // app box the same way, mirroring DropZone.vue's shared
    // appBoxCount/appBoxPerRow settings so the row-count/column-count math
    // stays a single formula instead of six independent ones.
    property bool appSettingsOpen: false
    property var uiSettings: JSON.parse(SettingsController.settingsJson || "{}")
    readonly property int appBoxCount: Math.max(1, Math.min(12, page.uiSettings.appBoxCount ?? 3))
    readonly property int appBoxPerRow: page.uiSettings.appBoxPerRow === 2 ? 2 : 1
    readonly property int appBoxRowH: 22
    readonly property int appBoxRowGap: 2
    readonly property int appBoxPad: 6
    readonly property int appBoxHeight: {
        const rows = Math.ceil(page.appBoxCount / page.appBoxPerRow)
        return rows * page.appBoxRowH + Math.max(0, rows - 1) * page.appBoxRowGap + page.appBoxPad * 2
    }

    Component.onCompleted: {
        AudioController.refresh()
        AudioController.refreshDevices()
        // Gates chatMixBar's visibility below — MixerPage.vue's onMounted
        // called this too. Without it virtualAudioReady stays at its
        // default false (nothing else on this page sets it), so the
        // ChatMix panel silently never appears no matter how ready the
        // real audio engine is.
        AudioController.refreshVirtualAudioStatus()
        AudioController.refreshEarBlast()
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
        }

        // Tab bar — Mixer (fader strips) + one tab per EQ/DSP channel, then a
        // spacer pushing Overdrive/Ear-Blast/gear to the row's far right edge.
        RowLayout {
            Layout.fillWidth: true
            spacing: 4

            Repeater {
                model: page.tabs
                Rectangle {
                    id: tabBtn
                    required property var modelData
                    property bool isActive: page.activeTab === modelData.id
                    // Capped rather than plain implicitWidth+24 for the same
                    // reason the label itself gets `elide` below: an
                    // uncapped width just moves the overflow problem from
                    // "text cut off mid-word" to "buttons run off the edge
                    // of the window" — neither is what a narrow window
                    // should do, so this is the ceiling a real translation
                    // (some are longer than English) can't exceed either.
                    readonly property int maxLabelWidth: 70
                    // Measures the label's natural width independently of
                    // tabLabel's own rendered `width`/`elide` state. Both
                    // tabBtn.width and tabLabel.width used to read
                    // tabLabel.implicitWidth directly — self-referencing
                    // implicitWidth from within a Text item's own `width`
                    // binding while `elide` is active is a known QML trap:
                    // eliding recomputation can re-emit implicitWidthChanged,
                    // which re-triggers tabBtn.width, which reflows the Row,
                    // which perturbs tabLabel's layout again — Qt's own
                    // binding-loop detector caught exactly this cycle live
                    // ("Binding loop detected for property 'width'"), and
                    // once tripped it stops re-evaluating the binding, which
                    // is what left every tab stuck showing icon-only even
                    // after compactTabs correctly settled to false. A
                    // TextMetrics has no `width`/`elide` of its own, so its
                    // `width` is a pure function of font+text — no cycle.
                    TextMetrics {
                        id: labelMetrics
                        font.pixelSize: 12
                        font.weight: tabBtn.isActive ? Font.DemiBold : Font.Normal
                        text: tabBtn.modelData.label
                    }
                    // Layout.preferredWidth, not a plain `width:` binding:
                    // this Rectangle is a Repeater delegate living directly
                    // inside a RowLayout that's now Layout.fillWidth: true
                    // (needed to push Overdrive/Ear-Blast/gear to the row's
                    // far edge). A plain `width:` is only an initial size
                    // hint to a Layout container — once the row itself
                    // started stretching, RowLayout's own arrange pass kept
                    // overwriting that width back down after every
                    // recompute (visible live as the correct 78/78/71/78/67/63
                    // px values immediately reverting to 30, the compact
                    // fallback, even with compactTabs already settled false).
                    // Layout.preferredWidth is the attached property Layout
                    // containers are actually built to keep re-reading.
                    // +4px over the raw TextMetrics reading: TextMetrics and
                    // Text compute layout via separate code paths and don't
                    // always agree to the sub-pixel, so sizing the label to
                    // the exact metrics width left `elide` firing on labels
                    // that visibly fit ("Mixer" clipped to "Mix…" at 78px).
                    readonly property int labelW: Math.min(labelMetrics.width + 4, maxLabelWidth)
                    Layout.preferredWidth: page.compactTabs
                        ? 30
                        : tabIcon.width + 6 + tabBtn.labelW + 24
                    height: 30
                    radius: Theme.radius
                    color: isActive ? Theme.surface : "transparent"

                    Row {
                        anchors.centerIn: parent
                        spacing: page.compactTabs ? 0 : 6

                        Icon {
                            id: tabIcon
                            name: tabBtn.modelData.icon
                            size: 14
                            color: tabBtn.isActive ? Theme.text : Theme.textDim
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        Text {
                            id: tabLabel
                            // Deliberately never toggles `visible` — Qt Quick
                            // skips layout polish on invisible items, and an
                            // item that starts invisible (compactTabs is true
                            // at page.width==0, before the first real layout
                            // pass) can end up with its implicitWidth stuck
                            // unrefreshed once it turns visible again. This
                            // showed up as headless ui-shots captures staying
                            // icon-only forever despite compactTabs correctly
                            // reading false by then. Collapsing via `width`
                            // instead keeps the Text always live.
                            clip: true
                            width: page.compactTabs ? 0 : tabBtn.labelW
                            text: tabBtn.modelData.label
                            color: tabBtn.isActive ? Theme.text : Theme.textDim
                            font.pixelSize: 12
                            font.weight: tabBtn.isActive ? Font.DemiBold : Font.Normal
                            elide: Text.ElideRight
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }
                    MouseArea {
                        id: tabArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: page.activeTab = tabBtn.modelData.id
                    }
                    // Icon-only mode drops the visible label entirely, so
                    // hovering is the only way left to confirm which tab
                    // this is before clicking it.
                    ToolTip {
                        visible: page.compactTabs && tabArea.containsMouse
                        text: tabBtn.modelData.label
                        delay: 400
                    }
                }
            }

            // Pushes Overdrive/Ear-Blast/gear to the row's far right edge
            // instead of them trailing immediately after the last tab.
            Item { Layout.fillWidth: true }

            // Overdrive — unlocks faders beyond 100% (up to 150%).
            Rectangle {
                visible: page.activeTab === "mixer"
                width: 30; height: 30
                radius: Theme.radius
                color: AudioController.overdriveEnabled ? Theme.tint(Theme.overdrive, 15) : "transparent"
                border.width: 1
                border.color: AudioController.overdriveEnabled ? Theme.overdrive : Theme.border

                Icon {
                    anchors.centerIn: parent
                    name: "zap"; size: 13
                }
                MouseArea {
                    id: overdriveArea
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    hoverEnabled: true
                    onClicked: AudioController.overdriveEnabled = !AudioController.overdriveEnabled
                }
                ToolTip {
                    visible: overdriveArea.containsMouse
                    text: AudioController.overdriveEnabled
                        ? "Overdrive ON — faders go to 150%"
                        : "Enable Overdrive (faders up to 150%)"
                    delay: 300
                }
            }

            // Ear Blast Protection — quick toggle mirroring MixerPage.vue's
            // header button. Full channel/threshold/target config still lives
            // in Settings → Mixer Routing; this is just the on/off switch.
            Rectangle {
                id: earBlastBtn
                readonly property var eb: AudioController.earBlastJson
                    ? JSON.parse(AudioController.earBlastJson)
                    : { enabled: false }
                visible: page.activeTab === "mixer"
                width: 30; height: 30
                radius: Theme.radius
                color: earBlastBtn.eb.enabled ? Theme.tint(Theme.accent, 15) : "transparent"
                border.width: 1
                border.color: earBlastBtn.eb.enabled ? Theme.accent : Theme.border

                Icon {
                    anchors.centerIn: parent
                    name: "ear"; size: 13
                    color: earBlastBtn.eb.enabled ? Theme.accent : Theme.text
                }
                MouseArea {
                    id: earBlastArea
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    hoverEnabled: true
                    onClicked: AudioController.setEarBlast("enabled", JSON.stringify(!earBlastBtn.eb.enabled))
                }
                ToolTip {
                    visible: earBlastArea.containsMouse
                    text: earBlastBtn.eb.enabled
                        ? (I18n.language, I18n.t("dashboard.earBlastOn"))
                        : (I18n.language, I18n.t("dashboard.earBlastOff"))
                    delay: 300
                }
            }

            // APP LIST SETTINGS — one gear controlling every channel's app
            // box the same way (rows shown before scrolling, 1 or 2 columns).
            Rectangle {
                id: appGearBtn
                visible: page.activeTab === "mixer"
                width: 30; height: 30
                radius: Theme.radius
                color: page.appSettingsOpen ? Theme.tint(Theme.accent, 15) : "transparent"
                border.width: 1
                border.color: page.appSettingsOpen ? Theme.accent : Theme.border

                Icon {
                    anchors.centerIn: parent
                    name: "gear"; size: 13
                    color: page.appSettingsOpen ? Theme.accent : Theme.text
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: page.appSettingsOpen = !page.appSettingsOpen
                }
            }
        }

        // Row of channel strips
        RowLayout {
            id: stripsRow
            visible: page.activeTab === "mixer"
            Layout.fillWidth: true
            // NOT Layout.fillHeight — a fillHeight item whose actual size is
            // then capped by Layout.maximumHeight still gets handed a CELL
            // sized to "whatever's left" in the ColumnLayout (all the way up
            // to the window bottom), and with no Layout.alignment set,
            // QtQuick.Layouts centers the capped content inside that oversized
            // cell. That's what was pushing this row ~200px down from the tab
            // bar and, in turn, pushing ChatMix below the visible column —
            // not a `spacing` value anywhere close to that. A fixed
            // preferredHeight makes this row exactly as tall as it needs to
            // be and nothing more, so it sits directly under the tab bar and
            // ChatMix follows immediately after it.
            Layout.preferredHeight: 420
            spacing: 10

            Component.onCompleted: TourController.registerTarget("mixer-channels", stripsRow)
            Component.onDestruction: TourController.unregisterTarget("mixer-channels")

            Repeater {
                // Deliberately the *count*, not the array. `stripChannels` is
                // rebuilt from scratch every time channelsJson changes, so a
                // `model: page.stripChannels` binding handed the Repeater a
                // brand-new array identity on every volume change and it tore
                // down and recreated all six delegates each time (measured:
                // three volume changes → 18 delegate constructions). That
                // destroyed the fader's MouseArea mid-drag, taking the mouse
                // grab with it — which is why every channel except Master
                // dropped the drag the instant it moved. Master survived only
                // because its value is synthesized client-side and so never
                // altered the JSON. The count only changes when a channel
                // appears or disappears, so the delegates now persist and each
                // one reads its own row. Wrapping ChannelStrip in a ColumnLayout
                // here (to stack the per-channel app box beneath it) doesn't
                // reintroduce that bug — the Repeater still only ever recreates
                // this wrapper, not the strip inside it, when the count changes.
                model: page.stripChannels.length

                ColumnLayout {
                    id: stripCol
                    required property int index
                    readonly property var modelData: page.stripChannels[index]
                    Layout.preferredWidth: 108
                    // Without a floor, 6 strips squeezed into a window too
                    // narrow to fit all of them at 108px each get compressed
                    // by the layout solver down toward single digits — at
                    // which point the device-name Text (itself sized off
                    // strip.width) has no room left to show anything but an
                    // ellipsis. This keeps enough width for a few readable
                    // characters before the ToolTip becomes the only way to
                    // read the rest. (Full narrow-window reflow is #41.)
                    Layout.minimumWidth: 96
                    Layout.fillHeight: true
                    spacing: 10

                    ChannelStrip {
                        Layout.fillWidth: true
                        Layout.fillHeight: true

                        name: stripCol.modelData.name
                        volume: stripCol.modelData.volume
                        muted: !!stripCol.modelData.muted
                        channelColor: page.channelColors[stripCol.modelData.name] || Theme.accent
                        iconName: page.channelIcons[stripCol.modelData.name] || "volume-2"
                        vuDb: page.vuDb(stripCol.modelData.name)
                        maxVolume: AudioController.overdriveEnabled ? 150 : 100
                        devices: stripCol.modelData.name === "Mic" ? page.inputDevices : page.outputDevices
                        selectedDevice: page.channelDevices[stripCol.modelData.name] || ""

                        onVolumeRequested: (v) => AudioController.setVolume(stripCol.modelData.name, v)
                        onMuteToggled: AudioController.setMute(stripCol.modelData.name, !stripCol.modelData.muted)
                        onDeviceRequested: (d) => AudioController.setChannelDevice(stripCol.modelData.name, d)
                    }

                    AppBox {
                        Layout.fillWidth: true
                        Layout.preferredHeight: page.appBoxHeight

                        channelName: stripCol.modelData.name
                        channelColor: page.channelColors[stripCol.modelData.name] || Theme.accent
                        apps: page.appsForChannel(stripCol.modelData.name)
                        perRow: page.appBoxPerRow
                        rowH: page.appBoxRowH
                        rowGap: page.appBoxRowGap
                        boxPad: page.appBoxPad
                        dragOverlay: dragLayer

                        onAppDropped: (appId, binary) => {
                            if (stripCol.modelData.name === "Master")
                                AudioController.unrouteApp(appId, binary)
                            else
                                AudioController.routeApp(appId, stripCol.modelData.name, binary)
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
                        Icon { anchors.centerIn: parent; name: "rotate-ccw"; size: 13; color: Theme.textDim}
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                chatMixSlider.dragging = false
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

                    // Custom-drawn slider rather than QQC2's Slider: Slider's
                    // built-in drag handling writes to its own `value`
                    // property directly, which silently severs a QML binding
                    // like `value: chatMixBar.balance` on the very first
                    // press — after that, `value` never tracks
                    // chatMixBar.balance again (no live re-sync, no visible
                    // motion beyond wherever the pointer last was), which is
                    // what made the slider feel unmovable/dead. ChannelStrip's
                    // vertical fader hit the identical problem and solved it
                    // with a local `dragging`/dragFraction` + settle() Timer;
                    // this is that same pattern, adapted to a horizontal
                    // -100..100 balance value instead of a 0..1 fraction.
                    Item {
                        id: chatMixSlider
                        Layout.fillWidth: true
                        height: 18

                        property bool dragging: false
                        property real dragBalance: 0
                        readonly property real displayBalance: chatMixSlider.dragging
                            ? chatMixSlider.dragBalance
                            : chatMixBar.balance
                        readonly property real fraction: (chatMixSlider.displayBalance + 100) / 200

                        Timer { id: chatMixSettle; interval: 600; onTriggered: chatMixSlider.dragging = false }

                        function apply(balance) {
                            const g = Math.round(Math.max(0, 100 - Math.max(0, balance)))
                            const c = Math.round(Math.max(0, 100 - Math.max(0, -balance)))
                            AudioController.setVolume("Game", g)
                            AudioController.setVolume("Chat", c)
                        }

                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width
                            height: 6
                            radius: 3
                            gradient: Gradient {
                                orientation: Gradient.Horizontal
                                GradientStop { position: 0.0; color: page.channelColors.Game }
                                GradientStop { position: 0.5; color: Theme.purple }
                                GradientStop { position: 1.0; color: page.channelColors.Chat }
                            }
                        }
                        Rectangle {
                            id: chatMixHandle
                            width: 18; height: 18; radius: 9
                            anchors.verticalCenter: parent.verticalCenter
                            x: chatMixSlider.fraction * (chatMixSlider.width - width)
                            color: Theme.text
                            border.width: 2
                            border.color: Theme.accent
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor

                            function moveTo(mx) {
                                const frac = Math.max(0, Math.min(1, mx / chatMixSlider.width))
                                chatMixSlider.dragBalance = frac * 200 - 100
                                chatMixSlider.apply(chatMixSlider.dragBalance)
                            }
                            onPressed: (m) => {
                                chatMixSlider.dragging = true
                                moveTo(m.x)
                            }
                            onPositionChanged: (m) => {
                                if (pressed) moveTo(m.x)
                            }
                            onReleased: chatMixSettle.restart()
                            onCanceled: chatMixSettle.restart()
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
        //
        // Wrapped in a plain Item so the wheel-accelerator overlay can anchor
        // to it: a direct Layout child can't be anchor-targeted by an item
        // outside that layout ("Cannot anchor to an item that isn't a parent
        // or sibling" — confirmed live; the same fix as ClipsPage's grid).
        Item {
            id: eqDspWrap
            visible: page.activeTab !== "mixer"
            Layout.fillWidth: true
            Layout.fillHeight: true

        ScrollView {
            id: eqDspScroll
            anchors.fill: parent
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

            // Wheel accelerator, sibling of eqDspScroll inside eqDspWrap.
            Item {
                anchors.fill: parent
                WheelScroller { anchors.fill: parent; flick: eqDspScroll.contentItem }
            }
        }
    }

    // Drag overlay for the per-channel app boxes — a chip being dragged is
    // reparented here (see AppBox.qml) so it isn't clipped by the box's own
    // scrollable Flickable while travelling across neighbouring boxes. Plain
    // child of `page` rather than of the ColumnLayout above: it must cover
    // the same screen area without being sized/positioned by that layout.
    Item {
        id: dragLayer
        anchors.fill: parent
        z: 1000
    }

    // APP LIST SETTINGS popover — same page-level-overlay-plus-click-away
    // pattern as HomePage.qml's card popovers (a plain positioned Item
    // rather than a QQC2 Popup, consistent with this migration's other
    // free-floating panels).
    Item {
        id: appSettingsOverlay
        anchors.fill: parent
        visible: page.appSettingsOpen
        z: 1001

        // Positions appSettingsPanel under appGearBtn every time the
        // overlay opens. Two other approaches were tried and both failed:
        // (1) a `readonly property point: appGearBtn.mapToItem(page, ...)`
        // binding evaluated exactly once, at construction (page.width==0,
        // nothing laid out yet), and then never again — confirmed live via
        // debug prints frozen at that first snapshot even after explicitly
        // reading page.width/height/appGearBtn.x/y/page.appSettingsOpen as
        // forced dependencies from inside the binding. (2) plain
        // `anchors.top/right: appGearBtn.bottom/right` — Qt Quick anchors
        // only work between a parent and its own children or siblings;
        // appSettingsPanel and appGearBtn are cousins (different branches
        // under `page`), and Qt logs exactly that at runtime: "Cannot
        // anchor to an item that isn't a parent or sibling." Recomputing
        // imperatively in onVisibleChanged sidesteps both: it runs once,
        // on-demand, well after everything is laid out, using mapToItem's
        // one-off snapshot at exactly the moment it's actually needed.
        onVisibleChanged: {
            if (!visible) return
            const p = appGearBtn.mapToItem(appSettingsOverlay, appGearBtn.width, appGearBtn.height + 6)
            appSettingsPanel.x = Math.max(6, p.x - appSettingsPanel.width)
            appSettingsPanel.y = p.y
        }

        MouseArea {
            anchors.fill: parent
            onClicked: page.appSettingsOpen = false
        }

        Rectangle {
            id: appSettingsPanel
            width: 200
            implicitHeight: appSettingsCol.implicitHeight + 20
            radius: Theme.radiusLg
            color: Theme.surface
            border.width: 1
            border.color: Theme.border

            MouseArea { anchors.fill: parent } // absorb clicks so they don't close the popover

            ColumnLayout {
                id: appSettingsCol
                anchors.fill: parent
                anchors.margins: 10
                spacing: 10

                Text {
                    text: ((I18n.language, I18n.t("devices.appBoxSettings"))).toUpperCase()
                    color: Theme.textDim
                    font.pixelSize: 10
                    font.weight: Font.Bold
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Text {
                        text: (I18n.language, I18n.t("devices.appsShown"))
                        color: Theme.text
                        font.pixelSize: 12
                        Layout.fillWidth: true
                    }
                    RowLayout {
                        spacing: 4
                        Rectangle {
                            width: 22; height: 22
                            radius: Theme.radius
                            color: Theme.bgDeep
                            border.width: 1; border.color: Theme.border
                            opacity: page.appBoxCount > 1 ? 1 : 0.4
                            Text { anchors.centerIn: parent; text: "−"; color: Theme.text; font.pixelSize: 13 }
                            MouseArea {
                                anchors.fill: parent
                                enabled: page.appBoxCount > 1
                                cursorShape: Qt.PointingHandCursor
                                onClicked: SettingsController.setValue("appBoxCount", JSON.stringify(page.appBoxCount - 1))
                            }
                        }
                        Text {
                            text: page.appBoxCount
                            color: Theme.text
                            font.pixelSize: 12
                            font.weight: Font.Bold
                            horizontalAlignment: Text.AlignHCenter
                            Layout.preferredWidth: 16
                        }
                        Rectangle {
                            width: 22; height: 22
                            radius: Theme.radius
                            color: Theme.bgDeep
                            border.width: 1; border.color: Theme.border
                            opacity: page.appBoxCount < 12 ? 1 : 0.4
                            Text { anchors.centerIn: parent; text: "+"; color: Theme.text; font.pixelSize: 13 }
                            MouseArea {
                                anchors.fill: parent
                                enabled: page.appBoxCount < 12
                                cursorShape: Qt.PointingHandCursor
                                onClicked: SettingsController.setValue("appBoxCount", JSON.stringify(page.appBoxCount + 1))
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Text {
                        text: (I18n.language, I18n.t("devices.appsPerRow"))
                        color: Theme.text
                        font.pixelSize: 12
                        Layout.fillWidth: true
                    }
                    Row {
                        spacing: 2
                        Repeater {
                            model: [1, 2]
                            Rectangle {
                                id: perRowBtn
                                required property int modelData
                                width: 26; height: 22
                                radius: Theme.radius
                                color: page.appBoxPerRow === perRowBtn.modelData ? Theme.accent : Theme.bgDeep
                                Text {
                                    anchors.centerIn: parent
                                    text: perRowBtn.modelData
                                    color: page.appBoxPerRow === perRowBtn.modelData ? "#fff" : Theme.textDim
                                    font.pixelSize: 11
                                    font.weight: Font.Bold
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: SettingsController.setValue("appBoxPerRow", JSON.stringify(perRowBtn.modelData))
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
