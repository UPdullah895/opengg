import QtQuick
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

    signal opened()
    signal editRequested()
    signal deleteRequested()
    signal trimRequested()
    signal favoriteToggled()
    signal menuRequested()

    radius: Theme.radiusLg
    color: Theme.surface
    border.width: 1
    border.color: hovered ? Theme.accent : Theme.border
    clip: true

    // ClipCard.vue lifts the card 2px on hover.
    transform: Translate { y: card.hovered ? -2 : 0 }

    HoverHandler { id: cardHover }

    // ── formatting (mirrors ClipCard.vue's fmt* helpers) ──────────────────
    function fmtDur(sec) {
        if (!sec || sec <= 0)
            return ""
        var s = Math.round(sec)
        var h = Math.floor(s / 3600)
        var m = Math.floor((s % 3600) / 60)
        var ss = s % 60
        var mm = (h > 0 && m < 10 ? "0" : "") + m
        return (h > 0 ? h + ":" : "") + mm + ":" + (ss < 10 ? "0" : "") + ss
    }
    function fmtSize(bytes) {
        if (!bytes || bytes <= 0)
            return ""
        var mb = bytes / (1024 * 1024)
        return mb >= 1024 ? (mb / 1024).toFixed(1) + " GB" : Math.round(mb) + " MB"
    }
    function fmtRes(w, h) {
        if (!w || !h)
            return ""
        // ClipCard.vue labels the common heights rather than printing WxH.
        if (h >= 2160) return "4K"
        if (h >= 1440) return "1440p"
        if (h >= 1080) return "1080p"
        if (h >= 720) return "720p"
        return w + "x" + h
    }
    // `created` arrives as a preformatted string from core; take the date and
    // clock parts out of it rather than reparsing into a Date.
    function datePart(s) {
        if (!s) return ""
        var t = s.indexOf(" ")
        return t > 0 ? s.substring(0, t) : s
    }
    function timePart(s) {
        if (!s) return ""
        var t = s.indexOf(" ")
        if (t < 0) return ""
        var rest = s.substring(t + 1)
        // Trim seconds: "14:23:07" -> "14:23"
        var p = rest.split(":")
        return p.length >= 2 ? p[0] + ":" + p[1] : rest
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
                visible: card.timePart(card.created).length > 0
                radius: 4
                color: Theme.scrim(80)
                implicitWidth: timeLabel.implicitWidth + 14
                implicitHeight: 18
                Text {
                    id: timeLabel
                    anchors.centerIn: parent
                    text: card.timePart(card.created)
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

            // Trim + delete (top-left cluster, hover-only). ClipCard.vue routes
            // these through its kebab context menu; a QML port of that popup is
            // tracked separately, so they stay as direct hover actions here.
            Row {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.margins: 6
                spacing: 4
                visible: card.hovered

                Rectangle {
                    width: 26; height: 26; radius: 13
                    color: trimArea.containsMouse ? Theme.accent : Theme.scrim(50)
                    Icon { anchors.centerIn: parent; name: "scissors"; size: 13; color: "#ffffff" }
                    MouseArea {
                        id: trimArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: card.trimRequested()
                    }
                }
                Rectangle {
                    width: 26; height: 26; radius: 13
                    color: delArea.containsMouse ? Theme.danger : Theme.scrim(50)
                    Icon { anchors.centerIn: parent; name: "trash"; size: 13; color: "#ffffff" }
                    MouseArea {
                        id: delArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: card.deleteRequested()
                    }
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

                    Text {
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 26
                        text: card.title
                        color: Theme.text
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
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
                            onClicked: card.menuRequested()
                        }
                    }
                }

                // Meta: size / resolution / date pills, then the game label
                // right-aligned in an accent wash (ClipCard.vue's .game).
                Item {
                    width: parent.width
                    height: 17

                    Row {
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 6

                        Repeater {
                            model: [
                                card.fmtSize(card.filesize),
                                card.fmtRes(card.clipWidth, card.clipHeight),
                                card.datePart(card.created)
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

    // Opens the player. Sits below the action buttons so they win the click.
    MouseArea {
        id: cardArea
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: card.opened()
        z: -1
    }
}
