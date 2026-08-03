import QtQuick
import QtQuick.Layouts
import com.opengg.app

// A settings-panel section card: title (+ optional info tooltip and header
// extra, e.g. a toggle) above a full-width divider, then arbitrary content.
//
// This is the Ear Blast Protection card's shape (MixerRoutingPanel.qml) —
// the one card in the settings pages that already had the title-divider
// pattern — pulled out into a reusable component so every OTHER card gets
// it too instead of a bare Text. Also full-width rather than capped at a
// fixed pixel width: a fixed cap left a large dead gap on wide windows,
// which is what made the far-right-aligned controls in some cards (Daemon &
// Startup's toggles) look inconsistent with others depending on window size,
// even though the controls themselves were already correctly right-aligned
// within their card.
//
// Cards that need a control in the header itself (Ear Blast Protection's
// toggle is the one case in this codebase) keep their own bespoke header
// row instead of using this component — reparenting an externally-declared
// Item into a named property is fragile in QML, and one bespoke card is a
// smaller risk than that mechanism.
//
// Usage:
//     SettingsCard {
//         title: "Theme File"
//         infoText: "..."               // omit for no tooltip icon
//         // children become the card body, below the divider
//     }
Rectangle {
    id: card

    property string title: ""
    property string infoText: ""

    /// Card body; declaring children of this component fills it, same
    /// convention as SegmentedToggle's `segments`.
    default property alias content: bodyCol.data

    Layout.fillWidth: true
    radius: Theme.radius
    color: Theme.surface
    border.width: 1
    border.color: Theme.border
    implicitHeight: outerCol.implicitHeight + 40

    ColumnLayout {
        id: outerCol
        anchors.fill: parent
        anchors.margins: 20
        spacing: 14

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Text {
                text: card.title
                color: Theme.text
                font.pixelSize: 16
                font.weight: Font.DemiBold
            }
            InfoIcon { visible: card.infoText.length > 0; tooltipText: card.infoText }
            Item { Layout.fillWidth: true }
        }

        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

        ColumnLayout {
            id: bodyCol
            Layout.fillWidth: true
            spacing: 14
        }
    }
}
