import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings → Capture & Sound. QML port of CaptureSoundSettings.vue: GSR
// recording settings + pre-flight diagnostics, and the OBS-style capture
// track list. Drag-to-reorder tracks (WebKitGTK dataTransfer workaround in
// the Vue version) is simplified to up/down move buttons here — QML doesn't
// need the dataTransfer workaround, but a full DnD port isn't worth the
// complexity for a settings-page reorder control.
ColumnLayout {
    id: root
    width: parent.width
    spacing: 20

    property var s: JSON.parse(SettingsController.settingsJson || "{}")
    property var monitors: JSON.parse(RecordingController.monitorsJson || "[]")
    property var audioSinks: JSON.parse(RecordingController.audioSinksJson || "[]")
    property var captureSources: JSON.parse(RecordingController.captureSourcesJson || "[]")
    property string sessionType: RecordingController.sessionType || "unknown"
    property bool isWayland: root.sessionType === "wayland"
    property var deps: JSON.parse(SystemController.depsJson || "[]")

    readonly property var monitorOptions: {
        const opts = root.monitors.map(m => ({ value: m.name, label: m.label }))
        if (root.isWayland) {
            opts.forEach(o => { if (o.value === "screen") o.label = I18n.t("settings.captureGsr.portalOption") })
        } else {
            opts.push({ value: "focused", label: "Fullscreen Application" })
        }
        return opts
    }
    readonly property var captureSourceOptions: root.captureSources.length
        ? root.captureSources
        : ["Game", "Chat", "Media", "Aux", "Mic"].map(v => ({ value: "OpenGG_" + v + ".monitor", label: v }))

    Connections {
        target: SettingsController
        function onSettingsJsonChanged() { root.s = JSON.parse(SettingsController.settingsJson || "{}") }
    }
    Connections {
        target: RecordingController
        function onMonitorsJsonChanged() { root.monitors = JSON.parse(RecordingController.monitorsJson || "[]") }
        function onAudioSinksJsonChanged() { root.audioSinks = JSON.parse(RecordingController.audioSinksJson || "[]") }
        function onCaptureSourcesJsonChanged() { root.captureSources = JSON.parse(RecordingController.captureSourcesJson || "[]") }
        function onSessionTypeChanged() { root.sessionType = RecordingController.sessionType }
    }
    Connections {
        target: SystemController
        function onDepsJsonChanged() { root.deps = JSON.parse(SystemController.depsJson || "[]") }
    }

    function set(key, value) {
        SettingsController.setValue(key, JSON.stringify(value))
        if (root.s.gsrEnabled) RecordingController.restart()
    }

    function captureTrackSources() {
        return (root.s.captureTracks || []).map(t => t.source)
    }

    // Functions, not `readonly property var` array literals: QML's automatic
    // dependency tracking only follows PROPERTY reads made while a binding
    // evaluates, not invokable calls like I18n.t() — an array built once from
    // I18n.t() results never re-evaluates on a live language switch (same
    // landmine as MixerPage.qml's `tabs`/`tabLabel()`). The
    // `(I18n.language, ...)` pattern at each SelectField call site is what
    // actually re-triggers this.
    function gsrQualityOptions() {
        return [
            { value: "cbr", label: I18n.t("settings.captureGsr.qualityCbr") },
            { value: "medium", label: I18n.t("settings.captureGsr.qualityMedium") },
            { value: "high", label: I18n.t("settings.captureGsr.qualityHigh") },
            { value: "very_high", label: I18n.t("settings.captureGsr.qualityVeryHigh") },
            { value: "ultra", label: I18n.t("settings.captureGsr.qualityUltra") },
        ]
    }
    function gsrFpsOptions() {
        return [30, 60, 120].map(v => ({ value: v, label: I18n.t("dashboard.gsrFps." + v) }))
    }
    function gsrReplayOptions() {
        return [
            { value: "15", label: I18n.t("dashboard.gsrReplay.15") },
            { value: "30", label: I18n.t("dashboard.gsrReplay.30") },
            { value: "60", label: I18n.t("dashboard.gsrReplay.60") },
            { value: "90", label: I18n.t("dashboard.gsrReplay.90") },
            { value: "120", label: I18n.t("dashboard.gsrReplay.120") },
            { value: "custom", label: I18n.t("settings.captureGsr.replayCustom") },
        ]
    }

    function estFileMb() {
        const q = root.s.gsrQuality
        const kbps = q === "cbr" ? (root.s.gsrCbrBitrate || 8000)
            : ({ medium: 4000, high: 6000, very_high: 12000, ultra: 20000 }[q] || 8000)
        return Math.round((kbps * (root.s.gsrReplaySecs || 30)) / 8 / 1024)
    }
    function estRamMb() { return Math.ceil(root.estFileMb() * 1.2) }

    SettingsHeading { titleText: (I18n.language, I18n.t("settings.captureSound.title")) }

    // ── GSR settings card ──
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: gsrCol.implicitHeight + 40

        ColumnLayout {
            id: gsrCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 14

            RecorderInstallHelper { Layout.fillWidth: true }

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.captureGsr.title"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                Rectangle {
                    radius: 8
                    color: Theme.accentAlpha(15)
                    implicitWidth: betaText.implicitWidth + 12
                    implicitHeight: 18
                    Text { id: betaText; anchors.centerIn: parent; text: (I18n.language, I18n.t("common.beta")); color: Theme.accent; font.pixelSize: 9; font.weight: Font.Bold }
                }
                InfoIcon { tooltipText: I18n.t("settings.captureGsr.hint") }
                Item { Layout.fillWidth: true }
                Text {
                    visible: !!root.s.gsrEnabled
                    text: (I18n.language, I18n.t("settings.captureGsr.estUsage")
                           .replace("{ram}", root.estRamMb()).replace("{file}", root.estFileMb()))
                    color: Theme.textDim
                    font.pixelSize: 10
                }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            ColumnLayout {
                visible: !!root.s.gsrEnabled
                Layout.fillWidth: true
                spacing: 12

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Text { text: (I18n.language, I18n.t("settings.captureGsr.quality")); color: Theme.textDim; font.pixelSize: 12; Layout.preferredWidth: 140 }
                    ComboBox {
                        id: qualityCombo
                        Layout.fillWidth: true
                        model: (I18n.language, root.gsrQualityOptions())
                        textRole: "label"; valueRole: "value"
                        currentIndex: root.gsrQualityOptions().findIndex(o => o.value === root.s.gsrQuality)
                        onActivated: root.set("gsrQuality", root.gsrQualityOptions()[currentIndex].value)
                        contentItem: Text { leftPadding: 12; text: qualityCombo.displayText; color: Theme.text; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter }
                        background: Rectangle { implicitHeight: 32; radius: Theme.radius; color: Theme.bg; border.width: 1; border.color: qualityCombo.activeFocus ? Theme.accent : Theme.border }
                        delegate: ItemDelegate {
                            width: qualityCombo.width
                            highlighted: qualityCombo.highlightedIndex === index
                            contentItem: Text { text: modelData.label; color: highlighted ? Theme.accent : Theme.text; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter }
                            background: Rectangle { color: highlighted ? Theme.accentAlpha(10) : Theme.surface }
                        }
                        popup: Popup {
                            y: qualityCombo.height + 4
                            width: qualityCombo.width
                            implicitHeight: contentItem.implicitHeight
                            padding: 4
                            contentItem: ListView { clip: true; implicitHeight: contentHeight; model: qualityCombo.popup.visible ? qualityCombo.delegateModel : null; currentIndex: qualityCombo.highlightedIndex }
                            background: Rectangle { radius: Theme.radius; color: Theme.surface; border.width: 1; border.color: Theme.border }
                        }
                    }
                    TextField {
                        visible: root.s.gsrQuality === "cbr"
                        Layout.preferredWidth: 100
                        text: String(root.s.gsrCbrBitrate || 8000)
                        color: Theme.text
                        font.pixelSize: 11
                        validator: IntValidator { bottom: 500; top: 100000 }
                        background: Rectangle { radius: Theme.radius; color: Theme.bg; border.width: 1; border.color: Theme.border }
                        onEditingFinished: root.set("gsrCbrBitrate", Math.max(500, Math.min(100000, parseInt(text) || 8000)))
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Text { text: (I18n.language, I18n.t("settings.captureGsr.fps")); color: Theme.textDim; font.pixelSize: 12; Layout.preferredWidth: 140 }
                    ComboBox {
                        id: fpsCombo
                        Layout.fillWidth: true
                        model: (I18n.language, root.gsrFpsOptions())
                        textRole: "label"; valueRole: "value"
                        currentIndex: root.gsrFpsOptions().findIndex(o => o.value === root.s.gsrFps)
                        onActivated: root.set("gsrFps", root.gsrFpsOptions()[currentIndex].value)
                        contentItem: Text { leftPadding: 12; text: fpsCombo.displayText; color: Theme.text; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter }
                        background: Rectangle { implicitHeight: 32; radius: Theme.radius; color: Theme.bg; border.width: 1; border.color: fpsCombo.activeFocus ? Theme.accent : Theme.border }
                        delegate: ItemDelegate {
                            width: fpsCombo.width
                            highlighted: fpsCombo.highlightedIndex === index
                            contentItem: Text { text: modelData.label; color: highlighted ? Theme.accent : Theme.text; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter }
                            background: Rectangle { color: highlighted ? Theme.accentAlpha(10) : Theme.surface }
                        }
                        popup: Popup {
                            y: fpsCombo.height + 4
                            width: fpsCombo.width
                            implicitHeight: contentItem.implicitHeight
                            padding: 4
                            contentItem: ListView { clip: true; implicitHeight: contentHeight; model: fpsCombo.popup.visible ? fpsCombo.delegateModel : null; currentIndex: fpsCombo.highlightedIndex }
                            background: Rectangle { radius: Theme.radius; color: Theme.surface; border.width: 1; border.color: Theme.border }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Text { text: (I18n.language, I18n.t("settings.captureGsr.replayBuffer")); color: Theme.textDim; font.pixelSize: 12; Layout.preferredWidth: 140 }
                    ComboBox {
                        id: replayCombo
                        Layout.fillWidth: true
                        model: (I18n.language, root.gsrReplayOptions())
                        textRole: "label"; valueRole: "value"
                        currentIndex: root.gsrReplayOptions().findIndex(o => o.value === root.s.gsrReplayPreset)
                        onActivated: {
                            const preset = root.gsrReplayOptions()[currentIndex].value
                            SettingsController.setValue("gsrReplayPreset", JSON.stringify(preset))
                            if (preset !== "custom") root.set("gsrReplaySecs", Number(preset))
                        }
                        contentItem: Text { leftPadding: 12; text: replayCombo.displayText; color: Theme.text; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter }
                        background: Rectangle { implicitHeight: 32; radius: Theme.radius; color: Theme.bg; border.width: 1; border.color: replayCombo.activeFocus ? Theme.accent : Theme.border }
                        delegate: ItemDelegate {
                            width: replayCombo.width
                            highlighted: replayCombo.highlightedIndex === index
                            contentItem: Text { text: modelData.label; color: highlighted ? Theme.accent : Theme.text; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter }
                            background: Rectangle { color: highlighted ? Theme.accentAlpha(10) : Theme.surface }
                        }
                        popup: Popup {
                            y: replayCombo.height + 4
                            width: replayCombo.width
                            implicitHeight: contentItem.implicitHeight
                            padding: 4
                            contentItem: ListView { clip: true; implicitHeight: contentHeight; model: replayCombo.popup.visible ? replayCombo.delegateModel : null; currentIndex: replayCombo.highlightedIndex }
                            background: Rectangle { radius: Theme.radius; color: Theme.surface; border.width: 1; border.color: Theme.border }
                        }
                    }
                    TextField {
                        visible: root.s.gsrReplayPreset === "custom"
                        Layout.preferredWidth: 80
                        text: String(root.s.gsrReplaySecs || 30)
                        color: Theme.text
                        font.pixelSize: 11
                        validator: IntValidator { bottom: 5; top: 600 }
                        background: Rectangle { radius: Theme.radius; color: Theme.bg; border.width: 1; border.color: Theme.border }
                        onEditingFinished: root.set("gsrReplaySecs", Math.max(5, Math.min(600, parseInt(text) || 30)))
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Text { text: I18n.t("settings.captureGsr.monitorTarget"); color: Theme.textDim; font.pixelSize: 12; Layout.preferredWidth: 140 }
                    ComboBox {
                        id: monitorCombo
                        Layout.fillWidth: true
                        model: root.monitorOptions
                        textRole: "label"; valueRole: "value"
                        currentIndex: Math.max(0, root.monitorOptions.findIndex(o => o.value === root.s.gsrMonitorTarget))
                        onActivated: root.set("gsrMonitorTarget", root.monitorOptions[currentIndex].value)
                        contentItem: Text { leftPadding: 12; text: monitorCombo.displayText; color: Theme.text; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight }
                        background: Rectangle { implicitHeight: 32; radius: Theme.radius; color: Theme.bg; border.width: 1; border.color: monitorCombo.activeFocus ? Theme.accent : Theme.border }
                        delegate: ItemDelegate {
                            width: monitorCombo.width
                            highlighted: monitorCombo.highlightedIndex === index
                            contentItem: Text { text: modelData.label; color: highlighted ? Theme.accent : Theme.text; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter }
                            background: Rectangle { color: highlighted ? Theme.accentAlpha(10) : Theme.surface }
                        }
                        popup: Popup {
                            y: monitorCombo.height + 4
                            width: monitorCombo.width
                            implicitHeight: contentItem.implicitHeight
                            padding: 4
                            contentItem: ListView { clip: true; implicitHeight: contentHeight; model: monitorCombo.popup.visible ? monitorCombo.delegateModel : null; currentIndex: monitorCombo.highlightedIndex }
                            background: Rectangle { radius: Theme.radius; color: Theme.surface; border.width: 1; border.color: Theme.border }
                        }
                    }
                }
                Text {
                    horizontalAlignment: Text.AlignLeft
                    visible: root.isWayland && root.monitorOptions.some(o => o.value === "focused")
                    text: I18n.t("settings.captureGsr.waylandHint")
                    color: Theme.overdrive
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                RowLayout {
                    Layout.fillWidth: true
                    Text { text: I18n.t("settings.captureGsr.autoStart"); color: Theme.textDim; font.pixelSize: 12; Layout.fillWidth: true; horizontalAlignment: Text.AlignLeft }
                    InfoIcon { tooltipText: I18n.t("settings.captureGsr.autoStartTooltip") }
                    ToggleSwitch {
                        checked: !!root.s.gsrAutoStart
                        onToggled: (v) => SettingsController.setValue("gsrAutoStart", JSON.stringify(v))
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    Text { text: I18n.t("settings.captureGsr.autoRestart"); color: Theme.textDim; font.pixelSize: 12; Layout.fillWidth: true; horizontalAlignment: Text.AlignLeft }
                    InfoIcon { tooltipText: I18n.t("settings.captureGsr.autoRestartTooltip") }
                    ToggleSwitch {
                        checked: !!root.s.gsrAutoRestart
                        onToggled: (v) => SettingsController.setValue("gsrAutoRestart", JSON.stringify(v))
                    }
                }

                // ── Diagnostics ──
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: 4
                    spacing: 10

                    Rectangle {
                        width: 160; height: 30
                        radius: Theme.radius
                        color: Theme.bg
                        border.width: 1
                        border.color: Theme.border
                        opacity: RecordingController.diagnosticsRunning ? 0.5 : 1.0
                        Text {
                            anchors.centerIn: parent
                            text: RecordingController.diagnosticsRunning ? "Running…" : I18n.t("settings.captureGsr.runDiagnostics")
                            color: Theme.text
                            font.pixelSize: 11
                        }
                        MouseArea {
                            anchors.fill: parent
                            enabled: !RecordingController.diagnosticsRunning
                            cursorShape: Qt.PointingHandCursor
                            onClicked: RecordingController.runDiagnostics(
                                JSON.stringify(root.captureTrackSources()),
                                root.s.gsrMonitorTarget || "screen"
                            )
                        }
                    }

                    ColumnLayout {
                        id: diagResults
                        visible: RecordingController.diagnosticsJson && RecordingController.diagnosticsJson !== "{}"
                        Layout.fillWidth: true
                        spacing: 8
                        property var result: JSON.parse(RecordingController.diagnosticsJson || "{}")

                        Text {
                            text: diagResults.result.ok ? I18n.t("settings.captureGsr.diagnosticsOk") : I18n.t("settings.captureGsr.diagnosticsFail")
                            color: diagResults.result.ok ? Theme.success : Theme.danger
                            font.pixelSize: 12
                            font.weight: Font.DemiBold
                        }

                        Repeater {
                            model: diagResults.result.items || []
                            ColumnLayout {
                                id: diagItem
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 2
                                // CaptureSoundSettings.vue:332 prefixes these with
                                // the text glyphs "✗"/"⚠". Deliberate divergence:
                                // we already have the vector equivalents, and
                                // glyph coverage is exactly what made emoji
                                // render as tofu boxes elsewhere in this shell.
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 6
                                    readonly property string sev: diagItem.modelData.severity
                                    readonly property color sevColor: sev === "error" ? Theme.danger
                                                                    : sev === "warning" ? Theme.overdrive
                                                                    : Theme.textDim
                                    Icon {
                                        name: parent.sev === "error" ? "x"
                                            : parent.sev === "warning" ? "alert-triangle" : "info"
                                        size: 12
                                        color: parent.sevColor
                                        Layout.alignment: Qt.AlignTop
                                    }
                                    Text {
                                        text: diagItem.modelData.message
                                        color: parent.sevColor
                                        font.pixelSize: 11
                                        wrapMode: Text.WordWrap
                                        Layout.fillWidth: true
                                    }
                                }
                                Text {
                                    visible: !!(diagItem.modelData.fix && diagItem.modelData.fix.command)
                                    text: diagItem.modelData.fix ? diagItem.modelData.fix.command : ""
                                    color: Theme.text
                                    font.pixelSize: 10
                                    font.family: "monospace"
                                    Layout.leftMargin: 14
                                    wrapMode: Text.WordWrap
                                    Layout.fillWidth: true
                                }
                            }
                        }

                        Rectangle {
                            visible: !!diagResults.result.report
                            width: 170; height: 26
                            radius: Theme.radius
                            color: Theme.bg
                            border.width: 1
                            border.color: Theme.border
                            Text { anchors.centerIn: parent; text: I18n.t("settings.captureGsr.copyDiagnostics"); color: Theme.textDim; font.pixelSize: 10 }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: SystemController.writeClipboard(diagResults.result.report)
                            }
                        }
                    }
                }
            }

            Text {
                horizontalAlignment: Text.AlignLeft
                visible: !root.s.gsrEnabled
                text: I18n.t("settings.captureGsr.extensionsHint")
                color: Theme.textDim
                font.pixelSize: 12
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    // ── Capture devices card ──
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: devCol.implicitHeight + 40

        ColumnLayout {
            id: devCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 12

            RowLayout {
                visible: root.deps.some(d => d.feature === "export" && !d.available)
                Layout.fillWidth: true
                spacing: 8
                Icon { name: "alert-triangle"; size: 13; color: Theme.danger}
                Text {
                    horizontalAlignment: Text.AlignLeft
                    text: I18n.t("settings.deps.missingFfmpeg")
                    color: Theme.text
                    font.pixelSize: 12
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }
            }

            RowLayout {
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.captureSound.captureDevices"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                InfoIcon { tooltipText: I18n.t("settings.captureSound.captureHint") }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            Repeater {
                model: root.s.captureTracks || []
                RowLayout {
                    id: trackRow
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    spacing: 8

                    ColumnLayout {
                        spacing: 0
                        Icon {
                            name: "chevron-up"; size: 12
                            color: trackRow.index === 0 ? Theme.border : Theme.textDim
                            MouseArea {
                                anchors.fill: parent
                                anchors.margins: -4
                                enabled: trackRow.index > 0
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    const arr = (root.s.captureTracks || []).slice()
                                    const tmp = arr[trackRow.index - 1]
                                    arr[trackRow.index - 1] = arr[trackRow.index]
                                    arr[trackRow.index] = tmp
                                    root.set("captureTracks", arr)
                                }
                            }
                        }
                        Icon {
                            name: "chevron-down"; size: 12
                            color: trackRow.index === (root.s.captureTracks || []).length - 1 ? Theme.border : Theme.textDim
                            MouseArea {
                                anchors.fill: parent
                                anchors.margins: -4
                                enabled: trackRow.index < (root.s.captureTracks || []).length - 1
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    const arr = (root.s.captureTracks || []).slice()
                                    const tmp = arr[trackRow.index + 1]
                                    arr[trackRow.index + 1] = arr[trackRow.index]
                                    arr[trackRow.index] = tmp
                                    root.set("captureTracks", arr)
                                }
                            }
                        }
                    }

                    Text {
                        text: (I18n.language, I18n.t("settings.captureSound.trackLabel")) + " " + (trackRow.index + 1)
                        color: Theme.text
                        font.pixelSize: 12
                        Layout.preferredWidth: 90
                    }

                    ComboBox {
                        id: sourceCombo
                        Layout.fillWidth: true
                        model: root.captureSourceOptions
                        textRole: "label"; valueRole: "value"
                        currentIndex: Math.max(0, root.captureSourceOptions.findIndex(o => o.value === trackRow.modelData.source))
                        onActivated: {
                            const arr = (root.s.captureTracks || []).slice()
                            arr[trackRow.index] = Object.assign({}, arr[trackRow.index], { source: root.captureSourceOptions[currentIndex].value })
                            root.set("captureTracks", arr)
                        }
                        contentItem: Text { leftPadding: 12; text: sourceCombo.displayText; color: Theme.text; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight }
                        background: Rectangle { implicitHeight: 30; radius: Theme.radius; color: Theme.bg; border.width: 1; border.color: sourceCombo.activeFocus ? Theme.accent : Theme.border }
                        delegate: ItemDelegate {
                            width: sourceCombo.width
                            highlighted: sourceCombo.highlightedIndex === index
                            contentItem: Text { text: modelData.label; color: highlighted ? Theme.accent : Theme.text; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter }
                            background: Rectangle { color: highlighted ? Theme.accentAlpha(10) : Theme.surface }
                        }
                        popup: Popup {
                            y: sourceCombo.height + 4
                            width: sourceCombo.width
                            implicitHeight: contentItem.implicitHeight
                            padding: 4
                            contentItem: ListView { clip: true; implicitHeight: contentHeight; model: sourceCombo.popup.visible ? sourceCombo.delegateModel : null; currentIndex: sourceCombo.highlightedIndex }
                            background: Rectangle { radius: Theme.radius; color: Theme.surface; border.width: 1; border.color: Theme.border }
                        }
                    }

                    Rectangle {
                        visible: (root.s.captureTracks || []).length > 1
                        width: 24; height: 24
                        radius: Theme.radius
                        color: "transparent"
                        Icon { anchors.centerIn: parent; name: "x"; size: 12; color: Theme.textDim}
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                const arr = (root.s.captureTracks || []).filter((_, i) => i !== trackRow.index)
                                root.set("captureTracks", arr)
                            }
                        }
                    }
                }
            }

            Rectangle {
                Layout.topMargin: 4
                width: 130; height: 30
                radius: Theme.radius
                color: Theme.bg
                border.width: 1
                border.color: Theme.border
                Text {
                    anchors.centerIn: parent
                    text: (I18n.language, I18n.t("settings.captureSound.addTrack"))
                    color: Theme.textDim
                    font.pixelSize: 12
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        const arr = (root.s.captureTracks || []).slice()
                        const n = arr.length + 1
                        const def = root.captureSourceOptions[0] ? root.captureSourceOptions[0].value : "OpenGG_Game.monitor"
                        arr.push({ name: "Track " + n, source: def })
                        root.set("captureTracks", arr)
                    }
                }
            }
        }
    }

    Component.onCompleted: {
        SettingsController.refresh()
        SystemController.refresh()
        RecordingController.refreshDevices()
    }
}
