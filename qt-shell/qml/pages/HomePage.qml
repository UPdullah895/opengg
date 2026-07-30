import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Dashboard — QML port of HomePage.vue.
//
// REWRITE, not a restyle. The previous version of this file was invented UI (a
// "Replay Buffer" card with a Start button, plus a list of channel sliders) and
// shared no structure with the real dashboard, which is a four-card stat grid
// above a changelog feed. See
// docs/superpowers/plans/2026-07-30-screenshot-gap-audit.md G1.
//
// Not yet ported: the click-to-expand popover on each card (quick mixer strips,
// recorder settings rows, recent clips, device list) and the dismissible
// GSR-missing banner. Those are interactive layers over this resting state and
// are tracked separately so this lands as a reviewable whole.
Rectangle {
    id: page
    color: Theme.bg

    // Card badge colours are hardcoded in HomePage.vue's scoped CSS
    // (.card-icon.accent/.red/.green/.purple), exactly like the channel identity
    // palette in MixerPage. Note that `.accent` there is BLUE (#3b82f6), not the
    // theme accent — a naming quirk in the original, preserved deliberately.
    readonly property var cardColors: ({
        mixer: "#3b82f6", recorder: "#EF4444", clips: "#10b981", devices: "#a855f7"
    })

    readonly property var cards: [
        {
            key: "mixer", icon: "sliders",
            label: I18n.t("dashboard.audioMixer"),
            value: I18n.t("dashboard.channelSummary"),
            sub: I18n.t("dashboard.channelList")
        },
        {
            key: "recorder", icon: "record",
            label: I18n.t("dashboard.recorder"),
            value: RecordingController.running ? I18n.t("dashboard.active")
                                               : I18n.t("dashboard.idle"),
            sub: RecordingController.statusText
        },
        {
            key: "clips", icon: "video",
            label: I18n.t("dashboard.clipsCard"),
            value: String(ClipsController.totalCount),
            sub: I18n.t("dashboard.videoClipsSaved")
        },
        {
            key: "devices", icon: "headphones",
            label: I18n.t("dashboard.devices"),
            value: I18n.t("dashboard.scan"),
            sub: I18n.t("dashboard.devicesSub")
        }
    ]

    // Changelog entries. `I18n.tRaw` returns the raw catalog subtree: the
    // flattened catalog keeps string leaves only, so array-valued locale content
    // was unreachable from QML until it was added (mirrors vue-i18n's `tm()`).
    readonly property var updates: {
        try {
            var a = JSON.parse(I18n.tRaw("dashboard.changelog"))
            return Array.isArray(a) ? a : []
        } catch (e) {
            return []
        }
    }
    property bool showAllUpdates: false

    Component.onCompleted: {
        AudioController.refresh()
        RecordingController.refresh()
    }

    // Light polling so external volume/recording state changes show up live.
    Timer {
        interval: 2000
        running: true
        repeat: true
        onTriggered: {
            AudioController.refresh()
            RecordingController.refresh()
        }
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            id: dashboardCol
            width: page.width - 64
            x: 32
            y: 28
            spacing: 18

            Component.onCompleted: TourController.registerTarget("home-dashboard", dashboardCol)
            Component.onDestruction: TourController.unregisterTarget("home-dashboard")

            // ── Title row ─────────────────────────────────────────────────
            RowLayout {
                Layout.fillWidth: true
                spacing: 10

                Text {
                    text: (I18n.language, I18n.t("dashboard.title"))
                    color: Theme.text
                    font.pixelSize: 26
                    font.weight: Font.Bold
                }
                // Replays the guided tour, as in HomePage.vue's title row.
                Rectangle {
                    width: 22; height: 22; radius: 11
                    color: tourBtn.containsMouse ? Theme.accentAlpha(15) : "transparent"
                    Layout.alignment: Qt.AlignVCenter
                    Icon {
                        anchors.centerIn: parent
                        name: "info"
                        size: 14
                        color: tourBtn.containsMouse ? Theme.accent : Theme.textMuted
                    }
                    MouseArea {
                        id: tourBtn
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: TourController.start()
                    }
                }
                Item { Layout.fillWidth: true }
            }

            // ── Four-card stat grid ───────────────────────────────────────
            GridLayout {
                Layout.fillWidth: true
                columns: 4
                columnSpacing: 14
                rowSpacing: 14

                Repeater {
                    model: page.cards

                    Rectangle {
                        id: statCard
                        required property var modelData
                        readonly property color badge: page.cardColors[modelData.key]

                        Layout.fillWidth: true
                        Layout.preferredHeight: 118
                        radius: Theme.radiusLg
                        color: Theme.surface
                        border.width: 1
                        border.color: cardHover.hovered ? Theme.accent : Theme.border

                        HoverHandler { id: cardHover }

                        // Registered so the tour can spotlight the recorder card.
                        Component.onCompleted: {
                            if (modelData.key === "recorder")
                                TourController.registerTarget("home-recorder", statCard)
                        }

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: 18
                            spacing: 0

                            RowLayout {
                                Layout.fillWidth: true
                                Text {
                                    text: statCard.modelData.label.toUpperCase()
                                    color: Theme.textDim
                                    font.pixelSize: 11
                                    font.weight: Font.Bold
                                    Layout.fillWidth: true
                                }
                                Rectangle {
                                    width: 36; height: 36
                                    radius: Theme.radius
                                    color: Theme.tint(statCard.badge, 10)
                                    Icon {
                                        anchors.centerIn: parent
                                        name: statCard.modelData.icon
                                        size: 18
                                        color: statCard.badge
                                    }
                                }
                            }

                            Item { Layout.fillHeight: true }

                            Text {
                                text: statCard.modelData.value
                                color: Theme.text
                                font.pixelSize: 22
                                font.weight: Font.Bold
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                            }
                            Text {
                                text: statCard.modelData.sub
                                color: Theme.textMuted
                                font.pixelSize: 11
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }

            // ── Latest Engine Updates ─────────────────────────────────────
            Text {
                text: (I18n.language, I18n.t("dashboard.latestUpdates"))
                color: Theme.text
                font.pixelSize: 15
                font.weight: Font.Bold
                Layout.topMargin: 6
            }

            Repeater {
                model: page.showAllUpdates ? page.updates.length
                                           : Math.min(1, page.updates.length)

                Rectangle {
                    id: updateCard
                    required property int index
                    readonly property var entry: page.updates[index]

                    Layout.fillWidth: true
                    radius: Theme.radiusLg
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.border
                    implicitHeight: upCol.implicitHeight + 32

                    ColumnLayout {
                        id: upCol
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 16
                        spacing: 8

                        RowLayout {
                            Layout.fillWidth: true
                            Text {
                                text: updateCard.entry ? updateCard.entry.version : ""
                                color: Theme.accent
                                font.pixelSize: 13
                                font.weight: Font.Bold
                                Layout.fillWidth: true
                            }
                            Text {
                                text: updateCard.entry ? updateCard.entry.date : ""
                                color: Theme.textMuted
                                font.pixelSize: 11
                            }
                        }

                        Repeater {
                            model: updateCard.entry ? updateCard.entry.items : []
                            RowLayout {
                                required property string modelData
                                Layout.fillWidth: true
                                spacing: 8
                                Text {
                                    text: "•"
                                    color: Theme.accent
                                    font.pixelSize: 13
                                    Layout.alignment: Qt.AlignTop
                                }
                                Text {
                                    text: modelData
                                    color: Theme.textDim
                                    font.pixelSize: 12
                                    wrapMode: Text.WordWrap
                                    Layout.fillWidth: true
                                }
                            }
                        }
                    }
                }
            }

            Text {
                visible: page.updates.length > 1
                text: page.showAllUpdates
                      ? (I18n.language, I18n.t("dashboard.showLess"))
                      : (page.updates.length - 1) + " more updates"
                color: Theme.accent
                font.pixelSize: 12
                font.weight: Font.Bold
                Layout.bottomMargin: 24

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: page.showAllUpdates = !page.showAllUpdates
                }
            }
        }
    }
}
