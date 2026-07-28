import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

ApplicationWindow {
    id: root
    width: 1280
    height: 800
    visible: true
    title: "OpenGG"
    color: Theme.bg

    // Frameless window with no decorations
    flags: Qt.Window | Qt.FramelessWindowHint

    // Current navigation page
    property string currentPage: "home"

    // RTL layout mirroring driven by the active language (§3.2). childrenInherit
    // propagates it down the whole tree; L4-exempt subtrees opt back out locally.
    LayoutMirroring.enabled: I18n.rtl
    LayoutMirroring.childrenInherit: true

    Component.onCompleted: ThemeController.reload()

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Titlebar {
            Layout.fillWidth: true
            Layout.preferredHeight: 40
        }

        RowLayout {
            spacing: 0
            Layout.fillWidth: true
            Layout.fillHeight: true

            Sidebar {
                Layout.fillHeight: true
                currentPage: root.currentPage
                onNavigate: (p) => root.currentPage = p
            }

            StackLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                currentIndex: ["home", "mixer", "clips", "devices", "settings"].indexOf(root.currentPage)

                HomePage {}
                MixerPage {}
                ClipsPage {}
                DevicesPage {}
                SettingsPage {}
            }
        }
    }
}
