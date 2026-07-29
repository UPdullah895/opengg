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
    // Clips targeted by the rename / delete dialogs (null = dialog closed).
    property var renameTarget: null
    property var deleteTarget: null

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
                    onTextChanged: ClipsController.setSearchText(text)
                }
            }

            Item { Layout.fillWidth: true }

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
        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: ClipsController.count > 0
            clip: true
            cacheBuffer: 400

            readonly property int columns: Math.max(1, Math.floor(width / 260))
            cellWidth: width / columns
            cellHeight: cellWidth * 0.5625 + 62  // 16:9 thumb + info strip

            model: ClipsController

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
                                visible: !!thumbnail
                                source: thumbnail ? "file://" + thumbnail : ""
                            }

                            // Placeholder when no thumbnail; kicks off async
                            // generation once (deduped on the Rust side across
                            // reloads/re-mounts) so the image pops in when ready.
                            Text {
                                anchors.centerIn: parent
                                visible: !thumbnail
                                text: "▶"
                                color: Theme.border
                                font.pixelSize: 34

                                Component.onCompleted: {
                                    if (!thumbnail && filepath) {
                                        ClipsController.requestThumbnail(filepath)
                                    }
                                }
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
                                    text: page.fmtDuration(duration)
                                    color: "#ffffff"
                                    font.pixelSize: 11
                                }
                            }

                        }

                        // Info strip
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.margins: 8
                            spacing: 2

                            Text {
                                Layout.fillWidth: true
                                text: title
                                color: Theme.text
                                font.pixelSize: 13
                                font.weight: Font.Medium
                                elide: Text.ElideRight
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 6
                                Text {
                                    text: game || "Unknown"
                                    color: Theme.textDim
                                    font.pixelSize: 11
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: page.fmtSize(filesize)
                                    color: Theme.textDim
                                    font.pixelSize: 11
                                }
                            }
                        }
                    }

                    // Opens the player (sits below the action buttons).
                    MouseArea {
                        id: cardArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: page.playerClip = { filepath: filepath, title: title }
                    }

                    // Card-level hover — stays true over the action buttons too.
                    HoverHandler { id: cardHover }

                    // Favorite toggle (top-left): shown on hover or when favorited.
                    Rectangle {
                        anchors.left: parent.left
                        anchors.top: parent.top
                        anchors.margins: 6
                        width: 26; height: 26; radius: 13
                        visible: cardHover.hovered || !!favorite
                        color: favArea.containsMouse ? "#aa000000" : "#66000000"
                        Text {
                            anchors.centerIn: parent
                            text: favorite ? "★" : "☆"
                            color: favorite ? "#fbbf24" : "#ffffff"
                            font.pixelSize: 15
                        }
                        MouseArea {
                            id: favArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: ClipsController.setFavorite(filepath, !favorite)
                        }
                    }

                    // Rename + delete (top-right): shown on hover.
                    Row {
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 6
                        spacing: 4
                        visible: cardHover.hovered

                        Rectangle {
                            width: 26; height: 26; radius: 13
                            color: renameArea.containsMouse ? Theme.accent : "#66000000"
                            Text { anchors.centerIn: parent; text: "✎"; color: "#ffffff"; font.pixelSize: 13 }
                            MouseArea {
                                id: renameArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: page.renameTarget = { filepath: filepath, title: title }
                            }
                        }
                        Rectangle {
                            width: 26; height: 26; radius: 13
                            color: delArea.containsMouse ? "#dc2626" : "#66000000"
                            Text { anchors.centerIn: parent; text: "🗑"; color: "#ffffff"; font.pixelSize: 12 }
                            MouseArea {
                                id: delArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: page.deleteTarget = { filepath: filepath, title: title }
                            }
                        }
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
        title: page.playerClip ? page.playerClip.title : ""
        onClosed: page.playerClip = null
    }

    // ── Rename dialog ─────────────────────────────────────────────────────
    Rectangle {
        anchors.fill: parent
        color: "#cc0b0d13"
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

    // ── Delete confirmation ───────────────────────────────────────────────
    Rectangle {
        anchors.fill: parent
        color: "#cc0b0d13"
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
                        color: "#dc2626"
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
