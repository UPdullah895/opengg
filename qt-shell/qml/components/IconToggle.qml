import QtQuick
import QtQuick.Controls
import com.opengg.app

// Square icon button with an on/off look — the Clips toolbar's view-mode,
// group-by-date and stats toggles are all this shape, as are the segments of
// the grid/list pair. Purely presentational: the caller owns the state and
// flips it in onTriggered.
Rectangle {
    id: btn

    property string icon: ""
    property bool active: false
    property string tooltip: ""
    /// Optional trailing text (the favourites button shows its count here).
    property string label: ""
    /// "" for a standalone button, or "left"/"right" to fuse this into an
    /// adjacent segment's pill — used by the grid/list view-mode pair, which
    /// should read as one control rather than two buttons with a gap between.
    property string segment: ""

    signal triggered()

    implicitWidth: btn.label.length > 0 ? row.implicitWidth + 18 : 32
    implicitHeight: 32
    topLeftRadius: btn.segment === "right" ? 0 : Theme.radius
    bottomLeftRadius: btn.segment === "right" ? 0 : Theme.radius
    topRightRadius: btn.segment === "left" ? 0 : Theme.radius
    bottomRightRadius: btn.segment === "left" ? 0 : Theme.radius
    color: btn.active ? Theme.accentAlpha(18)
         : area.containsMouse ? Theme.bgHover
         : Theme.surface
    border.width: 1
    border.color: btn.active ? Theme.accent : Theme.border

    // The shared edge between two fused segments would otherwise double up
    // into a visibly thicker seam; the left segment gives way and lets the
    // right segment's left border be the only line drawn there.
    Rectangle {
        visible: btn.segment === "left"
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 1
        color: parent.border.color
    }

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 6

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: btn.icon
            size: 14
            filled: btn.active && btn.icon === "heart"
            color: btn.active ? Theme.accent : Theme.textDim
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: btn.label.length > 0
            text: btn.label
            color: btn.active ? Theme.accent : Theme.textDim
            font.pixelSize: 12
            font.weight: Font.DemiBold
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: btn.triggered()
    }

    ToolTip {
        visible: btn.tooltip.length > 0 && area.containsMouse
        text: btn.tooltip
        delay: 400
    }
}
