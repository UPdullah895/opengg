import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Microphone processing: noise suppression, noise gate, compressor.
// The chain runs in the daemon (daemon/src/audio/micfx.rs), built from the
// plugins this system has — RNNoise for suppression, swh-plugins for the
// gate and compressor. A card whose plugin is missing is shown disabled,
// and the notice says what to install. Settings are saved in
// ui-settings.json (`mixer.micfx`) and restored on load.
ColumnLayout {
    id: root
    spacing: 16

    property string channel: ""
    property color accentColor: Theme.accent

    property bool nrEnabled: false
    property real nrIntensity: 50
    property bool gateEnabled: false
    property real gateThreshold: -40
    property bool compEnabled: false
    property real compLevel: 50
    property string activePreset: "Default"
    readonly property var caps: JSON.parse(EqController.micFxCapsJson || "{}")
    readonly property bool nrLive: caps.nr === true
    readonly property bool gateLive: caps.gate === true
    readonly property bool compLive: caps.comp === true
    /// What to tell the user about stages this system cannot run.
    readonly property string missing: {
        I18n.language
        if (EqController.unavailableReason.length > 0)
            return I18n.t("dsp.unavailable") + " " + EqController.unavailableReason
        var parts = []
        if (!root.nrLive) parts.push(I18n.t("dsp.missingNr"))
        if (!root.gateLive || !root.compLive) parts.push(I18n.t("dsp.missingSwh"))
        return parts.join(" ")
    }

    readonly property var presets: ({
        "Default":   { nr: { enabled: false, intensity: 50 }, gate: { enabled: false, threshold: -40 }, comp: { enabled: false, level: 50 } },
        "Broadcast": { nr: { enabled: true, intensity: 50 }, gate: { enabled: false, threshold: -40 }, comp: { enabled: true, level: 70 } },
        "Podcaster": { nr: { enabled: true, intensity: 30 }, gate: { enabled: true, threshold: -30 }, comp: { enabled: true, level: 60 } },
        "Noise-Reduction Focus": { nr: { enabled: true, intensity: 80 }, gate: { enabled: true, threshold: -40 }, comp: { enabled: false, level: 50 } },
    })
    readonly property var presetNames: ["Default", "Broadcast", "Podcaster", "Noise-Reduction Focus"]

    function push() {
        EqController.setMicFx(JSON.stringify({
            nr: { enabled: root.nrEnabled, intensity: root.nrIntensity },
            gate: { enabled: root.gateEnabled, threshold: root.gateThreshold },
            comp: { enabled: root.compEnabled, level: root.compLevel },
        }))
    }
    function setNr(enabled, intensity) {
        root.nrEnabled = enabled
        root.nrIntensity = intensity
        root.activePreset = "Custom"
        root.push()
    }
    function setGate(enabled, threshold) {
        root.gateEnabled = enabled
        root.gateThreshold = threshold
        root.activePreset = "Custom"
        root.push()
    }
    function setComp(enabled, level) {
        root.compEnabled = enabled
        root.compLevel = level
        root.activePreset = "Custom"
        root.push()
    }
    function applyPreset(name) {
        if (!(name in root.presets)) return
        const p = root.presets[name]
        root.nrEnabled = p.nr.enabled; root.nrIntensity = p.nr.intensity
        root.gateEnabled = p.gate.enabled; root.gateThreshold = p.gate.threshold
        root.compEnabled = p.comp.enabled; root.compLevel = p.comp.level
        root.activePreset = name
        root.push()
    }
    /// The preset matching the current values, or "Custom".
    function presetFor() {
        for (const name of root.presetNames) {
            const p = root.presets[name]
            if (p.nr.enabled === root.nrEnabled && p.nr.intensity === root.nrIntensity
                    && p.gate.enabled === root.gateEnabled && p.gate.threshold === root.gateThreshold
                    && p.comp.enabled === root.compEnabled && p.comp.level === root.compLevel)
                return name
        }
        return "Custom"
    }

    Component.onCompleted: {
        const s = JSON.parse(EqController.savedMicFx() || "{}")
        if (s.nr) { root.nrEnabled = !!s.nr.enabled; root.nrIntensity = Math.round(s.nr.intensity ?? 50) }
        if (s.gate) { root.gateEnabled = !!s.gate.enabled; root.gateThreshold = Math.round(s.gate.threshold ?? -40) }
        if (s.comp) { root.compEnabled = !!s.comp.enabled; root.compLevel = Math.round(s.comp.level ?? 50) }
        root.activePreset = root.presetFor()
        EqController.refresh()
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
            // Only this label is translated, not root.channel itself.
            text: (I18n.language, I18n.t("mixer.channels." + root.channel.toLowerCase()))
            color: root.accentColor
            font.pixelSize: 13
            font.weight: Font.DemiBold
        }
        Item { Layout.fillWidth: true }

        ComboBox {
            id: presetCombo
            Layout.preferredWidth: 180
            enabled: root.nrLive || root.gateLive || root.compLive
            model: root.activePreset === "Custom" ? root.presetNames.concat(["Custom"]) : root.presetNames
            currentIndex: model.indexOf(root.activePreset)
            onActivated: root.applyPreset(model[currentIndex])

            contentItem: Text {
                text: presetCombo.displayText
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
                contentItem: Text { text: modelData; color: Theme.text; font.pixelSize: 12 }
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

    Rectangle {
        Layout.fillWidth: true
        visible: root.missing.length > 0
        radius: Theme.radius
        color: Theme.bg
        border.width: 1
        border.color: Theme.border
        implicitHeight: notRow.implicitHeight + 20
        RowLayout {
            id: notRow
            anchors.fill: parent
            anchors.margins: 10
            spacing: 8
            Icon { name: "info"; size: 14; color: Theme.textDim; Layout.alignment: Qt.AlignTop }
            Text {
                Layout.fillWidth: true
                text: root.missing
                color: Theme.text
                font.pixelSize: 12
                wrapMode: Text.WordWrap
            }
        }
    }

    // ── Noise Reduction ──
    Rectangle {
        Layout.fillWidth: true
        enabled: root.nrLive
        opacity: root.nrLive ? 1 : 0.45
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
                ToggleSwitch { checked: root.nrEnabled; onToggled: (v) => root.setNr(v, root.nrIntensity) }
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
        enabled: root.gateLive
        opacity: root.gateLive ? 1 : 0.45
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
                ToggleSwitch { checked: root.gateEnabled; onToggled: (v) => root.setGate(v, root.gateThreshold) }
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
                        from: -70; to: 0
                        value: root.gateThreshold
                        suffix: " dB"
                        sliderColor: root.accentColor
                        enabled: root.gateEnabled
                        onMoved: (v) => root.setGate(root.gateEnabled, Math.round(v))
                    }
                }
            }
        }
    }

    // ── Compressor ──
    Rectangle {
        Layout.fillWidth: true
        enabled: root.compLive
        opacity: root.compLive ? 1 : 0.45
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
                ToggleSwitch { checked: root.compEnabled; onToggled: (v) => root.setComp(v, root.compLevel) }
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
