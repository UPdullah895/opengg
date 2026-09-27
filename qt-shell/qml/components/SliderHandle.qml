import QtQuick
import com.opengg.app

// The grab handle shared by every slider in the app.
//
// Replaces the white disc with a red ring that each slider used to draw for
// itself. That shape read as a stray light-mode dot on OpenGG's dark surfaces
// and gave no hint that it could be dragged; this is a dark, bordered slab
// with two grip lines — the same affordance a mixer fader cap uses.
//
// Declares implicit sizes (NOT width/height): a QQC2 Control computes its own
// implicit size from its handle's, and a handle that reports 0 leaves the
// Slider 0px tall and unclickable. See the note in HSlider.qml.
Rectangle {
    id: handle

    /// Set on a VERTICAL slider, where the handle lies across the track and
    /// the grip lines run with it.
    property bool vertical: false
    /// The slider's own colour — drives the border and grips while active, so
    /// per-channel faders and the EQ bands keep their identity.
    property color accentColor: Theme.accent
    /// Pressed or hovered: lifts the border and grips to `accentColor`.
    property bool active: false

    implicitWidth: handle.vertical ? 22 : 14
    implicitHeight: handle.vertical ? 14 : 22

    radius: 4
    color: handle.active ? Theme.bgHover : Theme.bgInput
    border.width: 1
    border.color: handle.active ? handle.accentColor : Theme.border

    // Two grip lines, across the drag axis so they read as ridges to push.
    Row {
        anchors.centerIn: parent
        spacing: 3
        visible: !handle.vertical
        Repeater {
            model: 2
            Rectangle {
                width: 1
                height: 9
                color: handle.active ? handle.accentColor : Theme.textDim
            }
        }
    }
    Column {
        anchors.centerIn: parent
        spacing: 3
        visible: handle.vertical
        Repeater {
            model: 2
            Rectangle {
                width: 9
                height: 1
                color: handle.active ? handle.accentColor : Theme.textDim
            }
        }
    }
}
