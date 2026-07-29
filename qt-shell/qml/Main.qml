import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

ApplicationWindow {
    id: root
    width: 1280
    height: 800
    visible: true
    title: "OpenGG"
    color: Theme.bg

    // Frameless window with no decorations
    flags: Qt.Window | Qt.FramelessWindowHint

    // Current navigation page
    property string currentPage: "home"

    // RTL layout mirroring driven by the active language (§3.2). childrenInherit
    // propagates it down the whole tree; L4-exempt subtrees opt back out locally.
    LayoutMirroring.enabled: I18n.rtl
    LayoutMirroring.childrenInherit: true

    Component.onCompleted: {
        ThemeController.reload()
        SettingsController.refresh()
        if (ScreenshotController.active && ScreenshotController.page.length > 0)
            root.currentPage = ScreenshotController.page
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
            if (ScreenshotController.active) {
                if (ScreenshotController.withTour)
                    TourController.start()
                return
            }
            var s = JSON.parse(SettingsController.settingsJson || "{}")
            var seen = !!(s.settings && s.settings.tutorialSeen)
            if (!seen)
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
                Layout.preferredHeight: 40
            }

            RowLayout {
                spacing: 0
                Layout.fillWidth: true
                Layout.fillHeight: true

                Sidebar {
                    Layout.fillHeight: true
                    currentPage: root.currentPage
                    onNavigate: (p) => root.currentPage = p
                }

                StackLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    currentIndex: ["home", "mixer", "clips", "devices", "settings"].indexOf(root.currentPage)

                    HomePage {}
                    MixerPage {}
                    ClipsPage {}
                    DevicesPage {}
                    SettingsPage {}
                }
            }
        }

        TourOverlay {}
    }
}
