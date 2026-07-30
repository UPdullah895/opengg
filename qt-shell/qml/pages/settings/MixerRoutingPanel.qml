import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings → Audio Engine (Mixer Routing). QML port of MixerRoutingSettings.vue.
ColumnLayout {
    id: root
    spacing: 20

    property bool confirmOpen: false
    property string confirmKind: "remove" // "remove" | "reset" | "create"

    // Ear Blast Protection config, mirrored from AudioController.earBlastJson
    // (mixer.earBlast in the settings envelope — see audio.rs's header note).
    property var eb: AudioController.earBlastJson
        ? JSON.parse(AudioController.earBlastJson)
        : { enabled: false, channels: ["Game"], threshold: 85, target: 60 }

    function setEb(key, value) {
        AudioController.setEarBlast(key, JSON.stringify(value))
    }
    function toggleEbChannel(ch) {
        const cur = root.eb.channels || []
        const next = cur.includes(ch) ? cur.filter(c => c !== ch) : [...cur, ch]
        root.setEb("channels", next)
    }

    readonly property var confirmText: ({
        remove: {
            title: I18n.t("settings.dangerZone.title"),
            msg: I18n.t("settings.dangerZone.confirmMsg"),
        },
        reset: {
            title: I18n.t("settings.dangerZone.resetVirtualAudio"),
            msg: I18n.t("settings.dangerZone.resetVirtualAudioDesc"),
        },
        create: {
            title: I18n.t("settings.dangerZone.createConfirmTitle"),
            msg: I18n.t("settings.dangerZone.createConfirmMsg"),
        },
    }[root.confirmKind])

    function openConfirm(kind) {
        root.confirmKind = kind
        root.confirmOpen = true
    }

    function runConfirmed() {
        root.confirmOpen = false
        if (root.confirmKind === "create") {
            AudioController.createVirtualAudio()
        } else {
            // "remove" and "reset" both unload the virtual sinks; recreation
            // (the "reset" half) happens via the onboarding wizard, same as
            // the Vue implementation.
            AudioController.removeVirtualAudio()
        }
    }

    Text {
        text: (I18n.language, I18n.t("settings.sections.mixerRouting"))
        color: Theme.text
        font.pixelSize: 22
        font.weight: Font.Bold
    }

    // ── Ear Blast Protection card ──
    Rectangle {
        id: ebCard
        Layout.fillWidth: true
        Layout.preferredWidth: 680
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: ebCol.implicitHeight + 40

        ColumnLayout {
            id: ebCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 14

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                // No icon: MixerRoutingSettings.vue's Ear Blast header is a bare
                // <span> + InfoIcon, so the 🛡️ glyph here was invented.
                Text {
                    text: (I18n.language, I18n.t("settings.earBlast.title"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                InfoIcon { tooltipText: I18n.t("settings.earBlast.desc") }
                Item { Layout.fillWidth: true }
                ToggleSwitch {
                    checked: !!root.eb.enabled
                    onToggled: (v) => root.setEb("enabled", v)
                }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            ColumnLayout {
                spacing: 6
                Text {
                    text: (I18n.language, I18n.t("settings.earBlast.channels"))
                    color: Theme.textDim
                    font.pixelSize: 12
                }
                RowLayout {
                    spacing: 6
                    Repeater {
                        model: ["Master", "Game", "Chat", "Media", "Aux", "Mic"]
                        Rectangle {
                            required property string modelData
                            readonly property bool active: (root.eb.channels || []).includes(modelData)
                            width: pillText.implicitWidth + 20
                            height: 26
                            radius: 13
                            color: active ? Theme.accent : "transparent"
                            border.width: 1
                            border.color: active ? Theme.accent : Theme.border
                            Text {
                                id: pillText
                                anchors.centerIn: parent
                                text: parent.modelData
                                color: parent.active ? "#fff" : Theme.textDim
                                font.pixelSize: 11
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.toggleEbChannel(parent.modelData)
                            }
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 24
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    Text {
                        text: (I18n.language, I18n.t("settings.earBlast.threshold"))
                        color: Theme.textDim
                        font.pixelSize: 12
                    }
                    HSlider {
                        Layout.fillWidth: true
                        from: 1; to: 100
                        suffix: "%"
                        value: root.eb.threshold !== undefined ? root.eb.threshold : 85
                        onMoved: root.setEb("threshold", Math.round(value))
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    Text {
                        text: (I18n.language, I18n.t("settings.earBlast.target"))
                        color: Theme.textDim
                        font.pixelSize: 12
                    }
                    HSlider {
                        Layout.fillWidth: true
                        from: 0; to: 100
                        suffix: "%"
                        value: root.eb.target !== undefined ? root.eb.target : 60
                        onMoved: root.setEb("target", Math.round(value))
                    }
                }
            }
        }
    }

    // ── Danger Zone card ──
    Rectangle {
        id: dzCard
        Layout.fillWidth: true
        Layout.preferredWidth: 680
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: "#ef4444"
        implicitHeight: dzCol.implicitHeight + 40

        ColumnLayout {
            id: dzCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 14

            RowLayout {
                spacing: 8
                Icon { name: "alert-triangle"; size: 14}
                Text {
                    text: (I18n.language, I18n.t("settings.dangerZone.title"))
                    color: "#ef4444"
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
            }
            Text {
                text: (I18n.language, I18n.t("settings.dangerZone.subtitle"))
                color: Theme.textDim
                font.pixelSize: 12
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            // Reset
            RowLayout {
                Layout.fillWidth: true
                ColumnLayout {
                    spacing: 2
                    Layout.fillWidth: true
                    RowLayout {
                        spacing: 6
                        Text {
                            text: (I18n.language, I18n.t("settings.dangerZone.resetVirtualAudio"))
                            color: Theme.text
                            font.pixelSize: 13
                        }
                        InfoIcon { tooltipText: I18n.t("settings.dangerZone.resetVirtualAudioDesc") }
                    }
                }
                Rectangle {
                    width: 32; height: 32; radius: Theme.radius
                    color: "transparent"
                    border.width: 1
                    border.color: Theme.border
                    Icon { anchors.centerIn: parent; name: "refresh-cw"; size: 15; color: Theme.text}
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.openConfirm("reset")
                    }
                }
            }

            // Create (when not ready) / Remove (when ready)
            RowLayout {
                Layout.fillWidth: true
                ColumnLayout {
                    spacing: 2
                    Layout.fillWidth: true
                    RowLayout {
                        spacing: 6
                        Text {
                            text: AudioController.virtualAudioReady
                                ? (I18n.language, I18n.t("settings.dangerZone.removeVirtualAudio"))
                                : (I18n.language, I18n.t("settings.dangerZone.createVirtualAudio"))
                            color: Theme.text
                            font.pixelSize: 13
                        }
                        InfoIcon {
                            tooltipText: AudioController.virtualAudioReady
                                ? I18n.t("settings.dangerZone.removeVirtualAudioDesc")
                                : I18n.t("settings.dangerZone.createVirtualAudioDesc")
                        }
                    }
                }
                Rectangle {
                    visible: !AudioController.virtualAudioReady
                    width: 120; height: 32; radius: Theme.radius
                    color: Theme.accent
                    Text {
                        anchors.centerIn: parent
                        text: AudioController.checkingVirtualAudio
                            ? I18n.t("settings.dangerZone.creating")
                            : "Create"
                        color: "#fff"
                        font.pixelSize: 12
                        font.weight: Font.DemiBold
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        enabled: !AudioController.checkingVirtualAudio
                        onClicked: root.openConfirm("create")
                    }
                }
                Rectangle {
                    visible: AudioController.virtualAudioReady
                    width: 32; height: 32; radius: Theme.radius
                    color: "transparent"
                    border.width: 1
                    border.color: "#ef4444"
                    Icon { anchors.centerIn: parent; name: "trash"; size: 13}
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.openConfirm("remove")
                    }
                }
            }
        }

        // ── Inline confirm overlay, scoped to this card (deliberately not a
        // separate Popup-rooted component — see RecorderInstallHelper.qml's
        // header note on the IconPicker Popup-hang landmine). `dzCard` is a
        // plain Rectangle, not a Layout, so a normal anchors.fill overlay
        // child is safe here (Qt Quick Layouts warn/ignore anchors on their
        // own direct children, which is why this lives inside the Rectangle
        // rather than as a ColumnLayout sibling). ──
        Rectangle {
            anchors.fill: parent
            visible: root.confirmOpen
            z: 1000
            radius: parent.radius
            color: Qt.rgba(0, 0, 0, 0.65)

            MouseArea {
                anchors.fill: parent
                onClicked: root.confirmOpen = false
            }

            Rectangle {
                anchors.centerIn: parent
                width: Math.min(340, parent.width - 32)
                radius: Theme.radius
                color: Theme.surface
                border.width: 1
                border.color: Theme.border
                implicitHeight: confirmCol.implicitHeight + 32

                MouseArea { anchors.fill: parent } // swallow clicks so they don't hit the scrim

                ColumnLayout {
                    id: confirmCol
                    anchors.fill: parent
                    anchors.margins: 16
                    spacing: 14

                    Text {
                        text: root.confirmText ? root.confirmText.title : ""
                        color: Theme.text
                        font.pixelSize: 15
                        font.weight: Font.DemiBold
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                    Text {
                        text: root.confirmText ? root.confirmText.msg : ""
                        color: Theme.textDim
                        font.pixelSize: 12
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                    RowLayout {
                        Layout.alignment: Qt.AlignRight
                        spacing: 10
                        Rectangle {
                            width: 80; height: 30; radius: Theme.radius
                            color: "transparent"
                            border.width: 1
                            border.color: Theme.border
                            Text { anchors.centerIn: parent; text: I18n.t("common.cancel"); color: Theme.textDim; font.pixelSize: 12 }
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.confirmOpen = false }
                        }
                        Rectangle {
                            width: 100; height: 30; radius: Theme.radius
                            color: root.confirmKind === "create" ? Theme.accent : "#ef4444"
                            Text { anchors.centerIn: parent; text: I18n.t("common.confirmDelete"); color: "#fff"; font.pixelSize: 12; font.weight: Font.DemiBold }
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.runConfirmed() }
                        }
                    }
                }
            }
        }
    }

    Component.onCompleted: {
        AudioController.refreshVirtualAudioStatus()
        AudioController.refreshEarBlast()
    }
}
