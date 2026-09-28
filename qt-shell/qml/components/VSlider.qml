import QtQuick
import QtQuick.Controls
import com.opengg.app

// Vertical companion to HSlider: fills bottom-to-top, with the numeric
// readout stacked above the track instead of beside it. HSlider reserves a
// 46px column for its number, which reads as dead space in a narrow flyout —
// that gap is exactly what this exists to avoid.
Item {
    id: root
    property real value: 0
    property real from: 0
    property real to: 100
    property color sliderColor: Theme.accent
    property string suffix: ""
    /// Height of the track itself; the readout sits above it.
    property real trackHeight: 96

    signal moved(real value)

    implicitWidth: 26
    implicitHeight: root.trackHeight + 18

    Text {
        id: valueText
        anchors.top: parent.top
        anchors.horizontalCenter: parent.horizontalCenter
        // The slider's own value, not root.value, so the number agrees with
        // the handle mid-drag (same reasoning as HSlider).
        text: Math.round(sl.value) + root.suffix
        color: Theme.textDim
        font.pixelSize: 10
    }

    Slider {
        id: sl
        orientation: Qt.Vertical
        anchors.top: valueText.bottom
        anchors.topMargin: 4
        anchors.horizontalCenter: parent.horizontalCenter
        // A Control derives implicitHeight from its background/handle
        // implicit size, and the delegates below set neither — without an
        // explicit height this collapses to 0 and silently ignores every
        // drag, the same trap HSlider documents.
        height: root.trackHeight
        width: 22
        from: root.from
        to: root.to
        onMoved: root.moved(value)

        // A plain `value:` binding is severed the first time the Slider
        // writes to value, so the handle would stop following the property
        // after the first drag. Re-bind on release, with a settle window so
        // an in-flight poll can't snap it back to a pre-drag value.
        Binding on value {
            value: root.value
            when: !sl.pressed && !settleTimer.running
            restoreMode: Binding.RestoreNone
        }
        onPressedChanged: if (!pressed) settleTimer.restart()

        background: Rectangle {
            x: sl.leftPadding + sl.availableWidth / 2 - width / 2
            y: sl.topPadding
            width: 5
            height: sl.availableHeight
            radius: 2.5
            color: Theme.border
            // Unipolar: fill from the BOTTOM up to the handle. visualPosition
            // is 0 at the top, so the filled part starts at the handle and
            // runs to the foot of the track.
            Rectangle {
                width: parent.width
                radius: 2.5
                color: root.sliderColor
                y: sl.visualPosition * parent.height
                height: parent.height - y
            }
        }
        handle: SliderHandle {
            x: sl.leftPadding + sl.availableWidth / 2 - width / 2
            y: sl.topPadding + sl.visualPosition * (sl.availableHeight - height)
            vertical: true
            accentColor: root.sliderColor
            active: sl.pressed || sl.hovered
        }
    }

    Timer { id: settleTimer; interval: 600 }
}
