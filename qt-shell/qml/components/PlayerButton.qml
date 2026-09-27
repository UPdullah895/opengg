import QtQuick
import QtQuick.Controls
import com.opengg.app

// Round icon button for the video player's control bar. Always white-on-
// translucent because it sits over video, not over the app's chrome — so it
// deliberately does not follow the surface/text tokens the rest of the UI uses.
Rectangle {
    id: btn

    property string icon: ""
    property string tooltip: ""
    /// Held-open look, for buttons that own a menu.
    property bool active: false

    signal triggered()

    anchors.verticalCenter: parent ? parent.verticalCenter : undefined
    width: 32
    height: 32
    radius: 16
    color: btn.active || area.containsMouse ? Theme.scrim(45) : "transparent"

    Icon {
        anchors.centerIn: parent
        name: btn.icon
        size: 15
        color: "#ffffff"
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: btn.triggered()
    }

    Tip {
        visible: btn.tooltip.length > 0 && area.containsMouse
        text: btn.tooltip
        delay: 500
    }
}
