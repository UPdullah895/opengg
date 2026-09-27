import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Qt.labs.platform as Labs
import com.opengg.app

ApplicationWindow {
    id: root
    width: 1280
    height: 800
    // A floor for floating windows. Note this is only a HINT: tiling
    // compositors (Hyprland, sway) size windows from their own layout and
    // will happily go below it, so components must still degrade on their
    // own rather than relying on this — see ChannelStrip's `compact`.
    minimumWidth: 880
    minimumHeight: 520
    visible: true
    title: "OpenGG"
    color: Theme.bg

    // Frameless window with no decorations
    flags: Qt.Window | Qt.FramelessWindowHint

    // Current navigation page
    property string currentPage: "home"

    /// Video is filling the view — the editor's full view or an expanded
    /// preview. Hides the nav rail so nothing eats into the picture.
    readonly property bool immersive:
        (root.currentPage === "editor"
            && (editorPage.theaterMode || editorPage.navHidden))
        || clipsPage.playerExpanded

    // Settings → General → "Minimize to Tray" was persisted but nothing
    // ever read it — the close button always fully quit regardless of the
    // toggle. `s` mirrors SettingsController's JSON the same way every
    // settings panel does, just at the root so the close handler below can
    // see it.
    property var s: JSON.parse(SettingsController.settingsJson || "{}")
    Connections {
        target: SettingsController
        function onSettingsJsonChanged() { root.s = JSON.parse(SettingsController.settingsJson || "{}") }
    }
    // Set right before a real quit (tray menu's Quit, or Ctrl+Q-style exits
    // if added later) so onClosing lets it through instead of hiding.
    property bool reallyQuit: false

    onClosing: (close) => {
        if (root.s.runInBackground && !root.reallyQuit) {
            close.accepted = false
            root.hide()
            return
        }
        // Safety net for Ear Blast Protection: if the app quits while a
        // channel is mid-duck, stopping the VU stream force-releases it
        // back to its original volume (see ear_blast::release_all)
        // instead of leaving PipeWire's real sink volume stuck at the
        // duck target forever.
        AudioController.stopVuStream()
    }

    Labs.SystemTrayIcon {
        id: trayIcon
        visible: true
        icon.name: "opengg"
        tooltip: "OpenGG"
        onActivated: (reason) => {
            if (reason === Labs.SystemTrayIcon.Trigger) {
                root.visible ? root.hide() : (root.show(), root.raise(), root.requestActivate())
            }
        }
        menu: Labs.Menu {
            Labs.MenuItem {
                text: (I18n.language, I18n.t("tray.show"))
                onTriggered: { root.show(); root.raise(); root.requestActivate() }
            }
            Labs.MenuItem {
                text: (I18n.language, I18n.t("tray.quit"))
                onTriggered: { root.reallyQuit = true; root.close() }
            }
        }
    }

    // RTL layout mirroring driven by the active language (§3.2). childrenInherit
    // propagates it down the whole tree; L4-exempt subtrees opt back out locally.
    LayoutMirroring.enabled: I18n.rtl
    LayoutMirroring.childrenInherit: true

    Component.onCompleted: {
        ThemeController.reload()
        SettingsController.refresh()
        // Restore saved audio state once per app start: per-channel output
        // devices and app→channel links. Deliberately here and not in
        // MixerPage — hydration must happen whether or not the user ever
        // opens the Mixer, and it must happen exactly once, not on every
        // visit to that page. Skipped under the screenshot harness, which
        // must not mutate the developer's real routing.
        if (!ScreenshotController.active)
            AudioController.hydrate()
        if (ScreenshotController.active && ScreenshotController.page.length > 0)
            root.currentPage = ScreenshotController.page
        // The editor is only reachable by clicking a clip, which left it
        // outside the capture harness entirely. For `--page editor`, `--panel`
        // carries the clip path to open instead of a sub-panel name.
        if (ScreenshotController.active && ScreenshotController.page === "editor"
                && ScreenshotController.panel.length > 0)
            editorPage.clip = { filepath: ScreenshotController.panel,
                                title: ScreenshotController.panel }
        // --with-tour opens the overlay directly. This used to hang off
        // SettingsController's change signal, which never arrives in a capture
        // run — so every `--with-tour` screenshot silently produced a plain
        // page instead of the tour.
        if (ScreenshotController.active && ScreenshotController.withTour)
            TourController.start()
    }

    // First-launch guided tour: waits for the first real settingsJson load, then
    // starts the tour once (a short delay so the window has settled) unless the
    // user already dismissed it with "don't show again" (settings.tutorialSeen).
    property bool tourChecked: false
    Connections {
        target: SettingsController
        function onSettingsJsonChanged() {
            if (root.tourChecked)
                return
            root.tourChecked = true
            // In a screenshot run the tour would cover whatever page we were
            // asked to capture, so it is suppressed unless --with-tour asked
            // for it explicitly (in which case it is started unconditionally,
            // regardless of tutorialSeen).
            // Capture runs handle the tour in Component.onCompleted above.
            if (ScreenshotController.active)
                return
            // settingsJson IS the inner `settings` object (see settings.rs
            // refresh(), which publishes v["settings"]) — there is no extra
            // `.settings` hop. Reading `s.settings.tutorialSeen` here made
            // `seen` permanently false, so the tour re-launched on every start
            // even after "don't show again".
            var s = JSON.parse(SettingsController.settingsJson || "{}")
            if (!s.tutorialSeen)
                tourStartTimer.start()
        }
    }

    // ── Dev-only headless capture (UI-fidelity plan Phase 0) ──────────────
    // Renders offscreen, grabs the window to a PNG, quits. Never runs unless
    // --screenshot was passed. `grabToImage` needs a live scene graph, so this
    // hangs off the settle Timer rather than Component.onCompleted.
    Timer {
        running: ScreenshotController.active
        interval: ScreenshotController.delayMs
        onTriggered: {
            var ok = captureRoot.grabToImage(function (result) {
                var path = ScreenshotController.outputPath
                if (result.saveToFile(path))
                    ScreenshotController.report(true, path)
                else
                    ScreenshotController.report(false, "saveToFile rejected " + path)
                Qt.callLater(Qt.quit)
            })
            // grabToImage returns false when the item has no renderable size or
            // no scene graph — surface that instead of hanging until timeout.
            if (!ok) {
                ScreenshotController.report(false,
                    "grabToImage refused; visible=" + root.visible
                    + " captureRoot=" + captureRoot.width + "x" + captureRoot.height
                    + " win=" + root.width + "x" + root.height)
                Qt.callLater(Qt.quit)
            }
        }
    }
    Timer { id: tourStartTimer; interval: 700; onTriggered: TourController.start() }

    // The tour drives page navigation through the same currentPage model used
    // by Sidebar's `navigate` signal (no router in this shell either).
    Connections {
        target: TourController
        function onNavigateRequested(page) { root.currentPage = page }
    }

    // Explicit QML-declared capture root. `Window.contentItem` cannot be used:
    // it is constructed by QQuickWindow in C++ and therefore has no associated
    // QQmlEngine, which makes `grabToImage()` refuse it outright (silently, with
    // no warning). Everything visible must live inside this Item so screenshots
    // capture the whole UI.
    Item {
        id: captureRoot
        anchors.fill: parent

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            Titlebar {
                Layout.fillWidth: true
                Layout.preferredHeight: Theme.titlebarH   // --titlebar-h
            }

            RowLayout {
                spacing: 0
                Layout.fillWidth: true
                Layout.fillHeight: true

                // The editor's full view and the preview's expand used to
                // leave the nav rail on screen, so "maximise" stopped ~200px
                // short of the window edge. Collapse it for both.
                Sidebar {
                    Layout.fillHeight: true
                    visible: !root.immersive
                    Layout.preferredWidth: root.immersive ? 0 : Theme.sidebarW
                    // The editor is reached from Clips and has no nav entry of
                    // its own, so keep Clips lit while it's open rather than
                    // leaving nothing selected.
                    currentPage: root.currentPage === "editor" ? "clips" : root.currentPage
                    onNavigate: (p) => root.currentPage = p
                }

                StackLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    currentIndex: ["home", "mixer", "clips", "devices", "settings", "editor"]
                                  .indexOf(root.currentPage)

                    HomePage {
                        onNavigate: (p) => root.currentPage = p
                        onNavigateSettings: (section) => settingsPage.active = section
                        onPreviewClipRequested: (fp, t) => clipsPage.playerClip = { filepath: fp, title: t }
                    }
                    MixerPage {}
                    ClipsPage {
                        id: clipsPage
                        onEditClipRequested: (c) => {
                            editorPage.clip = c
                            root.currentPage = "editor"
                        }
                    }
                    DevicesPage {}
                    SettingsPage { id: settingsPage }
                    ClipEditorPage {
                        id: editorPage
                        onClosed: {
                            root.currentPage = "clips"
                            editorPage.clip = null
                        }
                    }
                }
            }
        }

        TourOverlay {}

        // Dev-only icon contact sheet (--page icons). Inside captureRoot so it
        // is screenshottable; never instantiated in a normal run.
        Loader {
            anchors.fill: parent
            active: ScreenshotController.active && ScreenshotController.page === "icons"
            sourceComponent: IconGallery {}
        }
    }
}
