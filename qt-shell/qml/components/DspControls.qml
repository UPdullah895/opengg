import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Per-channel Noise Reduction / Noise Gate / Compressor controls. QML port
// of DspControls.vue. All three EqController.applyX() calls are genuine
// no-ops on the Rust side (matching the Tauri host's own apply_noise_gate/
// apply_compressor/apply_noise_reduction, which are also no-ops today —
// see the header comment in qt-shell/src/eq.rs) — this ships the same
// inert-but-present UI as the shipping app, not a stub ahead of it.
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
        EqController.applyNoiseReduction(root.channel, enabled, intensity)
    }
    function setGate(enabled, threshold, auto) {
        root.gateEnabled = enabled
        root.gateThreshold = threshold
        root.gateAuto = auto
        root.activePreset = "Custom"
        EqController.applyNoiseGate(root.channel, enabled, threshold, auto)
    }
    function setComp(enabled, level) {
        root.compEnabled = enabled
        root.compLevel = level
        root.activePreset = "Custom"
        EqController.applyCompressor(root.channel, enabled, level)
    }
    function applyPreset(name) {
        if (!(name in root.presets)) return
        const p = root.presets[name]
        root.nrEnabled = p.nr.enabled; root.nrIntensity = p.nr.intensity
        root.gateEnabled = p.gate.enabled; root.gateThreshold = p.gate.threshold; root.gateAuto = p.gate.auto
        root.compEnabled = p.comp.enabled; root.compLevel = p.comp.level
        root.activePreset = name
        EqController.applyNoiseReduction(root.channel, root.nrEnabled, root.nrIntensity)
        EqController.applyNoiseGate(root.channel, root.gateEnabled, root.gateThreshold, root.gateAuto)
        EqController.applyCompressor(root.channel, root.compEnabled, root.compLevel)
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
            text: root.channel
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
                ToggleSwitch { checked: root.gateEnabled; onToggled: (v) => root.setGate(v, root.gateThreshold, root.gateAuto) }
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
                        checked: root.gateAuto
                        enabled: root.gateEnabled
                        onToggled: root.setGate(root.gateEnabled, root.gateThreshold, checked)
                    }
                    Text { text: (I18n.language, I18n.t("dsp.autoDetect")); color: Theme.textDim; font.pixelSize: 11 }
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
