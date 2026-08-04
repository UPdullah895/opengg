import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.opengg.app

// Settings → Language. QML port of LanguageSettings.vue: a selectable list
// of language rows (code badge + name + LTR/RTL tag), not a dropdown — the
// Vue original never used a ComboBox here.
ColumnLayout {
    width: parent.width
    spacing: 20

    SettingsHeading { titleText: (I18n.language, I18n.t("settings.sections.language")) }

    SettingsCard {
        title: (I18n.language, I18n.t("settings.language.title"))

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6

            Repeater {
                model: I18n.availableLanguages()
                Rectangle {
                    id: langBtn
                    required property string modelData
                    readonly property bool active: modelData === I18n.language
                    Layout.fillWidth: true
                    implicitHeight: 46
                    radius: Theme.radius
                    color: active ? Theme.accentAlpha(10) : Theme.bgInput
                    border.width: 1
                    border.color: active ? Theme.accent : (langArea.containsMouse ? Theme.accent : Theme.border)

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 14
                        anchors.rightMargin: 14
                        spacing: 10

                        Text {
                            text: langBtn.modelData.toUpperCase()
                            color: Theme.accent
                            font.pixelSize: 11
                            font.weight: Font.Black
                            Layout.minimumWidth: 26
                        }
                        Text {
                            text: (I18n.language, I18n.languageName(langBtn.modelData))
                            color: langBtn.active ? Theme.text : Theme.textDim
                            font.pixelSize: 14
                            font.weight: Font.DemiBold
                            Layout.fillWidth: true
                        }
                        Rectangle {
                            radius: 3
                            color: Theme.bgDeep
                            implicitWidth: dirLabel.implicitWidth + 12
                            implicitHeight: 18
                            Text {
                                id: dirLabel
                                anchors.centerIn: parent
                                text: (I18n.language, I18n.languageDir(langBtn.modelData).toUpperCase())
                                color: Theme.textMuted
                                font.pixelSize: 10
                            }
                        }
                    }

                    MouseArea {
                        id: langArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: I18n.applyLanguage(langBtn.modelData)
                    }
                }
            }

            Text {
                text: (I18n.language, I18n.t("settings.language.hint"))
                color: Theme.textDim
                font.pixelSize: 12
                Layout.fillWidth: true
                Layout.topMargin: 4
                wrapMode: Text.WordWrap
            }
        }
    }
}
