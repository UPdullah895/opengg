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
    // The handle wears its own slider's colour rather than a neutral grey:
    // these tracks are colour-coded (per-channel faders, the Game→Chat
    // ChatMix gradient, per-band EQ), and a grey cap read as unrelated
    // chrome sitting on top of them.
    color: handle.accentColor
    border.width: 1
    border.color: handle.active ? Theme.text : Theme.scrim(30)

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
                // Dark on the coloured body, not tinted: a light grip on a
                // mid-tone accent had almost no contrast.
                color: Theme.scrim(handle.active ? 75 : 55)
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
                color: Theme.scrim(handle.active ? 75 : 55)
            }
        }
    }
}
