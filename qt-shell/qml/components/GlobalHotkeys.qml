import QtQuick
import com.opengg.app

/// Global (system-wide) shortcuts.
///
/// The daemon owns the evdev listener — it is the only part that can see key
/// presses while another window has focus, on X11 and Wayland alike — and
/// reports each recognised combo back over D-Bus. The actions run here
/// because the `gpu-screen-recorder` process belongs to this app, not to the
/// daemon (see `core/src/hotkeys.rs`).
///
/// In-app-only shortcuts (undo, split, export) are deliberately not routed
/// through this: they belong to whichever view has focus, not to a
/// system-wide grab.
QtObject {
    id: hk

    /// The app's stored shortcut bindings, as the Shortcuts panel writes them.
    property var shortcuts: ({})

    /// The built-in bindings. `ui-settings.json` has no `shortcuts` key until
    /// the user edits one, so without this fallback a fresh install would
    /// push three empty strings and *clear* the daemon's defaults — the
    /// hotkeys would go dead precisely on the machines that never touched
    /// them.
    readonly property var fallback:
        JSON.parse(SettingsController.defaultShortcutsJson() || "{}")

    function binding(name) {
        const v = hk.shortcuts[name]
        return (v === undefined || v === null) ? (hk.fallback[name] || "") : v
    }

    /// True when the daemon confirmed it is listening. False means the keys
    /// will not fire — usually because this user is not in the `input` group.
    readonly property bool active: HotkeyController.active
    readonly property string error: HotkeyController.error

    function push() {
        HotkeyController.pushBindings(hk.binding("saveReplay"),
                                      hk.binding("toggleRecording"),
                                      hk.binding("screenshot"))
    }

    function dispatch(action) {
        switch (action) {
        case "saveReplay":
            // Already notifies on the desktop, which matters here: the window
            // is usually not visible when this fires.
            RecordingController.save()
            break
        case "toggleRecording":
            if (RecordingController.running)
                RecordingController.stop()
            else
                RecordingController.start()
            break
        default:
            // "screenshot" lands here. Nothing in this project takes a
            // desktop screenshot yet (`media.takeScreenshot` grabs a frame
            // out of an existing clip, which is an editor feature), so the
            // binding is carried end to end but has nothing to run.
            console.warn("Global hotkey '" + action + "' has no handler")
        }
    }

    property var _conn: Connections {
        target: HotkeyController
        // pressCount, not lastAction: pressing the same hotkey twice leaves
        // lastAction unchanged and emits no change signal.
        function onPressCountChanged() { hk.dispatch(HotkeyController.lastAction) }
    }

    /// Re-push whenever the user edits a binding, so the daemon never runs on
    /// a stale copy.
    onShortcutsChanged: hk.push()

    Component.onCompleted: {
        HotkeyController.start()
        hk.push()
    }
}
