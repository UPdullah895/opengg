import QtQuick
import QtQuick.Layouts
import com.opengg.app

// Settings → Extension Store. QML port of StoreSettings.vue.
//
// The upstream Vue component itself gates its full registry-browser UI
// behind `const storeComingSoon = ref(true)` — the feature isn't live in
// the shipping app yet either. This ports that same coming-soon state
// faithfully rather than building ahead of upstream's own flag; when
// `storeComingSoon` flips there, revisit this file and port the browse
// list (it only needs `fetch_extension_registry`, already in
// opengg_core::extensions — no dynamic-UI-loading blocker like the
// Extensions panel has).
ColumnLayout {
    id: root
    spacing: 20

    Text {
        text: (I18n.language, I18n.t("settings.store.title"))
        color: Theme.text
        font.pixelSize: 22
        font.weight: Font.Bold
    }

    ColumnLayout {
        Layout.fillWidth: true
        Layout.preferredWidth: 680
        Layout.topMargin: 40
        spacing: 10

        Icon {
            name: "package"; size: 34
            opacity: 0.5
            Layout.alignment: Qt.AlignHCenter
        }
        Text {
            text: (I18n.language, I18n.t("settings.store.comingSoon"))
            color: Theme.text
            font.pixelSize: 16
            font.weight: Font.Bold
            Layout.alignment: Qt.AlignHCenter
        }
        Text {
            text: (I18n.language, I18n.t("settings.store.comingSoonDesc"))
            color: Theme.textDim
            font.pixelSize: 12
            wrapMode: Text.WordWrap
            horizontalAlignment: Text.AlignHCenter
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: 380
        }
    }
}
