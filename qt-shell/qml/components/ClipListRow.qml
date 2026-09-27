import QtQuick
import com.opengg.app

// One clip as a horizontal row — the Clips page's list view. Port of
// ClipListRow.vue: small 16:9 thumb, title, meta pills, game badge, and the
// same select/favourite affordances the grid card carries.
//
// Deliberately shares ClipCard's signal surface (opened/selectToggled/
// favoriteToggled/menuRequested/renamed…) so ClipsPage can wire either
// delegate with the same handlers.
Rectangle {
    id: row

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

    property bool selected: false
    property bool selectionMode: false
    readonly property bool hovered: rowHover.hovered

    signal opened()
    signal deleteRequested()
    signal trimRequested()
    signal favoriteToggled()
    signal selectToggled()
    signal renamed(string newName)
    signal menuRequested(real gx, real gy)

    height: 96
    radius: Theme.radius
    color: row.selected ? Theme.accentAlpha(10)
         : row.hovered ? Theme.bgHover
         : Theme.surface
    border.width: 1
    border.color: row.selected || row.hovered ? Theme.accent : Theme.tint(Theme.text, 12)

    HoverHandler { id: rowHover }

    // Reuses ClipCard's formatters — kept as local copies rather than a shared
    // singleton because they're four one-liners and a singleton import here
    // would be the only cross-component coupling in components/.
    function pad2(n) { return (n < 10 ? "0" : "") + n }
    function fmtDur(sec) {
        if (!sec) return "0:00"
        return Math.floor(sec / 60) + ":" + pad2(Math.floor(sec % 60))
    }
    function fmtSize(b) {
        if (!b) return ""
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
        return w + "×" + h
    }
    readonly property var monthNames: ["Jan","Feb","Mar","Apr","May","Jun",
                                       "Jul","Aug","Sep","Oct","Nov","Dec"]
    function fmtDate(created) {
        if (!created) return ""
        var d = new Date(String(created).replace(" ", "T"))
        if (isNaN(d.getTime())) return ""
        return monthNames[d.getMonth()] + " " + d.getDate() + ", " + d.getFullYear()
    }

    // Opens the clip. Below the controls so they win the click (same z:-1
    // trick ClipCard uses).
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        z: -1
        onClicked: (m) => {
            if (m.button === Qt.RightButton) {
                var p = row.mapToItem(null, m.x, m.y)
                row.menuRequested(p.x, p.y)
            } else {
                row.opened()
            }
        }
    }

    Row {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: 10
        anchors.rightMargin: 10
        spacing: 12

        // Selection checkbox
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 20; height: 20
            radius: 4
            visible: row.hovered || row.selected || row.selectionMode
            color: row.selected ? Theme.accent : "transparent"
            border.width: 1
            border.color: row.selected ? Theme.accent : Theme.tint(Theme.text, 40)
            Icon {
                anchors.centerIn: parent
                visible: row.selected
                name: "check"; size: 12; color: "#ffffff"
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: row.selectToggled()
            }
        }

        // Thumbnail — sized to make list mode a genuinely bigger preview than
        // the grid card gets, since a row has the full page width to spend on
        // one thumbnail instead of splitting it across several columns.
        Rectangle {
            id: thumb
            anchors.verticalCenter: parent.verticalCenter
            width: 140; height: 79
            radius: 4
            color: Theme.bgDeep
            clip: true

            Image {
                anchors.fill: parent
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                // The thumb is a fixed 140x79; decoding the source at its
                // native 854x480 would cost ~1.6 MB of RGBA to paint 140px.
                // 160 covers it with room for the crop.
                sourceSize.width: 160
                visible: !!row.thumbnail
                source: row.thumbnail ? "file://" + row.thumbnail : ""
            }
            Icon {
                anchors.centerIn: parent
                visible: !row.thumbnail
                name: "film"; size: 18
                color: Theme.text
                opacity: 0.3
                Component.onCompleted: {
                    if (!row.thumbnail && row.filepath)
                        ClipsController.requestThumbnail(row.filepath)
                }
            }
            Rectangle {
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: 3
                visible: row.duration > 0
                radius: 3
                color: Theme.scrim(80)
                implicitWidth: dur.implicitWidth + 10
                implicitHeight: 14
                Text {
                    id: dur
                    anchors.centerIn: parent
                    text: row.fmtDur(row.duration)
                    color: "#ffffff"
                    font.pixelSize: 10
                    font.weight: Font.DemiBold
                }
            }
        }

        // Title + meta
        Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - thumb.width - 20 - 12 * 3
                   - (gameBadge.visible ? gameBadge.width + 12 : 0)
                   - favBtn.width - kebab.width - 24
            spacing: 4

            Text {
                width: parent.width
                text: row.title
                color: Theme.text
                font.pixelSize: 13
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: [row.fmtSize(row.filesize),
                       row.fmtRes(row.clipWidth, row.clipHeight),
                       row.fmtDate(row.created)]
                      .filter(function (t) { return !!t }).join("  ·  ")
                color: Theme.textMuted
                font.pixelSize: 11
                elide: Text.ElideRight
            }
        }

        Item { width: 1; height: 1 }

        Rectangle {
            id: gameBadge
            anchors.verticalCenter: parent.verticalCenter
            visible: row.game.length > 0 && row.game !== "Unknown"
            radius: 4
            color: Theme.accentAlpha(14)
            implicitWidth: Math.min(gameText.implicitWidth + 16, 160)
            implicitHeight: 18
            Text {
                id: gameText
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                verticalAlignment: Text.AlignVCenter
                text: row.game
                color: Theme.accent
                font.pixelSize: 10
                font.weight: Font.Bold
                elide: Text.ElideRight
            }
        }

        Rectangle {
            id: favBtn
            anchors.verticalCenter: parent.verticalCenter
            width: 26; height: 26; radius: 13
            visible: row.hovered || row.favorite
            color: favArea.containsMouse ? Theme.bgHover : "transparent"
            Icon {
                anchors.centerIn: parent
                name: "heart"; size: 14
                filled: row.favorite
                color: row.favorite ? Theme.accent : Theme.textMuted
            }
            MouseArea {
                id: favArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: row.favoriteToggled()
            }
        }

        Rectangle {
            id: kebab
            anchors.verticalCenter: parent.verticalCenter
            width: 24; height: 24; radius: 4
            color: kebabArea.containsMouse ? Theme.bgHover : "transparent"
            Icon {
                anchors.centerIn: parent
                name: "more-vertical"; size: 14
                color: kebabArea.containsMouse ? Theme.text : Theme.textMuted
            }
            MouseArea {
                id: kebabArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    var p = kebabArea.mapToItem(null, 0, kebabArea.height)
                    row.menuRequested(p.x, p.y)
                }
            }
        }
    }
}
