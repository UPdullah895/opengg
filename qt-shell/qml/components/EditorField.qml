import QtQuick
import QtQuick.Controls
import com.opengg.app

// Compact bordered text field for the clip editor's header (clip name, game
// tag). Commits on Enter or on losing focus, and reverts on Esc — the same
// contract as ClipCard's inline rename.
Rectangle {
    id: field

    property alias text: input.text
    property string placeholder: ""

    /// Emitted with the trimmed value when the edit is accepted.
    signal committed(string value)

    /// Set externally; remembered so Esc can restore it and so a commit only
    /// fires when the value actually changed.
    property string committedValue: ""

    implicitHeight: 32
    radius: Theme.radius
    color: Theme.bg
    border.width: 1
    border.color: input.activeFocus ? Theme.accent : Theme.border

    onTextChanged: if (!input.activeFocus) field.committedValue = input.text

    function commit() {
        const v = input.text.trim()
        if (v === field.committedValue)
            return
        field.committedValue = v
        field.committed(v)
    }

    TextField {
        id: input
        anchors.fill: parent
        leftPadding: 10
        rightPadding: 10
        verticalAlignment: TextInput.AlignVCenter
        color: Theme.text
        font.pixelSize: 13
        placeholderText: field.placeholder
        placeholderTextColor: Theme.textMuted
        background: Item {}
        selectByMouse: true

        onAccepted: {
            field.commit()
            input.focus = false
        }
        onActiveFocusChanged: if (!activeFocus) field.commit()
        Keys.onEscapePressed: {
            input.text = field.committedValue
            input.focus = false
        }
    }
}
