import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings → Language. Extracted from the original inline SettingsPage.qml
// card, now that SettingsPage hosts a nav + multiple panels.
ColumnLayout {
    spacing: 20

    SettingsHeading { titleText: (I18n.language, I18n.t("settings.sections.language")) }

    Rectangle {
        Layout.fillWidth: true
        Layout.preferredWidth: 640
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.border
        implicitHeight: langCol.implicitHeight + 40

        ColumnLayout {
            id: langCol
            anchors.fill: parent
            anchors.margins: 20
            spacing: 14

            Text {
                text: (I18n.language, I18n.t("settings.language.title"))
                color: Theme.text
                font.pixelSize: 18
                font.weight: Font.DemiBold
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 16

                Text {
                    text: (I18n.language, I18n.t("settings.language.selectLanguage"))
                    color: Theme.textDim
                    font.pixelSize: 14
                    Layout.fillWidth: true
                }

                ComboBox {
                    id: langCombo
                    Layout.preferredWidth: 200
                    model: I18n.availableLanguages()
                    currentIndex: Math.max(0, model.indexOf(I18n.language))
                    displayText: I18n.languageName(I18n.language)
                    onActivated: (index) => I18n.applyLanguage(model[index])

                    contentItem: Text {
                        leftPadding: 12
                        rightPadding: 12
                        text: langCombo.displayText
                        color: Theme.text
                        font.pixelSize: 14
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }

                    background: Rectangle {
                        implicitHeight: 38
                        radius: Theme.radius
                        color: Theme.bg
                        border.width: 1
                        border.color: langCombo.activeFocus ? Theme.accent : Theme.border
                    }

                    delegate: ItemDelegate {
                        width: langCombo.width
                        highlighted: langCombo.highlightedIndex === index
                        contentItem: Text {
                            text: I18n.languageName(modelData)
                            color: highlighted ? Theme.accent : Theme.text
                            font.pixelSize: 14
                            verticalAlignment: Text.AlignVCenter
                        }
                        background: Rectangle {
                            color: highlighted ? Theme.accentAlpha(10) : Theme.surface
                        }
                    }

                    popup: Popup {
                        y: langCombo.height + 4
                        width: langCombo.width
                        implicitHeight: contentItem.implicitHeight
                        padding: 4
                        contentItem: ListView {
                            clip: true
                            implicitHeight: contentHeight
                            model: langCombo.popup.visible ? langCombo.delegateModel : null
                            currentIndex: langCombo.highlightedIndex
                        }
                        background: Rectangle {
                            radius: Theme.radius
                            color: Theme.surface
                            border.width: 1
                            border.color: Theme.border
                        }
                    }
                }
            }

            Text {
                text: (I18n.language, I18n.t("settings.language.hint"))
                color: Theme.textDim
                font.pixelSize: 12
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }
        }
    }
}
