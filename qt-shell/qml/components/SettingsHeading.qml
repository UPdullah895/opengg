import QtQuick
import QtQuick.Layouts
import com.opengg.app

// Settings panel heading: title above a full-width divider rule.
//
// Every settings page in the Vue UI renders its title with a horizontal rule
// beneath it; the QML port rendered a bare Text in each of the eleven panels,
// which is why the pages read as flatter and less structured than the original.
ColumnLayout {
    id: heading

    /// Already-translated title string.
    property string titleText: ""

    Layout.fillWidth: true
    spacing: 12

    Text {
        text: heading.titleText
        color: Theme.text
        // 18px/700, per the Vue .sec-title. 22px Bold out-sized even the
        // page titles once those came back down to 20px.
        font.pixelSize: 18
        font.weight: Font.Bold
    }

    Rectangle {
        Layout.fillWidth: true
        height: 1
        color: Theme.border
    }
}
