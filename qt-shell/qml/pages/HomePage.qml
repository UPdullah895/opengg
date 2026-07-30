import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Dashboard — QML port of HomePage.vue.
//
// REWRITE, not a restyle. The previous version of this file was invented UI (a
// "Replay Buffer" card with a Start button, plus a list of channel sliders) and
// shared no structure with the real dashboard, which is a four-card stat grid
// above a changelog feed. See
// docs/superpowers/plans/2026-07-30-screenshot-gap-audit.md G1.
//
// Click-to-expand popovers (this pass): each card opens an inline popover —
// quick per-channel mixer strips, recorder settings + start/stop, recent
// clips, or the devices "coming soon" note — matching HomePage.vue's
// `activeCard`/`toggleCard` behaviour. The GSR-missing warning banner is
// NOT part of this pass: it needs a new "is gpu-screen-recorder installed"
// check plumbed through from core::gsr, which nothing here currently exposes
// cheaply (the existing diagnostics path spawns a process). Left for later,
// tracked separately from this popover work.
Rectangle {
    id: page
    color: Theme.bg

    signal navigate(string page)
    signal navigateSettings(string section)
    signal previewClipRequested(string filepath, string title)

    // Card badge colours are hardcoded in HomePage.vue's scoped CSS
    // (.card-icon.accent/.red/.green/.purple), exactly like the channel identity
    // palette in MixerPage. Note that `.accent` there is BLUE (#3b82f6), not the
    // theme accent — a naming quirk in the original, preserved deliberately.
    readonly property var cardColors: ({
        mixer: "#3b82f6", recorder: "#EF4444", clips: "#10b981", devices: "#a855f7"
    })

    // Quick-mixer channel identity colours — HomePage.vue defines its own local
    // COLORS const rather than importing MixerPage's, so this duplicates
    // MixerPage.qml's channelColors map deliberately, matching the original.
    readonly property var channelColors: ({ Game: "#E94560", Chat: "#3B82F6", Media: "#10B981", Aux: "#A855F7", Mic: "#F59E0B" })

    readonly property var cards: [
        {
            key: "mixer", icon: "sliders",
            label: I18n.t("dashboard.audioMixer"),
            value: I18n.t("dashboard.channelSummary"),
            sub: I18n.t("dashboard.channelList")
        },
        {
            key: "recorder", icon: "record",
            label: I18n.t("dashboard.recorder"),
            value: RecordingController.running ? I18n.t("dashboard.active")
                                               : I18n.t("dashboard.idle"),
            sub: RecordingController.statusText
        },
        {
            key: "clips", icon: "video",
            label: I18n.t("dashboard.clipsCard"),
            value: String(ClipsController.totalCount),
            sub: I18n.t("dashboard.videoClipsSaved")
        },
        {
            key: "devices", icon: "headphones",
            label: I18n.t("dashboard.devices"),
            value: I18n.t("dashboard.scan"),
            sub: I18n.t("dashboard.devicesSub")
        }
    ]

    // Changelog entries. `I18n.tRaw` returns the raw catalog subtree: the
    // flattened catalog keeps string leaves only, so array-valued locale content
    // was unreachable from QML until it was added (mirrors vue-i18n's `tm()`).
    readonly property var updates: {
        try {
            var a = JSON.parse(I18n.tRaw("dashboard.changelog"))
            return Array.isArray(a) ? a : []
        } catch (e) {
            return []
        }
    }
    property bool showAllUpdates: false

    // ── Popover state ────────────────────────────────────────────────────
    // Tracked by key, NOT by a captured Item reference: `cards` is a `var`
    // array re-literal'd on every dependency change (recorder status, clip
    // count, live language switch), and QML's array-model Repeater fully
    // destroys and recreates its delegates whenever the model array's
    // identity changes — so a cached Item (e.g. from the card's own click
    // handler) can go stale mid-session. Concretely: opening the Recorder
    // popover, then starting/stopping the recorder, changes `cards[1].value`
    // ("Idle" → "Active"), which recreates every statCard delegate and
    // orphans a captured reference. Looking the item up fresh via
    // `cardsRepeater.itemAt()` every time the popover positions itself
    // avoids that entirely.
    property string activeCard: ""

    function toggleCard(key) {
        if (page.activeCard === key) { page.closePopovers(); return }
        page.activeCard = key
        if (key === "mixer") AudioController.refresh()
    }
    function closePopovers() {
        page.activeCard = ""
    }

    // ── Quick mixer (Game/Chat/Media/Aux/Mic — Master excluded, matching
    // HomePage.vue's mixerChannels) ─────────────────────────────────────────
    property var channels: AudioController.channelsJson ? JSON.parse(AudioController.channelsJson) : []
    function volumeFor(name) {
        for (var i = 0; i < page.channels.length; i++)
            if (page.channels[i].name === name) return page.channels[i].volume
        return 100
    }
    function muteFor(name) {
        for (var i = 0; i < page.channels.length; i++)
            if (page.channels[i].name === name) return !!page.channels[i].muted
        return false
    }

    // ── Recorder quick settings ─────────────────────────────────────────────
    property var settingsObj: JSON.parse(SettingsController.settingsJson || "{}")
    Connections {
        target: SettingsController
        function onSettingsJsonChanged() { page.settingsObj = JSON.parse(SettingsController.settingsJson || "{}") }
    }
    function setSetting(key, value) {
        SettingsController.setValue(key, JSON.stringify(value))
        if (page.settingsObj.gsrEnabled) RecordingController.restart()
    }

    readonly property var gsrQualityOptions: [
        { value: "cbr", label: I18n.t("dashboard.gsrQuality.cbr") },
        { value: "medium", label: I18n.t("dashboard.gsrQuality.medium") },
        { value: "high", label: I18n.t("dashboard.gsrQuality.high") },
        { value: "very_high", label: I18n.t("dashboard.gsrQuality.very_high") },
        { value: "ultra", label: I18n.t("dashboard.gsrQuality.ultra") },
    ]
    readonly property var gsrFpsOptions: [30, 60, 120].map(v => ({ value: v, label: I18n.t("dashboard.gsrFps." + v) }))
    readonly property var gsrReplayOptions: [15, 30, 60, 90, 120].map(v => ({ value: v, label: I18n.t("dashboard.gsrReplay." + v) }))
    readonly property var gsrTargetOptions: [
        { value: "screen", label: I18n.t("dashboard.gsrTarget.screen") },
        { value: "focused", label: I18n.t("dashboard.gsrTarget.focused") },
    ]
    readonly property var gsrQualityKbps: ({ medium: 4000, high: 6000, very_high: 12000, ultra: 20000 })
    function gsrEstFileMb() {
        var s = page.settingsObj
        var kbps = s.gsrQuality === "cbr" ? (s.gsrCbrBitrate || 8000) : (page.gsrQualityKbps[s.gsrQuality] || 8000)
        return Math.round((kbps * (s.gsrReplaySecs || 30)) / 8 / 1024)
    }
    function gsrEstRamMb() {
        return Math.ceil(page.gsrEstFileMb() * 1.2)
    }

    // ── Recent clips ─────────────────────────────────────────────────────
    // `ClipsController.count` is read only to force this to re-evaluate when
    // the library changes — `recentJson()` itself always reads the unfiltered
    // clip list (see clips.rs), independent of whatever search/sort/game
    // filter the Clips page currently has active.
    property var recentClips: {
        ClipsController.count
        try {
            var a = JSON.parse(ClipsController.recentJson(3))
            return Array.isArray(a) ? a : []
        } catch (e) {
            return []
        }
    }
    function openClipPreview(clip) {
        page.previewClipRequested(clip.filepath, clip.title)
        page.navigate("clips")
        page.closePopovers()
    }

    Component.onCompleted: {
        AudioController.refresh()
        RecordingController.refresh()
    }

    // Light polling so external volume/recording state changes show up live.
    Timer {
        interval: 2000
        running: true
        repeat: true
        onTriggered: {
            AudioController.refresh()
            RecordingController.refresh()
        }
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        // QQC2 ScrollView's automatic contentHeight inference doesn't pick
        // up `dashboardCol`'s explicit `y` offset, and — more importantly —
        // doesn't reliably re-track a ColumnLayout's implicitHeight as it
        // grows (verified live: expanding "14 more updates" to all 15 cards
        // produced no scrollbar at all and the Flickable didn't scroll).
        // Bound explicitly instead of relying on inference.
        contentHeight: dashboardCol.implicitHeight + dashboardCol.y * 2

        ColumnLayout {
            id: dashboardCol
            width: page.width - 64
            x: 32
            y: 28
            spacing: 18

            Component.onCompleted: TourController.registerTarget("home-dashboard", dashboardCol)
            Component.onDestruction: TourController.unregisterTarget("home-dashboard")

            // ── Title row ─────────────────────────────────────────────────
            RowLayout {
                Layout.fillWidth: true
                spacing: 10

                Text {
                    text: (I18n.language, I18n.t("dashboard.title"))
                    color: Theme.text
                    font.pixelSize: 26
                    font.weight: Font.Bold
                }
                // Replays the guided tour, as in HomePage.vue's title row.
                Rectangle {
                    width: 22; height: 22; radius: 11
                    color: tourBtn.containsMouse ? Theme.accentAlpha(15) : "transparent"
                    Layout.alignment: Qt.AlignVCenter
                    Icon {
                        anchors.centerIn: parent
                        name: "info"
                        size: 14
                        color: tourBtn.containsMouse ? Theme.accent : Theme.textMuted
                    }
                    MouseArea {
                        id: tourBtn
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: TourController.start()
                    }
                }
                Item { Layout.fillWidth: true }
            }

            // ── Four-card stat grid ───────────────────────────────────────
            GridLayout {
                Layout.fillWidth: true
                columns: 4
                columnSpacing: 14
                rowSpacing: 14

                Repeater {
                    id: cardsRepeater
                    // A stable integer model, NOT `page.cards` directly: that
                    // array is re-literal'd whenever any card's live value
                    // changes (recorder status, clip count), and an
                    // array-model Repeater fully destroys and recreates every
                    // delegate when the model's identity changes — which
                    // orphaned the popover's card-item lookup mid-session
                    // (start/stop the recorder while its popover is open and
                    // the popover would snap to the top-left corner). Each
                    // delegate reads `page.cards[index]` itself instead, so
                    // its *content* updates reactively without the Item ever
                    // being torn down.
                    model: 4

                    Rectangle {
                        id: statCard
                        required property int index
                        readonly property var modelData: page.cards[index]
                        readonly property color badge: page.cardColors[modelData.key]

                        Layout.fillWidth: true
                        Layout.preferredHeight: 118
                        radius: Theme.radiusLg
                        color: Theme.surface
                        border.width: 1
                        border.color: (cardHover.hovered || page.activeCard === modelData.key) ? Theme.accent : Theme.border

                        HoverHandler { id: cardHover }
                        TapHandler { onTapped: page.toggleCard(statCard.modelData.key) }

                        // Registered so the tour can spotlight the recorder card.
                        Component.onCompleted: {
                            if (modelData.key === "recorder")
                                TourController.registerTarget("home-recorder", statCard)
                        }

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: 18
                            spacing: 0

                            RowLayout {
                                Layout.fillWidth: true
                                Text {
                                    text: statCard.modelData.label.toUpperCase()
                                    color: Theme.textDim
                                    font.pixelSize: 11
                                    font.weight: Font.Bold
                                    Layout.fillWidth: true
                                }
                                Rectangle {
                                    width: 36; height: 36
                                    radius: Theme.radius
                                    color: Theme.tint(statCard.badge, 10)
                                    Icon {
                                        anchors.centerIn: parent
                                        name: statCard.modelData.icon
                                        size: 18
                                        color: statCard.badge
                                    }
                                }
                            }

                            Item { Layout.fillHeight: true }

                            Text {
                                text: statCard.modelData.value
                                color: Theme.text
                                font.pixelSize: 22
                                font.weight: Font.Bold
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                            }
                            Text {
                                text: statCard.modelData.sub
                                color: Theme.textMuted
                                font.pixelSize: 11
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }

            // ── Latest Engine Updates ─────────────────────────────────────
            Text {
                text: (I18n.language, I18n.t("dashboard.latestUpdates"))
                color: Theme.text
                font.pixelSize: 15
                font.weight: Font.Bold
                Layout.topMargin: 6
            }

            Repeater {
                model: page.showAllUpdates ? page.updates.length
                                           : Math.min(1, page.updates.length)

                Rectangle {
                    id: updateCard
                    required property int index
                    readonly property var entry: page.updates[index]

                    Layout.fillWidth: true
                    radius: Theme.radiusLg
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.border
                    implicitHeight: upCol.implicitHeight + 32

                    ColumnLayout {
                        id: upCol
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 16
                        spacing: 8

                        RowLayout {
                            Layout.fillWidth: true
                            Text {
                                text: updateCard.entry ? updateCard.entry.version : ""
                                color: Theme.accent
                                font.pixelSize: 13
                                font.weight: Font.Bold
                                Layout.fillWidth: true
                            }
                            Text {
                                text: updateCard.entry ? updateCard.entry.date : ""
                                color: Theme.textMuted
                                font.pixelSize: 11
                            }
                        }

                        Repeater {
                            model: updateCard.entry ? updateCard.entry.items : []
                            RowLayout {
                                required property string modelData
                                Layout.fillWidth: true
                                spacing: 8
                                Text {
                                    text: "•"
                                    color: Theme.accent
                                    font.pixelSize: 13
                                    Layout.alignment: Qt.AlignTop
                                }
                                Text {
                                    text: modelData
                                    color: Theme.textDim
                                    font.pixelSize: 12
                                    wrapMode: Text.WordWrap
                                    Layout.fillWidth: true
                                }
                            }
                        }
                    }
                }
            }

            Text {
                visible: page.updates.length > 1
                text: page.showAllUpdates
                      ? (I18n.language, I18n.t("dashboard.showLess"))
                      : (page.updates.length - 1) + " more updates"
                color: Theme.accent
                font.pixelSize: 12
                font.weight: Font.Bold
                Layout.bottomMargin: 24

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: page.showAllUpdates = !page.showAllUpdates
                }
            }
        }
    }

    // ── Popover overlay ──────────────────────────────────────────────────
    // Page-level overlay positioned at the active card's geometry, mirroring
    // ClipContextMenu.qml's pattern (a plain positioned Item rather than a
    // QQC2 Popup, with its own click-away MouseArea and a click-absorbing
    // MouseArea on the panel itself).
    Item {
        id: popoverOverlay
        anchors.fill: parent
        visible: page.activeCard !== ""
        z: 100

        MouseArea {
            anchors.fill: parent
            onClicked: page.closePopovers()
        }

        // Looked up fresh (not cached) every time this re-evaluates, so a
        // Repeater delegate recreation — e.g. the recorder card's own value
        // changing while its popover is open — can't leave this pointing at
        // a destroyed Item. See the note on `page.activeCard` above.
        readonly property var activeItem: {
            var idx = page.cards.findIndex(c => c.key === page.activeCard)
            return idx >= 0 ? cardsRepeater.itemAt(idx) : null
        }
        readonly property point anchorPos: popoverOverlay.activeItem
            ? popoverOverlay.activeItem.mapToItem(popoverOverlay, 0, popoverOverlay.activeItem.height + 8)
            : Qt.point(0, 0)
        readonly property real anchorWidth: popoverOverlay.activeItem ? popoverOverlay.activeItem.width : 280

        Rectangle {
            id: panel
            x: Math.max(6, Math.min(popoverOverlay.anchorPos.x, popoverOverlay.width - width - 6))
            y: Math.max(6, Math.min(popoverOverlay.anchorPos.y, popoverOverlay.height - height - 6))
            width: Math.max(popoverOverlay.anchorWidth, 280)
            implicitHeight: panelLoader.item ? panelLoader.item.implicitHeight + 28 : 0

            radius: Theme.radiusLg
            color: Theme.surface
            border.width: 1
            border.color: Theme.border

            MouseArea { anchors.fill: parent }   // absorb clicks

            Loader {
                id: panelLoader
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 14
                sourceComponent: {
                    switch (page.activeCard) {
                    case "mixer": return mixerPopoverComp
                    case "recorder": return recorderPopoverComp
                    case "clips": return clipsPopoverComp
                    case "devices": return devicesPopoverComp
                    default: return null
                    }
                }
            }
        }
    }

    // ── Mixer popover — quick per-channel strips ─────────────────────────
    Component {
        id: mixerPopoverComp
        ColumnLayout {
            spacing: 10

            Text {
                text: (I18n.language, I18n.t("dashboard.quickMixer"))
                color: Theme.textMuted
                font.pixelSize: 10
                font.weight: Font.Bold
                font.letterSpacing: 0.6
            }

            Repeater {
                model: ["Game", "Chat", "Media", "Aux", "Mic"]

                RowLayout {
                    required property string modelData
                    Layout.fillWidth: true
                    spacing: 8

                    Text {
                        text: modelData.toUpperCase()
                        color: page.channelColors[modelData]
                        font.pixelSize: 11
                        font.weight: Font.Bold
                        Layout.preferredWidth: 36
                    }
                    HSlider {
                        Layout.fillWidth: true
                        from: 0; to: 150
                        value: page.volumeFor(modelData)
                        sliderColor: page.channelColors[modelData]
                        onMoved: (v) => AudioController.setVolume(modelData, Math.round(v))
                    }
                    Rectangle {
                        width: 24; height: 24
                        radius: Theme.radius
                        color: page.muteFor(modelData) ? Theme.tint(Theme.danger, 15) : Theme.bgInput
                        border.width: 1
                        border.color: page.muteFor(modelData) ? Theme.danger : Theme.border
                        Icon {
                            anchors.centerIn: parent
                            name: page.muteFor(modelData) ? "volume-x" : "volume-1"
                            size: 12
                            color: page.muteFor(modelData) ? Theme.danger : Theme.textDim
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: AudioController.setMute(modelData, !page.muteFor(modelData))
                        }
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.topMargin: 2
                implicitHeight: 30
                radius: Theme.radius
                color: openMixerArea.containsMouse ? Theme.bgHover : Theme.bgInput
                border.width: 1
                border.color: Theme.border
                Text {
                    anchors.centerIn: parent
                    text: (I18n.language, I18n.t("dashboard.openMixer"))
                    color: Theme.textDim
                    font.pixelSize: 12
                    font.weight: Font.Bold
                }
                MouseArea {
                    id: openMixerArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: { page.navigate("mixer"); page.closePopovers() }
                }
            }
        }
    }

    // ── Recorder popover — quick settings + start/stop ───────────────────
    Component {
        id: recorderPopoverComp
        ColumnLayout {
            spacing: 10

            RowLayout {
                Layout.fillWidth: true
                Text {
                    text: (I18n.language, I18n.t("dashboard.recordingSettings"))
                    color: Theme.textMuted
                    font.pixelSize: 10
                    font.weight: Font.Bold
                    font.letterSpacing: 0.6
                    Layout.fillWidth: true
                }
                Rectangle {
                    width: 22; height: 22; radius: 11
                    color: gearArea.containsMouse ? Theme.bgHover : "transparent"
                    Icon {
                        anchors.centerIn: parent
                        name: "gear"
                        size: 13
                        color: gearArea.containsMouse ? Theme.text : Theme.textMuted
                    }
                    MouseArea {
                        id: gearArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            page.navigateSettings("captureSound")
                            page.navigate("settings")
                            page.closePopovers()
                        }
                    }
                }
            }

            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: 8
                rowSpacing: 7

                Text { text: (I18n.language, I18n.t("dashboard.quality")); color: Theme.textDim; font.pixelSize: 12 }
                SelectField {
                    Layout.fillWidth: true
                    options: page.gsrQualityOptions
                    value: page.settingsObj.gsrQuality
                    onPicked: (v) => page.setSetting("gsrQuality", v)
                }
                Text { text: (I18n.language, I18n.t("dashboard.fps")); color: Theme.textDim; font.pixelSize: 12 }
                SelectField {
                    Layout.fillWidth: true
                    options: page.gsrFpsOptions
                    value: page.settingsObj.gsrFps
                    onPicked: (v) => page.setSetting("gsrFps", v)
                }
                Text { text: (I18n.language, I18n.t("dashboard.buffer")); color: Theme.textDim; font.pixelSize: 12 }
                SelectField {
                    Layout.fillWidth: true
                    options: page.gsrReplayOptions
                    value: page.settingsObj.gsrReplaySecs
                    onPicked: (v) => page.setSetting("gsrReplaySecs", v)
                }
                Text { text: (I18n.language, I18n.t("dashboard.target")); color: Theme.textDim; font.pixelSize: 12 }
                SelectField {
                    Layout.fillWidth: true
                    options: page.gsrTargetOptions
                    value: page.settingsObj.gsrMonitorTarget
                    onPicked: (v) => page.setSetting("gsrMonitorTarget", v)
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.topMargin: 2
                implicitHeight: 30
                radius: Theme.radius
                property bool danger: RecordingController.running
                color: Theme.tint(danger ? Theme.danger : Theme.accent, recordBtnArea.containsMouse ? 25 : 15)
                border.width: 1
                border.color: danger ? Theme.danger : Theme.accent

                RowLayout {
                    anchors.centerIn: parent
                    spacing: 6
                    Rectangle {
                        width: 7; height: 7; radius: 3.5
                        color: RecordingController.running ? Theme.success : Theme.textMuted
                    }
                    Text {
                        text: RecordingController.running
                              ? (I18n.language, I18n.t("dashboard.stop"))
                              : (page.settingsObj.gsrEnabled
                                 ? (I18n.language, I18n.t("dashboard.startReplayBuffer"))
                                 : (I18n.language, I18n.t("dashboard.startReplay")))
                        color: RecordingController.running ? Theme.danger : Theme.accent
                        font.pixelSize: 12
                        font.weight: Font.Bold
                    }
                }
                MouseArea {
                    id: recordBtnArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: RecordingController.running ? RecordingController.stop() : RecordingController.start()
                }
            }

            Text {
                visible: !!page.settingsObj.gsrEnabled
                Layout.fillWidth: true
                Layout.topMargin: -2
                horizontalAlignment: Text.AlignHCenter
                text: "~" + page.gsrEstRamMb() + " " + I18n.t("dashboard.gsr.estRam") +
                      "  ·  ~" + page.gsrEstFileMb() + " " + I18n.t("dashboard.gsr.estFile")
                color: Theme.textMuted
                font.pixelSize: 10
            }
        }
    }

    // ── Clips popover — recent clips (2x2 grid, 4th slot is "More Clips") ─
    Component {
        id: clipsPopoverComp
        ColumnLayout {
            spacing: 10

            Text {
                text: (I18n.language, I18n.t("dashboard.recentClips"))
                color: Theme.textMuted
                font.pixelSize: 10
                font.weight: Font.Bold
                font.letterSpacing: 0.6
            }

            Text {
                visible: page.recentClips.length === 0
                Layout.fillWidth: true
                Layout.topMargin: 6
                Layout.bottomMargin: 6
                horizontalAlignment: Text.AlignHCenter
                text: (I18n.language, I18n.t("dashboard.noClips"))
                color: Theme.textMuted
                font.pixelSize: 12
            }

            GridLayout {
                visible: page.recentClips.length > 0
                Layout.fillWidth: true
                columns: 2
                columnSpacing: 8
                rowSpacing: 8

                Repeater {
                    model: page.recentClips

                    ColumnLayout {
                        id: miniClip
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 4

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: width * 0.5625
                            radius: Theme.radius
                            clip: true
                            color: Theme.bgInput
                            border.width: 1
                            border.color: miniArea.containsMouse ? Theme.accent : Theme.border

                            Image {
                                anchors.fill: parent
                                visible: !!miniClip.modelData.thumbnail
                                source: miniClip.modelData.thumbnail ? "file://" + miniClip.modelData.thumbnail : ""
                                fillMode: Image.PreserveAspectCrop
                                asynchronous: true
                            }
                            Icon {
                                anchors.centerIn: parent
                                visible: !miniClip.modelData.thumbnail
                                name: "video"
                                size: 18
                                color: Theme.textMuted
                            }
                            MouseArea {
                                id: miniArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: page.openClipPreview(miniClip.modelData)
                            }
                        }
                        Text {
                            Layout.fillWidth: true
                            text: miniClip.modelData.title
                            color: Theme.textDim
                            font.pixelSize: 10
                            elide: Text.ElideRight
                        }
                    }
                }

                // 4th slot: static "More Clips" button-card, not a real clip —
                // matches HomePage.vue's grid-mode layout exactly.
                Rectangle {
                    visible: page.recentClips.length > 0
                    Layout.fillWidth: true
                    Layout.preferredHeight: (panel.width - 28 - 8) / 2 * 0.5625 + 18
                    radius: Theme.radius
                    color: Theme.bgInput
                    border.width: 1
                    border.color: moreArea.containsMouse ? Theme.accent : Theme.border

                    ColumnLayout {
                        anchors.centerIn: parent
                        spacing: 5
                        Icon {
                            Layout.alignment: Qt.AlignHCenter
                            name: "video"
                            size: 16
                            color: moreArea.containsMouse ? Theme.accent : Theme.textMuted
                        }
                        Text {
                            Layout.alignment: Qt.AlignHCenter
                            text: (I18n.language, I18n.t("dashboard.moreClips"))
                            color: moreArea.containsMouse ? Theme.accent : Theme.textMuted
                            font.pixelSize: 10
                            font.weight: Font.Bold
                        }
                    }
                    MouseArea {
                        id: moreArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: { page.navigate("clips"); page.closePopovers() }
                    }
                }
            }
        }
    }

    // ── Devices popover — the shipping Vue app is a placeholder here too
    // (DevicesPage.vue renders "Coming Soon" — see G7 in the audit doc), so
    // this matches rather than under-selling the QML shell's live device
    // list on the Devices page itself. ────────────────────────────────────
    Component {
        id: devicesPopoverComp
        ColumnLayout {
            // Explicit width: this ColumnLayout is top-level (loaded via a
            // plain Loader, not itself managed by a parent Layout), so the
            // wrapping description Text below — which has `Layout.fillWidth`
            // and no content of its own to size from — has nothing concrete
            // to fill against without this, and the whole popover collapsed
            // to zero height (found by actually opening it, not just reading
            // the code — the other three popovers have no wrapping text, so
            // they never hit this).
            width: panel.width - 28
            spacing: 8

            Icon {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 4
                name: "headphones"
                size: 32
                color: Theme.tint(Theme.purple, 70)
            }
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: (I18n.language, I18n.t("dashboard.comingSoon"))
                color: Theme.text
                font.pixelSize: 14
                font.weight: Font.Bold
            }
            Text {
                Layout.fillWidth: true
                Layout.bottomMargin: 4
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                text: (I18n.language, I18n.t("dashboard.comingSoonDesc"))
                color: Theme.textMuted
                font.pixelSize: 11
                lineHeight: 1.4
            }
        }
    }
}
