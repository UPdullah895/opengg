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
        anchors.rightMargin: 90  // Exclude buttons area
        onPressed: root.startSystemMove()
    }

    // Right side: minimize and close buttons
    Row {
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8

        // Minimize button
        Rectangle {
            width: 36
            height: 32
            radius: Theme.radius
            color: "transparent"

            Text {
                anchors.centerIn: parent
                text: "−"
                color: Theme.textDim
                font.pixelSize: 16
                font.weight: Font.Bold
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

            Text {
                anchors.centerIn: parent
                text: "✕"
                color: Theme.textDim
                font.pixelSize: 16
                font.weight: Font.Bold
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
