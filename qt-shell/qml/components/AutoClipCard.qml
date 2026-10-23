import QtQuick
import QtQuick.Layouts
import com.opengg.app

// Settings → Capture & Sound → Auto-clip. The daemon listens for
// Counter-Strike 2's Game State Integration and signals each highlight;
// GlobalHotkeys saves it through the replay buffer and names the clip after
// it. This card holds the switch, which events count (`settings.autoClip`,
// which the daemon re-reads), and the one-time CS2 setup.
SettingsCard {
    id: card

    /// `settings` from ui-settings.json, as the panel already parses it.
    property var s: ({})
    readonly property var ac: card.s.autoClip || ({})
    readonly property bool on: !!card.ac.enabled
    readonly property var events: card.ac.events || ["multikill", "ace", "mvp"]

    property var st: JSON.parse(AutoClipController.statusJson || "{}")
    // CS2 sends a heartbeat every 30 s while running.
    readonly property bool connected: !!card.st.lastPayloadAt && (card.st.now - card.st.lastPayloadAt) < 75

    readonly property var eventOptions: [
        { key: "ace", label: "autoClip.events.ace" },
        { key: "multikill", label: "autoClip.events.multikill" },
        { key: "mvp", label: "autoClip.events.mvp" },
        { key: "headshot", label: "autoClip.events.headshot" },
        { key: "kill", label: "autoClip.events.kill" },
        { key: "death", label: "autoClip.events.death" },
    ]

    function toggleEvent(key) {
        const list = card.events.slice()
        const i = list.indexOf(key)
        if (i >= 0) list.splice(i, 1)
        else list.push(key)
        SettingsController.setValue("autoClip.events", JSON.stringify(list))
    }

    title: (I18n.language, I18n.t("autoClip.title"))
    infoText: (I18n.language, I18n.t("autoClip.info"))

    Connections {
        target: AutoClipController
        function onStatusJsonChanged() { card.st = JSON.parse(AutoClipController.statusJson || "{}") }
    }
    // Keeps "connected" honest while the page is open.
    Timer {
        interval: 5000
        repeat: true
        running: card.visible && !ScreenshotController.active
        onTriggered: AutoClipController.refresh()
    }
    Component.onCompleted: AutoClipController.refresh()

    RowLayout {
        Layout.fillWidth: true
        Text {
            text: (I18n.language, I18n.t("autoClip.enable"))
            color: Theme.textDim
            font.pixelSize: 12
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignLeft
        }
        ToggleSwitch {
            checked: card.on
            onToggled: (v) => SettingsController.setValue("autoClip.enabled", JSON.stringify(v))
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 8
        opacity: card.on ? 1 : 0.5
        Text {
            text: (I18n.language, I18n.t("autoClip.eventsLabel"))
            color: Theme.textDim
            font.pixelSize: 12
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignLeft
        }
        Flow {
            Layout.fillWidth: true
            spacing: 6
            Repeater {
                model: card.eventOptions
                ChoiceChip {
                    required property var modelData
                    text: (I18n.language, I18n.t(modelData.label))
                    selected: card.events.indexOf(modelData.key) >= 0
                    enabled: card.on
                    onTriggered: card.toggleEvent(modelData.key)
                }
            }
        }
    }

    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

    // ── Counter-Strike 2 setup ──
    RowLayout {
        Layout.fillWidth: true
        spacing: 10

        Icon {
            name: card.connected ? "check" : (card.st.installed ? "info" : "gamepad")
            size: 16
            color: card.connected ? Theme.success : Theme.textDim
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            Text {
                text: "Counter-Strike 2"
                color: Theme.text
                font.pixelSize: 13
                font.weight: Font.DemiBold
            }
            Text {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignLeft
                wrapMode: Text.WordWrap
                font.pixelSize: 11
                color: card.connected ? Theme.success : Theme.textDim
                text: {
                    I18n.language
                    if (card.st.gameFound === undefined) return ""
                    if (!card.st.gameFound) return I18n.t("autoClip.cs2.notFound")
                    if (!card.st.installed) return I18n.t("autoClip.cs2.notSetUp")
                    if (card.connected) return I18n.t("autoClip.cs2.connected")
                    return I18n.t("autoClip.cs2.waiting")
                }
            }
        }
        Rectangle {
            visible: !!card.st.gameFound
            implicitWidth: btnLabel.implicitWidth + 24
            implicitHeight: 30
            radius: Theme.radius
            color: card.st.installed ? Theme.bg : Theme.accent
            border.width: 1
            border.color: card.st.installed ? Theme.border : Theme.accent
            opacity: AutoClipController.busy ? 0.5 : 1
            Text {
                id: btnLabel
                anchors.centerIn: parent
                text: (I18n.language, I18n.t(card.st.installed ? "autoClip.cs2.remove" : "autoClip.cs2.setUp"))
                color: card.st.installed ? Theme.textDim : "#ffffff"
                font.pixelSize: 12
            }
            MouseArea {
                anchors.fill: parent
                enabled: !AutoClipController.busy
                cursorShape: Qt.PointingHandCursor
                onClicked: card.st.installed ? AutoClipController.uninstallCs2() : AutoClipController.installCs2()
            }
        }
    }

    // Daemon-side trouble (port taken, wrong token, no daemon) or a failed
    // install — the reason, verbatim.
    Text {
        readonly property string msg: AutoClipController.error || card.st.error || ""
        visible: msg.length > 0
        text: msg
        color: Theme.overdrive
        font.pixelSize: 11
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
        horizontalAlignment: Text.AlignLeft
    }

    Text {
        text: (I18n.language, I18n.t("autoClip.needsBuffer"))
        color: Theme.textDim
        font.pixelSize: 11
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
        horizontalAlignment: Text.AlignLeft
    }
}
