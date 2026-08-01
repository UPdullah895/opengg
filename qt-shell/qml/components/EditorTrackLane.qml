import QtQuick
import com.opengg.app

// One row of the clip editor's timeline: a fixed-width label gutter followed
// by the track's coloured lane. The lane is a flat tinted bar rather than a
// rendered waveform — core::media can produce peak data, but plumbing a
// per-track waveform through is a separate slice; the lane already conveys
// track identity, extent and monitoring state.
Item {
    id: lane

    property int gutter: 96
    property string label: ""
    property string icon: ""
    property color accent: Theme.accent
    /// Audio lanes can be monitored (Qt Multimedia plays one track at a time);
    /// the video lane cannot.
    property bool monitorable: false
    property bool monitoring: false

    signal monitorToggled()

    implicitHeight: 30

    Row {
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: lane.gutter
        spacing: 6

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: lane.icon
            size: 12
            color: lane.accent
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: lane.gutter - 46
            text: lane.label
            color: Theme.text
            font.pixelSize: 11
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            visible: lane.monitorable
            width: 20; height: 20; radius: 4
            color: monArea.containsMouse ? Theme.bgHover : "transparent"
            Icon {
                anchors.centerIn: parent
                name: lane.monitoring ? "volume-2" : "volume-x"
                size: 12
                color: lane.monitoring ? lane.accent : Theme.textMuted
            }
            MouseArea {
                id: monArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: lane.monitorToggled()
            }
        }
    }

    Rectangle {
        anchors.left: parent.left
        anchors.leftMargin: lane.gutter
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: 22
        radius: 4
        color: Theme.tint(lane.accent, lane.monitorable && !lane.monitoring ? 10 : 28)
        border.width: 1
        border.color: Theme.tint(lane.accent, 55)
    }
}
