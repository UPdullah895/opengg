import QtQuick
import QtQuick.Controls
import Qt.labs.platform as Labs
import QtQuick.Layouts
import com.opengg.app

// Settings → Storage. QML port of StorageSettings.vue.
// Omitted from this port: the Steam-library-import card — get_steam_games is
// an async reqwest call in core::steam and needs the same cxx_qt::Threading
// worker-thread bridge ClipsController uses for thumbnails; deferred rather
// than blocking the UI thread on a network call.
ColumnLayout {
    id: root
    width: parent.width
    spacing: 20

    property var s: JSON.parse(SettingsController.settingsJson || "{}")
    property var clipDirs: root.s.clip_directories || []
    property var shotDirs: root.s.screenshotDirs || []
    property var storageInfo: JSON.parse(StorageController.storageJson || "{}")

    Connections {
        target: SettingsController
        function onSettingsJsonChanged() {
            root.s = JSON.parse(SettingsController.settingsJson || "{}")
            StorageController.refresh(JSON.stringify(root.clipDirs.length ? root.clipDirs : ["~/Videos/OpenGG"]))
        }
    }
    Connections {
        target: StorageController
        function onStorageJsonChanged() { root.storageInfo = JSON.parse(StorageController.storageJson || "{}") }
    }

    function fmtBytes(b) {
        if (b >= 1e9) return (b / 1e9).toFixed(1) + " GB"
        if (b >= 1e6) return (b / 1e6).toFixed(1) + " MB"
        return (b / 1e3).toFixed(0) + " KB"
    }

    function urlToPath(url) {
        return decodeURIComponent(url.toString().replace(/^(file:\/{2,3})/, "/").replace(/^\/\//, "/"))
    }

    // Qt.labs.platform, NOT QtQuick.Dialogs: the Quick dialog draws Qt's own
    // bare-bones file browser, with no places sidebar, no recent locations
    // and none of the desktop's own conventions. The platform variant hands
    // off to the XDG portal / the desktop's real folder picker.
    Labs.FolderDialog {
        id: clipDirDialog
        title: "Add Clip Directory"
        onAccepted: {
            const p = root.urlToPath(folder)
            const next = root.clipDirs.concat(root.clipDirs.includes(p) ? [] : [p])
            SettingsController.setValue("clip_directories", JSON.stringify(next))
        }
    }
    Labs.FolderDialog {
        id: shotDirDialog
        title: "Add Screenshot Directory"
        onAccepted: {
            const p = root.urlToPath(folder)
            const next = root.shotDirs.concat(root.shotDirs.includes(p) ? [] : [p])
            SettingsController.setValue("screenshotDirs", JSON.stringify(next))
        }
    }

    SettingsHeading { titleText: (I18n.language, I18n.t("settings.storage.title")) }

    // ── Directories card ──
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: dirCol.implicitHeight + 40

        ColumnLayout {
            id: dirCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 16

            RowLayout {
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.storage.title"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                InfoIcon { tooltipText: I18n.t("settings.storage.mediaDirsHint") }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            ColumnLayout {
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.storage.clipDirectories"))
                    color: Theme.textDim
                    font.pixelSize: 13
                }
                Repeater {
                    model: root.clipDirs
                    Rectangle {
                        id: clipDirRow
                        required property string modelData
                        required property int index
                        Layout.fillWidth: true
                        implicitHeight: 36
                        radius: Theme.radius
                        color: Theme.bg
                        border.width: 1
                        border.color: Theme.border

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            spacing: 8
                            Icon { name: "folder"; size: 13; color: Theme.textDim }
                            Text {
                                horizontalAlignment: Text.AlignLeft
                                text: clipDirRow.modelData || I18n.t("settings.storage.defaultClipPath")
                                color: Theme.text
                                font.pixelSize: 12
                                Layout.fillWidth: true
                                elide: Text.ElideMiddle
                            }
                            Icon {
                                name: "x"; size: 12
                                color: Theme.textDim
                                MouseArea {
                                    anchors.fill: parent
                                    anchors.margins: -6
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        const next = root.clipDirs.filter((_, i) => i !== clipDirRow.index)
                                        SettingsController.setValue("clip_directories", JSON.stringify(next))
                                    }
                                }
                            }
                        }
                    }
                }
                Rectangle {
                    width: 140; height: 28
                    radius: Theme.radius
                    color: Theme.bg
                    border.width: 1
                    border.color: Theme.border
                    Text {
                        anchors.centerIn: parent
                        text: (I18n.language, I18n.t("settings.storage.addClipPath"))
                        color: Theme.textDim
                        font.pixelSize: 11
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: clipDirDialog.open()
                    }
                }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            ColumnLayout {
                spacing: 8
                Text {
                    text: (I18n.language, I18n.t("settings.storage.screenshotDirectories"))
                    color: Theme.textDim
                    font.pixelSize: 13
                }
                Repeater {
                    model: root.shotDirs
                    Rectangle {
                        id: shotDirRow
                        required property string modelData
                        required property int index
                        Layout.fillWidth: true
                        implicitHeight: 36
                        radius: Theme.radius
                        color: Theme.bg
                        border.width: 1
                        border.color: Theme.border

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            spacing: 8
                            Icon { name: "folder"; size: 13; color: Theme.textDim }
                            Text {
                                text: shotDirRow.modelData
                                color: Theme.text
                                font.pixelSize: 12
                                Layout.fillWidth: true
                                elide: Text.ElideMiddle
                            }
                            Icon {
                                name: "x"; size: 12
                                color: Theme.textDim
                                MouseArea {
                                    anchors.fill: parent
                                    anchors.margins: -6
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        const next = root.shotDirs.filter((_, i) => i !== shotDirRow.index)
                                        SettingsController.setValue("screenshotDirs", JSON.stringify(next))
                                    }
                                }
                            }
                        }
                    }
                }
                Rectangle {
                    width: 160; height: 28
                    radius: Theme.radius
                    color: Theme.bg
                    border.width: 1
                    border.color: Theme.border
                    Text {
                        anchors.centerIn: parent
                        text: (I18n.language, I18n.t("settings.storage.addScreenshotPath"))
                        color: Theme.textDim
                        font.pixelSize: 11
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: shotDirDialog.open()
                    }
                }
            }
        }
    }

    // ── Disk usage card ──
    Rectangle {
        Layout.fillWidth: true
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: usageCol.implicitHeight + 40

        ColumnLayout {
            id: usageCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 12

            RowLayout {
                Layout.fillWidth: true
                Text {
                    horizontalAlignment: Text.AlignLeft
                    text: (I18n.language, I18n.t("settings.storage.diskUsage"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                    Layout.fillWidth: true
                }
                // Clearing the cache deletes EVERY thumbnail, so the clips
                // grid goes blank until each card regenerates its own. This
                // used to happen with no confirmation, no refreshed figures
                // and no regeneration kick, which read as "the button does
                // nothing" while the whole library's artwork vanished.
                Rectangle {
                    id: clearBtn
                    width: 150; height: 28
                    radius: Theme.radius
                    color: clearArea.containsMouse ? Theme.bgHover : Theme.bg
                    border.width: 1
                    border.color: Theme.border

                    /// Files removed by the last click; -1 before any click.
                    property int lastRemoved: -1

                    Text {
                        anchors.centerIn: parent
                        text: (I18n.language, I18n.t("settings.clipSettings.clearCache"))
                        color: Theme.textDim
                        font.pixelSize: 11
                    }
                    MouseArea {
                        id: clearArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            clearBtn.lastRemoved = StorageController.clearThumbnailCache()
                            // Re-read the library so every visible card sees an
                            // empty thumbnail and kicks off regeneration, and
                            // re-read the disk figures the card above shows.
                            ClipsController.refresh()
                            StorageController.refresh(JSON.stringify(
                                root.clipDirs.length ? root.clipDirs : ["~/Videos/OpenGG"]))
                            clearedTimer.restart()
                        }
                    }
                    Timer { id: clearedTimer; interval: 4000 }
                }
                Text {
                    Layout.alignment: Qt.AlignVCenter
                    visible: clearedTimer.running && clearBtn.lastRemoved >= 0
                    text: clearBtn.lastRemoved + " "
                          + (I18n.language, I18n.t("settings.storage.thumbsRemoved"))
                    color: Theme.textDim
                    font.pixelSize: 11
                }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

            Text {
                visible: StorageController.loading
                text: (I18n.language, I18n.t("settings.storage.loading"))
                color: Theme.textDim
                font.pixelSize: 13
            }

            RowLayout {
                visible: !StorageController.loading
                spacing: 20
                ColumnLayout {
                    spacing: 2
                    Text { text: (I18n.language, I18n.t("settings.storage.clips")); color: Theme.textDim; font.pixelSize: 11 }
                    Text { text: root.storageInfo.clip_count || 0; color: Theme.accent; font.pixelSize: 18; font.weight: Font.Bold }
                }
                ColumnLayout {
                    spacing: 2
                    Text { text: (I18n.language, I18n.t("settings.storage.used")); color: Theme.textDim; font.pixelSize: 11 }
                    Text { text: root.fmtBytes(root.storageInfo.used_bytes || 0); color: Theme.text; font.pixelSize: 18; font.weight: Font.Bold }
                }
            }
        }
    }

    Component.onCompleted: {
        SettingsController.refresh()
        StorageController.refresh(JSON.stringify(root.clipDirs.length ? root.clipDirs : ["~/Videos/OpenGG"]))
    }
}
