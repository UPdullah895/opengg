import QtQuick
import QtQuick.Shapes
import com.opengg.app

// Icon — renders one entry from the Icons registry as a stroked (or filled)
// vector, generalising the pattern Sidebar.qml already proved: Shape/ShapePath/
// PathSvg with round caps and joins, which is exactly the feather/lucide stroke
// style the Vue UI uses.
//
// Usage:
//   Icon { name: "volume-x" }
//   Icon { name: "trash"; size: 14; color: Theme.danger }
//   Icon { name: "heart"; filled: clip.favorite }
//
// `size` is the rendered box in px. Keep it an INTEGER: fractional text sizing
// is a known hang on this Qt version (migration plan R1) and fractional icon
// geometry invites the same class of layout/AA surprise for no benefit.
Item {
    id: root

    /// Key into Icons.defs.
    property string name: ""
    /// Rendered edge length, px. The source geometry is a 24x24 viewBox and is
    /// scaled to fit — matching how a browser renders width:Npx on that SVG,
    /// stroke included.
    property int size: 16
    property color color: Theme.text
    /// Defaults to the source SVG's stroke-width; override to restyle.
    property real strokeWidth: Icons.strokeWidth(root.name)
    /// Defaults to the source SVG's fill; override for toggle states (e.g. a
    /// favourite heart that fills when active).
    property bool filled: Icons.isFilled(root.name)

    implicitWidth: size
    implicitHeight: size

    Shape {
        // Fixed 24x24 authoring space, then uniformly scaled to `size`.
        width: 24
        height: 24
        scale: root.size / 24
        transformOrigin: Item.TopLeft
        antialiasing: true
        // Keep the icon centred if the Item was given a larger explicit size.
        x: (root.width - root.size) / 2
        y: (root.height - root.size) / 2

        ShapePath {
            strokeColor: root.filled ? "transparent" : root.color
            strokeWidth: root.filled ? 0 : root.strokeWidth
            fillColor: root.filled ? root.color : "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            // All subpaths are pre-joined into one `d` string by Icons.qml: a
            // Repeater cannot generate ShapePaths (ShapePath is not an Item).
            PathSvg { path: Icons.path(root.name) }
        }
    }
}
