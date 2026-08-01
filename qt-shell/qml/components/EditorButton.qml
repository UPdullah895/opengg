import QtQuick
import QtQuick.Controls
import com.opengg.app

// Transport button for the clip editor. Unlike PlayerButton this sits on the
// app's own chrome rather than over video, so it follows the surface/text
// tokens instead of being hardcoded white.
Rectangle {
    id: btn

    property string icon: ""
    property string tooltip: ""

    signal triggered()

    // Plain implicit sizing rather than Layout attached properties, so the
    // component stays usable outside a Layout; callers set Layout.* themselves.
    implicitWidth: 30
    implicitHeight: 30
    radius: Theme.radius
    color: area.containsMouse ? Theme.bgHover : "transparent"

    Icon {
        anchors.centerIn: parent
        name: btn.icon
        size: 14
        color: Theme.text
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
        delay: 500
    }
}
