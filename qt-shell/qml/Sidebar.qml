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

    // Core-module toggles (Settings → Extensions → Modules) hide the
    // corresponding nav entry entirely rather than leaving a dead link —
    // home/settings have no backing module and always stay visible.
    property var modules: JSON.parse(ExtensionsController.modulesJson || "{}")
    readonly property var moduleForNav: ({ mixer: "audio", devices: "device", clips: "replay" })
    readonly property var visibleNavItems: sidebar.navItems.filter(function(item) {
        const modKey = sidebar.moduleForNav[item.id]
        return !modKey || sidebar.modules[modKey] !== false
    })

    // Navigation items: stable id (used for routing), i18n key, SVG stroke path.
    property var navItems: [
        {
            id: "home",
            tkey: "nav.home",
            d: "M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 0 0 1 1h3m10-11l2 2m-2-2v10a1 1 0 0 1 -1 1h-3m-4 0a1 1 0 0 1 -1-1v-4a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v4a1 1 0 0 1 -1 1h-2z"
        },
        {
            id: "mixer",
            tkey: "nav.mixer",
            d: "M4 21V14m0-4V3m8 18V12m0-4V3m8 18V16m0-4V3M1 14h6M9 8h6M17 16h6"
        },
        {
            id: "clips",
            tkey: "nav.clips",
            d: "M15 10l4.553-2.276A1 1 0 0 1 21 8.618v6.764a1 1 0 0 1 -1.447.894L15 14M3 6h10a2 2 0 0 1 2 2v8a2 2 0 0 1 -2 2H3a2 2 0 0 1 -2-2V8a2 2 0 0 1 2-2z"
        },
        {
            id: "devices",
            tkey: "nav.devices",
            d: "M3 18v-6a9 9 0 0 1 18 0v6M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"
        },
        {
            id: "settings",
            tkey: "nav.settings",
            d: "M12 6V4m0 2a2 2 0 1 0 0 4m0-4a2 2 0 1 1 0 4m-6 8a2 2 0 1 0 0-4m0 4a2 2 0 1 1 0-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 1 0 0-4m0 4a2 2 0 1 1 0-4m0 4v2m0-6V4"
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
            model: sidebar.visibleNavItems

            Rectangle {
                id: navItem
                Layout.fillWidth: true
                // 18px icon + 10px padding each side + 1px border each side,
                // matching the retired Vue rail (.nav-item: padding 10px 14px,
                // svg 18px). The Qt port had grown these to 48px rows with
                // 24px icons, which is what made the sidebar feel bulky.
                Layout.preferredHeight: 40
                radius: Theme.radius
                property bool active: sidebar.currentPage === modelData.id
                property bool hovered: navMouse.containsMouse
                color: (active || hovered) ? Theme.accentAlpha(10) : "transparent"
                // Always 1px, transparent when idle. Toggling the WIDTH
                // between 0 and 1 reflowed the row's contents by a pixel
                // every time you changed page; the Vue rule kept a
                // `1px solid transparent` border for exactly this reason.
                border.width: 1
                border.color: active ? Theme.accent : "transparent"

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 14
                    anchors.rightMargin: 14
                    spacing: 12

                    // SVG stroke icon in an 18×18 shape (Vue: .nav-item svg).
                    // The paths are authored on a 24×24 grid, so the Shape is
                    // scaled rather than re-drawn.
                    Shape {
                        id: iconShape
                        Layout.preferredWidth: 18
                        Layout.preferredHeight: 18
                        transform: Scale { xScale: 18 / 24; yScale: 18 / 24 }
                        Layout.alignment: Qt.AlignVCenter
                        // The default (geometry) renderer tessellates strokes
                        // on the CPU and falls apart on a scaled-down 24-grid
                        // path — the Settings glyph in particular came out as
                        // a jumble of disconnected strokes. CurveRenderer
                        // rasterises the curves on the GPU and holds up at
                        // this size. Qt 6.6+; the shell already needs 6.7.
                        preferredRendererType: Shape.CurveRenderer

                        ShapePath {
                            strokeColor: navItem.active || navItem.hovered ? Theme.accent : Theme.textDim
                            // Authored for a 24px grid; the 0.75 scale above
                            // renders this as ~1.5px, which is what the Vue
                            // rail's 18px SVGs produced.
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
                        horizontalAlignment: Text.AlignLeft
                        // Reading I18n.language makes this binding re-evaluate live
                        // when the language changes (invokables alone aren't tracked).
                        text: (I18n.language, I18n.t(modelData.tkey))
                        color: navItem.active ? Theme.text : (navItem.hovered ? Theme.accent : Theme.textDim)
                        // Vue used 13.5px; fractional pixelSize hangs the QML
                        // engine (see AGENTS.md landmine 1), so 13.
                        font.pixelSize: 13
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
                horizontalAlignment: Text.AlignLeft
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

    // modulesJson defaults to empty until refreshed — load it here so a
    // disabled module hides its nav item from the first frame, not only
    // after the user happens to open Settings → Extensions.
    Component.onCompleted: ExtensionsController.refresh()
}
