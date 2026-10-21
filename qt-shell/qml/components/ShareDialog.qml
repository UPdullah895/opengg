import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Share sheet: make a file that fits a platform (Discord's 10 MB, X's 140 s,
// a 9:16 crop for TikTok/Shorts/Reels) at the best quality its limits allow,
// then get it there — copy the file to paste anywhere, or open the upload
// page with a caption ready. All maths and encoding live in
// opengg_core::share behind ShareController; this file only collects choices
// and shows the plan.
Rectangle {
    id: dlg

    /// {filepath, title, game?}; null hides the dialog.
    property var clip: null
    /// Optional window from the editor's trim, in seconds (-1 = platform
    /// default: the last N seconds, where a replay's moment is).
    property real trimStart: -1
    property real trimEnd: -1

    signal closed()

    property string platform: "discord"
    property bool vertical: false
    property real cropPos: 0.5
    /// 0 = automatic resolution (the planner's pick).
    property int shortSide: 0
    /// Audio-track indices to mix in (all, by default, once probed).
    property var tracksOn: []
    property string captionText: ""
    property string toast: ""

    readonly property var plan: JSON.parse(ShareController.planJson || "{}")
    readonly property var platforms: JSON.parse(ShareController.platformsJson() || "[]")
    readonly property var settings: JSON.parse(SettingsController.settingsJson || "{}")
    readonly property string game: dlg.clip ? (dlg.clip.game || "") : ""

    anchors.fill: parent
    color: Theme.scrim(70)
    visible: dlg.clip !== null

    function requestJson() {
        var req = {
            platforms: [dlg.platform],
            vertical: dlg.vertical,
            cropPos: dlg.cropPos,
            tracks: dlg.tracksOn.map(i => [i, 1.0])
        }
        if (dlg.shortSide > 0) req.shortSide = dlg.shortSide
        if (dlg.trimStart >= 0 && dlg.trimEnd > dlg.trimStart) {
            req.start = dlg.trimStart
            req.end = dlg.trimEnd
        }
        return JSON.stringify(req)
    }
    function replan() { if (dlg.clip) ShareController.replan(dlg.requestJson()) }

    function fmt(sec) {
        const s = Math.max(0, Math.round(sec))
        return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0")
    }

    onClipChanged: {
        if (!dlg.clip)
            return
        dlg.platform = dlg.settings.sharePlatform || "discord"
        const p = dlg.platforms.find(x => x.id === dlg.platform)
        dlg.vertical = p ? p.vertical : false
        dlg.cropPos = 0.5
        dlg.shortSide = 0
        dlg.tracksOn = []
        dlg.toast = ""
        // Read the game off the new clip directly: inside this handler the
        // `game` binding can still hold the previous clip's value.
        dlg.captionText = ShareController.caption(dlg.clip.game || "", dlg.settings.shareTags || "#gaming")
        ShareController.open(dlg.clip.filepath, dlg.requestJson())
    }
    onPlatformChanged: {
        const p = dlg.platforms.find(x => x.id === dlg.platform)
        dlg.vertical = p ? p.vertical : false
        dlg.shortSide = 0
        dlg.replan()
    }
    onVerticalChanged: { dlg.shortSide = 0; dlg.replan() }
    onShortSideChanged: dlg.replan()
    onTracksOnChanged: dlg.replan()

    // First plan: every audio track on (the clip has no pre-mixed track).
    Connections {
        target: ShareController
        function onPlanJsonChanged() {
            const p = JSON.parse(ShareController.planJson || "{}")
            if (p.source && dlg.tracksOn.length === 0 && p.source.audioStreams > 0 && p.tracks.length === 0) {
                var all = []
                for (var i = 0; i < p.source.audioStreams; i++) all.push(i)
                dlg.tracksOn = all
            }
        }
    }

    Timer { id: toastTimer; interval: 1800; onTriggered: dlg.toast = "" }
    function flash(msg) { dlg.toast = msg; toastTimer.restart() }

    function close() {
        if (ShareController.running)
            ShareController.cancel()
        // Remember the platform and the user's own tags for next time.
        SettingsController.setValue("sharePlatform", JSON.stringify(dlg.platform))
        SettingsController.setValue("shareTags",
            JSON.stringify(ShareController.rememberTags(dlg.captionText, dlg.game)))
        dlg.closed()
    }

    MouseArea { anchors.fill: parent }   // swallow clicks behind the sheet
    Keys.onEscapePressed: dlg.close()

    component SectionLabel: Text {
        color: Theme.textMuted
        font.pixelSize: 10
        font.weight: Font.Bold
        font.letterSpacing: 0.8
    }

    Rectangle {
        anchors.centerIn: parent
        width: 480
        implicitHeight: col.implicitHeight + 40
        radius: Theme.radiusLg
        color: Theme.bg
        border.width: 1
        border.color: Theme.border

        ColumnLayout {
            id: col
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 20
            spacing: 12

            RowLayout {
                Layout.fillWidth: true
                Icon { name: "share"; size: 18; color: Theme.accent }
                Text {
                    text: (I18n.language, I18n.t("share.title"))
                    color: Theme.text
                    font.pixelSize: 18
                    font.weight: Font.Bold
                }
                Item { Layout.fillWidth: true }
                Text {
                    Layout.maximumWidth: 220
                    text: dlg.clip ? dlg.clip.title : ""
                    color: Theme.textMuted
                    font.pixelSize: 11
                    elide: Text.ElideMiddle
                }
            }

            SectionLabel { text: (I18n.language, I18n.t("share.platform").toUpperCase()) }
            Flow {
                Layout.fillWidth: true
                spacing: 6
                Repeater {
                    model: dlg.platforms
                    ChoiceChip {
                        required property var modelData
                        text: modelData.id === "original" ? I18n.t("share.original") : modelData.label
                        selected: dlg.platform === modelData.id
                        enabled: !ShareController.running
                        onTriggered: dlg.platform = modelData.id
                    }
                }
            }

            // Plan summary: what will actually be produced.
            Rectangle {
                Layout.fillWidth: true
                implicitHeight: planCol.implicitHeight + 20
                radius: Theme.radius
                color: Theme.surface
                border.width: 1
                border.color: dlg.plan.fits === false ? Theme.danger : Theme.border
                ColumnLayout {
                    id: planCol
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.margins: 12
                    spacing: 4
                    Text {
                        Layout.fillWidth: true
                        text: ShareController.probing ? (I18n.language, I18n.t("share.reading"))
                            : !dlg.plan.width ? (ShareController.error || "")
                            : dlg.plan.width + "×" + dlg.plan.height + " • " + Math.round(dlg.plan.fps) + " fps • "
                              + dlg.fmt(dlg.plan.duration) + " • ≈ " + dlg.plan.estimateLabel
                        color: ShareController.error.length > 0 && !dlg.plan.width ? Theme.danger : Theme.text
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                    }
                    Text {
                        Layout.fillWidth: true
                        visible: !!dlg.plan.width
                        text: (I18n.language, dlg.plan.fits === false ? I18n.t("share.tooBig")
                            : dlg.plan.spec && dlg.plan.spec.maxSecs && dlg.plan.source
                              && dlg.plan.source.duration > dlg.plan.spec.maxSecs
                              ? I18n.t("share.trimmedToLimit").replace("{secs}", Math.round(dlg.plan.spec.maxSecs))
                            : I18n.t("share.fits"))
                        color: dlg.plan.fits === false ? Theme.danger : Theme.textDim
                        font.pixelSize: 11
                        wrapMode: Text.WordWrap
                    }
                }
            }

            SectionLabel { text: (I18n.language, I18n.t("share.quality").toUpperCase()) }
            Flow {
                Layout.fillWidth: true
                spacing: 6
                ChoiceChip {
                    text: (I18n.language, I18n.t("share.auto"))
                    selected: dlg.shortSide === 0
                    enabled: !ShareController.running
                    onTriggered: dlg.shortSide = 0
                }
                Repeater {
                    model: dlg.plan.ladder || []
                    ChoiceChip {
                        required property var modelData
                        text: (I18n.language, modelData.shortSide + "p" + (modelData.original ? " · " + I18n.t("share.original") : ""))
                        selected: dlg.shortSide === modelData.shortSide
                        enabled: !ShareController.running
                        onTriggered: dlg.shortSide = modelData.shortSide
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                Text {
                    text: (I18n.language, I18n.t("share.vertical"))
                    color: Theme.text
                    font.pixelSize: 12
                }
                InfoIcon { tooltipText: I18n.t("share.verticalHint") }
                Item { Layout.fillWidth: true }
                ToggleSwitch {
                    checked: dlg.vertical
                    onToggled: (v) => dlg.vertical = v
                }
            }
            HSlider {
                Layout.fillWidth: true
                visible: dlg.vertical
                from: 0; to: 100
                value: dlg.cropPos * 100
                suffix: "%"
                onMoved: (v) => { dlg.cropPos = v / 100; dlg.replan() }
            }

            // Tracks: the clip carries one per mixer channel and no mix.
            ColumnLayout {
                Layout.fillWidth: true
                visible: !!dlg.plan.source && dlg.plan.source.audioStreams > 1
                spacing: 6
                SectionLabel { text: (I18n.language, I18n.t("share.tracks").toUpperCase()) }
                Flow {
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: dlg.plan.source ? dlg.plan.source.audioStreams : 0
                        ChoiceChip {
                            required property int index
                            text: (I18n.language, I18n.t("share.track").replace("{n}", index + 1))
                            selected: dlg.tracksOn.indexOf(index) >= 0
                            enabled: !ShareController.running
                            onTriggered: {
                                const on = dlg.tracksOn.slice()
                                const at = on.indexOf(index)
                                if (at >= 0) on.splice(at, 1); else on.push(index)
                                on.sort()
                                dlg.tracksOn = on
                            }
                        }
                    }
                }
            }

            SectionLabel { text: (I18n.language, I18n.t("share.caption").toUpperCase()) }
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 34
                radius: Theme.radius
                color: Theme.surface
                border.width: 1
                border.color: captionField.activeFocus ? Theme.accent : Theme.border
                TextField {
                    id: captionField
                    anchors.fill: parent
                    leftPadding: 10
                    rightPadding: 10
                    verticalAlignment: TextInput.AlignVCenter
                    color: Theme.text
                    font.pixelSize: 13
                    background: Item {}
                    selectByMouse: true
                    text: dlg.captionText
                    onTextEdited: dlg.captionText = text
                }
            }

            // Progress
            ColumnLayout {
                visible: ShareController.running
                Layout.fillWidth: true
                spacing: 4
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 6
                    radius: 3
                    color: Theme.border
                    Rectangle {
                        width: parent.width * ShareController.progress
                        height: parent.height
                        radius: 3
                        color: Theme.accent
                    }
                }
                Text {
                    text: (I18n.language, I18n.t("share.encoding")) + " · " + Math.round(ShareController.progress * 100) + "%"
                    color: Theme.textMuted
                    font.pixelSize: 11
                }
            }

            Text {
                Layout.fillWidth: true
                visible: ShareController.error.length > 0 && !!dlg.plan.width
                text: ShareController.error
                color: Theme.danger
                font.pixelSize: 11
                wrapMode: Text.WordWrap
            }

            // Done: get the file where it is going.
            Flow {
                Layout.fillWidth: true
                visible: ShareController.resultPath.length > 0
                spacing: 8
                ClipsBarButton {
                    label: (I18n.language, I18n.t("share.copyFile"))
                    icon: "copy"
                    onTriggered: {
                        const err = ShareController.copyFile(ShareController.resultPath)
                        dlg.flash(err.length > 0 ? err : I18n.t("share.copied"))
                    }
                }
                ClipsBarButton {
                    visible: dlg.platform !== "original" && dlg.platform !== "discord" && dlg.platform !== "discordnitro"
                    label: (I18n.language, I18n.t("share.openUpload"))
                    icon: "external-link"
                    onTriggered: {
                        SystemController.writeClipboard(dlg.captionText)
                        const err = ShareController.openUploadPage(dlg.platform, dlg.captionText)
                        dlg.flash(err.length > 0 ? err : I18n.t("share.captionCopied"))
                    }
                }
                ClipsBarButton {
                    label: (I18n.language, I18n.t("clips.contextMenu.showInFolder"))
                    icon: "folder"
                    onTriggered: SystemController.revealInFolder(ShareController.resultPath)
                }
            }

            Text {
                visible: dlg.toast.length > 0
                text: dlg.toast
                color: Theme.success
                font.pixelSize: 11
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 4
                spacing: 8
                Item { Layout.fillWidth: true }
                Rectangle {
                    Layout.preferredWidth: 84
                    Layout.preferredHeight: 32
                    radius: Theme.radius
                    color: closeArea.containsMouse ? Theme.bgHover : Theme.surface
                    border.width: 1
                    border.color: Theme.border
                    Text {
                        anchors.centerIn: parent
                        text: (I18n.language, ShareController.running ? I18n.t("common.cancel") : I18n.t("common.close"))
                        color: Theme.text
                        font.pixelSize: 13
                    }
                    MouseArea {
                        id: closeArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: ShareController.running ? ShareController.cancel() : dlg.close()
                    }
                }
                Rectangle {
                    visible: ShareController.resultPath.length === 0
                    Layout.preferredWidth: 120
                    Layout.preferredHeight: 32
                    radius: Theme.radius
                    readonly property bool ready: !!dlg.plan.width && !ShareController.running && dlg.plan.fits !== false
                    color: ready ? Theme.accent : Theme.bgHover
                    Text {
                        anchors.centerIn: parent
                        text: (I18n.language, ShareController.running ? I18n.t("share.encoding") : I18n.t("share.makeFile"))
                        color: parent.ready ? "#ffffff" : Theme.textDim
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                    }
                    MouseArea {
                        anchors.fill: parent
                        enabled: parent.ready
                        cursorShape: Qt.PointingHandCursor
                        onClicked: ShareController.render()
                    }
                }
            }
        }
    }
}
