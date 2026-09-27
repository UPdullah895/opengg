import QtQuick
import com.opengg.app

// Right-click menu for a clip — port of ClipsPage.vue's single page-level
// ClipsContextMenu (a Teleport'd popup driven by `replay.activeMenuClipId`).
// The QML shell had no context menu at all; the kebab opened the rename dialog
// as a stand-in.
//
// Built as a plain positioned Item rather than a QQC2 Popup, matching this
// migration's existing choice to avoid QQC2 Popup for card-scoped overlays.
// A single instance lives at page level and is positioned at the cursor, as in
// the original — not one menu per card.
Item {
    id: menu
    anchors.fill: parent
    visible: clip !== null

    /// {filepath, title, favorite} or null when closed.
    property var clip: null
    property real menuX: 0
    property real menuY: 0

    signal previewRequested()
    signal editRequested()
    signal selectRequested()
    signal favoriteRequested()
    signal renameRequested()
    signal revealRequested()
    signal copyPathRequested()
    signal deleteRequested()

    /// `x`/`y` are SCENE coordinates (callers use mapToItem(null, …)). This
    /// menu fills the Clips page, which is itself inset by the sidebar and
    /// titlebar, so scene coords land the panel down-right of the cursor by
    /// exactly that inset unless they're converted into local space first.
    function openAt(clipObj, x, y) {
        menu.clip = clipObj
        const p = menu.mapFromItem(null, x, y)
        menu.menuX = p.x
        menu.menuY = p.y
    }
    function close() { menu.clip = null }

    // Click-away closes, as the Vue version's document listener does.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: menu.close()
    }

    Rectangle {
        id: panel
        width: 208
        height: itemsCol.implicitHeight + 12
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border

        // Keep the panel inside the page on both axes.
        x: Math.max(6, Math.min(menu.menuX, menu.width - width - 6))
        y: Math.max(6, Math.min(menu.menuY, menu.height - height - 6))

        MouseArea { anchors.fill: parent }   // absorb clicks

        Column {
            id: itemsCol
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.topMargin: 6
            spacing: 0

            Repeater {
                model: [
                    { key: "preview",  icon: "play",          label: I18n.t("clips.contextMenu.preview") },
                    { key: "editor",   icon: "frame-check",   label: I18n.t("clips.contextMenu.edit") },
                    { sep: true },
                    { key: "select",   icon: "check-square",  label: I18n.t("clips.contextMenu.select") },
                    { key: "favorite", icon: "heart",
                      label: menu.clip && menu.clip.favorite ? I18n.t("clips.contextMenu.unfavorite")
                                                             : I18n.t("clips.contextMenu.favorite") },
                    { sep: true },
                    { key: "rename",   icon: "pencil",        label: I18n.t("clips.contextMenu.rename") },
                    { key: "location", icon: "folder",        label: I18n.t("clips.contextMenu.showInFolder") },
                    { key: "copyPath", icon: "copy",          label: I18n.t("clips.contextMenu.copyPath") },
                    { sep: true },
                    { key: "delete",   icon: "trash",         label: I18n.t("clips.contextMenu.delete"), danger: true }
                ]

                Item {
                    required property var modelData
                    width: panel.width
                    height: modelData.sep ? 7 : 30

                    // Separator
                    Rectangle {
                        visible: !!modelData.sep
                        anchors.centerIn: parent
                        width: parent.width - 16
                        height: 1
                        color: Theme.border
                    }

                    Rectangle {
                        visible: !modelData.sep
                        anchors.fill: parent
                        anchors.leftMargin: 4
                        anchors.rightMargin: 4
                        radius: Theme.radius
                        color: itemArea.containsMouse
                               ? (modelData.danger ? Theme.tint(Theme.danger, 12) : Theme.accentAlpha(10))
                               : "transparent"

                        Row {
                            anchors.left: parent.left
                            anchors.leftMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 9

                            Icon {
                                // Separator rows carry no icon/label; `visible:false`
                                // does not stop these bindings evaluating.
                                name: modelData.icon || "x"
                                size: 14
                                color: modelData.danger ? Theme.danger
                                     : itemArea.containsMouse ? Theme.accent : Theme.textDim
                                anchors.verticalCenter: parent.verticalCenter
                            }
                            Text {
                                text: modelData.label || ""
                                color: modelData.danger ? Theme.danger
                                     : itemArea.containsMouse ? Theme.text : Theme.textDim
                                font.pixelSize: 12
                                anchors.verticalCenter: parent.verticalCenter
                            }
                        }

                        MouseArea {
                            id: itemArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                switch (modelData.key) {
                                case "preview":  menu.previewRequested(); break
                                case "editor":   menu.editRequested(); break
                                case "select":   menu.selectRequested(); break
                                case "favorite": menu.favoriteRequested(); break
                                case "rename":   menu.renameRequested(); break
                                case "location": menu.revealRequested(); break
                                case "copyPath": menu.copyPathRequested(); break
                                case "delete":   menu.deleteRequested(); break
                                }
                                menu.close()
                            }
                        }
                    }
                }
            }
        }
    }
}
