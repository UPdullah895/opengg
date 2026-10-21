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
        case "screenshot":
            // A desktop screenshot via the session's own tool (grim,
            // spectacle, gnome-screenshot, maim…) — see core::desktop_shot.
            RecordingController.screenshot()
            break
        default:
            console.warn("Global hotkey '" + action + "' has no handler")
        }
    }

    // Recorder health: restarts a crashed or post-suspend-stale replay
    // buffer when "Auto-restart" is on. Lives here because this object
    // exists for the whole app lifetime, whatever page is showing. Inert
    // under the screenshot harness, which must not spawn recorders.
    property var _supervisor: Timer {
        interval: 3000
        repeat: true
        running: !ScreenshotController.active
        onTriggered: RecordingController.supervise(
            !!JSON.parse(SettingsController.settingsJson || "{}").gsrAutoRestart)
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
        // "Start replay buffer with OpenGG" (gsrAutoStart) — read nowhere
        // before this, so the toggle did nothing.
        if (!ScreenshotController.active)
            RecordingController.autoStart()
    }
}
