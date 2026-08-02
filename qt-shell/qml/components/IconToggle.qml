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
    /// Drops this button's own border and rounding so it can sit inside a
    /// SegmentedToggle, which supplies the shared outline instead.
    ///
    /// Deliberately NOT per-corner radii (topLeftRadius and friends): those
    /// need Qt 6.7, and the distro matrix builds on Debian stable, whose Qt
    /// is older. A clipping rounded parent achieves the same look on any Qt.
    property bool flat: false

    signal triggered()

    implicitWidth: btn.label.length > 0 ? row.implicitWidth + 18 : 32
    implicitHeight: 32
    radius: btn.flat ? 0 : Theme.radius
    color: btn.active ? Theme.accentAlpha(18)
         : area.containsMouse ? Theme.bgHover
         : Theme.surface
    border.width: btn.flat ? 0 : 1
    border.color: btn.active ? Theme.accent : Theme.border

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
