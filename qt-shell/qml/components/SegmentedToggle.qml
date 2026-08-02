import QtQuick
import com.opengg.app

// A row of IconToggles fused into a single pill, for mutually exclusive
// choices that should read as one control rather than separate buttons —
// the Clips page's grid/list view mode being the case it was built for.
//
// The rounded outline lives here, on a clipping parent, rather than on the
// segments themselves. Per-corner radii (topLeftRadius etc.) would express
// this more directly but need Qt 6.7, and the distro matrix builds against
// Debian stable's older Qt; clipping works everywhere.
//
// Usage — segments are declared as `flat` IconToggles:
//
//     SegmentedToggle {
//         IconToggle { flat: true; icon: "grid"; active: mode === "grid"; ... }
//         IconToggle { flat: true; icon: "list"; active: mode === "list"; ... }
//     }
Rectangle {
    id: seg

    /// Segments go here; declaring children of this component fills it.
    default property alias segments: inner.data

    implicitWidth: inner.implicitWidth
    implicitHeight: 32
    radius: Theme.radius
    // The segments paint their own backgrounds; this only supplies the
    // outline and the rounded ends.
    color: "transparent"
    border.width: 1
    border.color: Theme.border
    clip: true

    Row {
        id: inner
        anchors.fill: parent
        spacing: 0
    }

    // Divider between the two halves. Anchored to the first segment's edge so
    // it lands correctly whatever the segments' widths are, and drawn above
    // them so a segment's active tint cannot paint over it.
    Rectangle {
        visible: inner.children.length > 1
        width: 1
        color: Theme.border
        x: inner.children.length > 0 ? inner.children[0].width : 0
        anchors.top: parent.top
        anchors.bottom: parent.bottom
    }
}
