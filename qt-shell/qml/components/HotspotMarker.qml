import QtQuick
import com.opengg.app

// One draggable button-hotspot marker on a device photo (Devices roadmap
// Phase 5). Reports an ABSOLUTE position in the parent overlay's coordinate
// space, not a delta — same reasoning as TrimHandle.qml: a delta measured
// against a fixed press-point compounds as the marker moves out from under
// the cursor, so the drag barely tracks the mouse after the first pixel.
Item {
    id: marker

    /// 0-based button index this hotspot marks (see
    /// `opengg_core::button_hotspots`'s module doc comment for why this is
    /// the daemon's `ButtonMapping.index`, not a `ButtonAction::Button`
    /// target).
    property int buttonIndex: 0
    /// Human label for whatever this button is currently bound to (e.g.
    /// "Left Click", "E", "None") — shown on hover/selection, not the
    /// button index itself, since the index means nothing to a user.
    property string actionLabel: ""
    property bool dragging: false
    property bool selected: false

    /// New position, in the parent overlay's coordinates (pixels, NOT
    /// normalized — the caller divides by the overlay's width/height).
    signal movedTo(real x, real y)
    signal clicked()
    signal deleteRequested()

    width: 28
    height: 28

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: marker.selected ? Theme.accent : Theme.surface
        border.width: 2
        border.color: marker.dragging || grip.containsMouse ? Theme.accent : Theme.border
        opacity: marker.dragging ? 0.8 : 1.0

        Text {
            anchors.centerIn: parent
            text: String(marker.buttonIndex + 1)
            color: marker.selected ? Theme.bg : Theme.text
            font.pixelSize: 12
            font.weight: Font.DemiBold
        }
    }

    // Small delete glyph, only reachable when selected — keeps the marker
    // itself small enough not to obscure the photo underneath it.
    Rectangle {
        visible: marker.selected
        x: parent.width - 10
        y: -6
        width: 16
        height: 16
        radius: 8
        color: Theme.danger
        Icon { anchors.centerIn: parent; name: "x"; size: 10; color: Theme.bg }
        MouseArea {
            anchors.fill: parent
            anchors.margins: -4
            onClicked: marker.deleteRequested()
        }
    }

    // Label, shown below the marker so it never covers the photo detail
    // the marker itself is pointing at.
    Rectangle {
        visible: marker.selected && marker.actionLabel !== ""
        anchors.top: parent.bottom
        anchors.topMargin: 4
        anchors.horizontalCenter: parent.horizontalCenter
        width: label.implicitWidth + 12
        height: label.implicitHeight + 6
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        Text {
            id: label
            anchors.centerIn: parent
            text: marker.actionLabel
            color: Theme.text
            font.pixelSize: 11
        }
    }

    MouseArea {
        id: grip
        anchors.fill: parent
        anchors.margins: -4
        hoverEnabled: true
        cursorShape: Qt.SizeAllCursor
        preventStealing: true

        onPressed: marker.dragging = true
        onReleased: marker.dragging = false
        onCanceled: marker.dragging = false
        onPositionChanged: (m) => {
            if (!grip.pressed)
                return
            const p = grip.mapToItem(marker.parent, m.x, m.y)
            marker.movedTo(p.x, p.y)
        }
        onClicked: marker.clicked()
    }
}
