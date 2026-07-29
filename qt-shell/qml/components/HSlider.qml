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
        from: root.from
        to: root.to
        value: root.value
        onMoved: root.moved(value)

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
        handle: Rectangle {
            x: sl.leftPadding + sl.visualPosition * (sl.availableWidth - width)
            y: sl.topPadding + sl.availableHeight / 2 - height / 2
            width: 16; height: 16; radius: 8
            color: Theme.text
            border.width: 2
            border.color: root.sliderColor
        }
    }

    Text {
        id: valueText
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: 46
        horizontalAlignment: Text.AlignRight
        text: Math.round(root.value) + root.suffix
        color: Theme.textDim
        font.pixelSize: 11
    }
}
