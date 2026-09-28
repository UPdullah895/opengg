import QtQuick
import QtQuick.Controls
import com.opengg.app

// ClipCard — one clip tile in the Clips grid. Extracted from an inline
// ClipsPage.qml delegate and rebuilt against ClipCard.vue, which the inline
// version had drifted a long way from: it was missing the film-strip
// placeholder, the capture-time badge, the heart favourite (it used ★/☆ text),
// the kebab menu, and the size/resolution/date meta pills, rendering metadata
// as plain text instead.
//
// Geometry and colours follow ClipCard.vue's scoped styles (lines 133-165):
// radius 10 (--radius-lg), 16:9 thumb on --bg-deep, badges on black at 80%,
// pills on --bg-deep, and the game label right-aligned in an accent wash.
Rectangle {
    id: card

    // ── Data (set from the delegate's role context properties) ────────────
    property string filepath: ""
    property string thumbnail: ""
    property real duration: 0
    property string title: ""
    property string game: ""
    property real filesize: 0
    property bool favorite: false
    property string created: ""
    property int clipWidth: 0
    property int clipHeight: 0

    property bool hovered: cardHover.hovered
    /// True when this clip is part of the page's multi-select set.
    property bool selected: false
    /// True while ANY clip is selected — the checkbox then stays visible on
    /// every card, not just the hovered one, so the set is readable at a glance.
    property bool selectionMode: false

    signal opened()
    signal editRequested()
    signal deleteRequested()
    signal trimRequested()
    signal favoriteToggled()
    signal selectToggled()
    signal renamed(string newName)
    /// Emitted with page-space coords for the shared context menu.
    signal menuRequested(real gx, real gy)

    /// True while the title is swapped for its inline edit field.
    property bool editingName: false

    function beginRename() {
        nameEdit.text = card.title
        card.editingName = true
        nameEdit.forceActiveFocus()
        nameEdit.selectAll()
    }

    function commitRename() {
        // Guard against re-entry: committing drops focus, which fires
        // onActiveFocusChanged, which would call straight back into here.
        if (!card.editingName)
            return
        card.editingName = false
        const v = nameEdit.text.trim()
        if (v.length > 0 && v !== card.title)
            card.renamed(v)
    }

    radius: Theme.radiusLg
    color: Theme.surface
    // Border lives in `frame` below, painted OVER the content. Drawn here it
    // sat under the thumbnail (which fills the card's full width), so the
    // hover/selected accent ring was invisible along the card's top edge.
    border.width: 0
    clip: true

    // ClipCard.vue lifts the card 2px on hover.
    transform: Translate { y: card.hovered ? -2 : 0 }

    HoverHandler { id: cardHover }

    // ── formatting (mirrors ClipCard.vue's fmt* helpers) ──────────────────
    // ── formatting: ports utils/format.ts verbatim. These had drifted:
    //   fmtDur added an hours field the original never has
    //   fmtSize printed "142 MB" where the original gives "142.4 MB"
    //   fmtDate printed ISO "2026-07-20" instead of "Jul 20, 2026"
    //   fmtTime printed 24h "09:45" instead of "9:45 AM"
    function pad2(n) { return (n < 10 ? "0" : "") + n }

    function fmtDur(sec) {
        if (!sec) return "0:00"
        return Math.floor(sec / 60) + ":" + pad2(Math.floor(sec % 60))
    }
    function fmtSize(b) {
        if (!b) return "0 B"
        var u = ["B", "KB", "MB", "GB"]
        var i = 0, v = b
        while (v >= 1024 && i < 3) { v /= 1024; i++ }
        return (i ? v.toFixed(1) : Math.round(v)) + " " + u[i]
    }
    function fmtRes(w, h) {
        if (!w) return ""
        if (h >= 2160) return "4K"
        if (h >= 1440) return "1440p"
        if (h >= 1080) return "1080p"
        if (h >= 720) return "720p"
        return w + "\u00d7" + h
    }
    readonly property var monthNames: ["Jan","Feb","Mar","Apr","May","Jun",
                                      "Jul","Aug","Sep","Oct","Nov","Dec"]
    function parseCreated(created) {
        if (!created) return null
        var d = new Date(String(created).replace(" ", "T"))
        return isNaN(d.getTime()) ? null : d
    }
    function fmtDate(created) {
        var d = parseCreated(created)
        if (!d) return ""
        return monthNames[d.getMonth()] + " " + d.getDate() + ", " + d.getFullYear()
    }
    function fmtTime(created) {
        var d = parseCreated(created)
        if (!d) return ""
        var h = d.getHours()
        var ap = h >= 12 ? "PM" : "AM"
        h = h % 12
        if (h === 0) h = 12
        return h + ":" + pad2(d.getMinutes()) + " " + ap
    }

    Column {
        anchors.fill: parent
        spacing: 0

        // ── Thumbnail ─────────────────────────────────────────────────────
        Rectangle {
            id: thumb
            width: parent.width
            height: Math.round(width * 0.5625)   // 16:9
            color: Theme.bgDeep
            clip: true

            Image {
                anchors.fill: parent
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                // Decode at display size, not the file's native 854x480.
                // A native decode costs ~1.6 MB of RGBA per thumbnail, so a
                // full library held tens of MB of pixels to draw thumbnails a
                // third that size. Bucketed to 160px steps so a window resize
                // re-decodes at most once per step instead of on every frame,
                // and capped so we never ask for MORE than the file holds.
                sourceSize.width: Math.min(854, Math.ceil(Math.max(1, thumb.width) / 160) * 160)
                visible: !!card.thumbnail
                source: card.thumbnail ? "file://" + card.thumbnail : ""
            }

            // Film-strip placeholder (ClipCard.vue:81) at 30% opacity; also
            // kicks off async thumbnail generation once.
            Icon {
                anchors.centerIn: parent
                visible: !card.thumbnail
                name: "film"
                size: 28
                color: Theme.text
                opacity: 0.3
                Component.onCompleted: {
                    if (!card.thumbnail && card.filepath)
                        ClipsController.requestThumbnail(card.filepath)
                }
            }

            // Duration badge (bottom-right)
            Rectangle {
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: 6
                visible: card.duration > 0
                radius: 4
                color: Theme.scrim(80)
                implicitWidth: durLabel.implicitWidth + 14
                implicitHeight: 18
                Text {
                    id: durLabel
                    anchors.centerIn: parent
                    text: card.fmtDur(card.duration)
                    color: "#ffffff"
                    font.pixelSize: 11
                    font.weight: Font.DemiBold
                }
            }

            // Capture-time badge (bottom-left)
            Rectangle {
                anchors.left: parent.left
                anchors.bottom: parent.bottom
                anchors.margins: 6
                visible: card.fmtTime(card.created).length > 0
                radius: 4
                color: Theme.scrim(80)
                implicitWidth: timeLabel.implicitWidth + 14
                implicitHeight: 18
                Text {
                    id: timeLabel
                    anchors.centerIn: parent
                    text: card.fmtTime(card.created)
                    color: "#ffffff"
                    font.pixelSize: 11
                    font.weight: Font.DemiBold
                }
            }

            // Favourite heart (top-right): shown on hover, or always when set.
            Rectangle {
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 6
                width: 28; height: 28
                radius: 14
                visible: card.hovered || card.favorite
                color: heartArea.containsMouse ? Theme.scrim(80) : Theme.scrim(50)
                Icon {
                    anchors.centerIn: parent
                    name: "heart"
                    size: 14
                    filled: card.favorite
                    color: card.favorite ? Theme.accent : Theme.textMuted
                }
                MouseArea {
                    id: heartArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: card.favoriteToggled()
                }
            }

            // Selection checkbox (top-left). Trim and delete used to live here
            // as direct hover buttons; they're now reachable only from the
            // right-click menu, leaving the card's two hover affordances as
            // "select" (left) and "favourite" (right).
            Rectangle {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.margins: 6
                width: 22; height: 22
                radius: 4
                visible: card.hovered || card.selected || card.selectionMode
                color: card.selected ? Theme.accent
                     : selArea.containsMouse ? Theme.scrim(80) : Theme.scrim(50)
                border.width: 1
                border.color: card.selected ? Theme.accent : Theme.tint(Theme.text, 40)
                Icon {
                    anchors.centerIn: parent
                    visible: card.selected
                    name: "check"
                    size: 13
                    color: "#ffffff"
                }
                MouseArea {
                    id: selArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: card.selectToggled()
                }
            }
        }

        // ── Info strip ────────────────────────────────────────────────────
        Item {
            width: parent.width
            height: infoCol.implicitHeight + 22

            Column {
                id: infoCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.leftMargin: 12
                anchors.rightMargin: 12
                anchors.topMargin: 10
                spacing: 6

                // Name + kebab
                Item {
                    width: parent.width
                    height: 18

                    // Click the name to rename in place (ClipCard.vue does the
                    // same). The full rename dialog still exists on the
                    // right-click menu for keyboard-driven use.
                    Item {
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 26
                        height: 20

                        Text {
                            id: nameText
                            anchors.fill: parent
                            visible: !card.editingName
                            verticalAlignment: Text.AlignVCenter
                            text: card.title
                            color: nameArea.containsMouse ? Theme.accent : Theme.text
                            font.pixelSize: 13
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }

                        MouseArea {
                            id: nameArea
                            anchors.fill: parent
                            visible: !card.editingName
                            hoverEnabled: true
                            cursorShape: Qt.IBeamCursor
                            onClicked: card.beginRename()
                        }

                        Rectangle {
                            anchors.fill: parent
                            visible: card.editingName
                            radius: 4
                            color: Theme.bgDeep
                            border.width: 1
                            border.color: Theme.accent

                            TextField {
                                id: nameEdit
                                anchors.fill: parent
                                leftPadding: 6
                                rightPadding: 6
                                verticalAlignment: TextInput.AlignVCenter
                                color: Theme.text
                                font.pixelSize: 13
                                font.weight: Font.DemiBold
                                background: Item {}
                                selectByMouse: true
                                onAccepted: card.commitRename()
                                // Clicking anywhere else (including another
                                // card) drops focus — treat that as a commit,
                                // matching the Vue input's @blur handler.
                                onActiveFocusChanged: {
                                    if (!activeFocus && card.editingName)
                                        card.commitRename()
                                }
                                Keys.onEscapePressed: card.editingName = false
                            }
                        }
                    }

                    Rectangle {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        width: 22; height: 22
                        radius: 4
                        color: kebabArea.containsMouse ? Theme.bgHover : "transparent"
                        Icon {
                            anchors.centerIn: parent
                            name: "more-vertical"
                            size: 14
                            color: kebabArea.containsMouse ? Theme.text : Theme.textMuted
                        }
                        MouseArea {
                            id: kebabArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                var p = kebabArea.mapToItem(null, 0, kebabArea.height)
                                card.menuRequested(p.x, p.y)
                            }
                        }
                    }
                }

                // Meta: size / resolution / date pills, then the game label
                // right-aligned in an accent wash (ClipCard.vue's .game).
                Item {
                    width: parent.width
                    height: 17

                    Row {
                        id: metaPills
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 6
                        // .clip-meta is a flex row in the original, so the pills
                        // shrink rather than slide under the right-aligned game
                        // badge. Anchoring alone allowed them to overlap on a
                        // narrow card.
                        width: parent.width - (gameBadge.visible ? gameBadge.width + 8 : 0)
                        clip: true

                        Repeater {
                            model: [
                                card.fmtSize(card.filesize),
                                card.fmtRes(card.clipWidth, card.clipHeight),
                                card.fmtDate(card.created)
                            ].filter(function (t) { return !!t })

                            Rectangle {
                                required property string modelData
                                required property int index
                                radius: 3
                                color: Theme.bgDeep
                                implicitWidth: Math.min(pillText.implicitWidth + 12, index === 2 ? 180 : 90)
                                implicitHeight: 17
                                // .date-pill is rendered at 75% opacity.
                                opacity: index === 2 ? 0.75 : 1.0
                                Text {
                                    id: pillText
                                    anchors.fill: parent
                                    anchors.leftMargin: 6
                                    anchors.rightMargin: 6
                                    verticalAlignment: Text.AlignVCenter
                                    text: modelData
                                    color: Theme.textMuted
                                    font.pixelSize: 11
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }

                    Rectangle {
                        id: gameBadge
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        visible: card.game.length > 0 && card.game !== "Unknown"
                        radius: 4
                        color: Theme.accentAlpha(14)
                        implicitWidth: Math.min(gameText.implicitWidth + 16, parent.width * 0.55)
                        implicitHeight: 17
                        Text {
                            id: gameText
                            anchors.fill: parent
                            anchors.leftMargin: 8
                            anchors.rightMargin: 8
                            verticalAlignment: Text.AlignVCenter
                            text: card.game
                            color: Theme.accent
                            font.pixelSize: 10
                            font.weight: Font.Bold
                            elide: Text.ElideRight
                        }
                    }
                }
            }
        }
    }

    // Border ring, painted on top of the thumbnail. See the `border.width: 0`
    // note on the root — the thumbnail spans the card's full width, so a
    // border drawn by the root Rectangle is covered along the top edge and the
    // selected/hover accent never reads as a complete ring.
    Rectangle {
        id: frame
        anchors.fill: parent
        z: 5
        radius: card.radius
        color: "transparent"
        border.width: card.selected ? 2 : 1
        border.color: card.selected ? Theme.accent
                    : card.hovered ? Theme.accent
                    : Theme.tint(Theme.text, 12)
    }

    // Opens the player. Sits below the action buttons so they win the click.
    MouseArea {
        id: cardArea
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: (m) => {
            if (m.button === Qt.RightButton) {
                var p = card.mapToItem(null, m.x, m.y)
                card.menuRequested(p.x, p.y)
            } else {
                card.opened()
            }
        }
        z: -1
    }
}
