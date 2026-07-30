import QtQuick
import QtQuick.Shapes
import QtQuick.Layouts
import com.opengg.app

Rectangle {
    id: sidebar
    width: Theme.sidebarW    // --sidebar-w
    color: Theme.bg

    signal navigate(string page)
    property string currentPage: "home"

    // Navigation items: stable id (used for routing), i18n key, SVG stroke path.
    property var navItems: [
        {
            id: "home",
            tkey: "nav.home",
            d: "M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-4 0a1 1 0 01-1-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 01-1 1h-2z"
        },
        {
            id: "mixer",
            tkey: "nav.mixer",
            d: "M4 21V14m0-4V3m8 18V12m0-4V3m8 18V16m0-4V3M1 14h6M9 8h6M17 16h6"
        },
        {
            id: "clips",
            tkey: "nav.clips",
            d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M3 6h10a2 2 0 012 2v8a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2z"
        },
        {
            id: "devices",
            tkey: "nav.devices",
            d: "M3 18v-6a9 9 0 0 1 18 0v6M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"
        },
        {
            id: "settings",
            tkey: "nav.settings",
            d: "M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"
        }
    ]

    // Right border
    Rectangle {
        anchors.right: parent.right
        width: 1
        height: parent.height
        color: Theme.border
    }

    ColumnLayout {
        anchors.top: parent.top
        anchors.topMargin: 8
        anchors.left: parent.left
        anchors.leftMargin: 8
        width: parent.width - 16
        spacing: 2
        anchors.bottomMargin: 40

        Repeater {
            model: sidebar.navItems

            Rectangle {
                id: navItem
                Layout.fillWidth: true
                Layout.preferredHeight: 48
                radius: Theme.radius
                property bool active: sidebar.currentPage === modelData.id
                property bool hovered: navMouse.containsMouse
                color: (active || hovered) ? Theme.accentAlpha(10) : "transparent"
                border.width: active ? 1 : 0
                border.color: Theme.accent

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 12
                    spacing: 12

                    // SVG stroke icon in a 24×24 shape
                    Shape {
                        id: iconShape
                        Layout.preferredWidth: 24
                        Layout.preferredHeight: 24
                        Layout.alignment: Qt.AlignVCenter

                        ShapePath {
                            strokeColor: navItem.active || navItem.hovered ? Theme.accent : Theme.textDim
                            strokeWidth: 2
                            fillColor: "transparent"
                            capStyle: ShapePath.RoundCap
                            joinStyle: ShapePath.RoundJoin

                            PathSvg {
                                path: modelData.d
                            }
                        }
                    }

                    Text {
                        // Reading I18n.language makes this binding re-evaluate live
                        // when the language changes (invokables alone aren't tracked).
                        text: (I18n.language, I18n.t(modelData.tkey))
                        color: navItem.active ? Theme.text : (navItem.hovered ? Theme.accent : Theme.textDim)
                        font.pixelSize: 14
                        font.weight: Font.Medium
                        Layout.fillWidth: true
                    }

                    Layout.fillHeight: true
                }

                MouseArea {
                    id: navMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: sidebar.navigate(modelData.id)
                }

                Component.onCompleted: TourController.registerTarget("nav-" + modelData.id, navItem)
                Component.onDestruction: TourController.unregisterTarget("nav-" + modelData.id)
            }
        }

        Layout.fillHeight: true
    }

    // Bottom tip strip (Sidebar.vue:34-41) — absent from the original port.
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.rightMargin: 1          // clear the right border line
        height: 34
        color: "transparent"

        Rectangle {
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            height: 1
            color: Theme.border
        }

        Row {
            anchors.left: parent.left
            anchors.leftMargin: 12
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 7

            Icon {
                name: "info"
                size: 13
                color: Theme.accent
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                width: parent.width - 26
                text: (I18n.language, I18n.t("sidebar.tip"))
                color: Theme.textMuted
                font.pixelSize: 10
                elide: Text.ElideRight
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }

    // TODO(i18n): migrate to qsTrId + JsonTranslator per plan §3.1
}
