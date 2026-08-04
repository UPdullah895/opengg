import QtQuick
import QtQuick.Controls
import com.opengg.app

// Compact labelled dropdown — port of SelectField.vue's trigger+dropdown look
// via a styled QQC2 ComboBox rather than a hand-built popup (this migration's
// established choice for select-style controls; see ChannelStrip.qml's
// device selector). `options` is [{value, label}]; `value` is the current
// value (matched by `===`, so keep types consistent — string vs number).
ComboBox {
    id: box

    property var options: []
    property var value: null
    signal picked(var value)

    model: box.options
    textRole: "label"
    currentIndex: {
        for (var i = 0; i < box.options.length; i++)
            if (box.options[i].value === box.value) return i
        return -1
    }
    onActivated: box.picked(box.options[currentIndex].value)

    // Sized and colored to match the plain ComboBoxes used elsewhere in this
    // app (CaptureSoundPanel's Quality/FPS/etc) — this was previously a
    // visibly smaller, dimmer, focus-feedback-less variant (26px/11px/
    // textDim, no activeFocus border) that read as "weaker" than every
    // other dropdown despite being the more common one (Clip Preferences,
    // Notifications' Position/Duration).
    implicitHeight: 34
    font.pixelSize: 13

    contentItem: Text {
        leftPadding: 12
        text: box.displayText
        color: Theme.text
        font: box.font
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
    }
    indicator: Icon {
        x: box.width - width - 10
        anchors.verticalCenter: parent.verticalCenter
        name: "chevron-down"
        size: 12
        color: Theme.textMuted
    }
    background: Rectangle {
        radius: Theme.radius
        color: Theme.bg
        border.width: 1
        border.color: box.activeFocus ? Theme.accent : Theme.border
    }
    delegate: ItemDelegate {
        width: box.width
        highlighted: box.highlightedIndex === index
        contentItem: Text {
            text: modelData.label
            color: highlighted ? Theme.accent : Theme.text
            font.pixelSize: 12
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle { color: highlighted ? Theme.accentAlpha(10) : Theme.surface }
    }
    popup: Popup {
        y: box.height + 2
        width: box.width
        implicitHeight: Math.min(contentItem.implicitHeight, 220)
        padding: 1
        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: box.popup.visible ? box.delegateModel : null
            currentIndex: box.highlightedIndex
            ScrollBar.vertical: ScrollBar {}
        }
        background: Rectangle {
            color: Theme.surface
            border.width: 1
            border.color: Theme.border
            radius: Theme.radius
        }
    }
}
