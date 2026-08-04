import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings page — left nav (mirrors SettingsPage.vue's navGroups) + a
// StackLayout of per-section panels on the right.
Rectangle {
    id: page
    color: Theme.bg

    // Keys: key (Section id), label i18n key under settings.sections.*, built true
    // once real content exists (see qml/pages/settings/*Panel.qml).
    readonly property var navGroups: [
        {
            labelKey: "general",
            items: [
                { key: "general", built: true },
                { key: "language", built: true },
                { key: "shortcuts", built: true },
            ],
        },
        {
            labelKey: "audioEngine",
            items: [
                { key: "mixerRouting", built: true },
            ],
        },
        {
            labelKey: "moments",
            items: [
                { key: "captureSound", built: true },
                { key: "trackManagement", built: true },
                { key: "storage", built: true },
                { key: "notifications", built: true },
            ],
        },
        {
            labelKey: "extensions",
            items: [
                { key: "extensions", built: true, badge: "Beta" },
                { key: "store", built: true },
            ],
        },
        {
            labelKey: "",
            items: [
                { key: "about", built: true },
            ],
        },
    ]

    property string active: "general"

    // Dev-only: let `--screenshot --page settings --panel <key>` open a specific
    // panel so each one can be captured headlessly (UI-fidelity plan Phase 0).
    Component.onCompleted: {
        if (ScreenshotController.active && ScreenshotController.panel.length > 0)
            page.active = ScreenshotController.panel
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // ── Left nav ──
        Rectangle {
            Layout.preferredWidth: 196
            Layout.fillHeight: true
            color: Theme.bg
            border.width: 0

            Rectangle {
                anchors.right: parent.right
                width: 1
                height: parent.height
                color: Theme.border
            }

            ScrollView {
                id: navScroll
                anchors.fill: parent
                contentWidth: availableWidth
                // See HomePage.qml's ScrollView for why this is explicit —
                // QQC2's automatic contentHeight inference doesn't reliably
                // track a ColumnLayout's implicitHeight.
                contentHeight: navCol.implicitHeight

                ColumnLayout {
                    id: navCol
                    width: parent.width
                    spacing: 2

                    Repeater {
                        model: page.navGroups
                        ColumnLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 0

                            Text {
                                visible: modelData.labelKey.length > 0
                                text: modelData.labelKey.length > 0
                                    ? (I18n.language, I18n.t("settings.groups." + modelData.labelKey)) : ""
                                color: Theme.textDim
                                font.pixelSize: 10
                                font.weight: Font.Black
                                Layout.leftMargin: 16
                                Layout.topMargin: 12
                                Layout.bottomMargin: 5
                            }

                            Repeater {
                                model: modelData.items
                                Rectangle {
                                    required property var modelData
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 34
                                    property bool isActive: page.active === modelData.key
                                    color: isActive ? Theme.accentAlpha(12)
                                         : navArea.containsMouse ? Theme.accentAlpha(8) : "transparent"

                                    Rectangle {
                                        anchors.right: parent.right
                                        width: 2
                                        height: parent.height
                                        color: isActive ? Theme.accent : "transparent"
                                    }

                                    Text {
                                        id: navLabel
                                        anchors.left: parent.left
                                        anchors.leftMargin: 16
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: (I18n.language, I18n.t("settings.sections." + modelData.key))
                                        color: isActive ? Theme.accent : (navArea.containsMouse ? Theme.accent : Theme.textDim)
                                        font.pixelSize: 13
                                    }

                                    // "Beta" pill, as in SettingsPage.vue's nav.
                                    Rectangle {
                                        visible: !!modelData.badge
                                        anchors.left: navLabel.right
                                        anchors.leftMargin: 8
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: badgeLabel.implicitWidth + 10
                                        height: 14
                                        radius: 7
                                        color: Theme.accentAlpha(15)
                                        border.width: 1
                                        border.color: Theme.accentAlpha(40)
                                        Text {
                                            id: badgeLabel
                                            anchors.centerIn: parent
                                            text: modelData.badge || ""
                                            color: Theme.accent
                                            font.pixelSize: 8
                                            font.weight: Font.Bold
                                        }
                                    }

                                    MouseArea {
                                        id: navArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: page.active = modelData.key
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Wheel accelerator, sibling of navScroll inside the shared
            // Rectangle — see the comment on settingsWrap below for why a
            // direct Layout child can't be anchor-targeted from outside it.
            Item {
                anchors.fill: parent
                WheelScroller { anchors.fill: parent; flick: navScroll.contentItem }
            }
        }

        // ── Content ──
        //
        // Wrapped in a plain Item so the wheel-accelerator overlay can anchor
        // to it: ScrollView is a direct RowLayout child, and a Layout child
        // can't be anchor-targeted by an item outside that layout ("Cannot
        // anchor to an item that isn't a parent or sibling" — confirmed live,
        // same fix as ClipsPage's grid / MixerPage's EQ tab).
        Item {
            id: settingsWrap
            Layout.fillWidth: true
            Layout.fillHeight: true

        ScrollView {
            id: settingsScroll
            anchors.fill: parent
            contentWidth: availableWidth
            // See HomePage.qml's ScrollView for why this is explicit — QQC2's
            // automatic contentHeight inference doesn't reliably track a
            // ColumnLayout child positioned with an explicit x/y offset, so
            // long panels (Storage, Extensions, Capture & Sound) could look
            // like they had nothing to scroll even when their content
            // overflowed the window.
            contentHeight: settingsContentCol.implicitHeight + settingsContentCol.y * 2

            ColumnLayout {
                id: settingsContentCol
                width: parent.width
                y: 24
                // x was 28 here previously, WITH width: parent.width — that
                // pushed the column's right edge to 28 + parent.width,
                // 28px past the viewport, so the 28px gutter only existed
                // on the left. The gutter is now applied symmetrically via
                // the Loader's left/right margins below instead, keeping
                // the column itself at the full available width.

                Loader {
                    id: settingsPanelLoader
                    Layout.fillWidth: true
                    Layout.leftMargin: 28
                    Layout.rightMargin: 28
                    // Loader has no resize-item-to-loader mode — it sizes
                    // ITSELF around the loaded item, not the other way
                    // around. So without each panel's root explicitly
                    // binding `width: parent.width` (parent being this
                    // Loader, since that's where Loader.item is actually
                    // parented), every panel's Layout.fillWidth children
                    // filled nothing wider than their own implicit content
                    // width — this Loader (previously pinned to a literal
                    // 680) was the real source of every settings card's
                    // width cap, not anything in the panels themselves.
                    // Same convention `settingsContentCol` above already
                    // uses for the same reason (it isn't a Layout child of
                    // anything either).
                    sourceComponent: {
                        switch (page.active) {
                        case "general": return generalPanel
                        case "language": return languagePanel
                        case "shortcuts": return shortcutsPanel
                        case "notifications": return notificationsPanel
                        case "storage": return storagePanel
                        case "trackManagement": return trackManagementPanel
                        case "about": return aboutPanel
                        case "captureSound": return captureSoundPanel
                        case "mixerRouting": return mixerRoutingPanel
                        case "extensions": return extensionsPanel
                        case "store": return storePanel
                        default: return comingSoonPanel
                        }
                    }
                }
            }
        }

            // Wheel accelerator, sibling of settingsScroll inside settingsWrap.
            Item {
                anchors.fill: parent
                WheelScroller { anchors.fill: parent; flick: settingsScroll.contentItem }
            }
        }
    }

    Component { id: generalPanel; GeneralPanel {} }
    Component { id: languagePanel; LanguagePanel {} }
    Component { id: shortcutsPanel; ShortcutsPanel {} }
    Component { id: notificationsPanel; NotificationsPanel {} }
    Component { id: storagePanel; StoragePanel {} }
    Component { id: trackManagementPanel; TrackManagementPanel {} }
    Component { id: aboutPanel; AboutPanel {} }
    Component { id: captureSoundPanel; CaptureSoundPanel {} }
    Component { id: mixerRoutingPanel; MixerRoutingPanel {} }
    Component { id: extensionsPanel; ExtensionsPanel {} }
    Component { id: storePanel; StorePanel {} }
    Component {
        id: comingSoonPanel
        ComingSoonPanel { sectionTitle: I18n.t("settings.sections." + page.active) }
    }
}
