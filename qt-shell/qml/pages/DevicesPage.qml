import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Devices — live device list from the openggd daemon (real D-Bus data).
Rectangle {
    id: page
    color: Theme.bg

    property var devices: DeviceController.devicesJson
        ? JSON.parse(DeviceController.devicesJson)
        : []

    // Persisted like any other real UI preference (SettingsController's
    // `settings.<key>` envelope — see GeneralPanel.qml's `defaultClickAction`
    // for the same pattern), not the page-local `property string` ClipsPage
    // uses for its own grid/list toggle: the roadmap for this feature
    // explicitly asked for a setting that survives a restart, and this is
    // the mechanism every other persisted-across-restarts UI preference in
    // this app already goes through — no new persistence path invented.
    property var settingsObj: JSON.parse(SettingsController.settingsJson || "{}")
    readonly property string deviceViewMode: settingsObj.deviceViewMode || "list"
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

    ScrollView {
        id: devicesScroll
        anchors.fill: parent
        contentWidth: availableWidth
        // See HomePage.qml's ScrollView for why this is explicit: QQC2's
        // automatic contentHeight inference doesn't reliably track a
        // ColumnLayout child positioned with an explicit x/y offset, so the
        // page could look like it has nothing to scroll even when the
        // device list overflows the window.
        contentHeight: devicesCol.implicitHeight + devicesCol.y * 2

        ColumnLayout {
            id: devicesCol
            width: Math.min(parent.width, 760)
            x: 32
            y: 28
            spacing: 20

            RowLayout {
                Layout.fillWidth: true
                Text {
                    horizontalAlignment: Text.AlignLeft
                    text: (I18n.language, I18n.t("nav.devices"))
                    color: Theme.text
                    font.pixelSize: 26
                    font.weight: Font.Bold
                    Layout.fillWidth: true
                }

                // View-mode switcher (Devices roadmap Phase 4) — same
                // SegmentedToggle+IconToggle pair ClipsPage's grid/list
                // toggle already uses, just with a third segment. "square"
                // stands in for carousel/one-at-a-time: there's no
                // dedicated carousel glyph in Icons.qml yet and this reuses
                // an existing icon rather than adding a new asset for one
                // button (see AGENTS.md's icon-registry rule for what
                // adding a real one would require).
                SegmentedToggle {
                    IconToggle {
                        flat: true
                        icon: "list"
                        active: page.deviceViewMode === "list"
                        tooltip: "List view"
                        onTriggered: page.setViewMode("list")
                    }
                    IconToggle {
                        flat: true
                        icon: "grid"
                        active: page.deviceViewMode === "grid"
                        tooltip: "Grid view"
                        onTriggered: page.setViewMode("grid")
                    }
                    IconToggle {
                        flat: true
                        icon: "square"
                        active: page.deviceViewMode === "carousel"
                        tooltip: "Carousel view"
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
                    text: DeviceController.connected ? "daemon connected" : "daemon offline"
                    color: Theme.textDim
                    font.pixelSize: 12
                }
            }

            // Three view modes as static siblings, switched by `visible`
            // only — never a Loader that recreates content. Same reasoning
            // as MixerPage.qml's EQ/DSP tab panels: recreating DeviceCard
            // instances on every mode switch would re-run
            // DeviceController.imagePath()/re-evaluate every SelectField's
            // options from scratch and could flash empty content for a
            // frame, exactly what the roadmap's acceptance check ("no
            // re-fetch/flicker") rules out. All three read the same
            // `page.devices` — switching modes never touches
            // DeviceController itself.

            // List — unchanged from before Phase 4, just using the
            // extracted DeviceCard component.
            ColumnLayout {
                Layout.fillWidth: true
                visible: page.deviceViewMode === "list"
                spacing: 20

                Repeater {
                    model: page.devices
                    DeviceCard {
                        Layout.fillWidth: true
                        Layout.preferredWidth: 680
                    }
                }
            }

            // Grid — two columns of narrower cards.
            GridLayout {
                Layout.fillWidth: true
                visible: page.deviceViewMode === "grid"
                columns: 2
                columnSpacing: 20
                rowSpacing: 20

                Repeater {
                    model: page.devices
                    DeviceCard {
                        Layout.fillWidth: true
                        Layout.preferredWidth: 320
                    }
                }
            }

            // Carousel — one card at a time. A SwipeView (the standard QQC2
            // control for exactly this "page through items" interaction) is
            // used here rather than a hand-rolled PathView: it gets
            // swipe/keyboard paging and current-item tracking for free,
            // with far less surface area for a QML layout landmine than
            // manually computing a PathView's path.
            ColumnLayout {
                Layout.fillWidth: true
                visible: page.deviceViewMode === "carousel"
                spacing: 12

                SwipeView {
                    id: carousel
                    Layout.fillWidth: true
                    Layout.preferredWidth: 680
                    // SwipeView doesn't auto-size to its current page's
                    // implicit height — each DeviceCard's height differs
                    // (a headset's extra stats row, a mouse's controls row),
                    // so this tracks whichever page is actually showing.
                    // `currentItem` is a real SwipeView property; this is a
                    // normal reactive binding, not the mapToItem()-style
                    // one-shot-evaluation trap.
                    Layout.preferredHeight: currentItem ? currentItem.implicitHeight : 120
                    clip: true

                    Repeater {
                        model: page.devices
                        DeviceCard {
                            width: carousel.width
                            // SwipeView positions every page, including the
                            // off-screen ones, and relies on its own
                            // `clip: true` to hide them -- but Icon.qml's
                            // Shape-based glyphs don't respect that clip
                            // under the offscreen QPA backend (confirmed via
                            // ui-shots.sh: a headset card's battery icon
                            // painted through the clip and floated outside
                            // the visible page, reproducibly, independent of
                            // settle delay -- not a one-frame race). Hiding
                            // every non-current page outright sidesteps the
                            // Shape/clip interaction entirely, at the cost
                            // of the adjacent page no longer being visible
                            // mid-drag during an interactive swipe gesture.
                            visible: SwipeView.isCurrentItem
                        }
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
                visible: page.devices.length === 0
                text: (I18n.language, I18n.t("devices.noDevices"))
                color: Theme.textDim
                font.pixelSize: 13
            }
        }
    }
    // Above the ScrollView, not inside it — see WheelScroller.qml.
    Item {
        anchors.fill: devicesScroll
        WheelScroller { anchors.fill: parent; flick: devicesScroll.contentItem }
    }

}
