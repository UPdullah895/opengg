import QtQuick
import QtQuick.Layouts
import com.opengg.app

// Smart Volume for one output channel: an RMS compressor in the channel's
// FX chain (daemon/src/audio/eq.rs), ahead of the Personal fader, that
// pulls loud and quiet content toward one level. Off / Light / Medium /
// Strong; the daemon answers each change, so the chips show what is really
// in effect rather than what was clicked.
Rectangle {
    id: root

    property string channel: ""
    property color accentColor: Theme.accent

    readonly property var levels: ["off", "light", "medium", "strong"]
    readonly property string level: {
        const m = JSON.parse(EqController.smartJson || "{}")
        return m[root.channel] || "off"
    }
    readonly property string unavailable: EqController.smartUnavailableReason
    property bool busy: false

    Layout.fillWidth: true
    radius: Theme.radius
    color: Theme.surface
    border.width: 1
    border.color: root.level !== "off" ? root.accentColor : Theme.border
    implicitHeight: col.implicitHeight + 28

    ColumnLayout {
        id: col
        anchors.fill: parent
        anchors.margins: 14
        spacing: 12

        // Why it can't run here (no swh-plugins, no daemon) — shown only
        // while it is not running, so a stale reason never sits under a
        // working control.
        Rectangle {
            Layout.fillWidth: true
            visible: root.unavailable.length > 0 && root.level === "off"
            implicitHeight: msg.implicitHeight + 16
            radius: Theme.radius
            color: Theme.tint(Theme.overdrive, 12)
            border.width: 1
            border.color: Theme.tint(Theme.overdrive, 40)
            RowLayout {
                anchors.fill: parent
                anchors.margins: 8
                spacing: 8
                Icon { name: "alert-triangle"; size: 13; color: Theme.overdrive }
                Text {
                    id: msg
                    Layout.fillWidth: true
                    text: (I18n.language, I18n.t("smartVolume.unavailable")) + " " + root.unavailable
                    color: Theme.overdrive
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            Icon { name: "sliders"; size: 15; color: root.level !== "off" ? root.accentColor : Theme.textDim }
            Text {
                text: (I18n.language, I18n.t("smartVolume.title"))
                color: Theme.text
                font.pixelSize: 16
                font.weight: Font.DemiBold
            }
            Text {
                text: (I18n.language, I18n.t("mixer.channels." + root.channel.toLowerCase()))
                color: root.accentColor
                font.pixelSize: 13
                font.weight: Font.DemiBold
            }
            Item { Layout.fillWidth: true }
            Row {
                spacing: 6
                opacity: root.busy ? 0.5 : 1
                Repeater {
                    model: root.levels
                    ChoiceChip {
                        required property string modelData
                        text: (I18n.language, I18n.t("smartVolume." + modelData)).toUpperCase()
                        selected: root.level === modelData
                        enabled: !root.busy && (modelData === "off" || root.unavailable.length === 0)
                        onTriggered: {
                            if (modelData === root.level)
                                return
                            root.busy = true
                            EqController.setSmartVolume(root.channel, modelData)
                            root.busy = false
                        }
                    }
                }
            }
        }

        Text {
            Layout.fillWidth: true
            text: (I18n.language, I18n.t("smartVolume.hint"))
            color: Theme.textDim
            font.pixelSize: 12
            wrapMode: Text.WordWrap
        }
    }
}
