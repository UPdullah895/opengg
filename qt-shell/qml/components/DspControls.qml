import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Per-channel Noise Reduction / Noise Gate / Compressor controls.
//
// These drive real PipeWire stages now (see core/src/voicefx.rs); they were
// no-ops for a long time. The gate needs nothing beyond PipeWire. The
// compressor and noise reduction need optional LADSPA plugins; when one is
// missing its switch is disabled and the install command is shown instead.
// Settings persist per channel in ui-settings (`voiceFx`) and are re-applied
// when the page loads, so a restart no longer silently switches them off.
ColumnLayout {
    id: root
    spacing: 16

    property string channel: ""
    property color accentColor: Theme.accent

    property bool nrEnabled: false
    property real nrIntensity: 50
    property bool gateEnabled: false
    property real gateThreshold: -40
    property bool gateAuto: false
    property bool compEnabled: false
    property real compLevel: 50
    property string activePreset: "Default"

    readonly property var caps: JSON.parse(EqController.dspCapsJson || "{}")
    /// Waiting on a noise-floor measurement for auto-detect.
    property bool measuring: false
    property bool loaded: false

    // Slider drags call the setters on every step; coalesce the disk writes.
    Timer { id: saveDebounce; interval: 400; onTriggered: root.writeSettings() }
    function save() { if (root.loaded) saveDebounce.restart() }
    Component.onDestruction: if (saveDebounce.running) root.writeSettings()
    function writeSettings() {
        const all = JSON.parse(SettingsController.settingsJson || "{}").voiceFx || {}
        all[root.channel] = {
            nr: { enabled: root.nrEnabled, intensity: root.nrIntensity },
            gate: { enabled: root.gateEnabled, threshold: root.gateThreshold, auto: root.gateAuto },
            comp: { enabled: root.compEnabled, level: root.compLevel },
            preset: root.activePreset
        }
        SettingsController.setValue("voiceFx", JSON.stringify(all))
    }
    function applyAll() {
        EqController.applyNoiseReduction(root.channel, root.nrEnabled && !!root.caps.noiseReduction, root.nrIntensity)
        EqController.applyNoiseGate(root.channel, root.gateEnabled, root.gateThreshold, root.gateAuto)
        EqController.applyCompressor(root.channel, root.compEnabled && !!root.caps.compressor, root.compLevel)
    }
    Component.onCompleted: {
        const saved = (JSON.parse(SettingsController.settingsJson || "{}").voiceFx || {})[root.channel]
        if (saved) {
            root.nrEnabled = !!saved.nr.enabled; root.nrIntensity = saved.nr.intensity
            root.gateEnabled = !!saved.gate.enabled; root.gateThreshold = saved.gate.threshold
            root.gateAuto = !!saved.gate.auto
            root.compEnabled = !!saved.comp.enabled; root.compLevel = saved.comp.level
            root.activePreset = saved.preset || "Custom"
            root.applyAll()
        }
        root.loaded = true
        // The install hints are distro-specific; nothing else may have run
        // the system probe yet when the Mixer is the first page opened.
        if ((!root.caps.compressor || !root.caps.noiseReduction) && !SystemController.distroJson)
            SystemController.refresh()
    }

    // Auto-detect: listen to the input for a moment (the user stays quiet)
    // and open the gate a margin above what was heard.
    function detectThreshold() {
        root.measuring = true
        EqController.measureNoiseFloor(root.channel)
    }
    Connections {
        target: EqController
        function onNoiseFloorJsonChanged() {
            const r = JSON.parse(EqController.noiseFloorJson || "{}")
            if (r.channel !== root.channel || !root.measuring) return
            root.measuring = false
            if (typeof r.db !== "number") return
            const t = Math.max(-80, Math.min(-10, Math.round(r.db + 8)))
            root.gateThreshold = t
            EqController.applyNoiseGate(root.channel, root.gateEnabled, t, true)
            root.save()
        }
    }

    function installCommand(what) {
        const d = JSON.parse(SystemController.distroJson || "{}")
        const fam = ((d.id || "") + " " + (d.id_like || "")).toLowerCase()
        const arch = fam.includes("arch"), deb = fam.includes("debian") || fam.includes("ubuntu")
        if (what === "compressor")
            return arch ? "sudo pacman -S swh-plugins"
                 : deb ? "sudo apt install swh-plugins" : "sudo dnf install ladspa-swh-plugins"
        return arch ? "yay -S noise-suppression-for-voice"
             : deb ? "# RNNoise LADSPA: https://github.com/werman/noise-suppression-for-voice"
             : "sudo dnf install noise-suppression-for-voice"
    }

    readonly property var presets: ({
        "Default":   { nr: { enabled: false, intensity: 50 }, gate: { enabled: false, threshold: -40, auto: false }, comp: { enabled: false, level: 50 } },
        "Broadcast": { nr: { enabled: true, intensity: 50 }, gate: { enabled: false, threshold: -40, auto: false }, comp: { enabled: true, level: 70 } },
        "Podcaster": { nr: { enabled: true, intensity: 30 }, gate: { enabled: true, threshold: -30, auto: false }, comp: { enabled: true, level: 60 } },
        "Noise-Reduction Focus": { nr: { enabled: true, intensity: 80 }, gate: { enabled: true, threshold: -40, auto: true }, comp: { enabled: false, level: 50 } },
    })
    readonly property var presetNames: ["Default", "Broadcast", "Podcaster", "Noise-Reduction Focus"]

    function setNr(enabled, intensity) {
        root.nrEnabled = enabled
        root.nrIntensity = intensity
        root.activePreset = "Custom"
        EqController.applyNoiseReduction(root.channel, enabled && !!root.caps.noiseReduction, intensity)
        root.save()
    }
    function setGate(enabled, threshold, auto) {
        root.gateEnabled = enabled
        root.gateThreshold = threshold
        root.gateAuto = auto
        root.activePreset = "Custom"
        EqController.applyNoiseGate(root.channel, enabled, threshold, auto)
        root.save()
        if (enabled && auto && !root.measuring) root.detectThreshold()
    }
    function setComp(enabled, level) {
        root.compEnabled = enabled
        root.compLevel = level
        root.activePreset = "Custom"
        EqController.applyCompressor(root.channel, enabled && !!root.caps.compressor, level)
        root.save()
    }
    function applyPreset(name) {
        if (!(name in root.presets)) return
        const p = root.presets[name]
        root.nrEnabled = p.nr.enabled; root.nrIntensity = p.nr.intensity
        root.gateEnabled = p.gate.enabled; root.gateThreshold = p.gate.threshold; root.gateAuto = p.gate.auto
        root.compEnabled = p.comp.enabled; root.compLevel = p.comp.level
        root.activePreset = name
        root.applyAll()
        root.save()
        if (root.gateEnabled && root.gateAuto) root.detectThreshold()
    }
    function presetLabel(name) {
        const keys = { "Default": "default", "Broadcast": "broadcast", "Podcaster": "podcaster",
                       "Noise-Reduction Focus": "nrFocus", "Custom": "custom" }
        return I18n.t("dsp.presets." + (keys[name] || "custom"))
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 10

        Text {
            text: (I18n.language, I18n.t("dsp.title"))
            color: Theme.text
            font.pixelSize: 16
            font.weight: Font.DemiBold
        }
        Text {
            // root.channel is the identifier passed to EqController
            // (applyNoiseReduction/applyNoiseGate/applyCompressor) — only
            // this label is translated, not the property itself.
            text: (I18n.language, I18n.t("mixer.channels." + root.channel.toLowerCase()))
            color: root.accentColor
            font.pixelSize: 13
            font.weight: Font.DemiBold
        }
        Item { Layout.fillWidth: true }

        ComboBox {
            id: presetCombo
            Layout.preferredWidth: 180
            model: root.activePreset === "Custom" ? root.presetNames.concat(["Custom"]) : root.presetNames
            currentIndex: model.indexOf(root.activePreset)
            onActivated: root.applyPreset(model[currentIndex])

            contentItem: Text {
                text: (I18n.language, root.presetLabel(presetCombo.currentText))
                color: Theme.text
                font.pixelSize: 12
                verticalAlignment: Text.AlignVCenter
                leftPadding: 10
            }
            background: Rectangle {
                radius: Theme.radius
                color: Theme.bg
                border.width: 1
                border.color: Theme.border
            }
            delegate: ItemDelegate {
                width: presetCombo.width
                contentItem: Text { text: (I18n.language, root.presetLabel(modelData)); color: Theme.text; font.pixelSize: 12 }
                background: Rectangle { color: highlighted ? Theme.accent : "transparent"; opacity: highlighted ? 0.15 : 1 }
            }
            popup: Popup {
                y: presetCombo.height
                width: presetCombo.width
                implicitHeight: contentItem.implicitHeight
                padding: 1
                contentItem: ListView {
                    clip: true
                    implicitHeight: contentEnabled ? contentHeight : 0
                    model: presetCombo.popup.visible ? presetCombo.delegateModel : null
                }
                background: Rectangle { color: Theme.surface; border.width: 1; border.color: Theme.border; radius: Theme.radius }
            }
        }
    }

    // ── Noise Reduction ──
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: root.nrEnabled ? root.accentColor : Theme.border
        implicitHeight: nrCol.implicitHeight + 28

        ColumnLayout {
            id: nrCol
            anchors.fill: parent
            anchors.margins: 14
            spacing: 10

            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                ColumnLayout {
                    spacing: 1
                    Layout.fillWidth: true
                    Text { text: (I18n.language, I18n.t("dsp.noiseReduction")); color: Theme.text; font.pixelSize: 13 }
                    Text { text: (I18n.language, I18n.t("dsp.noiseReductionDesc")); color: Theme.textDim; font.pixelSize: 10; wrapMode: Text.WordWrap; Layout.fillWidth: true; horizontalAlignment: Text.AlignLeft }
                }
                ToggleSwitch {
                    checked: root.nrEnabled && !!root.caps.noiseReduction
                    enabled: !!root.caps.noiseReduction
                    onToggled: (v) => root.setNr(v, root.nrIntensity)
                }
            }
            ColumnLayout {
                visible: !root.caps.noiseReduction
                Layout.fillWidth: true
                spacing: 4
                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.textMuted
                    font.pixelSize: 11
                    text: (I18n.language, I18n.t("dsp.needsNoiseReduction"))
                }
                CommandLine { Layout.fillWidth: true; pixelSize: 10; text: root.installCommand("nr") }
                Text {
                    text: (I18n.language, I18n.t("dsp.recheck"))
                    color: root.accentColor
                    font.pixelSize: 11
                    font.underline: recheckManr.containsMouse
                    MouseArea {
                        id: recheckManr
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: { EqController.refreshCaps(); root.applyAll() }
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                opacity: root.nrEnabled ? 1.0 : 0.45
                Text { text: (I18n.language, I18n.t("dsp.intensity")); color: Theme.textDim; font.pixelSize: 11 }
                HSlider {
                    Layout.fillWidth: true
                    from: 0; to: 100
                    value: root.nrIntensity
                    sliderColor: root.accentColor
                    enabled: root.nrEnabled
                    onMoved: (v) => root.setNr(root.nrEnabled, Math.round(v))
                }
            }
        }
    }

    // ── Noise Gate ──
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: root.gateEnabled ? root.accentColor : Theme.border
        implicitHeight: gateCol.implicitHeight + 28

        ColumnLayout {
            id: gateCol
            anchors.fill: parent
            anchors.margins: 14
            spacing: 10

            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                ColumnLayout {
                    spacing: 1
                    Layout.fillWidth: true
                    Text { text: (I18n.language, I18n.t("dsp.noiseGate")); color: Theme.text; font.pixelSize: 13 }
                    Text { text: (I18n.language, I18n.t("dsp.noiseGateDesc")); color: Theme.textDim; font.pixelSize: 10; wrapMode: Text.WordWrap; Layout.fillWidth: true; horizontalAlignment: Text.AlignLeft }
                }
                ToggleSwitch {
                    checked: root.gateEnabled && !!root.caps.gate
                    enabled: !!root.caps.gate
                    onToggled: (v) => root.setGate(v, root.gateThreshold, root.gateAuto)
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 8
                opacity: root.gateEnabled ? 1.0 : 0.45
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    Text { text: (I18n.language, I18n.t("dsp.threshold")); color: Theme.textDim; font.pixelSize: 11 }
                    HSlider {
                        Layout.fillWidth: true
                        from: -80; to: 0
                        value: root.gateThreshold
                        suffix: " dB"
                        sliderColor: root.accentColor
                        enabled: root.gateEnabled
                        onMoved: (v) => root.setGate(root.gateEnabled, Math.round(v), root.gateAuto)
                    }
                }
                RowLayout {
                    spacing: 6
                    CheckBox {
                        id: autoBox
                        checked: root.gateAuto
                        enabled: root.gateEnabled
                        opacity: enabled ? 1 : 0.4
                        onToggled: root.setGate(root.gateEnabled, root.gateThreshold, checked)
                        indicator: Rectangle {
                            implicitWidth: 16
                            implicitHeight: 16
                            x: autoBox.leftPadding
                            y: (autoBox.height - height) / 2
                            radius: 4
                            color: autoBox.checked ? root.accentColor : Theme.surface
                            border.width: 1
                            border.color: autoBox.checked ? root.accentColor : Theme.border
                            Icon {
                                anchors.centerIn: parent
                                visible: autoBox.checked
                                name: "check"; size: 11
                                color: Theme.bg
                            }
                        }
                    }
                    Text {
                        text: root.measuring ? (I18n.language, I18n.t("dsp.measuring"))
                                             : (I18n.language, I18n.t("dsp.autoDetect"))
                        color: root.measuring ? root.accentColor : Theme.textDim
                        font.pixelSize: 11
                    }
                }
            }
        }
    }

    // ── Compressor ──
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: root.compEnabled ? root.accentColor : Theme.border
        implicitHeight: compCol.implicitHeight + 28

        ColumnLayout {
            id: compCol
            anchors.fill: parent
            anchors.margins: 14
            spacing: 10

            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                ColumnLayout {
                    spacing: 1
                    Layout.fillWidth: true
                    Text { text: (I18n.language, I18n.t("dsp.compressor")); color: Theme.text; font.pixelSize: 13 }
                    Text { text: (I18n.language, I18n.t("dsp.compressorDesc")); color: Theme.textDim; font.pixelSize: 10; wrapMode: Text.WordWrap; Layout.fillWidth: true; horizontalAlignment: Text.AlignLeft }
                }
                ToggleSwitch {
                    checked: root.compEnabled && !!root.caps.compressor
                    enabled: !!root.caps.compressor
                    onToggled: (v) => root.setComp(v, root.compLevel)
                }
            }
            ColumnLayout {
                visible: !root.caps.compressor
                Layout.fillWidth: true
                spacing: 4
                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.textMuted
                    font.pixelSize: 11
                    text: (I18n.language, I18n.t("dsp.needsCompressor"))
                }
                CommandLine { Layout.fillWidth: true; pixelSize: 10; text: root.installCommand("compressor") }
                Text {
                    text: (I18n.language, I18n.t("dsp.recheck"))
                    color: root.accentColor
                    font.pixelSize: 11
                    font.underline: recheckMacompressor.containsMouse
                    MouseArea {
                        id: recheckMacompressor
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: { EqController.refreshCaps(); root.applyAll() }
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                opacity: root.compEnabled ? 1.0 : 0.45
                Text { text: (I18n.language, I18n.t("dsp.level")); color: Theme.textDim; font.pixelSize: 11 }
                HSlider {
                    Layout.fillWidth: true
                    from: 0; to: 100
                    value: root.compLevel
                    sliderColor: root.accentColor
                    enabled: root.compEnabled
                    onMoved: (v) => root.setComp(root.compEnabled, Math.round(v))
                }
            }
        }
    }
}
