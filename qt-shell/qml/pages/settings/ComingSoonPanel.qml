import QtQuick
import QtQuick.Layouts
import com.opengg.app

// Placeholder for settings sections not yet ported to qt-shell.
ColumnLayout {
    property string sectionTitle: ""
    spacing: 12

    SettingsHeading { titleText: sectionTitle }
    Text {
        text: (I18n.language, I18n.t("common.notAvailableInQt"))
        color: Theme.textDim
        font.pixelSize: 13
    }
}
