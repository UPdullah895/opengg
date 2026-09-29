import QtQuick
import QtQuick.Controls
import com.opengg.app

// A timeline track's colour swatch. Click it for a picker: presets, then a
// hue strip and a shade grid built from it, with the current colour marked.
// Every colour comes from Theme.hsv()/tokens rather than a literal, so
// check-colors.sh still holds.
//
// The popup is deliberately NOT `id: palette`: every Qt 6 Item has a
// `palette` property, and inside the cell delegates it shadowed the id — each
// click threw "choose is not a function" and the hue strip wrote into the
// cell's own QQuickPalette, so no colour could ever be picked.
Rectangle {
    id: swatch

    /// The track's current colour.
    property color current: Theme.textDim
    /// Emitted with the picked colour as a "#rrggbb" string.
    signal picked(string color)

    width: 22; height: 22
    radius: 4
    color: swatch.current
    border.width: 1
    border.color: swatchArea.containsMouse || colorFlyout.visible ? Theme.text : Theme.border

    MouseArea {
        id: swatchArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: colorFlyout.toggle()
    }

    FlyoutPopup {
        id: colorFlyout
        anchorItem: swatch
        width: 6 * 24 + 16
        implicitHeight: pickerCol.implicitHeight + 16

        /// Hue of the shade grid, 0..1. Seeded from the current colour each
        /// time it opens so the grid starts somewhere recognisable.
        property real hue: 0
        onOpened: colorFlyout.hue = Theme.toHsv(swatch.current).h

        function choose(c) {
            swatch.picked(String(c))
            colorFlyout.close()
        }

        contentItem: Column {
            id: pickerCol
            spacing: 6

            Text {
                text: (I18n.language, I18n.t("settings.timelineTracks.presets"))
                color: Theme.textMuted
                font.pixelSize: 10
                font.weight: Font.ExtraBold
                font.letterSpacing: 1.2
            }
            Grid {
                columns: 6
                spacing: 4
                Repeater {
                    model: [
                        Theme.channelColor("Game"), Theme.channelColor("Chat"),
                        Theme.channelColor("Media"), Theme.channelColor("Aux"),
                        Theme.channelColor("Mic"), Theme.accent,
                        Theme.success, Theme.purple,
                        Theme.overdrive, Theme.textDim,
                        Theme.text, Theme.textMuted
                    ]
                    Rectangle {
                        required property var modelData
                        readonly property bool isCurrent: Qt.colorEqual(color, swatch.current)
                        width: 20; height: 20
                        radius: 4
                        color: modelData
                        border.width: isCurrent ? 2 : 1
                        border.color: presetArea.containsMouse || isCurrent ? Theme.text : Theme.border
                        MouseArea {
                            id: presetArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: colorFlyout.choose(parent.color)
                        }
                    }
                }
            }

            Rectangle { width: pickerCol.width; height: 1; color: Theme.border }

            Text {
                text: (I18n.language, I18n.t("settings.timelineTracks.hue"))
                color: Theme.textMuted
                font.pixelSize: 10
                font.weight: Font.ExtraBold
                font.letterSpacing: 1.2
            }
            // Discrete hue cells rather than a gradient: QML gradients cannot
            // be built from tokens, and 30 cells already reads as continuous.
            Row {
                id: hueStrip
                readonly property int cells: 30
                readonly property real cellW: pickerCol.width / hueStrip.cells
                Repeater {
                    model: hueStrip.cells
                    Rectangle {
                        required property int index
                        width: hueStrip.cellW
                        height: 16
                        color: Theme.hsv(index / hueStrip.cells, 0.85, 0.95)
                        // Marks which hue the grid below is showing.
                        Rectangle {
                            anchors.fill: parent
                            color: "transparent"
                            border.width: 2
                            border.color: Theme.text
                            visible: Math.round(colorFlyout.hue * hueStrip.cells)
                                     % hueStrip.cells === parent.index
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: colorFlyout.hue = parent.index / hueStrip.cells
                        }
                    }
                }
            }

            // Saturation across, brightness down.
            Grid {
                columns: 6
                spacing: 4
                Repeater {
                    model: 24
                    Rectangle {
                        required property int index
                        readonly property real sat: 0.25 + (index % 6) * 0.15
                        readonly property real val: 1.0 - Math.floor(index / 6) * 0.22
                        readonly property bool isCurrent: Qt.colorEqual(color, swatch.current)
                        width: 20; height: 20
                        radius: 4
                        color: Theme.hsv(colorFlyout.hue, sat, val)
                        border.width: isCurrent ? 2 : 1
                        border.color: shadeArea.containsMouse || isCurrent ? Theme.text : Theme.border
                        MouseArea {
                            id: shadeArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: colorFlyout.choose(parent.color)
                        }
                    }
                }
            }
        }
    }
}
