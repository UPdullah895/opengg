import QtQuick
import com.opengg.app

// AppBox — small per-channel app-list box shown directly under each
// ChannelStrip on the Mixer page. Master's box holds every unrouted app;
// each other channel's box holds the apps currently routed to it. A chip can
// be dragged from one box and dropped on another to re-route it.
//
// Built on Qt Quick's native Drag/DropArea attached properties rather than a
// port of DropZone.vue's hand-rolled pointer-drag-with-ghost-element system —
// consistent with this migration's established "boring primitive over
// fragile fidelity" precedent (GraphicEQ's discrete sliders instead of an
// SVG bezier curve, MixerPage's old click-to-route picker instead of a 1:1
// port of the same ghost-element drag).
Rectangle {
    id: box

    property string channelName: ""
    property color channelColor: Theme.accent
    /// [{id, name, binary, ...}] — already filtered to this channel by the
    /// caller (MixerPage.appsForChannel).
    property var apps: []
    property int perRow: 1
    property int rowH: 22
    property int rowGap: 2
    property int boxPad: 6
    /// An unclipped Item covering the whole strips row. A dragged chip is
    /// reparented into it for the duration of the drag — left parented to
    /// this box's own clipped Flickable, the chip would be cut off at the
    /// box edge the instant it crossed into a neighbouring box.
    property Item dragOverlay: null

    signal appDropped(int appId, string binary)

    radius: Theme.radius
    color: Theme.bg
    border.width: 1
    border.color: dropArea.containsDrag ? box.channelColor : Theme.border

    DropArea {
        id: dropArea
        anchors.fill: parent
        keys: ["opengg-app-chip"]
        onDropped: (drop) => {
            if (drop.source.sourceChannel !== box.channelName)
                box.appDropped(drop.source.appId, drop.source.appBinary)
        }
    }

    Flickable {
        id: flick
        anchors.fill: parent
        anchors.margins: box.boxPad
        contentWidth: width
        contentHeight: grid.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        Grid {
            id: grid
            width: flick.width
            columns: box.perRow
            columnSpacing: 4
            rowSpacing: box.rowGap

            Repeater {
                model: box.apps

                delegate: Rectangle {
                    id: chip
                    required property var modelData
                    readonly property int appId: chip.modelData.id
                    readonly property string appBinary: chip.modelData.binary || ""
                    readonly property string sourceChannel: box.channelName

                    width: (grid.width - (box.perRow - 1) * grid.columnSpacing) / box.perRow
                    height: box.rowH
                    radius: 4
                    color: chipArea.dragging ? Theme.tint(box.channelColor, 15) : Theme.bgDeep
                    opacity: chipArea.dragging ? 0.55 : 1

                    Drag.active: chipArea.dragging
                    Drag.keys: ["opengg-app-chip"]
                    Drag.hotSpot.x: chip.width / 2
                    Drag.hotSpot.y: chip.height / 2

                    Row {
                        anchors.fill: parent
                        anchors.leftMargin: 7
                        anchors.rightMargin: 7
                        spacing: 5
                        Rectangle {
                            width: 5; height: 5; radius: 2.5
                            anchors.verticalCenter: parent.verticalCenter
                            color: box.channelColor
                        }
                        Text {
                            width: parent.width - 10
                            anchors.verticalCenter: parent.verticalCenter
                            text: chip.modelData.name
                            color: Theme.textDim
                            font.pixelSize: 10
                            elide: Text.ElideRight
                        }
                    }

                    MouseArea {
                        id: chipArea
                        anchors.fill: parent
                        cursorShape: chipArea.dragging ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                        property bool dragging: false
                        property point pressPos: Qt.point(0, 0)
                        property Item originalParent: null

                        onPressed: (mouse) => { chipArea.pressPos = Qt.point(mouse.x, mouse.y) }

                        onPositionChanged: (mouse) => {
                            if (!chipArea.pressed) return
                            if (!chipArea.dragging) {
                                const dx = mouse.x - chipArea.pressPos.x
                                const dy = mouse.y - chipArea.pressPos.y
                                if (Math.sqrt(dx * dx + dy * dy) < 6) return
                                if (!box.dragOverlay) return
                                chipArea.originalParent = chip.parent
                                chip.parent = box.dragOverlay
                                chipArea.dragging = true
                            }
                            const p = chipArea.mapToItem(box.dragOverlay, mouse.x, mouse.y)
                            chip.x = p.x - chip.width / 2
                            chip.y = p.y - chip.height / 2
                        }

                        function endDrag() {
                            if (chipArea.dragging && chipArea.originalParent) {
                                chip.Drag.drop()
                                chip.parent = chipArea.originalParent
                                chip.x = 0; chip.y = 0
                                chipArea.originalParent = null
                            }
                            chipArea.dragging = false
                        }
                        onReleased: chipArea.endDrag()
                        onCanceled: chipArea.endDrag()
                    }
                }
            }
        }
    }

    Text {
        visible: box.apps.length === 0
        anchors.centerIn: parent
        text: dropArea.containsDrag
            ? (I18n.language, I18n.t("devices.dropHere"))
            : (I18n.language, I18n.t("devices.noApps"))
        color: dropArea.containsDrag ? box.channelColor : Theme.textMuted
        font.pixelSize: 10
        font.weight: dropArea.containsDrag ? Font.Bold : Font.Normal
    }
}
