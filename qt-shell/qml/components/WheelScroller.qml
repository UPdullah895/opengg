import QtQuick

// Mouse-wheel accelerator for a ScrollView.
//
// Qt's stock Flickable wheel step works out to about 72px per notch here
// (measured), so getting down a long page — Storage, Extensions, the
// Dashboard changelog — took a dozen-plus notches and felt like the app was
// resisting. This scales one notch up to roughly what a browser does.
//
// Two constraints shaped the implementation:
//
//  * It must sit *above* the ScrollView, not inside it. Flickable filters
//    wheel events away from its own children before a handler down there
//    ever sees them.
//  * It has to be a MouseArea rather than a WheelHandler. A WheelHandler
//    over the ScrollView is constructed and bound correctly but simply never
//    receives the event (verified: the handler logged as alive with a valid
//    Flickable, and its onWheel never fired once). MouseArea participates in
//    the older wheel-delivery path and does get them.
//
// `acceptedButtons: Qt.NoButton` is what keeps this from swallowing clicks —
// the area takes wheel events only, and presses fall through to whatever is
// underneath.
MouseArea {
    id: area

    /// The ScrollView's Flickable — pass `<scrollViewId>.contentItem`.
    property Flickable flick: null
    property real pixelsPerNotch: 170

    acceptedButtons: Qt.NoButton
    propagateComposedEvents: true

    onWheel: (wheel) => {
        if (!area.flick) {
            wheel.accepted = false
            return
        }
        const max = Math.max(0, area.flick.contentHeight - area.flick.height)
        if (max <= 0) {
            wheel.accepted = false
            return
        }
        // Touchpads send pixelDelta and Flickable already scrolls those
        // smoothly; only take over the discrete notches of a real wheel.
        if (wheel.pixelDelta.y !== 0) {
            wheel.accepted = false
            return
        }
        const step = wheel.angleDelta.y / 120 * area.pixelsPerNotch
        area.flick.contentY = Math.max(0, Math.min(max, area.flick.contentY - step))
        wheel.accepted = true
    }
}
