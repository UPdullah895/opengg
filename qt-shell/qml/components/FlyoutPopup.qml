import QtQuick
import QtQuick.Controls
import QtQuick.Window
import com.opengg.app

// A Popup that a button can genuinely toggle, with the panel chrome the
// shell's flyouts all share.
//
// The landmine this exists for: with the default CloseOnPressOutside, a
// press on the button that opened the popup is already "outside" it, so the
// popup closes itself before the button's own click handler runs. A plain
// `visible ? close() : open()` then sees a closed popup and reopens it —
// the flyout appears to ignore every second click. Guarding on the time of
// the last close is what makes the second click actually dismiss it.
//
// It also flips above its anchor when there is no room below, which is what
// kept the editor's per-track volume fader from opening off the bottom of
// the window.
Popup {
    id: flyout

    /// The item the popup hangs off — usually the button that opens it.
    /// Used for placement; defaults to the popup's parent.
    property Item anchorItem: flyout.parent
    /// Gap between the anchor and the popup.
    property real gap: 4
    /// Open upward when there isn't room below. Set false to always drop down.
    property bool flipWhenClipped: true
    /// Centre horizontally on the anchor instead of aligning to its left.
    property bool centerOnAnchor: false

    /// Window height, read through the ANCHOR rather than the popup. The
    /// Window attached property only works on Items, and a Popup is not one
    /// ("Window.window only supports types derived from Item").
    readonly property real windowHeight:
        flyout.anchorItem ? (flyout.anchorItem.Window.height || 0) : 0

    /// True when the popup had to flip above its anchor.
    ///
    /// Deliberately NOT a binding. `mapToItem()` is a plain function call:
    /// it does not re-evaluate when the item it measures moves, so a bound
    /// version latched at whatever the layout happened to be during
    /// construction — which is before anything has been positioned, so it
    /// always read "plenty of room" and never flipped. Recomputed at the
    /// moment the popup is about to show, when the geometry is real.
    property bool flipped: false

    function recomputeFlip() {
        if (!flyout.flipWhenClipped || !flyout.anchorItem || flyout.windowHeight <= 0) {
            flyout.flipped = false
            return
        }
        // Distance from the anchor's bottom edge to the foot of the window.
        const below = flyout.anchorItem.mapToItem(null, 0, flyout.anchorItem.height).y
        const room = flyout.windowHeight - below
        flyout.flipped = room < flyout.implicitHeight + flyout.gap * 2
    }

    onAboutToShow: flyout.recomputeFlip()

    y: flyout.flipped
       ? -flyout.implicitHeight - flyout.gap
       : (flyout.anchorItem ? flyout.anchorItem.height : 0) + flyout.gap
    x: flyout.centerOnAnchor && flyout.anchorItem
       ? (flyout.anchorItem.width - flyout.width) / 2
       : 0

    padding: 8
    closePolicy: Popup.CloseOnPressOutside | Popup.CloseOnEscape

    background: Rectangle {
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
    }

    /// Timestamp of the last close, in ms. See the note above.
    property double lastClosedAt: 0
    onClosed: flyout.lastClosedAt = Date.now()

    /// What a toggle button should call. Never `visible ? close() : open()`.
    function toggle() {
        if (flyout.visible) {
            flyout.close()
        } else if (Date.now() - flyout.lastClosedAt > 200) {
            flyout.open()
        }
    }
}
