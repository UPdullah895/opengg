import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings → About. QML port of AboutSettings.vue: hero card, goals,
// connect links, dependency probe, device-access probe, credits.
ColumnLayout {
    id: root
    width: parent.width
    spacing: 20

    property var deps: JSON.parse(SystemController.depsJson || "[]")
    property var distro: JSON.parse(SystemController.distroJson || "{}")
    property var access: JSON.parse(SystemController.accessJson || "{}")

    property bool showAllDeps: false
    property string expandedDep: ""
    property bool showAllAccess: false
    property string expandedAccessItem: ""

    readonly property var accessItems: [
        { id: "ratbagd", label: "ratbagd", status: !!root.access.ratbagd_available },
        { id: "inputGroup", label: "input group", status: !!root.access.in_input_group },
        { id: "audioGroup", label: "audio group", status: !!root.access.in_audio_group },
        { id: "videoGroup", label: "video group", status: !!root.access.in_video_group },
        { id: "udevRules", label: "udev rules", status: !!root.access.udev_rules_present },
    ]

    readonly property var packageMaps: ({
        "gpu-screen-recorder": { arch: "pacman -S gpu-screen-recorder", debian: "flatpak install flathub com.dec05eba.gpu_screen_recorder", fedora: "flatpak install flathub com.dec05eba.gpu_screen_recorder", note: "installHint.gsrNote" },
        "ffmpeg": { arch: "pacman -S ffmpeg", debian: "apt install ffmpeg", fedora: "dnf install ffmpeg", note: "installHint.ffmpegNote" },
        "ffprobe": { arch: "pacman -S ffmpeg", debian: "apt install ffmpeg", fedora: "dnf install ffmpeg", note: "installHint.ffprobeNote" },
        "pactl": { arch: "pacman -S libpulse", debian: "apt install pulseaudio-utils", fedora: "dnf install pulseaudio-utils" },
        "pw-link": { arch: "pacman -S pipewire", debian: "apt install pipewire", fedora: "dnf install pipewire" },
        "jalv": { arch: "pacman -S jalv", debian: "apt install jalv", fedora: "dnf install jalv" },
        "headsetcontrol": { arch: "pacman -S headsetcontrol", debian: "apt install headsetcontrol", fedora: "dnf install headsetcontrol" },
        "xdotool": { arch: "pacman -S xdotool", debian: "apt install xdotool", fedora: "dnf install xdotool" },
    })

    readonly property var accessFixMaps: ({
        ratbagd: { commands: ["sudo pacman -S libratbag / apt install ratbagd / dnf install libratbag-ratbagd", "sudo systemctl enable --now ratbagd"], note: "accessFixHint.ratbagdNote" },
        inputGroup: { commands: ["sudo usermod -aG input $USER"], note: "accessFixHint.reloginNote" },
        audioGroup: { commands: ["sudo usermod -aG audio $USER"], note: "accessFixHint.reloginNote" },
        videoGroup: { commands: ["sudo usermod -aG video $USER"], note: "accessFixHint.reloginNote" },
        udevRules: { commands: ["./dev.sh setup  # From the OpenGG repository root"], note: "accessFixHint.udevNote" },
    })

    function distroFamily() {
        const s = ((root.distro.id || "") + " " + (root.distro.id_like || "")).toLowerCase()
        if (s.includes("arch")) return "arch"
        if (s.includes("debian") || s.includes("ubuntu")) return "debian"
        if (s.includes("fedora") || s.includes("rhel") || s.includes("centos")) return "fedora"
        return "unknown"
    }

    function installCommand(binary) {
        const pkg = root.packageMaps[binary]
        if (!pkg) return { command: "", note: "" }
        const family = root.distroFamily()
        if (family === "unknown") {
            return { command: "# Try one of:\n" + pkg.arch + "\n" + pkg.debian + "\n" + pkg.fedora, note: pkg.note || "" }
        }
        return { command: pkg[family], note: pkg.note || "" }
    }

    function missingDeps() {
        return root.deps.filter(d => !d.available)
    }

    function missingAccessItems() {
        return root.accessItems.filter(i => !i.status)
    }

    SettingsHeading { titleText: (I18n.language, I18n.t("settings.sections.about")) }

    // ── Hero card ──
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: heroCol.implicitHeight + 48

        ColumnLayout {
            id: heroCol
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: parent.top
            anchors.topMargin: 24
            width: parent.width - 48
            spacing: 6

            Rectangle {
                Layout.alignment: Qt.AlignHCenter
                width: 48; height: 48
                radius: 24
                color: Theme.accentAlpha(12)
                border.width: 2
                border.color: Theme.accent
                // The OpenGG mark, not the generic gamepad glyph that stood
                // in for it — this is the one place the product's own logo
                // belongs.
                Icon {
                    anchors.centerIn: parent
                    name: "logo"; size: 30
                    color: Theme.accent
                }
            }
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "OpenGG"
                color: Theme.text
                font.pixelSize: 20
                font.weight: Font.Bold
            }
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "v" + SystemController.appVersion()
                color: Theme.accent
                font.pixelSize: 12
                font.weight: Font.DemiBold
            }
            Text {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 8
                text: (I18n.language, I18n.t("settings.about.tagline"))
                color: Theme.text
                font.pixelSize: 13
                horizontalAlignment: Text.AlignHCenter
            }
            Text {
                Layout.alignment: Qt.AlignHCenter
                Layout.fillWidth: true
                text: (I18n.language, I18n.t("settings.about.description"))
                color: Theme.textDim
                font.pixelSize: 12
                wrapMode: Text.WordWrap
                horizontalAlignment: Text.AlignHCenter
            }
        }
    }

    // ── Goals card ──
    SettingsCard {
        title: (I18n.language, I18n.t("settings.about.goals"))

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 8

            Repeater {
                model: [
                    I18n.t("settings.about.goal1"),
                    I18n.t("settings.about.goal2"),
                    I18n.t("settings.about.goal3"),
                ]
                RowLayout {
                    id: goalRow
                    required property string modelData
                    Layout.fillWidth: true
                    spacing: 8
                    Text { text: "•"; color: Theme.accent; font.pixelSize: 13 }
                    Text {
                        text: goalRow.modelData
                        color: Theme.textDim
                        font.pixelSize: 12
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }

    // ── Connect card ──
    SettingsCard {
        title: (I18n.language, I18n.t("settings.about.connect"))

        RowLayout {
                spacing: 10
                Rectangle {
                    width: 90; height: 30
                    radius: Theme.radius
                    color: Theme.bg
                    border.width: 1
                    border.color: Theme.border
                    Text { anchors.centerIn: parent; text: "GitHub"; color: Theme.text; font.pixelSize: 12 }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: Qt.openUrlExternally("https://github.com/UPdullah895/opengg")
                    }
                }
                Rectangle {
                    width: 130; height: 30
                    radius: Theme.radius
                    color: Theme.bg
                    opacity: 0.5
                    border.width: 1
                    border.color: Theme.border
                    RowLayout {
                        anchors.centerIn: parent
                        spacing: 6
                        Text { text: "Discord"; color: Theme.textDim; font.pixelSize: 12 }
                        Rectangle {
                            radius: 8
                            color: Theme.border
                            implicitWidth: soonText.implicitWidth + 10
                            implicitHeight: 16
                            Text {
                                id: soonText
                                anchors.centerIn: parent
                                text: (I18n.language, I18n.t("settings.about.soon"))
                                color: Theme.textDim
                                font.pixelSize: 9
                            }
                        }
                    }
                }
            }
    }

    // ── System Dependencies card ──
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: depsCol.implicitHeight + 40

        ColumnLayout {
            id: depsCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 10

            RowLayout {
                Layout.fillWidth: true
                Text {
                    horizontalAlignment: Text.AlignLeft
                    text: (I18n.language, I18n.t("settings.deps.title"))
                    color: Theme.text
                    font.pixelSize: 14
                    font.weight: Font.Bold
                    Layout.fillWidth: true
                }
                Text {
                    visible: root.deps.length > 0 && root.missingDeps().length > 0
                    text: (I18n.language, root.showAllDeps ? I18n.t("settings.deps.hideAll") : I18n.t("settings.deps.showAll"))
                    color: Theme.accent
                    font.pixelSize: 11
                    MouseArea {
                        anchors.fill: parent
                        anchors.margins: -6
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.showAllDeps = !root.showAllDeps
                    }
                }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            Text {
                visible: root.deps.length === 0
                text: (I18n.language, I18n.t("settings.deps.loading"))
                color: Theme.textDim
                font.pixelSize: 12
            }

            ColumnLayout {
                visible: root.deps.length > 0 && root.missingDeps().length === 0
                spacing: 6
                RowLayout {
                    spacing: 10
                    Rectangle { width: 20; height: 20; radius: 10; color: Theme.accent; Text { anchors.centerIn: parent; text: "✓"; color: Theme.bg; font.pixelSize: 11; font.weight: Font.Bold } }
                    Text { text: (I18n.language, I18n.t("settings.deps.allSatisfied")); color: Theme.text; font.pixelSize: 12 }
                }
            }

            ColumnLayout {
                visible: root.deps.length > 0 && root.missingDeps().length > 0
                spacing: 6
                Repeater {
                    model: root.showAllDeps ? root.deps : root.missingDeps()
                    ColumnLayout {
                        id: depRow
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 4

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 10
                            Rectangle {
                                width: 20; height: 20; radius: 10
                                color: depRow.modelData.available ? Theme.tint(Theme.success, 15) : Theme.tint(Theme.danger, 15)
                                Text {
                                    anchors.centerIn: parent
                                    text: depRow.modelData.available ? "✓" : "✗"
                                    color: depRow.modelData.available ? Theme.success : Theme.danger
                                    font.pixelSize: 11
                                    font.weight: Font.Bold
                                }
                            }
                            ColumnLayout {
                                spacing: 1
                                Layout.fillWidth: true
                                Text { text: depRow.modelData.binary; color: Theme.text; font.pixelSize: 11; font.family: "monospace" }
                                Text { text: (I18n.language, I18n.t("settings.deps.feature." + depRow.modelData.feature)); color: Theme.textDim; font.pixelSize: 11 }
                            }
                            Rectangle {
                                visible: !depRow.modelData.available
                                width: 22; height: 22; radius: 11
                                color: Theme.tint(Theme.danger, 15)
                                border.width: 1
                                border.color: Theme.tint(Theme.danger, 30)
                                Text { anchors.centerIn: parent; text: "?"; color: Theme.danger; font.pixelSize: 11; font.weight: Font.Bold }
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.expandedDep = root.expandedDep === depRow.modelData.binary ? "" : depRow.modelData.binary
                                }
                            }
                        }

                        ColumnLayout {
                            visible: !depRow.modelData.available && root.expandedDep === depRow.modelData.binary
                            Layout.fillWidth: true
                            Layout.leftMargin: 30
                            spacing: 4
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 6
                                Text {
                                    text: root.installCommand(depRow.modelData.binary).command
                                    color: Theme.text
                                    font.pixelSize: 10
                                    font.family: "monospace"
                                    wrapMode: Text.WordWrap
                                    Layout.fillWidth: true
                                }
                                Rectangle {
                                    width: 50; height: 22
                                    radius: Theme.radius
                                    color: Theme.bg
                                    border.width: 1
                                    border.color: Theme.border
                                    Text { anchors.centerIn: parent; text: (I18n.language, I18n.t("common.copy")); color: Theme.accent; font.pixelSize: 10 }
                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: SystemController.writeClipboard(root.installCommand(depRow.modelData.binary).command)
                                    }
                                }
                            }
                            Text {
                                horizontalAlignment: Text.AlignLeft
                                visible: !!root.installCommand(depRow.modelData.binary).note
                                text: (I18n.language, I18n.t(root.installCommand(depRow.modelData.binary).note))
                                color: Theme.textDim
                                font.pixelSize: 10
                                font.italic: true
                                wrapMode: Text.WordWrap
                                Layout.fillWidth: true
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Device Access card ──
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: accessCol.implicitHeight + 40

        ColumnLayout {
            id: accessCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 10

            RowLayout {
                Layout.fillWidth: true
                Text {
                    horizontalAlignment: Text.AlignLeft
                    text: (I18n.language, I18n.t("settings.deviceAccess.title"))
                    color: Theme.text
                    font.pixelSize: 14
                    font.weight: Font.Bold
                    Layout.fillWidth: true
                }
                Text {
                    visible: root.missingAccessItems().length > 0
                    text: (I18n.language, root.showAllAccess ? I18n.t("settings.deviceAccess.hideAll") : I18n.t("settings.deviceAccess.showAll"))
                    color: Theme.accent
                    font.pixelSize: 11
                    MouseArea {
                        anchors.fill: parent
                        anchors.margins: -6
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.showAllAccess = !root.showAllAccess
                    }
                }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            ColumnLayout {
                visible: root.missingAccessItems().length === 0
                spacing: 6
                RowLayout {
                    spacing: 10
                    Rectangle { width: 20; height: 20; radius: 10; color: Theme.accent; Text { anchors.centerIn: parent; text: "✓"; color: Theme.bg; font.pixelSize: 11; font.weight: Font.Bold } }
                    Text { text: (I18n.language, I18n.t("settings.deviceAccess.allGranted")); color: Theme.text; font.pixelSize: 12 }
                }
            }

            ColumnLayout {
                visible: root.missingAccessItems().length > 0
                spacing: 6
                Repeater {
                    model: root.showAllAccess ? root.accessItems : root.missingAccessItems()
                    ColumnLayout {
                        id: accessRow
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 4

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 10
                            Rectangle {
                                width: 20; height: 20; radius: 10
                                color: accessRow.modelData.status ? Theme.tint(Theme.success, 15) : Theme.tint(Theme.danger, 15)
                                Text {
                                    anchors.centerIn: parent
                                    text: accessRow.modelData.status ? "✓" : "✗"
                                    color: accessRow.modelData.status ? Theme.success : Theme.danger
                                    font.pixelSize: 11
                                    font.weight: Font.Bold
                                }
                            }
                            ColumnLayout {
                                spacing: 1
                                Layout.fillWidth: true
                                Text { text: accessRow.modelData.label; color: Theme.text; font.pixelSize: 11; font.family: "monospace" }
                                Text { text: (I18n.language, I18n.t("settings.deviceAccess." + accessRow.modelData.id)); color: Theme.textDim; font.pixelSize: 11 }
                            }
                            Rectangle {
                                visible: !accessRow.modelData.status
                                width: 22; height: 22; radius: 11
                                color: Theme.tint(Theme.danger, 15)
                                border.width: 1
                                border.color: Theme.tint(Theme.danger, 30)
                                Text { anchors.centerIn: parent; text: "?"; color: Theme.danger; font.pixelSize: 11; font.weight: Font.Bold }
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.expandedAccessItem = root.expandedAccessItem === accessRow.modelData.id ? "" : accessRow.modelData.id
                                }
                            }
                        }

                        ColumnLayout {
                            visible: !accessRow.modelData.status && root.expandedAccessItem === accessRow.modelData.id
                            Layout.fillWidth: true
                            Layout.leftMargin: 30
                            spacing: 4
                            Repeater {
                                model: (root.accessFixMaps[accessRow.modelData.id] || { commands: [] }).commands
                                RowLayout {
                                    id: cmdRow
                                    required property string modelData
                                    Layout.fillWidth: true
                                    spacing: 6
                                    Text {
                                        text: cmdRow.modelData
                                        color: Theme.text
                                        font.pixelSize: 10
                                        font.family: "monospace"
                                        wrapMode: Text.WordWrap
                                        Layout.fillWidth: true
                                    }
                                    Rectangle {
                                        width: 50; height: 22
                                        radius: Theme.radius
                                        color: Theme.bg
                                        border.width: 1
                                        border.color: Theme.border
                                        Text { anchors.centerIn: parent; text: (I18n.language, I18n.t("common.copy")); color: Theme.accent; font.pixelSize: 10 }
                                        MouseArea {
                                            anchors.fill: parent
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: SystemController.writeClipboard(cmdRow.modelData)
                                        }
                                    }
                                }
                            }
                            Text {
                                horizontalAlignment: Text.AlignLeft
                                property string noteKey: (root.accessFixMaps[accessRow.modelData.id] || {}).note || ""
                                visible: noteKey.length > 0
                                text: (I18n.language, noteKey.length > 0 ? I18n.t(noteKey) : "")
                                color: Theme.textDim
                                font.pixelSize: 10
                                font.italic: true
                                wrapMode: Text.WordWrap
                                Layout.fillWidth: true
                            }
                        }
                    }
                }
            }
        }
    }

    Text {
        Layout.fillWidth: true
        text: (I18n.language, I18n.t("settings.about.credits"))
        color: Theme.textDim
        font.pixelSize: 11
        font.italic: true
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
    }

    Component.onCompleted: SystemController.refresh()
}
