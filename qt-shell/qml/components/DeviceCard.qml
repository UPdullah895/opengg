import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// One device's summary row in the master list — image, trimmed name,
// connection badge, device type. Selects a device; it is deliberately NOT a
// control surface any more.
//
// Devices regression pass: DPI/polling-rate dropdowns and the button-editor
// entry point used to live in here, which made the card's height depend on how
// many controls a given device happened to report. Two cards side by side in
// Grid view therefore rendered at different heights off the same row, and the
// card's own `implicitHeight` was derived from a child that was simultaneously
// anchored to fill it — a fragile arrangement that showed up as cards visibly
// overlapping their neighbours. Those controls now live in DeviceDetailPanel,
// and this card is a FIXED height regardless of device type or content, so a
// row of them can never be ragged and nothing can overlap.
Rectangle {
    id: root

    required property var modelData
    /// Drawn as selected; the owning page decides which device that is.
    property bool selected: false

    signal clicked()

    // Fixed, uniform, content-independent. A headset (fewer fields) and a
    // mouse (more) must occupy exactly the same box.
    implicitHeight: 72

    radius: Theme.radius
    color: root.selected ? Theme.accentAlpha(10)
                         : (hover.hovered ? Theme.bgHover : Theme.surface)
    border.width: 1
    border.color: root.selected ? Theme.accent : Theme.border

    function iconFor(type) {
        return type === "headset" ? "headphones" : "mouse"
    }

    HoverHandler { id: hover }
    TapHandler { onTapped: root.clicked() }

    // `imagePath` returns "" when there is genuinely nothing to show (an
    // unrecognized deviceType has no bundled silhouette). Appending the
    // cache-busting query to that empty string would produce a bare
    // "?v=0", which QML resolves *relative to this QML file* and then tries
    // to decode the .qml source as an image — "Unsupported image format".
    // Keep the empty case empty.
    readonly property string imageSource: {
        var p = DeviceController.imagePath(root.modelData.vid, root.modelData.pid,
                                           root.modelData.deviceType)
        return p === "" ? "" : p + "?v=" + DeviceController.photoRevision
    }

    RowLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 12

        // Device image (per-user photo if one was saved, else the bundled
        // silhouette), falling back to a line glyph so a card is never blank.
        Item {
            Layout.preferredWidth: 40
            Layout.preferredHeight: 40
            Layout.alignment: Qt.AlignVCenter

            Image {
                id: deviceImage
                anchors.fill: parent
                fillMode: Image.PreserveAspectFit
                // `cache: false` + photoRevision — see that qproperty's doc
                // comment in device.rs: the button editor can replace this
                // device's photo while the card is showing, and the resolved
                // file:// URL is otherwise unchanged, so nothing would
                // otherwise tell this Image to reload.
                cache: false
                source: root.imageSource
                visible: source !== "" && status === Image.Ready
            }
            Icon {
                anchors.centerIn: parent
                name: root.iconFor(root.modelData.deviceType)
                size: 22
                color: Theme.textDim
                visible: !deviceImage.visible
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            // Must not exceed the card; without this the column would grow to
            // its content and push text past the card's edge instead of
            // eliding inside it. (image 40 + spacing 12 + margins 2*12.)
            Layout.maximumWidth: root.width - 76
            spacing: 3

            Text {
                // Gets the whole first line to itself. The connection badge
                // deliberately does NOT share this row: in Grid view a card is
                // only ~200px wide, and a badge competing for that row squeezed
                // the name down to "Logi…" / "Raze…" — technically elided, but
                // useless. The badge belongs with the type below, which is
                // short and leaves room.
                //
                // Trimmed to brand+model; the type line already says "mouse"
                // and the badge already says how it is attached, so the
                // vendor's marketing tail is pure width cost. See
                // core/src/device_display.rs.
                text: DeviceController.displayName(root.modelData.name)
                color: Theme.text
                font.pixelSize: 14
                font.weight: Font.DemiBold
                elide: Text.ElideRight
                maximumLineCount: 1
                Layout.fillWidth: true
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 6

                Text {
                    // The raw `usb:vid:pid` model string is deliberately NOT
                    // here — it is an internal identifier with no meaning to a
                    // normal user. It lives behind the detail panel's
                    // "Advanced" disclosure instead.
                    text: root.modelData.deviceType
                    color: Theme.textMuted
                    font.pixelSize: 11
                    elide: Text.ElideRight
                    maximumLineCount: 1
                }
                ConnectionBadge {
                    connection: root.modelData.connection || ""
                    Layout.alignment: Qt.AlignVCenter
                }
                Item { Layout.fillWidth: true }
            }
        }
    }
}
