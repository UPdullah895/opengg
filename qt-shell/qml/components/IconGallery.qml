import QtQuick
import com.opengg.app

// Dev-only contact sheet of every entry in the Icons registry, reachable via
//   opengg-qt --screenshot icons.png --page icons
//
// Exists because icon geometry fails *silently*: QML's PathSvg drops the rest
// of a subpath when it cannot parse a segment (notably SVG's compact arc-flag
// shorthand) without emitting any warning. Eyeballing two icons is not
// evidence that the other 29 survived. Render this sheet after touching
// Icons.qml and look for anything truncated or blank.
Rectangle {
    color: Theme.bg

    readonly property var names: Object.keys(Icons.defs).sort()

    Text {
        id: hdr
        x: 20; y: 14
        text: "Icons registry — " + parent.names.length + " entries"
        color: Theme.text
        font.pixelSize: 15
        font.weight: Font.Bold
    }

    Grid {
        x: 20
        y: 44
        width: parent.width - 40
        columns: 6
        spacing: 4

        Repeater {
            model: parent.parent.names

            Rectangle {
                required property string modelData
                width: 190
                height: 88
                color: Theme.surface
                border.width: 1
                border.color: Theme.border
                radius: Theme.radius

                Column {
                    anchors.centerIn: parent
                    spacing: 6

                    Row {
                        anchors.horizontalCenter: parent.horizontalCenter
                        spacing: 10
                        // Three sizes: catches clipping and scaling errors that
                        // are invisible at a single size.
                        Icon { name: modelData; size: 16; color: Theme.text; anchors.verticalCenter: parent.verticalCenter }
                        Icon { name: modelData; size: 24; color: Theme.accent; anchors.verticalCenter: parent.verticalCenter }
                        Icon { name: modelData; size: 32; color: Theme.textDim; anchors.verticalCenter: parent.verticalCenter }
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: modelData
                        color: Theme.textDim
                        font.pixelSize: 11
                    }
                }
            }
        }
    }
}
