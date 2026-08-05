import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Per-channel 10-band graphic EQ. QML port of GraphicEQ.vue, simplified:
// the Vue version renders an interactive draggable SVG curve through the
// 10 band points; that depends on multi-point cubic-bezier <path> data and
// drag-to-SVG-coordinate math. QML's Shape/PathSvg has already proven
// fragile in this app for far simpler paths (see the IconPicker glyph
// investigation — abandoned Shape/PathSvg for a plain Rectangle+Text after
// it rendered distorted). A bank of discrete vertical sliders is the
// "boring primitive" substitute: same underlying 10-value band-gain model,
// same live jalv apply-on-change behavior, just discrete controls instead
// of a continuous curve.
//
// State here is deliberately NOT persisted anywhere (no ui-settings.json
// key) — frontend/src/stores/dsp.ts isn't persisted either (plain in-memory
// Pinia store), so both apps reset EQ state to defaults on restart alike.
// Each channel gets its own GraphicEQ instance kept alive for the whole
// session (MixerPage.qml uses a StackLayout, not a Loader, for the tab
// panels) so enabling EQ on one channel and switching tabs doesn't tear
// down its jalv engine or lose its band values.
ColumnLayout {
    id: root
    spacing: 16

    property string channel: ""
    property color accentColor: Theme.accent

    readonly property var bandLabels: ["32", "64", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"]
    property var bands: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    property real preamp: 0
    property bool eqEnabled: false   // NOT `enabled`: that shadows Item.enabled
    property string activePreset: "Default"

    readonly property var presets: ({
        "Default": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "Gaming":  [3, 2, 0, 0, 0, 0, 2, 2, 1, 1],
        "Movies":  [2, 2, 1, 0, 1, 2, 2, 1, 0, 0],
        "Music":   [3, 2, 0, 0, -1, 0, 1, 2, 2, 1],
    })
    readonly property var presetNames: ["Default", "Gaming", "Movies", "Music"]

    function applyToEngine() {
        if (root.eqEnabled) EqController.applyEq(root.channel, JSON.stringify(root.bands))
    }
    function setEnabled(v) {
        root.eqEnabled = v
        if (v) {
            EqController.startEngine(root.channel)
            root.applyToEngine()
        } else {
            EqController.stopEngine(root.channel)
        }
    }
    function setBand(i, val) {
        var arr = root.bands.slice()
        arr[i] = val
        root.bands = arr
        root.activePreset = "Custom"
        root.applyToEngine()
    }
    function setPreamp(val) {
        // Matches upstream: preamp is not sent to the jalv engine (the Vue
        // store's setPreamp() never includes it in the apply_eq payload
        // either) — purely a UI-level value for now.
        root.preamp = val
        root.activePreset = "Custom"
    }
    function resetEq() {
        root.bands = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        root.preamp = 0
        root.activePreset = "Default"
        root.applyToEngine()
    }
    function applyPreset(name) {
        if (!(name in root.presets)) return
        root.bands = root.presets[name].slice()
        root.preamp = 0
        root.activePreset = name
        root.applyToEngine()
    }

    // Whole EQ (header + bands + preamp) in one bordered card — matching
    // DspControls' Noise Reduction/Gate/Compressor cards directly below on
    // the same tab, which previously made this the only unbordered block
    // floating on the bare page background next to fully-carded siblings.
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: root.eqEnabled ? root.accentColor : Theme.border
        implicitHeight: eqCardCol.implicitHeight + 28

        ColumnLayout {
            id: eqCardCol
            anchors.fill: parent
            anchors.margins: 14
            spacing: 14

            RowLayout {
                Layout.fillWidth: true
                spacing: 10

                Text {
                    text: (I18n.language, I18n.t("eq.title"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                Text {
                    // root.channel is the identifier passed to EqController
                    // (applyEq/startEngine/stopEngine) — only this label
                    // is translated, not the property itself.
                    text: (I18n.language, I18n.t("mixer.channels." + root.channel.toLowerCase()))
                    color: root.accentColor
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                }

                Item { Layout.fillWidth: true }

                // Preset dropdown (custom-styled ComboBox — established pattern)
                ComboBox {
                    id: presetCombo
                    Layout.preferredWidth: 150
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

                // A bare toggle with no label reads ambiguous ("enabled for
                // what?") — every other enable/disable toggle in this app
                // (DspControls' three cards, Settings' Daemon & Startup)
                // pairs the switch with adjacent text.
                Text {
                    text: (I18n.language, I18n.t("eq.enabled"))
                    color: Theme.textDim
                    font.pixelSize: 12
                }
                ToggleSwitch {
                    checked: root.eqEnabled
                    onToggled: (v) => root.setEnabled(v)
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 10
                opacity: root.eqEnabled ? 1.0 : 0.45

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    Repeater {
                        model: 10
                        ColumnLayout {
                            id: bandCol
                            required property int index
                            Layout.fillWidth: true
                            spacing: 4

                            Text {
                                text: root.bands[bandCol.index] > 0 ? "+" + root.bands[bandCol.index] : String(root.bands[bandCol.index])
                                color: Theme.text
                                font.pixelSize: 10
                                font.weight: Font.DemiBold
                                Layout.alignment: Qt.AlignHCenter
                            }
                            Slider {
                                id: bandSlider
                                orientation: Qt.Vertical
                                Layout.alignment: Qt.AlignHCenter
                                Layout.preferredHeight: 110
                                from: -12; to: 12
                                value: root.bands[bandCol.index]
                                enabled: root.eqEnabled
                                onMoved: root.setBand(bandCol.index, Math.round(value))

                                background: Rectangle {
                                    x: bandSlider.leftPadding + bandSlider.availableWidth / 2 - width / 2
                                    y: bandSlider.topPadding
                                    width: 5
                                    height: bandSlider.availableHeight
                                    radius: 2.5
                                    color: Theme.border
                                    Rectangle {
                                        // 0dB is the midpoint; fill from center toward the handle.
                                        // visualPosition 0 = top = max value (+12), matching the
                                        // convention already established by the unipolar fader above.
                                        width: parent.width
                                        radius: 2.5
                                        color: root.accentColor
                                        property real curY: bandSlider.visualPosition * parent.height
                                        property real midY: parent.height / 2
                                        y: Math.min(curY, midY)
                                        height: Math.abs(curY - midY)
                                    }
                                }
                                handle: Rectangle {
                                    x: bandSlider.leftPadding + bandSlider.availableWidth / 2 - width / 2
                                    y: bandSlider.topPadding + bandSlider.visualPosition * (bandSlider.availableHeight - height)
                                    width: 16; height: 16; radius: 8
                                    color: Theme.text
                                    border.width: 2
                                    border.color: root.accentColor
                                }
                            }
                            Text {
                                text: root.bandLabels[bandCol.index]
                                color: Theme.textDim
                                font.pixelSize: 9
                                Layout.alignment: Qt.AlignHCenter
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: 6
                    spacing: 12

                    Text { text: (I18n.language, I18n.t("eq.preamp")); color: Theme.textDim; font.pixelSize: 12 }
                    HSlider {
                        Layout.preferredWidth: 200
                        from: -12; to: 12
                        value: root.preamp
                        suffix: " dB"
                        sliderColor: root.accentColor
                        enabled: root.eqEnabled
                        onMoved: (v) => root.setPreamp(Math.round(v))
                    }

                    Item { Layout.fillWidth: true }

                    Rectangle {
                        width: 70; height: 26; radius: Theme.radius
                        color: flatArea.containsMouse ? Theme.bgHover : "transparent"
                        border.width: 1
                        border.color: Theme.border
                        Text { anchors.centerIn: parent; text: I18n.t("eq.flat"); color: Theme.textDim; font.pixelSize: 11 }
                        MouseArea {
                            id: flatArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.resetEq()
                        }
                    }
                }
            }
        }
    }
}
