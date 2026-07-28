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
                { key: "mixerRouting", built: false },
            ],
        },
        {
            labelKey: "moments",
            items: [
                { key: "captureSound", built: false },
                { key: "trackManagement", built: true },
                { key: "storage", built: true },
                { key: "notifications", built: true },
            ],
        },
        {
            labelKey: "extensions",
            items: [
                { key: "extensions", built: false },
                { key: "store", built: false },
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
                anchors.fill: parent
                contentWidth: availableWidth

                ColumnLayout {
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
                                    color: isActive ? Qt.rgba(0.914, 0.271, 0.376, 0.12)
                                         : navArea.containsMouse ? Qt.rgba(0.914, 0.271, 0.376, 0.08) : "transparent"

                                    Rectangle {
                                        anchors.right: parent.right
                                        width: 2
                                        height: parent.height
                                        color: isActive ? Theme.accent : "transparent"
                                    }

                                    Text {
                                        anchors.left: parent.left
                                        anchors.leftMargin: 16
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: (I18n.language, I18n.t("settings.sections." + modelData.key))
                                        color: isActive ? Theme.accent : (navArea.containsMouse ? Theme.accent : Theme.textDim)
                                        font.pixelSize: 13
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
        }

        // ── Content ──
        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth

            ColumnLayout {
                width: parent.width
                x: 28
                y: 24

                Loader {
                    Layout.preferredWidth: 680
                    sourceComponent: {
                        switch (page.active) {
                        case "general": return generalPanel
                        case "language": return languagePanel
                        case "shortcuts": return shortcutsPanel
                        case "notifications": return notificationsPanel
                        case "storage": return storagePanel
                        case "trackManagement": return trackManagementPanel
                        case "about": return aboutPanel
                        default: return comingSoonPanel
                        }
                    }
                }
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
    Component {
        id: comingSoonPanel
        ComingSoonPanel { sectionTitle: I18n.t("settings.sections." + page.active) }
    }
}
