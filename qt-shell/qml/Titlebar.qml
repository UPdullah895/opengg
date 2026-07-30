import QtQuick
import QtQuick.Controls
import com.opengg.app

Rectangle {
    id: titlebar
    height: 40
    color: Theme.surface

    // Bottom border line
    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.border
    }

    // Left side: title text and drag area
    Row {
        anchors.left: parent.left
        anchors.leftMargin: 14
        anchors.verticalCenter: parent.verticalCenter
        spacing: 10

        Text {
            text: "OpenGG"
            color: Theme.text
            font.pixelSize: 14
            font.weight: Font.DemiBold
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    // Drag handler for frameless window movement
    MouseArea {
        anchors.fill: parent
        anchors.rightMargin: 140  // Exclude the controls cluster
        onPressed: root.startSystemMove()
    }

    // Right side: language toggle, minimize and close buttons
    Row {
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8

        // Language toggle (interim — to be replaced by Settings → Language, §1.1 S9)
        Rectangle {
            width: 40
            height: 32
            radius: Theme.radius
            color: "transparent"

            Text {
                anchors.centerIn: parent
                text: (I18n.language, I18n.language.toUpperCase())
                color: Theme.textDim
                font.pixelSize: 12
                font.weight: Font.DemiBold
            }

            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: parent.color = Qt.rgba(1, 1, 1, 0.05)
                onExited: parent.color = "transparent"
                onClicked: I18n.applyLanguage(I18n.language === "ar" ? "en" : "ar")
            }
        }

        // Minimize button
        Rectangle {
            width: 36
            height: 32
            radius: Theme.radius
            color: "transparent"

            Icon {
                anchors.centerIn: parent
                name: "minus"; size: 14
                color: Theme.textDim
            }

            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: parent.color = Qt.rgba(1, 1, 1, 0.05)
                onExited: parent.color = "transparent"
                onClicked: root.showMinimized()
            }
        }

        // Close button
        Rectangle {
            width: 36
            height: 32
            radius: Theme.radius
            color: "transparent"

            Icon {
                anchors.centerIn: parent
                name: "x"; size: 14
                color: Theme.textDim
            }

            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: parent.color = Qt.rgba(1, 1, 1, 0.05)
                onExited: parent.color = "transparent"
                onClicked: Qt.callLater(root.close)
            }
        }
    }
}
