import QtQuick
import com.opengg.app

// Draggable trim boundary on the clip editor's timeline. Reports deltas rather
// than absolute positions so the page can clamp each handle against the other.
Item {
    id: handle

    /// Emitted with the horizontal movement since the last emit, in pixels.
    signal dragged(real dx)

    width: 12

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
        width: 12
        height: 16
        radius: 3
        color: Theme.accent
    }

    MouseArea {
        anchors.fill: parent
        anchors.margins: -4      // enlarge the hit target past the visual grip
        cursorShape: Qt.SizeHorCursor
        property real lastX: 0
        onPressed: (m) => handle.lastX = m.x
        onPositionChanged: (m) => {
            if (!pressed) return
            handle.dragged(m.x - handle.lastX)
        }
    }
}
