import QtQuick
import QtQuick.Controls
import QtQuick.Window
import com.opengg.app

Rectangle {
    id: titlebar
    height: Theme.titlebarH   // --titlebar-h
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

        // Brand mark — Titlebar.vue renders this accent-filled logo left of the
        // wordmark; the QML port previously showed the text alone.
        Icon {
            name: "logo"
            size: 22
            color: Theme.accent
            anchors.verticalCenter: parent.verticalCenter
        }

        Text {
            text: "OpenGG"
            color: Theme.text
            font.pixelSize: 14
            font.weight: Font.DemiBold
            anchors.verticalCenter: parent.verticalCenter
        }

        // "Beta" pill, also missing from the original port (Titlebar.vue:49).
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: betaLabel.implicitWidth + 12
            height: 16
            radius: 8
            color: Theme.accentAlpha(15)
            border.width: 1
            border.color: Theme.accentAlpha(40)
            Text {
                id: betaLabel
                anchors.centerIn: parent
                text: (I18n.language, I18n.t("common.beta"))
                color: Theme.accent
                font.pixelSize: 9
                font.weight: Font.Bold
            }
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
                onEntered: parent.color = Theme.bgHover
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
                onEntered: parent.color = Theme.bgHover
                onExited: parent.color = "transparent"
                onClicked: root.showMinimized()
            }
        }

        // Maximize / restore — present in Titlebar.vue (toggleMaximize) but
        // missing from the original port, which shipped minimize + close only.
        Rectangle {
            width: 36
            height: 32
            radius: Theme.radius
            color: "transparent"

            Icon {
                anchors.centerIn: parent
                name: "square"; size: 13
                color: Theme.textDim
            }

            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: parent.color = Theme.bgHover
                onExited: parent.color = "transparent"
                onClicked: root.visibility === Window.Maximized
                           ? root.showNormal() : root.showMaximized()
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
                onEntered: parent.color = Theme.bgHover
                onExited: parent.color = "transparent"
                onClicked: Qt.callLater(root.close)
            }
        }
    }
}
