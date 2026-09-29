import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Devices — live device list from the openggd daemon (real D-Bus data).
//
// Devices regression pass: this is a master-detail layout. The left column is
// the device list in whichever view mode is selected (list/grid/carousel) and
// carries compact, uniformly-sized summary cards; the right column is a real
// DeviceDetailPanel for the selected device, which is what finally uses the
// page width that previously sat empty. Per-device controls belong to the
// detail panel — the list selects, it does not configure.
Rectangle {
    id: page
    color: Theme.bg

    // ── Headless-capture hooks ──────────────────────────────────────────────
    // ui-shots.sh drives these through ScreenshotController.panel (the same
    // generic per-page sub-state property MixerPage.qml uses for its tabs).
    // They exist because two of this page's acceptance criteria are otherwise
    // untestable in the offscreen harness: it cannot click, and the developer
    // machine only has two devices attached while the layout requirement is
    // specifically about three or more of differing types sharing a row.
    // Everything here is gated on ScreenshotController.active and is inert in
    // a normal run.
    readonly property bool shotMode: ScreenshotController.active
                                     && ScreenshotController.page === "devices"
    readonly property string shotPanel: page.shotMode ? ScreenshotController.panel : ""
    readonly property bool shotMany: page.shotPanel.indexOf("many") !== -1

    // Stand-ins with deliberately long names, so the capture exercises the
    // eliding and uniform-height rules rather than only the short real ones.
    readonly property var shotExtraDevices: [
        {
            id: "shot:demo:keeb", name: "Logitech G915 TKL LIGHTSPEED Wireless RGB Mechanical Gaming Keyboard",
            model: "usb:046d:408e:0", deviceType: "keyboard", vid: 1133, pid: 16526,
            connection: "wireless", capabilities: []
        },
        {
            id: "shot:demo:pad", name: "SteelSeries Arctis Nova Pro Wireless Gaming Headset",
            model: "usb:1038:12e0:0", deviceType: "headset", vid: 4152, pid: 4832,
            connection: "bluetooth", batteryLevel: 72, chatmix: 50,
            // The real capability list the connected Arctis Nova 7 reports,
            // so a capture actually exercises the headset control surface
            // rather than rendering an empty panel.
            capabilities: ["sidetone", "battery_status", "inactive_time", "chatmix_status",
                           "equalizer_preset", "equalizer", "microphone_mute_led_brightness",
                           "microphone_volume", "volume_limiter", "bt_when_powered_on",
                           "bt_call_volume"],
            eqPresets: { "flat": [0,0,0,0,0,0,0,0,0,0], "bass": [3.5,5.5,4,1,-1.5,-1.5,-1,-1,-1,-1],
                         "focus": [-5,-3.5,-1,-3.5,-2.5,4,6,-3.5,0,0],
                         "smiley": [3,3.5,1.5,-1.5,-4,-4,-2.5,1.5,3,4] }
        },
        {
            id: "shot:demo:mouse2", name: "Razer DeathAdder V3 Pro Wired Gaming Mouse",
            model: "usb:1532:00b6:0", deviceType: "mouse", vid: 5426, pid: 182,
            connection: "wired", capabilities: []
        }
    ]

    property var devices: {
        var real = DeviceController.devicesJson
            ? JSON.parse(DeviceController.devicesJson)
            : []
        return page.shotMany ? real.concat(page.shotExtraDevices) : real
    }

    /// `id` of the selected device. Kept as the id string rather than an
    /// index or an object reference because `devices` is re-parsed into fresh
    /// JS objects on every 3s refresh — an index would silently point at a
    /// different device when the list reorders, and an object reference would
    /// go stale immediately.
    property string selectedId: ""

    readonly property var selectedDevice: {
        for (var i = 0; i < page.devices.length; i++) {
            if (page.devices[i].id === page.selectedId)
                return page.devices[i]
        }
        return null
    }

    // Select the first device once data arrives, and recover if whatever was
    // selected disappears (unplugged, or re-grouped by a merge change).
    onDevicesChanged: {
        if (page.devices.length === 0) {
            page.selectedId = ""
            return
        }
        if (page.selectedDevice === null)
            page.selectedId = page.devices[0].id

        // A capture target ending in "headset" selects the first headset, so
        // the headset control surface can be swept without a click.
        if (page.shotPanel.indexOf("headset") !== -1) {
            for (var h = 0; h < page.devices.length; h++) {
                if (page.devices[h].deviceType === "headset") {
                    page.selectedId = page.devices[h].id
                    break
                }
            }
        }

        // Headless-capture hook (qt-shell/tools/ui-shots.sh's
        // `devices:buttons` target) — the button-mapping editor only opens
        // from a live click, which the offscreen harness cannot perform, so
        // this opens it deterministically for capture instead. Fires once,
        // the first time a "buttons"-capable device actually shows up, since
        // the device list arrives asynchronously over D-Bus.
        if (ScreenshotController.active && ScreenshotController.page === "devices"
                && ScreenshotController.panel === "buttons"
                && !page.screenshotButtonEditorOpened) {
            for (var j = 0; j < page.devices.length; j++) {
                var caps = page.devices[j].capabilities || []
                if (caps.indexOf("buttons") !== -1) {
                    buttonEditor.device = page.devices[j]
                    page.screenshotButtonEditorOpened = true
                    break
                }
            }
        }
    }
    property bool screenshotButtonEditorOpened: false

    // Persisted like any other real UI preference (SettingsController's
    // `settings.<key>` envelope — see GeneralPanel.qml's `defaultClickAction`
    // for the same pattern), not a page-local `property string`: the roadmap
    // for this feature explicitly asked for a setting that survives a restart.
    property var settingsObj: JSON.parse(SettingsController.settingsJson || "{}")
    // A capture target may force a view mode (`devices:grid`,
    // `devices:carousel-many`, …) so all three can be swept without mutating
    // the user's persisted preference; otherwise the saved setting wins.
    readonly property string deviceViewMode: {
        if (page.shotPanel.indexOf("grid") !== -1) return "grid"
        if (page.shotPanel.indexOf("carousel") !== -1) return "carousel"
        if (page.shotPanel.indexOf("list") !== -1) return "list"
        return settingsObj.deviceViewMode || "list"
    }
    Connections {
        target: SettingsController
        function onSettingsJsonChanged() {
            page.settingsObj = JSON.parse(SettingsController.settingsJson || "{}")
        }
    }
    function setViewMode(mode) {
        SettingsController.setValue("deviceViewMode", JSON.stringify(mode))
    }

    Component.onCompleted: {
        DeviceController.refresh()
        TourController.registerTarget("devices-list", page)
    }
    Component.onDestruction: TourController.unregisterTarget("devices-list")
    Timer { interval: 3000; running: true; repeat: true; onTriggered: DeviceController.refresh() }

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: 32
        anchors.rightMargin: 32
        anchors.topMargin: 28
        anchors.bottomMargin: 28
        spacing: 20

        // ── Header ──────────────────────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true

            Text {
                horizontalAlignment: Text.AlignLeft
                text: (I18n.language, I18n.t("nav.devices"))
                color: Theme.text
                // 20px / ExtraBold, matching the retired Vue PageHeader
                // (.page-title: font-size 20px; font-weight 800). The Qt port
                // had these at 26px Bold, which is most of why every page
                // header reads bulkier than the old shell.
                font.pixelSize: 20
                font.weight: Font.ExtraBold
                Layout.fillWidth: true
            }

            // View-mode switcher — same SegmentedToggle+IconToggle pair
            // ClipsPage's grid/list toggle uses, with a third segment.
            // "square" stands in for carousel: there is no dedicated carousel
            // glyph in Icons.qml and this reuses an existing icon rather than
            // adding a new asset for one button (see AGENTS.md's icon rule).
            SegmentedToggle {
                IconToggle {
                    flat: true
                    icon: "list"
                    active: page.deviceViewMode === "list"
                    tooltip: (I18n.language, I18n.t("devices.viewList"))
                    onTriggered: page.setViewMode("list")
                }
                IconToggle {
                    flat: true
                    icon: "grid"
                    active: page.deviceViewMode === "grid"
                    tooltip: (I18n.language, I18n.t("devices.viewGrid"))
                    onTriggered: page.setViewMode("grid")
                }
                IconToggle {
                    flat: true
                    icon: "square"
                    active: page.deviceViewMode === "carousel"
                    tooltip: (I18n.language, I18n.t("devices.viewCarousel"))
                    onTriggered: page.setViewMode("carousel")
                }
            }

            Rectangle {
                width: 8; height: 8; radius: 4
                color: DeviceController.connected ? Theme.success : Theme.danger
                Layout.alignment: Qt.AlignVCenter
                Layout.leftMargin: 8
            }
            Text {
                text: (I18n.language, DeviceController.connected ? I18n.t("devices.daemonConnected") : I18n.t("devices.daemonOffline"))
                color: Theme.textDim
                font.pixelSize: 12
            }
        }

        // ── Master + detail ─────────────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 20

            // Master: the device list.
            ScrollView {
                id: listScroll
                Layout.preferredWidth: 420
                Layout.fillHeight: true
                contentWidth: availableWidth
                contentHeight: listCol.implicitHeight
                clip: true

                ColumnLayout {
                    id: listCol
                    width: listScroll.availableWidth
                    spacing: 12

                    // Three view modes as static siblings switched by
                    // `visible` only — never a Loader that recreates content.
                    // Same reasoning as MixerPage.qml's tab panels: rebuilding
                    // DeviceCards on every mode switch would re-run
                    // imagePath() and could flash empty content for a frame.

                    // List — one card per row.
                    ColumnLayout {
                        Layout.fillWidth: true
                        visible: page.deviceViewMode === "list"
                        spacing: 12

                        Repeater {
                            model: page.devices
                            DeviceCard {
                                Layout.fillWidth: true
                                selected: modelData.id === page.selectedId
                                onClicked: page.selectedId = modelData.id
                            }
                        }
                    }

                    // Grid — two columns. Both columns share the row height
                    // because DeviceCard is a fixed height, so a mouse and a
                    // headset in the same row can no longer render ragged or
                    // overlap.
                    GridLayout {
                        Layout.fillWidth: true
                        visible: page.deviceViewMode === "grid"
                        columns: 2
                        columnSpacing: 12
                        rowSpacing: 12

                        Repeater {
                            model: page.devices
                            DeviceCard {
                                Layout.fillWidth: true
                                selected: modelData.id === page.selectedId
                                onClicked: page.selectedId = modelData.id
                            }
                        }
                    }

                    // Carousel — one card at a time.
                    ColumnLayout {
                        Layout.fillWidth: true
                        visible: page.deviceViewMode === "carousel"
                        spacing: 10

                        SwipeView {
                            id: carousel
                            Layout.fillWidth: true
                            Layout.preferredHeight: 72
                            clip: true

                            Repeater {
                                model: page.devices
                                DeviceCard {
                                    width: carousel.width
                                    // SwipeView positions every page including
                                    // off-screen ones and relies on its own
                                    // clip -- but Icon.qml's Shape glyphs do
                                    // not respect that clip under the offscreen
                                    // QPA backend (confirmed via ui-shots.sh:
                                    // a battery icon painted through the clip
                                    // and floated outside the visible page,
                                    // reproducibly). Hiding non-current pages
                                    // sidesteps the interaction entirely.
                                    visible: SwipeView.isCurrentItem
                                    selected: modelData.id === page.selectedId
                                    onClicked: page.selectedId = modelData.id
                                }
                            }
                            // Guarded on the active view mode: a SwipeView
                            // still constructs and emits currentIndexChanged
                            // (with index 0) even while its parent is
                            // `visible: false`, so without this the hidden
                            // carousel silently yanked the selection back to
                            // the first device every time the page loaded in
                            // List or Grid mode — overriding whatever the user
                            // had actually clicked.
                            onCurrentIndexChanged: {
                                if (page.deviceViewMode !== "carousel")
                                    return
                                if (currentIndex >= 0 && currentIndex < page.devices.length)
                                    page.selectedId = page.devices[currentIndex].id
                            }
                        }
                        PageIndicator {
                            Layout.alignment: Qt.AlignHCenter
                            visible: carousel.count > 1
                            count: carousel.count
                            currentIndex: carousel.currentIndex
                        }
                    }

                    Text {
                        Layout.fillWidth: true
                        visible: page.devices.length === 0
                        text: (I18n.language, I18n.t("devices.noDevices"))
                        color: Theme.textDim
                        font.pixelSize: 13
                    }

                    Item { Layout.fillHeight: true }
                }
            }

            // Detail: the selected device's real settings surface.
            DeviceDetailPanel {
                id: detailPanel
                Layout.fillWidth: true
                Layout.fillHeight: true
                device: page.selectedDevice
                onConfigureButtonsRequested: buttonEditor.device = page.selectedDevice
            }
        }
    }

    // Above the ScrollView, not inside it — see WheelScroller.qml.
    Item {
        anchors.fill: listScroll
        WheelScroller { anchors.fill: parent; flick: listScroll.contentItem }
    }

    // One shared instance rather than one per card.
    ButtonMapEditor {
        id: buttonEditor
        z: 100
    }
}
