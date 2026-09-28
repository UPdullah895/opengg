import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.opengg.app

// Themed in-app photo drop target — Devices regression pass.
//
// Replaces dropping the user straight into the OS file-chooser portal the
// instant they express interest in adding a photo. That portal is drawn by the
// desktop/portal implementation, not by this app, so it looks nothing like the
// rest of the UI, offers no drag-and-drop of its own, and reads as a dead end.
// This gives a real in-app step first: drag a file onto it, or fall back to
// Browse.
//
// The Browse fallback asks Qt for its own non-native chooser
// (`FileDialog.DontUseNativeDialog`, which does resolve on this Qt 6.11 build)
// so the fallback is at least Qt-drawn rather than portal-drawn — but a file
// chooser is still a separate top-level window either way, so drag-and-drop is
// the path this component is actually built around, not a nice-to-have.
Item {
    id: zone

    /// Emitted with a `file://` URL once something acceptable arrives, by
    /// either path. The caller does the saving — this component only collects.
    signal photoChosen(string url)

    property string title: "Drag a photo here"
    property string subtitle: "or use Browse to pick one. It stays on this computer."
    property string browseLabel: "Browse…"

    implicitHeight: 132

    readonly property var acceptedExtensions: ["png", "jpg", "jpeg", "webp", "bmp"]

    function extensionOf(url) {
        var s = url.toString()
        var q = s.indexOf("?")
        if (q !== -1) s = s.substring(0, q)
        var dot = s.lastIndexOf(".")
        return dot === -1 ? "" : s.substring(dot + 1).toLowerCase()
    }
    function isAcceptable(url) {
        return zone.acceptedExtensions.indexOf(zone.extensionOf(url)) !== -1
    }

    /// Set when a drag carries something this zone cannot use, so the user is
    /// told why nothing happened instead of the drop silently doing nothing.
    property bool rejecting: false

    FileDialog {
        id: browseDialog
        title: "Choose a photo"
        nameFilters: ["Images (*.png *.jpg *.jpeg *.webp *.bmp)"]
        // Ask Qt for its own Quick-drawn chooser rather than the platform
        // portal. Verified present on this Qt build (the enum resolves); if a
        // given platform ignores it and forces the portal anyway, the drop
        // path above is unaffected.
        options: FileDialog.DontUseNativeDialog
        onAccepted: zone.photoChosen(selectedFile.toString())
    }

    Rectangle {
        id: surface
        anchors.fill: parent
        radius: Theme.radiusLg
        color: dropArea.containsDrag && !zone.rejecting ? Theme.accentAlpha(10) : Theme.bgDeep
        border.width: 1
        border.color: dropArea.containsDrag
                      ? (zone.rejecting ? Theme.danger : Theme.accent)
                      : Theme.border

        ColumnLayout {
            anchors.centerIn: parent
            width: parent.width - 40
            spacing: 8

            Icon {
                Layout.alignment: Qt.AlignHCenter
                name: "camera"
                size: 24
                color: dropArea.containsDrag && !zone.rejecting ? Theme.accent : Theme.textMuted
            }
            Text {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                text: zone.rejecting ? "That file isn't an image" : zone.title
                color: zone.rejecting ? Theme.danger : Theme.text
                font.pixelSize: 13
                font.weight: Font.DemiBold
            }
            Text {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                text: zone.rejecting
                      ? "PNG, JPEG, WebP or BMP only."
                      : zone.subtitle
                color: Theme.textMuted
                font.pixelSize: 11
            }
            ClipsBarButton {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 2
                label: zone.browseLabel
                icon: "folder"
                onTriggered: browseDialog.open()
            }
        }
    }

    DropArea {
        id: dropArea
        anchors.fill: parent

        onEntered: (drag) => {
            // Decide up front whether this drag is usable, so the zone can
            // show the answer during the hover rather than after the drop.
            zone.rejecting = !(drag.hasUrls && drag.urls.length > 0
                               && zone.isAcceptable(drag.urls[0]))
            drag.accepted = !zone.rejecting
        }
        onExited: zone.rejecting = false
        onDropped: (drop) => {
            zone.rejecting = false
            if (!drop.hasUrls || drop.urls.length === 0)
                return
            var url = drop.urls[0].toString()
            if (!zone.isAcceptable(url))
                return
            drop.accept()
            zone.photoChosen(url)
        }
    }
}
