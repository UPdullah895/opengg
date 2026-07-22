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
                    text: page.clips.length + (page.clips.length === 1 ? " clip" : " clips")
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

        // ── Clip grid ─────────────────────────────────────────────────────
        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: page.clips.length > 0
            clip: true
            cacheBuffer: 400

            readonly property int columns: Math.max(1, Math.floor(width / 260))
            cellWidth: width / columns
            cellHeight: cellWidth * 0.5625 + 62  // 16:9 thumb + info strip

            model: page.clips

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
                        // Playback wired in the VideoPlayer slice (plan S5).
                    }
                }
            }
        }
    }
}
