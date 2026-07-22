import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Clips — the local clip library, read live from opengg_core::clips (SQLite +
// filesystem scan) via ClipsController. Grid of clip cards with cached
// thumbnails; per-clip actions + the video player land in later Phase 2 slices.
Rectangle {
    id: page
    color: Theme.bg

    property var clips: ClipsController.clipsJson
        ? JSON.parse(ClipsController.clipsJson)
        : []

    // Clip currently open in the player overlay (null = closed).
    property var playerClip: null

    // ── Toolbar filter/sort state ─────────────────────────────────────────
    property string searchText: ""
    property string sortMode: "newest"   // newest | oldest | longest | shortest
    property string gameFilter: ""        // "" = all games

    // Distinct game names present in the library (for the filter dropdown).
    property var gameList: {
        var seen = {}
        for (var i = 0; i < clips.length; i++) {
            var g = clips[i].game || "Unknown"
            seen[g] = true
        }
        return ["All games"].concat(Object.keys(seen).sort())
    }

    // Clips after search + game filter + sort — the grid's actual model.
    property var filteredClips: {
        var q = searchText.toLowerCase()
        var out = clips.filter(function (c) {
            if (gameFilter && (c.game || "Unknown") !== gameFilter)
                return false
            if (q.length > 0) {
                var name = displayName(c).toLowerCase()
                var game = (c.game || "").toLowerCase()
                if (name.indexOf(q) < 0 && game.indexOf(q) < 0)
                    return false
            }
            return true
        }).slice()
        if (sortMode === "newest")
            out.sort(function (a, b) { return (b.createdTs || 0) - (a.createdTs || 0) })
        else if (sortMode === "oldest")
            out.sort(function (a, b) { return (a.createdTs || 0) - (b.createdTs || 0) })
        else if (sortMode === "longest")
            out.sort(function (a, b) { return (b.duration || 0) - (a.duration || 0) })
        else if (sortMode === "shortest")
            out.sort(function (a, b) { return (a.duration || 0) - (b.duration || 0) })
        return out
    }

    Component.onCompleted: ClipsController.refresh()

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
    function displayName(clip) {
        return clip.customName || clip.custom_name || clip.game || clip.filename || "Untitled"
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
                visible: page.clips.length > 0
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
                    text: page.filteredClips.length === page.clips.length
                          ? page.clips.length + (page.clips.length === 1 ? " clip" : " clips")
                          : page.filteredClips.length + " of " + page.clips.length
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
            visible: page.clips.length > 0
            spacing: 10

            // Search
            Rectangle {
                Layout.fillWidth: true
                Layout.maximumWidth: 360
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
                    onTextChanged: page.searchText = text
                }
            }

            Item { Layout.fillWidth: true }

            // Game filter
            ComboBox {
                id: gameBox
                implicitWidth: 170
                implicitHeight: 32
                model: page.gameList
                currentIndex: 0
                onActivated: page.gameFilter = (currentIndex === 0 ? "" : currentText)
                // reset selection if the game list changes out from under us
                Connections {
                    target: page
                    function onGameListChanged() {
                        if (gameBox.currentIndex >= page.gameList.length)
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
                indicator: Text {
                    x: gameBox.width - width - 8
                    y: (gameBox.height - height) / 2
                    text: "▾"
                    color: Theme.textDim
                    font.pixelSize: 11
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
                onActivated: page.sortMode = currentValue
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
                indicator: Text {
                    x: sortBox.width - width - 8
                    y: (sortBox.height - height) / 2
                    text: "▾"
                    color: Theme.textDim
                    font.pixelSize: 11
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
        }

        // ── Error banner ──────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            visible: ClipsController.error.length > 0
            radius: Theme.radius
            color: "#3a1a1f"
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
            visible: page.clips.length === 0 && !ClipsController.loading
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
            visible: page.clips.length > 0 && page.filteredClips.length === 0
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
        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: page.filteredClips.length > 0
            clip: true
            cacheBuffer: 400

            readonly property int columns: Math.max(1, Math.floor(width / 260))
            cellWidth: width / columns
            cellHeight: cellWidth * 0.5625 + 62  // 16:9 thumb + info strip

            model: page.filteredClips

            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            delegate: Item {
                width: grid.cellWidth
                height: grid.cellHeight

                Rectangle {
                    id: card
                    anchors.fill: parent
                    anchors.margins: 6
                    radius: Theme.radius
                    color: Theme.surface
                    border.width: 1
                    border.color: cardArea.containsMouse ? Theme.accent : Theme.border

                    ColumnLayout {
                        anchors.fill: parent
                        spacing: 0

                        // Thumbnail (16:9)
                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: width * 0.5625
                            radius: Theme.radius
                            color: "#0b0d13"
                            clip: true

                            // Cached thumbnail if present
                            Image {
                                anchors.fill: parent
                                fillMode: Image.PreserveAspectCrop
                                asynchronous: true
                                cache: true
                                visible: !!modelData.thumbnail
                                source: modelData.thumbnail ? "file://" + modelData.thumbnail : ""
                            }

                            // Placeholder when no thumbnail
                            Text {
                                anchors.centerIn: parent
                                visible: !modelData.thumbnail
                                text: "▶"
                                color: Theme.border
                                font.pixelSize: 34
                            }

                            // Duration pill
                            Rectangle {
                                anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                anchors.margins: 6
                                radius: 3
                                color: "#cc000000"
                                implicitWidth: durText.implicitWidth + 10
                                implicitHeight: 18
                                Text {
                                    id: durText
                                    anchors.centerIn: parent
                                    text: page.fmtDuration(modelData.duration)
                                    color: "#ffffff"
                                    font.pixelSize: 11
                                }
                            }

                            // Favorite star
                            Text {
                                anchors.left: parent.left
                                anchors.top: parent.top
                                anchors.margins: 6
                                visible: !!modelData.favorite
                                text: "★"
                                color: "#fbbf24"
                                font.pixelSize: 15
                            }
                        }

                        // Info strip
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.margins: 8
                            spacing: 2

                            Text {
                                Layout.fillWidth: true
                                text: page.displayName(modelData)
                                color: Theme.text
                                font.pixelSize: 13
                                font.weight: Font.Medium
                                elide: Text.ElideRight
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 6
                                Text {
                                    text: modelData.game || "Unknown"
                                    color: Theme.textDim
                                    font.pixelSize: 11
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: page.fmtSize(modelData.filesize)
                                    color: Theme.textDim
                                    font.pixelSize: 11
                                }
                            }
                        }
                    }

                    MouseArea {
                        id: cardArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: page.playerClip = modelData
                    }
                }
            }
        }
    }

    // ── Player overlay ────────────────────────────────────────────────────
    VideoPlayer {
        anchors.fill: parent
        visible: page.playerClip !== null
        source: page.playerClip ? "file://" + page.playerClip.filepath : ""
        title: page.playerClip ? page.displayName(page.playerClip) : ""
        onClosed: page.playerClip = null
    }
}
