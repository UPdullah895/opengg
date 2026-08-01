import QtQuick
import com.opengg.app

// Draggable trim boundary on the clip editor's timeline.
//
// Reports an ABSOLUTE x in the parent's coordinate space, not a delta: the
// handle is repositioned by the value it reports, so the MouseArea slides out
// from under the cursor between events. A delta measured against a fixed
// press-point therefore compounded and the handle barely tracked the mouse.
Item {
    id: handle

    /// New position, in the parent overlay's coordinates.
    signal movedTo(real x)

    property bool dragging: false

    width: 14

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        width: 3
        height: parent.height
        color: Theme.accent
    }

    // Grip, wide enough to grab without covering the lane.
    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        width: 14
        height: 18
        radius: 3
        color: handle.dragging || grip.containsMouse
               ? Theme.tint(Theme.text, 90) : Theme.accent
    }

    MouseArea {
        id: grip
        anchors.fill: parent
        anchors.margins: -6      // enlarge the hit target past the visual grip
        hoverEnabled: true
        cursorShape: Qt.SizeHorCursor
        preventStealing: true    // the timeline's seek handler must not grab it

        onPressed: handle.dragging = true
        onReleased: handle.dragging = false
        onCanceled: handle.dragging = false
        onPositionChanged: (m) => {
            if (!grip.pressed)
                return
            handle.movedTo(grip.mapToItem(handle.parent, m.x, m.y).x)
        }
    }
}
