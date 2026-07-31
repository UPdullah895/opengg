import QtQuick

// Mouse-wheel accelerator for a ScrollView.
//
// Qt's stock Flickable wheel step works out to about 72px per notch here
// (measured), so getting down a long page — Storage, Extensions, the
// Dashboard changelog — took a dozen-plus notches and felt like the app was
// resisting. `pixelsPerNotch` below is the ONE place that speed is set for
// the whole app — every page passes its own `flick` but leaves this at the
// default, so there's exactly one number to tune, not six. (170 was tried
// first and judged too fast; 100 is the current value — comfortably quicker
// than stock without feeling like it's throwing you down the page.)
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
    property real pixelsPerNotch: 100

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
