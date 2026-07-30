import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Distro-aware install helper for gpu-screen-recorder. QML port of
// RecorderInstallHelper.vue. Reused inside the Capture & Sound settings panel.
ColumnLayout {
    id: root
    spacing: 8

    property bool compact: false

    property var deps: JSON.parse(SystemController.depsJson || "[]")
    property var distro: JSON.parse(SystemController.distroJson || "{}")
    readonly property bool isMissing: root.deps.some(d => d.feature === "recording" && !d.available)

    property bool copied: false
    property bool rechecking: false

    function distroFamily() {
        const s = ((root.distro.id || "") + " " + (root.distro.id_like || "")).toLowerCase()
        if (s.includes("arch")) return "arch"
        if (s.includes("debian") || s.includes("ubuntu")) return "debian"
        if (s.includes("fedora") || s.includes("rhel") || s.includes("centos")) return "fedora"
        return "unknown"
    }

    readonly property var gsrPkg: ({
        arch: "sudo pacman -S gpu-screen-recorder",
        debian: "flatpak install flathub com.dec05eba.gpu_screen_recorder",
        fedora: "flatpak install flathub com.dec05eba.gpu_screen_recorder",
    })

    function installCommand() {
        const family = root.distroFamily()
        if (family === "unknown") {
            return "# Try one of:\n" + root.gsrPkg.arch + "\n" + root.gsrPkg.debian + "\n" + root.gsrPkg.fedora
        }
        return root.gsrPkg[family]
    }

    // ── Installed confirmation ──
    RowLayout {
        visible: !root.isMissing
        spacing: 8
        // RecorderInstallHelper.vue uses an SVG check here, not a text glyph.
        Icon { name: "check"; size: 15; color: Theme.success }
        Text {
            text: (I18n.language, I18n.t("settings.recorderInstall.installed"))
            color: Theme.success
            font.pixelSize: 13
            font.weight: Font.DemiBold
        }
    }

    // ── Missing → distro-aware install command + Copy + Recheck ──
    Rectangle {
        visible: root.isMissing
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.tint(Theme.danger, 8)
        border.width: 1
        border.color: Theme.tint(Theme.danger, 28)
        implicitHeight: missCol.implicitHeight + (root.compact ? 20 : 24)

        ColumnLayout {
            id: missCol
            anchors.fill: parent
            anchors.margins: root.compact ? 10 : 12
            spacing: 8

            RowLayout {
                spacing: 8
                Icon { name: "alert-triangle"; size: 14; color: Theme.danger}
                Text {
                    text: (I18n.language, I18n.t("settings.recorderInstall.notFound"))
                    color: Theme.text
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Rectangle {
                    Layout.fillWidth: true
                    radius: Theme.radius
                    color: Theme.bg
                    border.width: 1
                    border.color: Theme.border
                    implicitHeight: cmdText.implicitHeight + 12
                    Text {
                        id: cmdText
                        anchors.fill: parent
                        anchors.margins: 6
                        text: root.installCommand()
                        color: Theme.text
                        font.pixelSize: 11
                        font.family: "monospace"
                        wrapMode: Text.WordWrap
                    }
                }
                Rectangle {
                    width: 60; height: 24
                    radius: Theme.radius
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.border
                    Text {
                        anchors.centerIn: parent
                        text: root.copied ? (I18n.language, I18n.t("settings.captureGsr.copied")) : (I18n.language, I18n.t("common.copy"))
                        color: Theme.textDim
                        font.pixelSize: 10
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            SystemController.writeClipboard(root.installCommand())
                            root.copied = true
                            copiedTimer.restart()
                        }
                    }
                }
            }
            Timer { id: copiedTimer; interval: 1500; onTriggered: root.copied = false }

            Text {
                text: (I18n.language, I18n.t("settings.installHint.gsrNote"))
                color: Theme.textDim
                font.pixelSize: 10
                font.italic: true
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Rectangle {
                Layout.alignment: Qt.AlignLeft
                width: recheckRow.implicitWidth + 20; height: 26
                radius: Theme.radius
                color: Theme.surface
                border.width: 1
                border.color: Theme.border
                opacity: root.rechecking ? 0.5 : 1.0
                RowLayout {
                    id: recheckRow
                    anchors.centerIn: parent
                    spacing: 6
                    Text {
                        text: root.rechecking ? (I18n.language, I18n.t("settings.recorderInstall.rechecking")) : (I18n.language, I18n.t("settings.recorderInstall.recheck"))
                        color: Theme.textDim
                        font.pixelSize: 11
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    enabled: !root.rechecking
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        root.rechecking = true
                        SystemController.refresh()
                        recheckTimer.restart()
                    }
                }
            }
            Timer { id: recheckTimer; interval: 400; onTriggered: root.rechecking = false }
        }
    }

    Component.onCompleted: SystemController.refresh()
}
