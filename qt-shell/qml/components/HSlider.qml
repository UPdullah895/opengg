import QtQuick
import QtQuick.Controls
import com.opengg.app

// Small reusable horizontal slider with a numeric readout — used by
// GraphicEQ.qml (band gains, preamp) and DspControls.qml (intensity/
// threshold/level). Not a custom-styled QQC2 subclass wrapped in its own
// Popup or ComboBox, so it carries none of the risk flagged for those
// constructs elsewhere in this migration — just a plain Item + Slider.
Item {
    id: root
    property real value: 0
    property real from: 0
    property real to: 100
    property color sliderColor: Theme.accent
    property string suffix: ""

    signal moved(real value)

    implicitHeight: 28

    Slider {
        id: sl
        anchors.left: parent.left
        anchors.right: valueText.left
        anchors.rightMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        // A Control sizes itself from its background/handle *implicit* size,
        // and the custom delegates below only set `height` — so this Slider
        // computed implicitHeight 0 and, with no top/bottom anchor, ended up
        // 0px tall. It still painted (children aren't clipped) but was
        // invisible to hit-testing, so every HSlider in the app — the Home
        // popover's Quick Mixer, GraphicEQ's bands, DspControls — silently
        // ignored every click and drag. Give it a real height.
        height: 22
        from: root.from
        to: root.to
        onMoved: root.moved(value)

        // Follow `root.value` only while the user isn't holding the handle,
        // and for a moment after they let go. A plain `value: root.value`
        // binding is severed the first time the Slider writes to value, so
        // the handle and the numeric readout drifted apart permanently once
        // you had dragged; re-binding on release keeps them together, and
        // the settle window stops a poll that was already in flight from
        // snapping the handle back to a pre-drag value.
        Binding on value {
            value: root.value
            when: !sl.pressed && !settleTimer.running
            restoreMode: Binding.RestoreNone
        }
        onPressedChanged: if (!pressed) settleTimer.restart()

        background: Rectangle {
            x: sl.leftPadding
            y: sl.topPadding + sl.availableHeight / 2 - height / 2
            width: sl.availableWidth
            height: 5
            radius: 2.5
            color: Theme.border
            Rectangle {
                width: sl.visualPosition * parent.width
                height: parent.height
                radius: 2.5
                color: root.sliderColor
            }
        }
        handle: SliderHandle {
            x: sl.leftPadding + sl.visualPosition * (sl.availableWidth - width)
            y: sl.topPadding + sl.availableHeight / 2 - height / 2
            accentColor: root.sliderColor
            active: sl.pressed || sl.hovered
        }
    }

    Timer { id: settleTimer; interval: 600 }

    Text {
        id: valueText
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: 46
        horizontalAlignment: Text.AlignRight
        // The slider's own value, not root.value — so the number always
        // agrees with where the handle actually is, including mid-drag.
        text: Math.round(sl.value) + root.suffix
        color: Theme.textDim
        font.pixelSize: 11
    }
}
