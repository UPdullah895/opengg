import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Clips — the local clip library, read live from opengg_core::clips (SQLite +
// filesystem scan) via ClipsController, which is itself the QAbstractListModel
// (roles: filepath/thumbnail/duration/title/game/filesize/favorite). Search,
// game filter, and sort are server-side state (setSearchText/setGameFilter/
// setSortMode) — QML no longer parses JSON or filters/sorts an array locally.
Rectangle {
    id: page
    color: Theme.bg

    // Clip currently open in the player overlay (null = closed). Each target
    // is a small {filepath, title} object built from the delegate's role
    // context properties at the point of interaction.
    property var playerClip: null
    /// Raised when a clip should open in the dedicated editor page. Main.qml
    /// owns the navigation; the editor is a page now, not an overlay here.
    signal editClipRequested(var clip)
    // Clips targeted by the rename / delete dialogs (null = dialog closed).
    property var renameTarget: null
    property var deleteTarget: null
    /// Set to a filepath list to confirm a bulk delete (null = dialog closed).
    property var bulkDeleteTarget: null

    // ── Multi-select ──────────────────────────────────────────────────────
    // A plain JS object used as a set (filepath -> true). QML can't observe
    // in-place mutation of an object property, so every mutator REPLACES the
    // whole object — the same "reassign, never mutate" rule audio.ts documents
    // for Vue's reactivity.
    property var selected: ({})
    readonly property int selectedCount: Object.keys(page.selected).length

    function isSelected(fp) { return page.selected[fp] === true }

    function toggleSelect(fp) {
        var next = {}
        for (var k in page.selected) next[k] = true
        if (next[fp]) delete next[fp]
        else next[fp] = true
        page.selected = next
    }

    function clearSelection() { page.selected = ({}) }

    function selectAllVisible() {
        var next = {}
        var paths = ClipsController.visibleFilepaths()
        for (var i = 0; i < paths.length; i++) next[paths[i]] = true
        page.selected = next
    }

    function selectedList() { return Object.keys(page.selected) }

    /// A plain click on a clip. While a selection is active it extends the
    /// selection instead of opening the player — otherwise building a batch
    /// would mean hitting the small checkbox on every single card.
    function activate(fp, clipTitle, thumb) {
        if (page.selectedCount > 0)
            page.toggleSelect(fp)
        else
            page.playerClip = { filepath: fp, title: clipTitle, thumbnail: thumb }
    }

    // ── Date grouping ─────────────────────────────────────────────────────
    property var dateGroups: []

    function groupLabel(dateKey) {
        if (dateKey === "Unknown")
            return "Unknown date"
        const d = new Date(dateKey)
        if (isNaN(d.getTime()))
            return dateKey
        d.setHours(0, 0, 0, 0)
        const today = new Date(); today.setHours(0, 0, 0, 0)
        const yesterday = new Date(today); yesterday.setDate(today.getDate() - 1)
        if (d.getTime() === today.getTime()) return "Today"
        if (d.getTime() === yesterday.getTime()) return "Yesterday"
        // Bare Qt.locale() is the SYSTEM locale, which on an Arabic-locale
        // desktop rendered these headers in Arabic while the rest of the UI
        // stayed English. Follow the app's own language instead — the Vue
        // original hardcodes 'en-US' here for the same reason.
        return d.toLocaleDateString(Qt.locale(I18n.language === "ar" ? "ar" : "en_US"),
                                    "dddd, MMMM d, yyyy")
    }

    function rebuildGroups() {
        if (!page.dateGrouped) {
            page.dateGroups = []
            return
        }
        const rows = JSON.parse(ClipsController.visibleJson() || "[]")
        var buckets = {}
        var order = []
        for (var i = 0; i < rows.length; i++) {
            const key = rows[i].created ? String(rows[i].created).split(" ")[0] : "Unknown"
            if (!buckets[key]) { buckets[key] = []; order.push(key) }
            buckets[key].push(rows[i])
        }
        var out = []
        for (var j = 0; j < order.length; j++)
            out.push({ date: order[j], label: page.groupLabel(order[j]), clips: buckets[order[j]] })
        page.dateGroups = out
    }

    onDateGroupedChanged: page.rebuildGroups()
    Connections {
        target: ClipsController
        // `revision` is bumped on every view recompute, including ones that
        // leave count/stats identical (a favourite toggle, a rename).
        function onRevisionChanged() { page.rebuildGroups() }
    }

    /// True when every selected clip is already favourited — the bulk button
    /// then un-favourites instead, matching the single-clip heart's behaviour.
    readonly property bool allSelectedFavorited: {
        const paths = page.selectedList()
        if (paths.length === 0) return false
        const favs = ClipsController.favoritePaths()
        for (var i = 0; i < paths.length; i++)
            if (favs.indexOf(paths[i]) < 0) return false
        return true
    }

    // ── View state ────────────────────────────────────────────────────────
    /// "grid" | "list"
    property string viewMode: "grid"
    property bool dateGrouped: false
    property bool showStats: true
    property bool favoritesOnly: false

    readonly property int clipsPerRow: Math.max(2, Math.min(5,
        page.settings.clipsPerRow || Theme.clipsGridCols))
    readonly property var stats: JSON.parse(ClipsController.statsJson || "{}")

    // Live ui-settings.json `settings` object (drives the grid column count).
    // `settingsJson` is ALREADY that inner object — settings.rs's refresh()
    // publishes `v["settings"]`, not the whole envelope — so parse it directly,
    // matching NotificationsPanel.qml. Adding a `.settings` hop here silently
    // yielded {} and made the grid fall back to the theme default.
    property var settings: JSON.parse(SettingsController.settingsJson || "{}")
    Connections {
        target: SettingsController
        function onSettingsJsonChanged() {
            page.settings = JSON.parse(SettingsController.settingsJson || "{}")
        }
    }

    function applyRename() {
        if (renameTarget && nameField.text.trim().length > 0)
            ClipsController.setCustomName(renameTarget.filepath, nameField.text.trim())
        renameTarget = null
    }

    Component.onCompleted: {
        ClipsController.refresh()
        ClipsController.startWatcher()
    }

    // ── formatting helpers ────────────────────────────────────────────────
    function fmtDuration(sec) {
        if (!sec || sec <= 0)
            return "--:--"
        var s = Math.round(sec)
        var h = Math.floor(s / 3600)
        var m = Math.floor((s % 3600) / 60)
        var ss = s % 60
        var mm = (h > 0 && m < 10 ? "0" : "") + m
        var pad = (ss < 10 ? "0" : "") + ss
        return (h > 0 ? h + ":" : "") + mm + ":" + pad
    }
    function fmtSize(bytes) {
        if (!bytes || bytes <= 0)
            return ""
        var mb = bytes / (1024 * 1024)
        if (mb >= 1024)
            return (mb / 1024).toFixed(1) + " GB"
        return Math.round(mb) + " MB"
    }

    // Stats-bar formatters — ported from ClipsStatsBar.vue's fmtStat* so the
    // strip reads identically ("50m 58s", "10.7 GB") in both shells.
    function fmtStatDuration(s) {
        s = s || 0
        var m = Math.floor(s / 60)
        var h = Math.floor(m / 60)
        if (h > 0)
            return h + "h " + (m % 60) + "m"
        return m + "m " + Math.floor(s % 60) + "s"
    }
    function fmtStatSize(bytes) {
        bytes = bytes || 0
        var gb = bytes / (1024 * 1024 * 1024)
        if (gb >= 1)
            return gb.toFixed(1) + " GB"
        var mb = bytes / (1024 * 1024)
        if (mb >= 1)
            return mb.toFixed(0) + " MB"
        return bytes + " B"
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 28
        spacing: 18

        // ── Header ────────────────────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Text {
                text: (I18n.language, I18n.t("nav.clips"))
                color: Theme.text
                font.pixelSize: 26
                font.weight: Font.Bold
            }

            Rectangle {
                visible: ClipsController.totalCount > 0
                radius: 10
                color: Theme.surface
                border.width: 1
                border.color: Theme.border
                implicitWidth: countLabel.implicitWidth + 18
                implicitHeight: 22
                Layout.alignment: Qt.AlignVCenter
                Text {
                    id: countLabel
                    anchors.centerIn: parent
                    text: ClipsController.count === ClipsController.totalCount
                          ? ClipsController.totalCount + (ClipsController.totalCount === 1 ? " clip" : " clips")
                          : ClipsController.count + " of " + ClipsController.totalCount
                    color: Theme.textDim
                    font.pixelSize: 12
                }
            }

            Item { Layout.fillWidth: true }

            // Refresh button
            Rectangle {
                id: refreshBtn
                implicitWidth: 92
                implicitHeight: 32
                radius: Theme.radius
                color: refreshArea.containsMouse ? Theme.border : Theme.surface
                border.width: 1
                border.color: Theme.border
                Text {
                    anchors.centerIn: parent
                    text: ClipsController.loading ? "Scanning…" : "Refresh"
                    color: Theme.text
                    font.pixelSize: 13
                }
                MouseArea {
                    id: refreshArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    enabled: !ClipsController.loading
                    onClicked: ClipsController.refresh()
                }
            }
        }

        // ── Toolbar: search + sort + game filter ──────────────────────────
        RowLayout {
            Layout.fillWidth: true
            visible: ClipsController.totalCount > 0
            spacing: 10
            // The recorder menu drops out of this row into the grid's area.
            // z on the button itself only orders it WITHIN this row — the grid
            // is a later sibling of the row in the ColumnLayout, so the row is
            // what has to be lifted for the menu to be visible at all.
            z: 60

            // Recorder status + start/save/stop (ClipsToolbar.vue's #recording
            // slot). The Home page's recording card drives the same controller.
            RecordingControl {
                Layout.alignment: Qt.AlignVCenter
            }

            // Search
            Rectangle {
                Layout.fillWidth: true
                Layout.maximumWidth: 300
                implicitHeight: 32
                radius: Theme.radius
                color: Theme.surface
                border.width: 1
                border.color: searchField.activeFocus ? Theme.accent : Theme.border

                TextField {
                    id: searchField
                    anchors.fill: parent
                    leftPadding: 10
                    rightPadding: 10
                    verticalAlignment: TextInput.AlignVCenter
                    color: Theme.text
                    font.pixelSize: 13
                    placeholderText: "Search clips…"
                    placeholderTextColor: Theme.textDim
                    selectByMouse: true
                    background: Item {}
                    onTextChanged: ClipsController.setSearchText(text)
                }
            }

            // Game filter
            ComboBox {
                id: gameBox
                implicitWidth: 170
                implicitHeight: 32
                model: ClipsController.gameList
                currentIndex: 0
                onActivated: ClipsController.setGameFilter(currentIndex === 0 ? "" : currentText)
                // reset selection if the game list changes out from under us
                Connections {
                    target: ClipsController
                    function onGameListChanged() {
                        if (gameBox.currentIndex >= ClipsController.gameList.length)
                            gameBox.currentIndex = 0
                    }
                }
                font.pixelSize: 13
                background: Rectangle {
                    radius: Theme.radius
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.border
                }
                contentItem: Text {
                    leftPadding: 10
                    rightPadding: gameBox.indicator.width + 6
                    text: gameBox.displayText
                    color: Theme.text
                    font: gameBox.font
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                }
                indicator: Icon {
                    x: gameBox.width - width - 8
                    y: (gameBox.height - height) / 2
                    name: "chevron-down"; size: 12
                    color: Theme.textDim
                }
                popup: Popup {
                    y: gameBox.height + 2
                    width: gameBox.width
                    implicitHeight: Math.min(contentItem.implicitHeight + 2, 320)
                    padding: 1
                    background: Rectangle {
                        radius: Theme.radius
                        color: Theme.surface
                        border.width: 1
                        border.color: Theme.border
                    }
                    contentItem: ListView {
                        clip: true
                        implicitHeight: contentHeight
                        model: gameBox.popup.visible ? gameBox.delegateModel : null
                        ScrollBar.vertical: ScrollBar {}
                    }
                }
                delegate: ItemDelegate {
                    width: gameBox.width
                    height: 30
                    contentItem: Text {
                        text: modelData
                        color: Theme.text
                        font.pixelSize: 13
                        elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter
                    }
                    highlighted: gameBox.highlightedIndex === index
                    background: Rectangle {
                        color: highlighted ? Theme.border : "transparent"
                    }
                }
            }

            // Sort
            ComboBox {
                id: sortBox
                implicitWidth: 130
                implicitHeight: 32
                textRole: "label"
                valueRole: "value"
                model: [
                    { label: "Newest",   value: "newest" },
                    { label: "Oldest",   value: "oldest" },
                    { label: "Longest",  value: "longest" },
                    { label: "Shortest", value: "shortest" }
                ]
                currentIndex: 0
                onActivated: ClipsController.setSortMode(currentValue)
                font.pixelSize: 13
                background: Rectangle {
                    radius: Theme.radius
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.border
                }
                contentItem: Text {
                    leftPadding: 10
                    rightPadding: sortBox.indicator.width + 6
                    text: sortBox.displayText
                    color: Theme.text
                    font: sortBox.font
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                }
                indicator: Icon {
                    x: sortBox.width - width - 8
                    y: (sortBox.height - height) / 2
                    name: "chevron-down"; size: 12
                    color: Theme.textDim
                }
                popup: Popup {
                    y: sortBox.height + 2
                    width: sortBox.width
                    implicitHeight: contentItem.implicitHeight + 2
                    padding: 1
                    background: Rectangle {
                        radius: Theme.radius
                        color: Theme.surface
                        border.width: 1
                        border.color: Theme.border
                    }
                    contentItem: ListView {
                        clip: true
                        implicitHeight: contentHeight
                        model: sortBox.popup.visible ? sortBox.delegateModel : null
                    }
                }
                delegate: ItemDelegate {
                    width: sortBox.width
                    height: 30
                    contentItem: Text {
                        text: modelData.label
                        color: Theme.text
                        font.pixelSize: 13
                        verticalAlignment: Text.AlignVCenter
                    }
                    highlighted: sortBox.highlightedIndex === index
                    background: Rectangle {
                        color: highlighted ? Theme.border : "transparent"
                    }
                }
            }

            // Favourites-only filter, badged with the library's favourite count.
            IconToggle {
                icon: "heart"
                label: String(ClipsController.favCount)
                active: page.favoritesOnly
                tooltip: "Show favorites only"
                onTriggered: {
                    page.favoritesOnly = !page.favoritesOnly
                    ClipsController.setFavoritesOnly(page.favoritesOnly)
                }
            }

            // Everything past here is right-aligned view control, matching
            // ClipsToolbar.vue's ctrl-left / ctrl-right split.
            Item { Layout.fillWidth: true }

            // Clips-per-row (2–5), persisted to ui-settings.json's
            // `clipsPerRow` — the same key ClipsPage.vue's size slider writes,
            // so both shells agree on grid density.
            // RowLayout, not Row: a plain Row sizes itself from its children's
            // `width`, and a QQC2 Slider given a bare `width` inside one ends
            // up contributing zero — the same "use the Layout attached
            // properties, not width/height" rule the Mixer tab bar needed.
            RowLayout {
                spacing: 6
                Layout.alignment: Qt.AlignVCenter
                visible: page.viewMode === "grid"

                Icon {
                    Layout.alignment: Qt.AlignVCenter
                    name: "grid"; size: 13; color: Theme.textDim
                }
                Slider {
                    id: sizeSlider
                    Layout.preferredWidth: 90
                    Layout.alignment: Qt.AlignVCenter
                    from: 2; to: 5; stepSize: 1
                    snapMode: Slider.SnapAlways
                    // NOT `value: page.clipsPerRow` — QQC2 writes `value`
                    // directly while dragging, which severs any binding on it
                    // and leaves the handle stuck. Seed it once and re-sync
                    // only when the setting changes from elsewhere.
                    Component.onCompleted: value = page.clipsPerRow
                    onMoved: SettingsController.setValue("clipsPerRow",
                                                         JSON.stringify(Math.round(sizeSlider.value)))
                    Connections {
                        target: page
                        function onClipsPerRowChanged() {
                            if (!sizeSlider.pressed)
                                sizeSlider.value = page.clipsPerRow
                        }
                    }

                    background: Rectangle {
                        x: sizeSlider.leftPadding
                        y: sizeSlider.topPadding + sizeSlider.availableHeight / 2 - height / 2
                        width: sizeSlider.availableWidth
                        height: 4
                        radius: 2
                        color: Theme.border
                        Rectangle {
                            width: sizeSlider.visualPosition * parent.width
                            height: parent.height
                            radius: 2
                            color: Theme.accent
                        }
                    }
                    handle: Rectangle {
                        x: sizeSlider.leftPadding
                           + sizeSlider.visualPosition * (sizeSlider.availableWidth - width)
                        y: sizeSlider.topPadding + sizeSlider.availableHeight / 2 - height / 2
                        width: 14; height: 14; radius: 7
                        color: Theme.text
                        border.width: 2
                        border.color: Theme.accent
                    }
                }
            }

            IconToggle {
                icon: "bar-chart"
                active: page.showStats
                tooltip: "Toggle clip details"
                onTriggered: page.showStats = !page.showStats
            }
            IconToggle {
                icon: "calendar"
                active: page.dateGrouped
                tooltip: "Group by date"
                onTriggered: page.dateGrouped = !page.dateGrouped
            }
            IconToggle {
                icon: "grid"
                active: page.viewMode === "grid"
                tooltip: "Grid view"
                onTriggered: page.viewMode = "grid"
            }
            IconToggle {
                icon: "list"
                active: page.viewMode === "list"
                tooltip: "List view"
                onTriggered: page.viewMode = "list"
            }
        }

        // ── Stats bar (port of ClipsStatsBar.vue) ─────────────────────────
        Rectangle {
            Layout.fillWidth: true
            visible: page.showStats && ClipsController.count > 0
            implicitHeight: 36
            radius: Theme.radius
            color: Theme.surface
            border.width: 1
            border.color: Theme.border

            Row {
                anchors.left: parent.left
                anchors.leftMargin: 14
                anchors.verticalCenter: parent.verticalCenter
                spacing: 14

                Repeater {
                    model: [
                        { v: String(page.stats.count || 0),                  l: "clips" },
                        { v: page.fmtStatDuration(page.stats.totalDuration), l: "total duration" },
                        { v: page.fmtStatSize(page.stats.totalSize),         l: "total size" },
                        { v: page.fmtStatDuration(page.stats.avgDuration),   l: "avg duration" }
                    ]

                    Row {
                        required property var modelData
                        required property int index
                        spacing: 14

                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: index > 0
                            width: visible ? 1 : 0
                            height: 12
                            color: Theme.border
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData.v
                            color: Theme.text
                            font.pixelSize: 12
                            font.weight: Font.DemiBold
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData.l
                            color: Theme.textMuted
                            font.pixelSize: 12
                        }
                    }
                }
            }
        }

        // ── Selection action bar ──────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            visible: page.selectedCount > 0
            implicitHeight: 44
            radius: Theme.radius
            color: Theme.accentAlpha(10)
            border.width: 1
            border.color: Theme.accentAlpha(35)

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 10
                spacing: 10

                Text {
                    text: page.selectedCount + (page.selectedCount === 1 ? " clip selected" : " clips selected")
                    color: Theme.text
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                }

                Item { Layout.fillWidth: true }

                ClipsBarButton {
                    label: "Select all"
                    icon: "check-square"
                    onTriggered: page.selectAllVisible()
                }
                ClipsBarButton {
                    label: page.allSelectedFavorited ? "Unfavorite" : "Favorite"
                    icon: "heart"
                    onTriggered: {
                        ClipsController.setFavorites(page.selectedList(),
                                                     !page.allSelectedFavorited)
                        page.clearSelection()
                    }
                }
                ClipsBarButton {
                    label: "Delete"
                    icon: "trash"
                    danger: true
                    onTriggered: page.bulkDeleteTarget = page.selectedList()
                }
                ClipsBarButton {
                    label: "Clear"
                    icon: "x"
                    onTriggered: page.clearSelection()
                }
            }
        }

        // ── Error banner ──────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            visible: ClipsController.error.length > 0
            radius: Theme.radius
            color: Theme.tint(Theme.danger, 10)
            border.width: 1
            border.color: Theme.accent
            implicitHeight: 40
            Text {
                anchors.left: parent.left
                anchors.leftMargin: 14
                anchors.verticalCenter: parent.verticalCenter
                text: "Failed to load clips: " + ClipsController.error
                color: Theme.text
                font.pixelSize: 13
            }
        }

        // ── Empty state ───────────────────────────────────────────────────
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: ClipsController.totalCount === 0 && !ClipsController.loading
                     && ClipsController.error.length === 0
            ColumnLayout {
                anchors.centerIn: parent
                spacing: 8
                Text {
                    text: "No clips yet"
                    color: Theme.text
                    font.pixelSize: 18
                    font.weight: Font.Bold
                    Layout.alignment: Qt.AlignHCenter
                }
                Text {
                    text: "Records saved from the replay buffer will appear here."
                    color: Theme.textDim
                    font.pixelSize: 13
                    Layout.alignment: Qt.AlignHCenter
                }
            }
        }

        // ── No-matches state (library non-empty, filters exclude all) ─────
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: ClipsController.totalCount > 0 && ClipsController.count === 0
            ColumnLayout {
                anchors.centerIn: parent
                spacing: 8
                Text {
                    text: "No clips match"
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.Bold
                    Layout.alignment: Qt.AlignHCenter
                }
                Text {
                    text: "Try a different search or game filter."
                    color: Theme.textDim
                    font.pixelSize: 13
                    Layout.alignment: Qt.AlignHCenter
                }
            }
        }

        // ── Clip grid ─────────────────────────────────────────────────────
        // Wrapped in a plain Item so the wheel-accelerator overlay below can
        // anchor to it: GridView itself is a direct ColumnLayout child, and a
        // Layout child can't be anchor-targeted by an item outside that
        // layout ("Cannot anchor to an item that isn't a parent or sibling" —
        // confirmed live, which is why the grid was still scrolling at the
        // old default speed despite the overlay existing). This wrapper is
        // the actual Layout child; grid and the overlay are both its
        // children and so share a common parent to anchor against.
        Item {
            id: gridWrap
            Layout.fillWidth: true
            Layout.fillHeight: true

        GridView {
            id: grid
            anchors.fill: parent
            visible: ClipsController.count > 0 && page.viewMode === "grid" && !page.dateGrouped
            clip: true
            cacheBuffer: 400

            // Column count comes from `settings.clipsPerRow` (the 2–5 slider in
            // ClipsPage.vue's toolbar), NOT from a width breakpoint — a
            // width-based count ignored the user's choice entirely. Note that
            // theme.json's `--clips-grid-cols` is vestigial in the Vue UI too
            // (defined in :root, read by nothing), so it is deliberately not
            // wired up here; Theme.clipsGridCols only supplies the fallback.
            cellWidth: width / page.clipsPerRow
            // 16:9 thumb + info strip. The old +62 was ~6px short of the real
            // info height, so `clip: true` shaved the card's bottom border and
            // rounded corners off — the "border not fully displayed" defect.
            cellHeight: cellWidth * 0.5625 + 80

            model: ClipsController

            Component.onCompleted: TourController.registerTarget("clips-grid", grid)
            Component.onDestruction: TourController.unregisterTarget("clips-grid")

            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            delegate: Item {
                width: grid.cellWidth
                height: grid.cellHeight

                // Extracted component — see components/ClipCard.qml. The inline
                // delegate this replaces had drifted far from ClipCard.vue
                // (no film placeholder, time badge, heart, kebab or meta pills).
                ClipCard {
                    anchors.fill: parent
                    anchors.margins: 6

                    filepath: model.filepath
                    thumbnail: model.thumbnail
                    duration: model.duration
                    title: model.title
                    game: model.game
                    filesize: model.filesize
                    favorite: model.favorite
                    created: model.created
                    clipWidth: model.width
                    clipHeight: model.height

                    selected: page.isSelected(model.filepath)
                    selectionMode: page.selectedCount > 0

                    onOpened: page.activate(model.filepath, model.title, model.thumbnail)
                    onSelectToggled: page.toggleSelect(model.filepath)
                    onRenamed: (newName) => ClipsController.setCustomName(model.filepath, newName)
                    onTrimRequested: page.editClipRequested({ filepath: model.filepath, title: model.title, game: model.game })
                    onDeleteRequested: page.deleteTarget = { filepath: model.filepath, title: model.title }
                    onMenuRequested: (gx, gy) => clipMenu.openAt(
                        { filepath: model.filepath, title: model.title,
                          favorite: model.favorite, game: model.game }, gx, gy)
                    onFavoriteToggled: ClipsController.setFavorite(model.filepath, !model.favorite)
                }
            }
        }

            // ── List view ─────────────────────────────────────────────────
            ListView {
                id: listView
                anchors.fill: parent
                visible: ClipsController.count > 0 && page.viewMode === "list" && !page.dateGrouped
                clip: true
                cacheBuffer: 400
                spacing: 6
                model: ClipsController

                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                delegate: ClipListRow {
                    width: listView.width
                    filepath: model.filepath
                    thumbnail: model.thumbnail
                    duration: model.duration
                    title: model.title
                    game: model.game
                    filesize: model.filesize
                    favorite: model.favorite
                    created: model.created
                    clipWidth: model.width
                    clipHeight: model.height

                    selected: page.isSelected(model.filepath)
                    selectionMode: page.selectedCount > 0

                    onOpened: page.activate(model.filepath, model.title, model.thumbnail)
                    onSelectToggled: page.toggleSelect(model.filepath)
                    onFavoriteToggled: ClipsController.setFavorite(model.filepath, !model.favorite)
                    onMenuRequested: (gx, gy) => clipMenu.openAt(
                        { filepath: model.filepath, title: model.title,
                          favorite: model.favorite, game: model.game }, gx, gy)
                }
            }

            // ── Date-grouped view ─────────────────────────────────────────
            // Feeds off `page.dateGroups` (a JS snapshot from visibleJson())
            // rather than the model, because a QAbstractListModel can't express
            // section headers to QML and GridView has no section support at all.
            ListView {
                id: groupedView
                anchors.fill: parent
                visible: ClipsController.count > 0 && page.dateGrouped
                clip: true
                cacheBuffer: 600
                spacing: 18
                model: page.dateGroups

                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                delegate: Column {
                    required property var modelData
                    width: groupedView.width
                    spacing: 8

                    Row {
                        spacing: 8
                        Text {
                            text: modelData.label
                            color: Theme.text
                            font.pixelSize: 14
                            font.weight: Font.Bold
                        }
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            radius: 9
                            color: Theme.surface
                            border.width: 1
                            border.color: Theme.border
                            implicitWidth: groupCount.implicitWidth + 16
                            implicitHeight: 18
                            Text {
                                id: groupCount
                                anchors.centerIn: parent
                                text: String(modelData.clips.length)
                                color: Theme.textDim
                                font.pixelSize: 11
                            }
                        }
                    }

                    // Grid and list bodies are both instantiated and gated by
                    // `visible` rather than swapped through a Loader: a Loader's
                    // component would have to reach the delegate's `modelData`
                    // by unqualified lookup across a Repeater boundary, which
                    // this toolchain resolves to blank silently (see the
                    // ShortcutsPanel landmine in the migration notes).
                    Grid {
                        width: parent.width
                        visible: page.viewMode === "grid"
                        columns: page.clipsPerRow
                        spacing: 12

                        Repeater {
                            model: parent.visible ? modelData.clips : []

                            ClipCard {
                                required property var modelData
                                readonly property real cellW:
                                    (groupedView.width - (page.clipsPerRow - 1) * 12)
                                    / page.clipsPerRow
                                width: cellW
                                height: cellW * 0.5625 + 80

                                filepath: modelData.filepath
                                thumbnail: modelData.thumbnail
                                duration: modelData.duration
                                title: modelData.title
                                game: modelData.game
                                filesize: modelData.filesize
                                favorite: modelData.favorite
                                created: modelData.created
                                clipWidth: modelData.width
                                clipHeight: modelData.height

                                selected: page.isSelected(modelData.filepath)
                                selectionMode: page.selectedCount > 0

                                onOpened: page.activate(modelData.filepath, modelData.title, modelData.thumbnail)
                                onSelectToggled: page.toggleSelect(modelData.filepath)
                                onRenamed: (newName) => ClipsController.setCustomName(modelData.filepath, newName)
                                onTrimRequested: page.editClipRequested({ filepath: modelData.filepath, title: modelData.title, game: modelData.game })
                                onDeleteRequested: page.deleteTarget = { filepath: modelData.filepath, title: modelData.title }
                                onFavoriteToggled: ClipsController.setFavorite(modelData.filepath, !modelData.favorite)
                                onMenuRequested: (gx, gy) => clipMenu.openAt(
                                    { filepath: modelData.filepath, title: modelData.title,
                                      favorite: modelData.favorite, game: modelData.game }, gx, gy)
                            }
                        }
                    }

                    Column {
                        width: parent.width
                        visible: page.viewMode === "list"
                        spacing: 6

                        Repeater {
                            model: parent.visible ? modelData.clips : []

                            ClipListRow {
                                required property var modelData
                                width: groupedView.width

                                filepath: modelData.filepath
                                thumbnail: modelData.thumbnail
                                duration: modelData.duration
                                title: modelData.title
                                game: modelData.game
                                filesize: modelData.filesize
                                favorite: modelData.favorite
                                created: modelData.created
                                clipWidth: modelData.width
                                clipHeight: modelData.height

                                selected: page.isSelected(modelData.filepath)
                                selectionMode: page.selectedCount > 0

                                onOpened: page.activate(modelData.filepath, modelData.title, modelData.thumbnail)
                                onSelectToggled: page.toggleSelect(modelData.filepath)
                                onFavoriteToggled: ClipsController.setFavorite(modelData.filepath, !modelData.favorite)
                                onMenuRequested: (gx, gy) => clipMenu.openAt(
                                    { filepath: modelData.filepath, title: modelData.title,
                                      favorite: modelData.favorite, game: modelData.game }, gx, gy)
                            }
                        }
                    }
                }
            }

            // Wheel accelerator, sibling of the views inside the shared wrapper
            // — see the comment on `gridWrap` above.
            Item {
                anchors.fill: parent
                WheelScroller {
                    anchors.fill: parent
                    flick: page.dateGrouped ? groupedView
                         : page.viewMode === "list" ? listView
                         : grid
                }
            }
        }
    }

    // ── Context menu (one instance for the whole page, as in ClipsPage.vue) ──
    ClipContextMenu {
        id: clipMenu
        z: 50
        onPreviewRequested: page.playerClip = { filepath: clip.filepath, title: clip.title }
        onEditRequested: page.editClipRequested({ filepath: clip.filepath, title: clip.title, game: clip.game })
        onRenameRequested: page.renameTarget = { filepath: clip.filepath, title: clip.title }
        onDeleteRequested: page.deleteTarget = { filepath: clip.filepath, title: clip.title }
        onFavoriteRequested: ClipsController.setFavorite(clip.filepath, !clip.favorite)
        onRevealRequested: SystemController.revealInFolder(clip.filepath)
        onCopyPathRequested: SystemController.writeClipboard(clip.filepath)
        onSelectRequested: page.toggleSelect(clip.filepath)
    }

    // ── Player overlay ────────────────────────────────────────────────────
    VideoPlayer {
        anchors.fill: parent
        visible: page.playerClip !== null
        source: page.playerClip ? "file://" + page.playerClip.filepath : ""
        title: page.playerClip ? page.playerClip.title : ""
        posterSource: page.playerClip ? (page.playerClip.thumbnail || "") : ""
        onClosed: page.playerClip = null
    }


    // ── Rename dialog ─────────────────────────────────────────────────────
    Rectangle {
        anchors.fill: parent
        color: Theme.scrim(80)
        visible: page.renameTarget !== null
        MouseArea { anchors.fill: parent; onClicked: page.renameTarget = null }

        Rectangle {
            anchors.centerIn: parent
            width: 380
            implicitHeight: rcol.implicitHeight + 32
            radius: Theme.radius
            color: Theme.surface
            border.width: 1
            border.color: Theme.border
            MouseArea { anchors.fill: parent }   // absorb backdrop clicks

            Column {
                id: rcol
                anchors.fill: parent
                anchors.margins: 16
                spacing: 12

                Text {
                    text: "Rename clip"
                    color: Theme.text
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }

                Rectangle {
                    width: parent.width
                    implicitHeight: 34
                    radius: Theme.radius
                    color: Theme.bg
                    border.width: 1
                    border.color: nameField.activeFocus ? Theme.accent : Theme.border
                    TextField {
                        id: nameField
                        anchors.fill: parent
                        leftPadding: 10
                        rightPadding: 10
                        verticalAlignment: TextInput.AlignVCenter
                        color: Theme.text
                        font.pixelSize: 13
                        background: Item {}
                        selectByMouse: true
                        onAccepted: page.applyRename()
                    }
                }

                Row {
                    anchors.right: parent.right
                    spacing: 8
                    Rectangle {
                        width: 84; height: 32; radius: Theme.radius
                        color: cancelR.containsMouse ? Theme.border : "transparent"
                        border.width: 1
                        border.color: Theme.border
                        Text { anchors.centerIn: parent; text: "Cancel"; color: Theme.text; font.pixelSize: 13 }
                        MouseArea {
                            id: cancelR
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: page.renameTarget = null
                        }
                    }
                    Rectangle {
                        width: 84; height: 32; radius: Theme.radius
                        color: Theme.accent
                        Text { anchors.centerIn: parent; text: "Save"; color: "#ffffff"; font.pixelSize: 13 }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: page.applyRename()
                        }
                    }
                }
            }
        }

        // Prefill + focus the field whenever a new target opens.
        Connections {
            target: page
            function onRenameTargetChanged() {
                if (page.renameTarget) {
                    nameField.text = page.renameTarget.title
                    nameField.forceActiveFocus()
                    nameField.selectAll()
                }
            }
        }
    }

    // ── Bulk delete confirmation ──────────────────────────────────────────
    Rectangle {
        anchors.fill: parent
        color: Theme.scrim(80)
        visible: page.bulkDeleteTarget !== null
        MouseArea { anchors.fill: parent; onClicked: page.bulkDeleteTarget = null }

        Rectangle {
            anchors.centerIn: parent
            width: 400
            implicitHeight: bcol.implicitHeight + 32
            radius: Theme.radius
            color: Theme.surface
            border.width: 1
            border.color: Theme.border
            MouseArea { anchors.fill: parent }

            Column {
                id: bcol
                anchors.fill: parent
                anchors.margins: 16
                spacing: 12

                Text {
                    text: page.bulkDeleteTarget
                          ? "Delete " + page.bulkDeleteTarget.length + " clips?"
                          : ""
                    color: Theme.text
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: "This permanently deletes every selected clip from disk."
                    color: Theme.textDim
                    font.pixelSize: 13
                    wrapMode: Text.WordWrap
                }

                Row {
                    anchors.right: parent.right
                    spacing: 8
                    Rectangle {
                        width: 84; height: 32; radius: Theme.radius
                        color: cancelB.containsMouse ? Theme.border : "transparent"
                        border.width: 1
                        border.color: Theme.border
                        Text { anchors.centerIn: parent; text: "Cancel"; color: Theme.text; font.pixelSize: 13 }
                        MouseArea {
                            id: cancelB
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: page.bulkDeleteTarget = null
                        }
                    }
                    Rectangle {
                        width: 84; height: 32; radius: Theme.radius
                        color: Theme.danger
                        Text { anchors.centerIn: parent; text: "Delete"; color: "#ffffff"; font.pixelSize: 13 }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                if (page.bulkDeleteTarget)
                                    ClipsController.deleteClips(page.bulkDeleteTarget)
                                page.bulkDeleteTarget = null
                                page.clearSelection()
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Delete confirmation ───────────────────────────────────────────────
    Rectangle {
        anchors.fill: parent
        color: Theme.scrim(80)
        visible: page.deleteTarget !== null
        MouseArea { anchors.fill: parent; onClicked: page.deleteTarget = null }

        Rectangle {
            anchors.centerIn: parent
            width: 400
            implicitHeight: dcol.implicitHeight + 32
            radius: Theme.radius
            color: Theme.surface
            border.width: 1
            border.color: Theme.border
            MouseArea { anchors.fill: parent }

            Column {
                id: dcol
                anchors.fill: parent
                anchors.margins: 16
                spacing: 12

                Text {
                    text: "Delete clip?"
                    color: Theme.text
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: page.deleteTarget
                          ? "This permanently deletes “" + page.deleteTarget.title + "” from disk."
                          : ""
                    color: Theme.textDim
                    font.pixelSize: 13
                    wrapMode: Text.WordWrap
                }

                Row {
                    anchors.right: parent.right
                    spacing: 8
                    Rectangle {
                        width: 84; height: 32; radius: Theme.radius
                        color: cancelD.containsMouse ? Theme.border : "transparent"
                        border.width: 1
                        border.color: Theme.border
                        Text { anchors.centerIn: parent; text: "Cancel"; color: Theme.text; font.pixelSize: 13 }
                        MouseArea {
                            id: cancelD
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: page.deleteTarget = null
                        }
                    }
                    Rectangle {
                        width: 84; height: 32; radius: Theme.radius
                        color: Theme.danger
                        Text { anchors.centerIn: parent; text: "Delete"; color: "#ffffff"; font.pixelSize: 13 }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                if (page.deleteTarget)
                                    ClipsController.deleteClip(page.deleteTarget.filepath)
                                page.deleteTarget = null
                            }
                        }
                    }
                }
            }
        }
    }
}
