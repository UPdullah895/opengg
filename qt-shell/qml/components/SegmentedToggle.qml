import QtQuick
import com.opengg.app

// A row of IconToggles fused into a single pill, for mutually exclusive
// choices that should read as one control rather than separate buttons —
// the Clips page's grid/list view mode being the case it was built for.
//
// Usage — segments are declared as `flat` IconToggles:
//
//     SegmentedToggle {
//         IconToggle { flat: true; icon: "grid"; active: mode === "grid"; ... }
//         IconToggle { flat: true; icon: "list"; active: mode === "list"; ... }
//     }
Item {
    id: seg

    /// Segments go here; declaring children of this component fills it.
    default property alias segments: inner.data

    /// Corner rounding of the pill as a whole.
    property real radius: Theme.radius

    implicitWidth: inner.implicitWidth
    implicitHeight: 32

    // Round the END segments' OUTER corners so the fills follow the pill.
    //
    // `clip: true` on a rounded Rectangle used to be relied on for this, and
    // it cannot do the job: Qt Quick clipping is RECTANGULAR — it clips to an
    // item's bounding box, never to its `radius`. The segments' square fills
    // therefore kept painting right into the rounded corners. Because the
    // active tint is translucent (Theme.accentAlpha), the rounded outline
    // showed through it while the square fill stayed visible around it —
    // the "coloured layer over a sharp-edged background" this fixes.
    //
    // Per-corner radii genuinely round the fill. They need Qt 6.7, which is
    // already this project's baseline: ChannelStrip.qml uses them, and
    // .github/workflows/ci.yml builds the shell in an Arch container for
    // exactly that reason. (The older note here claiming Debian stable's Qt
    // was the constraint predated that move.)
    function applyCorners() {
        const kids = inner.children
        for (let i = 0; i < kids.length; i++) {
            const first = i === 0
            const last = i === kids.length - 1
            kids[i].topLeftRadius     = first ? seg.radius : 0
            kids[i].bottomLeftRadius  = first ? seg.radius : 0
            kids[i].topRightRadius    = last  ? seg.radius : 0
            kids[i].bottomRightRadius = last  ? seg.radius : 0
        }
    }

    Component.onCompleted: seg.applyCorners()
    onRadiusChanged: seg.applyCorners()

    Row {
        id: inner
        anchors.fill: parent
        spacing: 0
        onChildrenChanged: seg.applyCorners()
    }

    // Divider between the halves. Anchored to the first segment's edge so it
    // lands correctly whatever the segments' widths are.
    Rectangle {
        visible: inner.children.length > 1
        width: 1
        color: Theme.border
        x: inner.children.length > 0 ? inner.children[0].width : 0
        anchors.top: parent.top
        anchors.bottom: parent.bottom
    }

    // The shared outline, declared LAST so it paints above the segments.
    // As a sibling *under* the segments it was simply covered by their
    // opaque fills, which is why the pair never read as one pill.
    Rectangle {
        anchors.fill: parent
        color: "transparent"
        radius: seg.radius
        border.width: 1
        border.color: Theme.border
    }
}
