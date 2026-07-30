import QtQuick
import QtQuick.Layouts
import com.opengg.app

// Placeholder for settings sections not yet ported to qt-shell.
ColumnLayout {
    property string sectionTitle: ""
    spacing: 12

    SettingsHeading { titleText: sectionTitle }
    Text {
        text: "Not yet available in the Qt UI — use the Tauri app for this section."
        color: Theme.textDim
        font.pixelSize: 13
    }
}
