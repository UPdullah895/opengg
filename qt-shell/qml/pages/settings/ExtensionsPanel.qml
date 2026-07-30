import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings → Extensions. QML port of ExtensionsSettings.vue.
//
// Omitted from this port (see the header comment in
// src/extensions.rs for the full rationale):
//   - The per-extension "Configure" gear button. The Vue app opens the
//     extension's own settings UI by dynamically mounting a Vue component
//     the extension's IIFE bundle registers at runtime
//     (`extStore.loadExtension()` / `window.__ext_<id>.settingsComponent`).
//     There is no QML/cxx-qt equivalent of loading and sandboxing
//     third-party UI code yet — this needs its own design pass, not a
//     mechanical port.
//   - The dev-mode hot-reload button — Vite/Vue-dev-server specific, no
//     equivalent build pipeline in the Rust/QML shell.
//   - The GSR install-guide sub-card (Ubuntu/Arch/Fedora copy-paste
//     commands) — already fully covered by RecorderInstallHelper.qml,
//     embedded in the Capture & Sound panel.
ColumnLayout {
    id: root
    spacing: 20

    property var extList: JSON.parse(ExtensionsController.extensionsJson || "[]")
    property var consents: JSON.parse(ExtensionsController.consentsJson || "{}")
    property var modules: JSON.parse(ExtensionsController.modulesJson || "{}")
    property var s: JSON.parse(SettingsController.settingsJson || "{}")

    property bool consentOpen: false
    property var consentExt: null

    function needsConsent(p) {
        return !!p.has_daemon && !root.consents[p.id]
    }

    function setExtEnabled(p) {
        if (root.needsConsent(p)) {
            root.consentExt = p
            root.consentOpen = true
            return
        }
        ExtensionsController.setEnabled(p.id, !p.enabled)
    }

    function confirmConsent() {
        if (root.consentExt) ExtensionsController.grantConsent(root.consentExt.id)
        root.consentOpen = false
        root.consentExt = null
    }

    // I18n.t() takes only a key — no vue-i18n-style param interpolation — so
    // {name}/{version}/{permissions} placeholders are substituted manually
    // below rather than passed as a second argument (which I18n.t ignores).
    function permissionsLabel(p) {
        if (!p.permissions || p.permissions.length === 0) return I18n.t("ext.consent.legacyAccess")
        return p.permissions.map(perm => I18n.t("ext.consent.permission." + perm.replace(":", "_"))).join(", ")
    }

    function consentMessage(p) {
        if (!p) return ""
        return I18n.t("ext.consent.message")
            .replace("{name}", p.name || p.id)
            .replace("{version}", p.version || "unknown")
            .replace("{permissions}", root.permissionsLabel(p))
    }

    function toggleGsr() {
        if (root.s.gsrEnabled) {
            RecordingController.stop()
        } else {
            RecordingController.start()
        }
        SettingsController.setValue("gsrEnabled", JSON.stringify(!root.s.gsrEnabled))
    }

    Connections {
        target: SettingsController
        function onSettingsJsonChanged() { root.s = JSON.parse(SettingsController.settingsJson || "{}") }
    }

    Text {
        text: (I18n.language, I18n.t("settings.extensions.title"))
        color: Theme.text
        font.pixelSize: 22
        font.weight: Font.Bold
    }

    // ── Core Modules card ──
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredWidth: 680
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: modCol.implicitHeight + 40

        ColumnLayout {
            id: modCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 12

            RowLayout {
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.general.modules"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                InfoIcon { tooltipText: I18n.t("settings.extensions.hint") }
            }

            Repeater {
                model: [
                    { key: "audio", nameKey: "settings.general.audioHub", descKey: "settings.general.audioHubDesc" },
                    { key: "device", nameKey: "settings.general.deviceManager", descKey: "settings.general.deviceManagerDesc" },
                    { key: "replay", nameKey: "settings.general.replayClips", descKey: "settings.general.replayClipsDesc" },
                ]
                RowLayout {
                    id: modRow
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 12
                    ToggleSwitch {
                        checked: root.modules[modRow.modelData.key] !== false
                        onToggled: (v) => ExtensionsController.setModule(modRow.modelData.key, v)
                    }
                    ColumnLayout {
                        spacing: 1
                        Text { text: (I18n.language, I18n.t(modRow.modelData.nameKey)); color: Theme.text; font.pixelSize: 13 }
                        Text { text: (I18n.language, I18n.t(modRow.modelData.descKey)); color: Theme.textDim; font.pixelSize: 11 }
                    }
                    Item { Layout.fillWidth: true }
                }
            }
        }
    }

    // ── GPU Screen Recorder card ──
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredWidth: 680
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: gsrCol.implicitHeight + 40

        ColumnLayout {
            id: gsrCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 10

            RowLayout {
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.captureGsr.title"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                Rectangle {
                    radius: 4
                    color: Qt.rgba(0.914, 0.271, 0.376, 0.15)
                    width: betaText.implicitWidth + 12
                    height: 18
                    Text { id: betaText; anchors.centerIn: parent; text: "Beta"; color: Theme.accent; font.pixelSize: 10; font.weight: Font.DemiBold }
                }
                InfoIcon { tooltipText: I18n.t("settings.captureGsr.hint") }
                Item { Layout.fillWidth: true }
                ToggleSwitch {
                    checked: !!root.s.gsrEnabled
                    onToggled: root.toggleGsr()
                }
            }
            Text {
                text: !!root.s.gsrEnabled ? "Replay buffer enabled — configure it in Capture & Sound." : "Disabled — enable to configure quality, FPS, and audio sources in Capture & Sound."
                color: Theme.textDim
                font.pixelSize: 12
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    // ── Unified Extensions List ──
    Rectangle {
        id: extListCard
        Layout.fillWidth: true
        Layout.preferredWidth: 680
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: extCol.implicitHeight + 40

        ColumnLayout {
            id: extCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 14

            RowLayout {
                Layout.fillWidth: true
                Text {
                    text: (I18n.language, I18n.t("settings.extensions.sectionTitle"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                    Layout.fillWidth: true
                }
                Rectangle {
                    width: 28; height: 28; radius: Theme.radius
                    color: "transparent"; border.width: 1; border.color: Theme.border
                    Icon { anchors.centerIn: parent; name: "refresh-cw"; size: 14; color: Theme.text}
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: ExtensionsController.refresh() }
                }
                Rectangle {
                    width: 28; height: 28; radius: Theme.radius
                    color: "transparent"; border.width: 1; border.color: Theme.border
                    Icon { anchors.centerIn: parent; name: "folder"; size: 13}
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: ExtensionsController.openFolder() }
                }
            }

            Text {
                visible: root.extList.length === 0
                text: (I18n.language, I18n.t("settings.extensions.noExtensions"))
                color: Theme.textDim
                font.pixelSize: 12
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Repeater {
                model: root.extList
                ColumnLayout {
                    id: extRow
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    spacing: 0

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 12
                        opacity: root.needsConsent(extRow.modelData) ? 0.75 : 1.0

                        ColumnLayout {
                            spacing: 2
                            Layout.fillWidth: true
                            RowLayout {
                                spacing: 8
                                Text {
                                    text: extRow.modelData.name
                                    color: Theme.text
                                    font.pixelSize: 13
                                    font.weight: Font.DemiBold
                                }
                                Text {
                                    text: "v" + extRow.modelData.version
                                    color: Theme.textDim
                                    font.pixelSize: 10
                                }
                                Rectangle {
                                    visible: root.needsConsent(extRow.modelData)
                                    radius: 4
                                    color: Qt.rgba(0.914, 0.271, 0.376, 0.12)
                                    width: consentBadge.implicitWidth + 12
                                    height: 18
                                    Text {
                                        id: consentBadge
                                        anchors.centerIn: parent
                                        text: (I18n.language, I18n.t("ext.consent.badge"))
                                        color: Theme.accent
                                        font.pixelSize: 10
                                        font.weight: Font.DemiBold
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: {
                                            root.consentExt = extRow.modelData
                                            root.consentOpen = true
                                        }
                                    }
                                }
                            }
                            Text {
                                text: extRow.modelData.description
                                color: Theme.textDim
                                font.pixelSize: 11
                                wrapMode: Text.WordWrap
                                Layout.fillWidth: true
                            }
                        }

                        ToggleSwitch {
                            checked: !!extRow.modelData.enabled
                            enabled: !(root.needsConsent(extRow.modelData) && !extRow.modelData.enabled)
                            onToggled: root.setExtEnabled(extRow.modelData)
                        }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.topMargin: 10
                        Layout.bottomMargin: 10
                        height: 1
                        color: Theme.border
                        visible: extRow.index < root.extList.length - 1
                    }
                }
            }
        }

        // ── Consent confirm overlay, scoped to this card (same proven
        // pattern as MixerRoutingPanel.qml's Danger Zone confirm — a plain
        // Rectangle child of `extListCard`, which is a Rectangle not a
        // Layout, so anchors.fill is safe here). Deliberately not a QQC2
        // `Popup`, which shares enough machinery with the ComboBox/ItemPicker
        // combination that caused the earlier IconPicker hang to be worth
        // avoiding until proven safe in this codebase. ──
        Rectangle {
            anchors.fill: parent
            visible: root.consentOpen
            z: 1000
            radius: parent.radius
            color: Qt.rgba(0, 0, 0, 0.65)

            MouseArea {
                anchors.fill: parent
                onClicked: { root.consentOpen = false; root.consentExt = null }
            }

            Rectangle {
                anchors.centerIn: parent
                width: Math.min(360, parent.width - 32)
                radius: Theme.radius
                color: Theme.surface
                border.width: 1
                border.color: Theme.border
                implicitHeight: consentCol.implicitHeight + 32

                MouseArea { anchors.fill: parent } // swallow clicks so they don't hit the scrim

                ColumnLayout {
                    id: consentCol
                    anchors.fill: parent
                    anchors.margins: 16
                    spacing: 14

                    Text {
                        text: (I18n.language, I18n.t("ext.consent.title"))
                        color: Theme.text
                        font.pixelSize: 15
                        font.weight: Font.DemiBold
                    }
                    Text {
                        text: root.consentMessage(root.consentExt)
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
                            color: "transparent"; border.width: 1; border.color: Theme.border
                            Text { anchors.centerIn: parent; text: I18n.t("ext.consent.deny"); color: Theme.textDim; font.pixelSize: 12 }
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { root.consentOpen = false; root.consentExt = null } }
                        }
                        Rectangle {
                            width: 90; height: 30; radius: Theme.radius
                            color: Theme.accent
                            Text { anchors.centerIn: parent; text: I18n.t("ext.consent.allow"); color: "#fff"; font.pixelSize: 12; font.weight: Font.DemiBold }
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.confirmConsent() }
                        }
                    }
                }
            }
        }
    }

    RowLayout {
        spacing: 6
        Icon { name: "info"; size: 11}
        Text {
            text: (I18n.language, I18n.t("settings.extensions.restartHint"))
            color: Theme.textDim
            font.pixelSize: 11
        }
    }

    Component.onCompleted: {
        ExtensionsController.refresh()
        SettingsController.refresh()
    }
}
