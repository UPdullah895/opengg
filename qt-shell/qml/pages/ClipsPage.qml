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
    ///
    /// `game` is optional (older call sites only had filepath/title/thumb) —
    /// it is only needed to route "editor" the same way editClipRequested's
    /// other call sites do, so a missing value just means an empty game tag
    /// rather than a broken click.
    function activate(fp, clipTitle, thumb, game) {
        if (page.selectedCount > 0) {
            page.toggleSelect(fp)
            return
        }
        if (page.settings.defaultClickAction === "editor")
            page.editClipRequested({ filepath: fp, title: clipTitle, game: game || "" })
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
        page.rebuildRows()
    }

    /// `dateGroups` flattened to one entry per ListView row: a date header, a
    /// grid row of up to `clipsPerRow` cards, or a single list item. Built so
    /// the grouped view can virtualise clips instead of only groups — see the
    /// delegate's note.
    property var dateRows: []

    function rebuildRows() {
        if (!page.dateGrouped) {
            page.dateRows = []
            return
        }
        var out = []
        const per = Math.max(1, page.clipsPerRow)
        for (var g = 0; g < page.dateGroups.length; g++) {
            const grp = page.dateGroups[g]
            out.push({ kind: "header", label: grp.label, count: grp.clips.length })
            if (page.viewMode === "list") {
                for (var i = 0; i < grp.clips.length; i++)
                    out.push({ kind: "item", clip: grp.clips[i] })
            } else {
                for (var r = 0; r < grp.clips.length; r += per)
                    out.push({ kind: "row", clips: grp.clips.slice(r, r + per) })
            }
        }
        page.dateRows = out
    }

    // Row shape depends on both of these, so re-chunk when either changes.
    onViewModeChanged: page.rebuildRows()
    onClipsPerRowChanged: page.rebuildRows()

    onDateGroupedChanged: page.rebuildGroups()
    Connections {
        target: ClipsController
        // `revision` is bumped on every view recompute, including ones that
        // leave count/stats identical (a favourite toggle, a rename).
        function onRevisionChanged() { page.rebuildGroups() }
    }

    /// Any of search / games / favourites-only is narrowing the library.
    /// Drives the "clear filters" button's visibility.
    /// `revision` is listed first purely to make this re-evaluate: the game
    /// count is an invokable with no change signal, so a binding on it alone
    /// would latch at its startup value (same idiom as gameBox.picked).
    readonly property bool anyFilterActive:
        searchField.text.length > 0
        || (ClipsController.revision, ClipsController.gameFilterCount()) > 0
        || page.favoritesOnly

    function clearAllFilters() {
        searchField.text = ""          // onTextChanged pushes it to the controller
        ClipsController.clearGameFilters()
        page.favoritesOnly = false
        ClipsController.setFavoritesOnly(false)
    }

    /// Visible rows keyed by filepath. Batch rename needs each clip's game
    /// to expand {game}, and `visibleJson` is the only place QML can read a
    /// row's fields (the list model exposes roles, not objects). Built once
    /// per use — a per-clip lookup would re-parse the whole library.
    function visibleByPath() {
        const rows = JSON.parse(ClipsController.visibleJson() || "[]")
        var out = ({})
        for (var i = 0; i < rows.length; i++)
            out[rows[i].filepath] = rows[i]
        return out
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
    // Persisted to ui-settings.json so the toolbar comes back the way it was
    // left. These used to be plain local properties seeded with a literal,
    // which is why "Show details" switched itself back on at every launch.
    //
    // One-way flow, exactly like `clipsPerRow` below: derived from `settings`,
    // and the toolbar toggles write through SettingsController.setValue rather
    // than assigning here. A read/write property would need a binding on
    // `settings` that the first toggle would sever.
    /// "grid" | "list"
    readonly property string viewMode:
        page.settings.clipsViewMode === "list" ? "list" : "grid"
    readonly property bool dateGrouped: page.settings.clipsDateGrouped === true
    /// Defaults to on, so absent-and-unset must read as true.
    readonly property bool showStats: page.settings.clipsShowStats !== false
    /// Session-only: a filter, not a view preference.
    property bool favoritesOnly: false

    function persistView(key, value) {
        SettingsController.setValue(key, JSON.stringify(value))
    }

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
            ClipsController.setDateFormat(page.settings.dateFormat || "YMD")
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
        ClipsController.setDateFormat(page.settings.dateFormat || "YMD")
    }

    // Dev-only capture hooks. The selection bar and the "filters active"
    // toolbar only exist in states a headless run cannot click its way into,
    // so `--page clips --panel selection|filtered` puts the page there. Same
    // trick as the editor's `--panel <clip path>`; never runs otherwise.
    Connections {
        target: ClipsController
        enabled: ScreenshotController.active
        function onRevisionChanged() {
            if (page.shotStateApplied)
                return
            const rows = JSON.parse(ClipsController.visibleJson() || "[]")
            if (rows.length === 0)
                return
            page.shotStateApplied = true
            if (ScreenshotController.panel === "selection") {
                var next = ({})
                for (var i = 0; i < Math.min(3, rows.length); i++)
                    next[rows[i].filepath] = true
                page.selected = next
            } else if (ScreenshotController.panel === "filtered") {
                searchField.text = "re"
                page.favoritesOnly = true
                ClipsController.setFavoritesOnly(true)
            }
        }
    }
    property bool shotStateApplied: false

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
                // 20px / ExtraBold, matching the retired Vue PageHeader
                // (.page-title: font-size 20px; font-weight 800). The Qt port
                // had these at 26px Bold, which is most of why every page
                // header reads bulkier than the old shell.
                font.pixelSize: 20
                font.weight: Font.ExtraBold
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
                    placeholderText: (I18n.language, I18n.t("clips.search"))
                    placeholderTextColor: Theme.textDim
                    selectByMouse: true
                    background: Item {}
                    onTextChanged: ClipsController.setSearchText(text)
                }
            }

            // Game filter — MULTI-select, as the Vue toolbar was.
            //
            // A plain single-select ComboBox replaced it during the Qt port,
            // which lost both the ability to combine games and the per-game
            // clip counts. This is a button + Popup rather than a ComboBox:
            // QQC2's ComboBox is built around one current index and fights a
            // checkbox list.
            Rectangle {
                id: gameBox
                implicitWidth: 170
                implicitHeight: 32
                radius: Theme.radius
                color: Theme.surface
                border.width: 1
                border.color: gamePopup.visible ? Theme.accent : Theme.border

                readonly property int picked: (ClipsController.revision,
                                               ClipsController.gameFilterCount())
                readonly property var counts:
                    JSON.parse(ClipsController.gameCountsJson || "{}")
                /// Everything in `gameList` except its leading "All games".
                readonly property var games: ClipsController.gameList.slice(1)
                /// Height of the scrolling game list. Capped to a WHOLE
                /// number of 30px rows so the list never ends on a half-drawn
                /// one, which looked like a rendering fault rather than a
                /// scroll hint.
                readonly property int listH: Math.min(gameBox.games.length, 9) * 30

                Row {
                    anchors.left: parent.left
                    anchors.leftMargin: 10
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6

                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "gamepad"; size: 13
                        color: gameBox.picked > 0 ? Theme.accent : Theme.textDim
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: gameBox.width - 52
                        text: gameBox.picked === 0
                              ? (I18n.language, I18n.t("clips.gamesFilter.allGames"))
                              : gameBox.picked === 1
                                ? gameBox.selectedName()
                                : gameBox.picked + " "
                                  + (I18n.language, I18n.t("clips.gamesFilter.label"))
                        color: Theme.text
                        font.pixelSize: 13
                        elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter
                    }
                }
                Icon {
                    x: gameBox.width - width - 8
                    y: (gameBox.height - height) / 2
                    name: "chevron-down"; size: 12
                    color: Theme.textDim
                }

                function selectedName() {
                    for (var i = 0; i < gameBox.games.length; i++)
                        if (ClipsController.isGameFiltered(gameBox.games[i]))
                            return gameBox.games[i]
                    return ""
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: gamePopup.visible ? gamePopup.close() : gamePopup.open()
                }

                Popup {
                    id: gamePopup
                    y: gameBox.height + 2
                    width: Math.max(gameBox.width, 260)
                    // Computed from the parts rather than read off the
                    // Column's implicitHeight. `gameList` arrives after the
                    // first layout, and the Column did not re-derive its
                    // implicit height when the list grew from 0 to 280 —
                    // the popup stayed 35px tall and looked like it had
                    // failed to open. 1px divider + 32px footer + 2 padding.
                    implicitHeight: gameBox.listH + 35
                    padding: 1
                    background: Rectangle {
                        radius: Theme.radius
                        color: Theme.surface
                        border.width: 1
                        border.color: Theme.border
                    }

                    contentItem: Column {
                        id: gameCol
                        // Explicit width from the Popup, NOT the other way
                        // round: a Column derives its own width from its
                        // children, so a child taking `parent.width` resolves
                        // to 0 — which left the list 0px wide, with no
                        // delegates, no contentHeight and a popup that opened
                        // 35px tall and looked like it had failed to appear.
                        width: gamePopup.availableWidth
                        height: gameBox.listH + 33
                        spacing: 0

                        ListView {
                            width: parent.width
                            // From the model count, not contentHeight, which
                            // is only known after a layout pass that cannot
                            // happen while the width is still being resolved.
                            height: gameBox.listH
                            clip: true
                            model: gamePopup.visible ? gameBox.games : []
                            ScrollBar.vertical: ScrollBar {}

                            delegate: Rectangle {
                                id: gameRow
                                required property var modelData
                                width: ListView.view.width
                                height: 30
                                color: rowHover.containsMouse ? Theme.bgHover : "transparent"

                                readonly property bool ticked:
                                    (ClipsController.revision,
                                     ClipsController.isGameFiltered(gameRow.modelData))

                                Row {
                                    anchors.fill: parent
                                    anchors.leftMargin: 10
                                    anchors.rightMargin: 10
                                    spacing: 8

                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 22
                                        horizontalAlignment: Text.AlignRight
                                        text: String(gameBox.counts[gameRow.modelData] || 0)
                                        color: Theme.accent
                                        font.pixelSize: 11
                                        font.weight: Font.DemiBold
                                    }
                                    Rectangle {
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 14; height: 14
                                        radius: 3
                                        color: gameRow.ticked ? Theme.accent : "transparent"
                                        border.width: 1
                                        border.color: gameRow.ticked ? Theme.accent : Theme.border
                                        Icon {
                                            anchors.centerIn: parent
                                            visible: gameRow.ticked
                                            name: "check"; size: 10
                                            color: Theme.bg
                                        }
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: parent.width - 60
                                        text: gameRow.modelData
                                        color: gameRow.ticked ? Theme.text : Theme.textDim
                                        font.pixelSize: 13
                                        elide: Text.ElideRight
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                }

                                MouseArea {
                                    id: rowHover
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    // Stays open: picking several games in one
                                    // visit is the whole point of the control.
                                    onClicked: ClipsController.toggleGameFilter(gameRow.modelData)
                                }
                            }
                        }

                        Rectangle {
                            width: parent.width
                            height: 1
                            color: Theme.border
                        }
                        Rectangle {
                            width: parent.width
                            height: 32
                            color: clearHover.containsMouse && gameBox.picked > 0
                                   ? Theme.bgHover : "transparent"
                            Text {
                                anchors.centerIn: parent
                                text: (I18n.language, I18n.t("clips.gamesFilter.clear"))
                                color: gameBox.picked > 0 ? Theme.accent : Theme.textMuted
                                font.pixelSize: 12
                            }
                            MouseArea {
                                id: clearHover
                                anchors.fill: parent
                                hoverEnabled: true
                                enabled: gameBox.picked > 0
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    ClipsController.clearGameFilters()
                                    gamePopup.close()
                                }
                            }
                        }
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

            // Resets every filter at once. Clearing search, games and
            // favourites one control at a time was the only way to get back
            // to the full library.
            IconToggle {
                icon: "filter-x"
                visible: page.anyFilterActive
                tooltip: "Clear all filters"
                onTriggered: page.clearAllFilters()
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
            // The icon and slider used to sit in a bordered pill, like every
            // other control on this bar; rebuilding the row dropped it and
            // left them floating against the page.
            Rectangle {
                Layout.alignment: Qt.AlignVCenter
                Layout.preferredWidth: sizeRow.implicitWidth + 20
                Layout.preferredHeight: 32
                visible: page.viewMode === "grid"
                radius: Theme.radius
                color: Theme.bg
                border.width: 1
                border.color: Theme.border

            RowLayout {
                id: sizeRow
                anchors.centerIn: parent
                spacing: 6

                Icon {
                    Layout.alignment: Qt.AlignVCenter
                    name: "grid"; size: 13; color: Theme.textDim
                }
                Slider {
                    id: sizeSlider
                    Layout.preferredWidth: 90
                    // A Control sizes itself from its background's and
                    // handle's *implicit* size. The delegates below set only
                    // `width`/`height`, so this Slider computed an implicit
                    // height of 0 and the layout arranged it 0px tall: it
                    // still painted (children aren't clipped) but was
                    // invisible to hit-testing, which is why the handle never
                    // moved. Same defect HSlider.qml documents; fixed here by
                    // declaring implicit sizes on the handle AND giving the
                    // control a comfortable hit target.
                    Layout.preferredHeight: 22
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
                    handle: SliderHandle {
                        x: sizeSlider.leftPadding
                           + sizeSlider.visualPosition * (sizeSlider.availableWidth - width)
                        y: sizeSlider.topPadding + sizeSlider.availableHeight / 2 - height / 2
                        active: sizeSlider.pressed || sizeSlider.hovered
                    }
                }
            }
            }

            IconToggle {
                icon: "bar-chart"
                active: page.showStats
                tooltip: "Toggle clip details"
                onTriggered: page.persistView("clipsShowStats", !page.showStats)
            }
            IconToggle {
                icon: "calendar"
                active: page.dateGrouped
                tooltip: "Group by date"
                onTriggered: page.persistView("clipsDateGrouped", !page.dateGrouped)
            }
            // A fused pair rather than two separate buttons, so the pair
            // reads as one view-mode control (per design reference).
            SegmentedToggle {
                IconToggle {
                    flat: true
                    icon: "grid"
                    active: page.viewMode === "grid"
                    tooltip: "Grid view"
                    onTriggered: page.persistView("clipsViewMode", "grid")
                }
                IconToggle {
                    flat: true
                    icon: "list"
                    active: page.viewMode === "list"
                    tooltip: "List view"
                    onTriggered: page.persistView("clipsViewMode", "list")
                }
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
                // ── Change Game drop-up ──────────────────────────────
                // Retagging a batch used to be a per-clip trip through the
                // editor; this is the old bulk bar's tool, restored.
                ClipsBarButton {
                    id: gameBtn
                    label: "Change Game"
                    icon: "gamepad"
                    active: bulkGamePop.visible
                    onTriggered: bulkGamePop.visible ? bulkGamePop.close() : bulkGamePop.open()

                    Popup {
                        id: bulkGamePop
                        // Drops UP: the bar sits at the foot of the page.
                        y: -bulkGamePop.implicitHeight - 6
                        x: (gameBtn.width - bulkGamePop.width) / 2
                        width: 240
                        // Derived from its parts, never from the content
                        // Column — a Column that sizes from children which
                        // size from the Popup resolves to zero (the games
                        // filter shipped invisible that way once already).
                        implicitHeight: bulkGamePop.listH + 84
                        padding: 8
                        readonly property int listH:
                            Math.min(Math.max(bulkGamePop.rows.length, 1), 7) * 28

                        /// Library games (gameList[0] is the "All" sentinel)
                        /// narrowed by the search box.
                        readonly property var rows: {
                            const all = ClipsController.gameList.slice(1)
                            const q = gameSearch.text.trim().toLowerCase()
                            var out = []
                            for (var i = 0; i < all.length; i++) {
                                const g = String(all[i])
                                if (q.length === 0 || g.toLowerCase().indexOf(q) >= 0)
                                    out.push(g)
                            }
                            return out
                        }
                        property string chosen: ""

                        onOpened: { gameSearch.text = ""; bulkGamePop.chosen = "" }

                        background: Rectangle {
                            radius: Theme.radius
                            color: Theme.surface
                            border.width: 1
                            border.color: Theme.border
                        }

                        contentItem: Column {
                            spacing: 6

                            TextField {
                                id: gameSearch
                                width: bulkGamePop.availableWidth
                                height: 28
                                placeholderText: "Search games…"
                                placeholderTextColor: Theme.textMuted
                                color: Theme.text
                                font.pixelSize: 12
                                leftPadding: 8
                                verticalAlignment: TextInput.AlignVCenter
                                background: Rectangle {
                                    radius: Theme.radius
                                    color: Theme.bg
                                    border.width: 1
                                    border.color: gameSearch.activeFocus ? Theme.accent : Theme.border
                                }
                                // Typing a game nobody has used yet is a
                                // legitimate retag, so Enter applies the raw
                                // text when nothing in the list matches.
                                onAccepted: bulkApply(gameSearch.text.trim())
                            }

                            Item {
                                width: bulkGamePop.availableWidth
                                height: bulkGamePop.listH

                                ListView {
                                    anchors.fill: parent
                                    clip: true
                                    model: bulkGamePop.rows
                                    boundsBehavior: Flickable.StopAtBounds

                                    delegate: Rectangle {
                                        required property string modelData
                                        width: ListView.view.width
                                        height: 28
                                        radius: Theme.radius
                                        color: bulkGamePop.chosen === modelData
                                               ? Theme.accentAlpha(20)
                                               : rowArea.containsMouse ? Theme.bgHover : "transparent"
                                        Text {
                                            anchors.left: parent.left
                                            anchors.leftMargin: 8
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: parent.width - 16
                                            text: parent.modelData
                                            color: Theme.text
                                            font.pixelSize: 12
                                            elide: Text.ElideRight
                                        }
                                        MouseArea {
                                            id: rowArea
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: bulkGamePop.chosen = parent.modelData
                                        }
                                    }
                                }
                            }

                            Rectangle {
                                width: bulkGamePop.availableWidth
                                height: 28
                                radius: Theme.radius
                                readonly property string target:
                                    bulkGamePop.chosen || gameSearch.text.trim()
                                enabled: target.length > 0
                                opacity: enabled ? 1 : 0.4
                                color: Theme.accent
                                Text {
                                    anchors.centerIn: parent
                                    text: "Apply"
                                    color: Theme.text
                                    font.pixelSize: 12
                                    font.weight: Font.DemiBold
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    enabled: parent.enabled
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: bulkApply(parent.target)
                                }
                            }
                        }

                        function bulkApply(game) {
                            if (!game || game.length === 0)
                                return
                            ClipsController.setGames(page.selectedList(), game)
                            bulkGamePop.close()
                            page.clearSelection()
                        }
                    }
                }

                // ── Batch rename drop-up ─────────────────────────────────
                ClipsBarButton {
                    id: renameBtn
                    label: "Rename"
                    icon: "pencil"
                    active: bulkRenamePop.visible
                    onTriggered: bulkRenamePop.visible ? bulkRenamePop.close() : bulkRenamePop.open()

                    Popup {
                        id: bulkRenamePop
                        y: -bulkRenamePop.implicitHeight - 6
                        x: (renameBtn.width - bulkRenamePop.width) / 2
                        width: 260
                        implicitHeight: 106
                        padding: 8

                        onOpened: patternField.text = ""

                        background: Rectangle {
                            radius: Theme.radius
                            color: Theme.surface
                            border.width: 1
                            border.color: Theme.border
                        }

                        contentItem: Column {
                            spacing: 6

                            TextField {
                                id: patternField
                                width: bulkRenamePop.availableWidth
                                height: 28
                                placeholderText: "e.g. {game} run {n}"
                                placeholderTextColor: Theme.textMuted
                                color: Theme.text
                                font.pixelSize: 12
                                leftPadding: 8
                                verticalAlignment: TextInput.AlignVCenter
                                background: Rectangle {
                                    radius: Theme.radius
                                    color: Theme.bg
                                    border.width: 1
                                    border.color: patternField.activeFocus ? Theme.accent : Theme.border
                                }
                                onAccepted: bulkRenamePop.apply()
                            }
                            Text {
                                width: bulkRenamePop.availableWidth
                                text: "{n} number · {game} game · {filename} file name"
                                color: Theme.textMuted
                                font.pixelSize: 10
                                wrapMode: Text.WordWrap
                            }
                            Rectangle {
                                width: bulkRenamePop.availableWidth
                                height: 28
                                radius: Theme.radius
                                enabled: patternField.text.trim().length > 0
                                opacity: enabled ? 1 : 0.4
                                color: Theme.accent
                                Text {
                                    anchors.centerIn: parent
                                    text: "Apply"
                                    color: Theme.text
                                    font.pixelSize: 12
                                    font.weight: Font.DemiBold
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    enabled: parent.enabled
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: bulkRenamePop.apply()
                                }
                            }
                        }

                        /// Expand the pattern per clip, in selection order.
                        /// Done here rather than in Rust because the visible
                        /// rows already carry game and filename.
                        function apply() {
                            const pattern = patternField.text.trim()
                            if (pattern.length === 0)
                                return
                            const paths = page.selectedList()
                            const rows = page.visibleByPath()
                            var names = []
                            for (var i = 0; i < paths.length; i++) {
                                const c = rows[paths[i]]
                                const base = paths[i].split("/").pop().replace(/\.[^.]+$/, "")
                                names.push(pattern
                                    .replace(/\{n\}/g, String(i + 1))
                                    .replace(/\{game\}/g, (c && c.game) || "Unknown")
                                    .replace(/\{filename\}/g, base))
                            }
                            ClipsController.setCustomNames(paths, names)
                            bulkRenamePop.close()
                            page.clearSelection()
                        }
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
                text: (I18n.language, I18n.t("clips.loadError") + ": " + ClipsController.error)
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
                    text: (I18n.language, I18n.t("clips.emptyTitle"))
                    color: Theme.text
                    font.pixelSize: 18
                    font.weight: Font.Bold
                    Layout.alignment: Qt.AlignHCenter
                }
                Text {
                    text: (I18n.language, I18n.t("clips.emptyHint"))
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
                    text: (I18n.language, I18n.t("clips.noMatchTitle"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.Bold
                    Layout.alignment: Qt.AlignHCenter
                }
                Text {
                    text: (I18n.language, I18n.t("clips.noMatchHint"))
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

                    onOpened: page.activate(model.filepath, model.title, model.thumbnail, model.game)
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

                    onOpened: page.activate(model.filepath, model.title, model.thumbnail, model.game)
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
                // Roughly a row above and below the viewport, now that a
                // "row" really is one row of cards rather than a whole group.
                cacheBuffer: 300
                spacing: 10
                model: page.dateRows

                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                // ONE DELEGATE PER ROW, not per group.
                //
                // This used to be a delegate per date group, each nesting a
                // Grid + Repeater over that group's clips. The ListView
                // virtualised the groups, but nothing virtualised their
                // CONTENTS: a single large bucket ("Today") instantiated every
                // card in it at once and decoded every thumbnail with it.
                // `dateRows` flattens the groups into headers and fixed-width
                // rows so this one ListView virtualises the lot, and
                // cacheBuffer genuinely means "about a row above and below".
                delegate: Item {
                    id: rowDelegate
                    required property var modelData
                    width: groupedView.width

                    readonly property real cellW:
                        (groupedView.width - (page.clipsPerRow - 1) * 12) / page.clipsPerRow

                    height: modelData.kind === "header" ? 34
                          : modelData.kind === "row" ? rowDelegate.cellW * 0.5625 + 80
                          : 96

                    // ── Date header ───────────────────────────────────────
                    Row {
                        anchors.left: parent.left
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 4
                        spacing: 8
                        visible: rowDelegate.modelData.kind === "header"

                        Text {
                            text: rowDelegate.modelData.kind === "header"
                                  ? rowDelegate.modelData.label : ""
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
                                text: rowDelegate.modelData.kind === "header"
                                      ? String(rowDelegate.modelData.count) : ""
                                color: Theme.textDim
                                font.pixelSize: 11
                            }
                        }
                    }

                    // ── Grid row: at most `clipsPerRow` cards ─────────────
                    Row {
                        width: parent.width
                        spacing: 12
                        visible: rowDelegate.modelData.kind === "row"

                        Repeater {
                            // Empty for every other kind, so a header or a
                            // list item builds no cards at all.
                            model: rowDelegate.modelData.kind === "row"
                                   ? rowDelegate.modelData.clips : []

                            ClipCard {
                                required property var modelData
                                width: rowDelegate.cellW
                                height: rowDelegate.cellW * 0.5625 + 80

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

                                onOpened: page.activate(modelData.filepath, modelData.title, modelData.thumbnail, modelData.game)
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

                    // ── List row ──────────────────────────────────────────
                    // A 0-or-1 Repeater rather than a `visible` instance, so
                    // nothing is built for the other kinds. Note the row's
                    // clip is read through `rowDelegate`: bare `modelData`
                    // inside here is the Repeater's index, not the row.
                    Repeater {
                        model: rowDelegate.modelData.kind === "item" ? 1 : 0

                        ClipListRow {
                            readonly property var c: rowDelegate.modelData.clip
                            width: groupedView.width

                            filepath: c.filepath
                            thumbnail: c.thumbnail
                            duration: c.duration
                            title: c.title
                            game: c.game
                            filesize: c.filesize
                            favorite: c.favorite
                            created: c.created
                            clipWidth: c.width
                            clipHeight: c.height

                            selected: page.isSelected(c.filepath)
                            selectionMode: page.selectedCount > 0

                            onOpened: page.activate(c.filepath, c.title, c.thumbnail, c.game)
                            onSelectToggled: page.toggleSelect(c.filepath)
                            onFavoriteToggled: ClipsController.setFavorite(c.filepath, !c.favorite)
                            onMenuRequested: (gx, gy) => clipMenu.openAt(
                                { filepath: c.filepath, title: c.title,
                                  favorite: c.favorite, game: c.game }, gx, gy)
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
    /// True while the preview is expanded to fill the view — Main hides the
    /// nav rail on this, so "expand" really does fill the window.
    readonly property bool playerExpanded:
        page.playerClip !== null && clipPlayer.expanded

    VideoPlayer {
        id: clipPlayer
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
                    text: (I18n.language, I18n.t("clips.renameDialog.title"))
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
                        Text { anchors.centerIn: parent; text: (I18n.language, I18n.t("common.cancel")); color: Theme.text; font.pixelSize: 13 }
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
                        Text { anchors.centerIn: parent; text: (I18n.language, I18n.t("clips.renameDialog.save")); color: "#ffffff"; font.pixelSize: 13 }
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
                    text: (I18n.language, page.bulkDeleteTarget
                          ? I18n.t("clips.bulkDeleteConfirm.title").replace("{count}", page.bulkDeleteTarget.length)
                          : "")
                    color: Theme.text
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: (I18n.language, I18n.t("clips.bulkDeleteConfirm.body"))
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
                        Text { anchors.centerIn: parent; text: (I18n.language, I18n.t("common.cancel")); color: Theme.text; font.pixelSize: 13 }
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
                        Text { anchors.centerIn: parent; text: (I18n.language, I18n.t("clips.contextMenu.delete")); color: "#ffffff"; font.pixelSize: 13 }
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
                    text: (I18n.language, I18n.t("clips.deleteConfirm.title"))
                    color: Theme.text
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: (I18n.language, page.deleteTarget
                          ? I18n.t("clips.deleteConfirm.body").replace("{name}", page.deleteTarget.title)
                          : "")
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
                        Text { anchors.centerIn: parent; text: (I18n.language, I18n.t("common.cancel")); color: Theme.text; font.pixelSize: 13 }
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
                        Text { anchors.centerIn: parent; text: (I18n.language, I18n.t("clips.contextMenu.delete")); color: "#ffffff"; font.pixelSize: 13 }
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
