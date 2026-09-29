import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.opengg.app

// Per-device button-mapping editor (Devices roadmap Phase 5) — photo
// upload, hotspot placement, and action binding, all on top of a photo the
// USER supplies of their OWN mouse. Never bundles, hosts, or syncs anyone
// else's product photography (see core/src/device_assets.rs and
// core/src/button_hotspots.rs's module doc comments for the full boundary).
//
// Full-screen scrim + centered card, same structure as ExportDialog.qml
// (this codebase's established modal pattern) rather than a QQC2 Popup.
Rectangle {
    id: dlg

    /// The device being configured (one entry from DeviceController's
    /// devicesJson), or null when closed.
    property var device: null

    signal closed()

    // ── Loaded state, refreshed each time `device` changes ──────────────
    property bool hasPhoto: false
    property var hotspots: []                     // [{buttonIndex, x, y}]
    property var mappings: []                      // parsed buttonMappingsJson
    property var catalog: ({ special: [], key: [] })
    property int selectedIndex: -1                  // index into `hotspots`
    property string photoError: ""
    property string presetOffer: ""                 // model name, or "" if none/declined
    property var presetHotspots: []

    function reload() {
        if (!dlg.device)
            return
        dlg.hasPhoto = DeviceController.hasDevicePhoto(dlg.device.vid, dlg.device.pid)
        dlg.hotspots = JSON.parse(
            DeviceController.deviceHotspots(dlg.device.vid, dlg.device.pid) || "[]")
        dlg.selectedIndex = -1
        dlg.photoError = ""

        const presetJson = DeviceController.findDevicePreset(dlg.device.vid, dlg.device.pid)
        if (presetJson !== "" && dlg.hotspots.length === 0) {
            const preset = JSON.parse(presetJson)
            dlg.presetOffer = preset.modelName
            dlg.presetHotspots = preset.hotspots
        } else {
            dlg.presetOffer = ""
            dlg.presetHotspots = []
        }

        DeviceController.refreshButtonMappings(dlg.device.id)
        DeviceController.refreshButtonActionCatalog()
    }

    onDeviceChanged: dlg.reload()

    Connections {
        target: DeviceController
        function onButtonMappingsJsonChanged() {
            dlg.mappings = JSON.parse(DeviceController.buttonMappingsJson || "[]")
        }
        function onButtonActionCatalogJsonChanged() {
            dlg.catalog = JSON.parse(DeviceController.buttonActionCatalogJson || "{\"special\":[],\"key\":[]}")
        }
    }

    /// This device's actual button count — hotspots can never exceed it.
    readonly property int buttonCount: dlg.device && dlg.device.buttonCount ? dlg.device.buttonCount : 0

    /// The current action for a given button index, from the live
    /// `mappings` (not the hotspot itself — a hotspot only records WHERE a
    /// button is on the photo, not what it currently does).
    function actionFor(buttonIndex) {
        for (let i = 0; i < dlg.mappings.length; i++)
            if (dlg.mappings[i].index === buttonIndex)
                return dlg.mappings[i].action
        return null
    }

    function actionLabel(action) {
        if (!action)
            return ""
        if (action.type === "none")
            return (I18n.language, I18n.t("devices.buttonMap.none"))
        if (action.type === "button")
            return (I18n.language, I18n.t("devices.buttonMap.click")).replace("{n}", action.target)
        if (action.type === "special") {
            const found = dlg.catalog.special.find(s => s.name === action.name)
            return found ? found.label : action.name
        }
        if (action.type === "key") {
            const found = dlg.catalog.key.find(k => k.name === action.name)
            return found ? found.label : action.name
        }
        if (action.type === "macro")
            return (I18n.language, I18n.t("devices.buttonMap.macro"))
        return (I18n.language, I18n.t("devices.buttonMap.unknown"))
    }

    function lowestFreeButtonIndex() {
        const used = dlg.hotspots.map(h => h.buttonIndex)
        for (let i = 0; i < dlg.buttonCount; i++)
            if (used.indexOf(i) === -1)
                return i
        return -1
    }

    function persistHotspots() {
        if (!dlg.device)
            return
        const linkedIds = JSON.stringify(dlg.device.linkedIds || [])
        const err = DeviceController.saveDeviceHotspots(
            dlg.device.vid, dlg.device.pid, dlg.buttonCount,
            linkedIds, JSON.stringify(dlg.hotspots))
        if (err !== "")
            dlg.photoError = err
    }

    function applyPreset() {
        const count = Math.min(dlg.presetHotspots.length, dlg.buttonCount)
        const placed = []
        for (let i = 0; i < count; i++) {
            const p = dlg.presetHotspots[i]
            placed.push({ buttonIndex: p.buttonIndex, x: p.x, y: p.y })
        }
        dlg.hotspots = placed
        dlg.presetOffer = ""
        dlg.persistHotspots()
    }

    anchors.fill: parent
    color: Theme.scrim(70)
    visible: dlg.device !== null

    MouseArea {
        // Click-outside-to-close, same as ExportDialog's scrim.
        anchors.fill: parent
        onClicked: { dlg.device = null; dlg.closed() }
    }

    // Both the drop zone and its Browse fallback funnel here, so the save +
    // error handling is written once regardless of how the file arrived.
    function acceptPhoto(url) {
        const err = DeviceController.saveDevicePhoto(dlg.device.vid, dlg.device.pid, url)
        if (err !== "") {
            dlg.photoError = err
        } else {
            dlg.photoError = ""
            dlg.hasPhoto = true
        }
    }

    // The actual card — stops the scrim's click-outside handler from
    // firing for clicks inside the dialog itself.
    Rectangle {
        anchors.centerIn: parent
        width: 720
        implicitHeight: card.implicitHeight + 40
        radius: Theme.radiusLg
        color: Theme.bg
        border.width: 1
        border.color: Theme.border

        MouseArea { anchors.fill: parent } // eat clicks, don't close

        ColumnLayout {
            id: card
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 20
            spacing: 14

            RowLayout {
                Layout.fillWidth: true
                Text {
                    Layout.fillWidth: true
                    text: dlg.device ? ((I18n.language, I18n.t("devices.buttonMap.title")).replace("{name}", DeviceController.displayName(dlg.device.name))) : ""
                    color: Theme.text
                    font.pixelSize: 18
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
                IconToggle {
                    flat: true
                    icon: "x"
                    tooltip: (I18n.language, I18n.t("common.close"))
                    onTriggered: { dlg.device = null; dlg.closed() }
                }
            }

            // ── Photo step ───────────────────────────────────────────
            ColumnLayout {
                Layout.fillWidth: true
                visible: !dlg.hasPhoto
                spacing: 10
                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.textDim
                    font.pixelSize: 13
                    text: (I18n.language, I18n.t("devices.buttonMap.photoIntro"))
                }
                PhotoDropZone {
                    Layout.fillWidth: true
                    title: (I18n.language, I18n.t("devices.buttonMap.dragPhoto"))
                    subtitle: (I18n.language, I18n.t("devices.buttonMap.browseHint"))
                    onPhotoChosen: (url) => dlg.acceptPhoto(url)
                }
                ErrorText {
                    Layout.fillWidth: true
                    width: parent.width
                    pixelSize: 12
                    text: dlg.photoError
                }
            }

            // ── Preset-assist banner ─────────────────────────────────
            RowLayout {
                Layout.fillWidth: true
                visible: dlg.hasPhoto && dlg.presetOffer !== ""
                spacing: 10
                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.textDim
                    font.pixelSize: 12
                    text: (I18n.language, I18n.t("devices.buttonMap.presetOffer")).replace("{name}", dlg.presetOffer)
                }
                ClipsBarButton { label: (I18n.language, I18n.t("devices.buttonMap.usePreset")); icon: "check"; onTriggered: dlg.applyPreset() }
                IconToggle {
                    flat: true
                    icon: "x"
                    tooltip: (I18n.language, I18n.t("devices.buttonMap.dismiss"))
                    onTriggered: dlg.presetOffer = ""
                }
            }

            // ── Editor step ──────────────────────────────────────────
            RowLayout {
                Layout.fillWidth: true
                visible: dlg.hasPhoto
                spacing: 14

                Item {
                    Layout.preferredWidth: 440
                    Layout.preferredHeight: 330

                    Image {
                        id: photo
                        anchors.fill: parent
                        fillMode: Image.PreserveAspectFit
                        // `cache: false` + DeviceController.photoRevision as
                        // a cache-busting query param — see that
                        // qproperty's doc comment in device.rs. Without
                        // both, re-uploading a photo under the same cache
                        // filename would keep showing the stale pixmap.
                        cache: false
                        source: dlg.device
                            ? DeviceController.imagePath(dlg.device.vid, dlg.device.pid, dlg.device.deviceType)
                              + "?v=" + DeviceController.photoRevision
                            : ""
                    }

                    // Sized/positioned to the actual PAINTED area, not the
                    // Image item's full bounds — PreserveAspectFit letterboxes
                    // whenever the photo's aspect ratio doesn't match this
                    // box exactly, and normalized hotspot coordinates must be
                    // relative to the visible photo, not the empty margin
                    // around it.
                    Item {
                        id: overlay
                        x: (photo.width - photo.paintedWidth) / 2
                        y: (photo.height - photo.paintedHeight) / 2
                        width: photo.paintedWidth
                        height: photo.paintedHeight
                        clip: true

                        MouseArea {
                            anchors.fill: parent
                            onClicked: (m) => {
                                const freeIndex = dlg.lowestFreeButtonIndex()
                                if (freeIndex === -1)
                                    return // every button already has a hotspot
                                const nx = m.x / overlay.width
                                const ny = m.y / overlay.height
                                dlg.hotspots = dlg.hotspots.concat([
                                    { buttonIndex: freeIndex, x: nx, y: ny }
                                ])
                                dlg.selectedIndex = dlg.hotspots.length - 1
                                dlg.persistHotspots()
                            }
                        }

                        Repeater {
                            model: dlg.hotspots
                            HotspotMarker {
                                x: modelData.x * overlay.width - width / 2
                                y: modelData.y * overlay.height - height / 2
                                buttonIndex: modelData.buttonIndex
                                selected: index === dlg.selectedIndex
                                actionLabel: dlg.actionLabel(dlg.actionFor(modelData.buttonIndex))
                                onClicked: dlg.selectedIndex = index
                                onMovedTo: (mx, my) => {
                                    const clampedX = Math.max(0, Math.min(overlay.width, mx + width / 2))
                                    const clampedY = Math.max(0, Math.min(overlay.height, my + height / 2))
                                    const updated = dlg.hotspots.slice()
                                    updated[index] = {
                                        buttonIndex: modelData.buttonIndex,
                                        x: clampedX / overlay.width,
                                        y: clampedY / overlay.height
                                    }
                                    dlg.hotspots = updated
                                }
                                onDeleteRequested: {
                                    const updated = dlg.hotspots.slice()
                                    updated.splice(index, 1)
                                    dlg.hotspots = updated
                                    dlg.selectedIndex = -1
                                    dlg.persistHotspots()
                                }
                            }
                        }
                    }
                }

                // ── Side panel: replace photo, hint text, and (when a
                // hotspot is selected) the action binding controls.
                ColumnLayout {
                    Layout.preferredWidth: 220
                    Layout.fillHeight: true
                    spacing: 10

                    Text {
                        Layout.fillWidth: true
                        color: Theme.textDim
                        font.pixelSize: 11
                        text: (I18n.language, I18n.t("devices.buttonMap.placed")).replace("{placed}", dlg.hotspots.length).replace("{total}", dlg.buttonCount)
                    }
                    PhotoDropZone {
                        Layout.fillWidth: true
                        implicitHeight: 104
                        title: (I18n.language, I18n.t("devices.buttonMap.dropNew"))
                        subtitle: (I18n.language, I18n.t("devices.buttonMap.replacesCurrent"))
                        browseLabel: (I18n.language, I18n.t("devices.buttonMap.replace"))
                        onPhotoChosen: (url) => dlg.acceptPhoto(url)
                    }

                    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

                    ColumnLayout {
                        Layout.fillWidth: true
                        visible: dlg.selectedIndex >= 0 && dlg.selectedIndex < dlg.hotspots.length
                        spacing: 8

                        readonly property var hotspot: (dlg.selectedIndex >= 0 && dlg.selectedIndex < dlg.hotspots.length)
                            ? dlg.hotspots[dlg.selectedIndex] : null
                        readonly property var currentAction: parent.hotspot ? dlg.actionFor(parent.hotspot.buttonIndex) : null

                        Text {
                            color: Theme.text
                            font.pixelSize: 13
                            font.weight: Font.DemiBold
                            text: (I18n.language, I18n.t("devices.buttonMap.button")).replace("{n}", parent.hotspot ? parent.hotspot.buttonIndex + 1 : "")
                        }
                        Text {
                            color: Theme.textDim
                            font.pixelSize: 12
                            text: (I18n.language, I18n.t("devices.buttonMap.currently")).replace("{action}", dlg.actionLabel(parent.currentAction))
                        }

                        Text { text: "Action type"; color: Theme.textDim; font.pixelSize: 11 }
                        SelectField {
                            id: typeSelect
                            Layout.fillWidth: true
                            options: [
                                { value: "none", label: (I18n.language, I18n.t("devices.buttonMap.noneDisabled")) },
                                { value: "button", label: (I18n.language, I18n.t("devices.buttonMap.mouseButton")) },
                                { value: "special", label: (I18n.language, I18n.t("devices.buttonMap.special")) },
                                { value: "key", label: (I18n.language, I18n.t("devices.buttonMap.keyboardKey")) },
                            ]
                            value: parent.currentAction ? parent.currentAction.type : "none"
                            onPicked: (v) => {
                                if (v === "none")
                                    DeviceController.setButtonAction(
                                        dlg.device.id, parent.hotspot.buttonIndex,
                                        JSON.stringify({ type: "none" }))
                                // For button/special/key, wait for the value
                                // picker below rather than writing a
                                // half-chosen action immediately.
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            visible: typeSelect.value === "button"
                            spacing: 4
                            Text { text: "Acts as button"; color: Theme.textDim; font.pixelSize: 11 }
                            SelectField {
                                Layout.fillWidth: true
                                options: {
                                    const opts = []
                                    for (let i = 1; i <= dlg.buttonCount; i++)
                                        opts.push({ value: i, label: (I18n.language, I18n.t("devices.buttonMap.button")).replace("{n}", i) })
                                    return opts
                                }
                                value: (parent.parent.currentAction && parent.parent.currentAction.type === "button")
                                    ? parent.parent.currentAction.target : 1
                                onPicked: (v) => DeviceController.setButtonAction(
                                    dlg.device.id, parent.parent.hotspot.buttonIndex,
                                    JSON.stringify({ type: "button", target: v }))
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            visible: typeSelect.value === "special"
                            spacing: 4
                            Text { text: "Special action"; color: Theme.textDim; font.pixelSize: 11 }
                            SelectField {
                                Layout.fillWidth: true
                                options: dlg.catalog.special.map(s => ({ value: s.name, label: s.label }))
                                value: (parent.parent.currentAction && parent.parent.currentAction.type === "special")
                                    ? parent.parent.currentAction.name : ""
                                onPicked: (v) => DeviceController.setButtonAction(
                                    dlg.device.id, parent.parent.hotspot.buttonIndex,
                                    JSON.stringify({ type: "special", name: v }))
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            visible: typeSelect.value === "key"
                            spacing: 4
                            Text { text: "Key"; color: Theme.textDim; font.pixelSize: 11 }
                            SelectField {
                                Layout.fillWidth: true
                                options: dlg.catalog.key.map(k => ({ value: k.name, label: k.label }))
                                value: (parent.parent.currentAction && parent.parent.currentAction.type === "key")
                                    ? parent.parent.currentAction.name : ""
                                onPicked: (v) => DeviceController.setButtonAction(
                                    dlg.device.id, parent.parent.hotspot.buttonIndex,
                                    JSON.stringify({ type: "key", name: v }))
                            }
                        }
                    }

                    Text {
                        Layout.fillWidth: true
                        visible: dlg.selectedIndex < 0
                        wrapMode: Text.WordWrap
                        color: Theme.textDim
                        font.pixelSize: 12
                        text: (I18n.language, I18n.t("devices.buttonMap.placeHint"))
                    }

                    Item { Layout.fillHeight: true }

                    ErrorText {
                        Layout.fillWidth: true
                        text: DeviceController.lastErrorDeviceId === (dlg.device ? dlg.device.id : "")
                              ? DeviceController.lastError : ""
                    }
                    ErrorText {
                        Layout.fillWidth: true
                        text: dlg.photoError
                    }
                }
            }
        }
    }
}
