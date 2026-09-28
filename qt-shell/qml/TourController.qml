pragma Singleton
import QtQuick
import com.opengg.app

// TourController — guided-tour state machine (QML port of useTour.ts + tour/steps.ts).
// Pure UI-only state (no daemon/audio data of its own), so unlike the rest of this
// migration's shared logic it lives entirely in QML rather than a cxx-qt QObject —
// there is no "core" behaviour here to share with a second frontend.
//
// The Vue original's `editorTracks`/`editorFilters` steps (multi-track timeline +
// per-track filters, gated on a real clip existing) are deliberately NOT ported:
// that UI doesn't exist yet in the trim-only Phase 5 slice (see TrimEditor.qml's
// header comment) — there is nothing to point the tour at. Likewise the Vue
// original's `condition()` mechanism (used only by those two steps) is dropped
// entirely; every step in this list is always visible.
QtObject {
    id: root

    readonly property var steps: [
        { id: "welcome", page: "home", deep: true },
        { id: "nav", page: "home", target: "nav-mixer", placement: "right", deep: true },
        { id: "dashboard", page: "home", target: "home-dashboard", placement: "bottom", deep: true },
        { id: "recorder", page: "home", target: "home-recorder", placement: "bottom", deep: true, cta: "recorderInstall" },
        { id: "mixerIntro", page: "mixer", target: "mixer-channels", placement: "bottom", deep: true },
        { id: "mixerAction", page: "mixer", target: "mixer-channels", placement: "top", deep: true, cta: "audioSetup", action: true },
        { id: "clips", page: "clips", target: "clips-grid", placement: "top", deep: true },
        { id: "devices", page: "devices", target: "devices-list", placement: "top", deep: true },
        { id: "settings", page: "settings", target: "nav-settings", placement: "right", deep: true },
        { id: "finish", page: "settings", target: "settings-replay", placement: "top" }
    ]

    property bool active: false
    property int stepIndex: 0
    property bool dontShowAgain: true

    readonly property var current: active ? steps[stepIndex] : null
    readonly property bool isFirst: stepIndex <= 0
    readonly property bool isLast: stepIndex >= steps.length - 1

    // Target registry: id -> Item. Populated by each page's Component.onCompleted
    // (all pages are eagerly instantiated by Main.qml's StackLayout, so every
    // target is already registered before the tour can possibly start).
    property var targets: ({})
    function registerTarget(id, item) {
        var t = targets
        t[id] = item
        targets = t
    }
    function unregisterTarget(id) {
        if (targets[id] === undefined)
            return
        var t = targets
        delete t[id]
        targets = t
    }

    signal navigateRequested(string page)

    function go(i) {
        if (i < 0 || i >= steps.length)
            return
        stepIndex = i
        navigateRequested(steps[i].page)
    }
    function start() {
        active = true
        go(0)
    }
    function next() {
        if (isLast)
            finish()
        else
            go(stepIndex + 1)
    }
    function back() {
        if (!isFirst)
            go(stepIndex - 1)
    }
    function skip() { finish() }
    function finish() {
        if (dontShowAgain)
            SettingsController.setValue("tutorialSeen", "true")
        active = false
    }
}
