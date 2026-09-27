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

    // Bespoke header (not SettingsCard) so the title row can host the three
    // action buttons below — same reasoning as MixerRoutingPanel's Ear Blast
    // Protection card, per SettingsCard.qml's own header-note comment.
    Rectangle {
        id: langCard
        Layout.fillWidth: true
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

            RowLayout {
                Layout.fillWidth: true
                spacing: 8

                Text {
                    text: (I18n.language, I18n.t("settings.language.title"))
                    color: Theme.text
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                }
                Item { Layout.fillWidth: true }

                // Open the locales folder (en.json + ar.json side by side) so a
                // translator can edit/copy the English pack directly — this is
                // task #101's "export the English language pack" ask; opening
                // the folder rather than just en.json also surfaces ar.json as
                // a second reference file for whoever is adding a new language.
                Rectangle {
                    width: 30; height: 30
                    radius: Theme.radius
                    color: openFolderArea.containsMouse ? Theme.bgHover : Theme.bgInput
                    border.width: 1
                    border.color: Theme.border
                    Icon { anchors.centerIn: parent; name: "folder"; size: 14; color: Theme.textDim }
                    MouseArea {
                        id: openFolderArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: I18n.openLocalesFolder()
                    }
                    Tip {
                        visible: openFolderArea.containsMouse
                        text: I18n.t("settings.language.addLanguage")
                    }
                }

                // Reload locale files from disk — picks up a translator's edits
                // (or a newly-dropped locale JSON) without restarting the app.
                Rectangle {
                    width: 30; height: 30
                    radius: Theme.radius
                    color: reloadLangArea.containsMouse ? Theme.bgHover : Theme.bgInput
                    border.width: 1
                    border.color: Theme.border
                    Icon { anchors.centerIn: parent; name: "refresh-cw"; size: 14; color: Theme.textDim }
                    MouseArea {
                        id: reloadLangArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: I18n.reloadLocales()
                    }
                    Tip {
                        visible: reloadLangArea.containsMouse
                        text: I18n.t("settings.language.reloadLanguages")
                    }
                }

                // RTL toggle — only shown for languages actually tagged RTL
                // (their _meta.dir). Arabic defaults to LTR ("set to English"
                // layout) like every other language; this lets the user opt
                // back into RTL, and the choice is a separate persisted field
                // (I18n.rtlOverride) from the active language, so switching
                // languages later doesn't reset it.
                Rectangle {
                    id: rtlToggle
                    visible: (I18n.language, I18n.languageDir(I18n.language) === "rtl")
                    width: rtlLabel.implicitWidth + 20
                    height: 30
                    radius: Theme.radius
                    color: I18n.rtlOverride ? Theme.accentAlpha(10) : (rtlArea.containsMouse ? Theme.bgHover : Theme.bgInput)
                    border.width: 1
                    border.color: I18n.rtlOverride ? Theme.accent : Theme.border
                    Text {
                        id: rtlLabel
                        anchors.centerIn: parent
                        text: "RTL"
                        color: I18n.rtlOverride ? Theme.accent : Theme.textDim
                        font.pixelSize: 11
                        font.weight: Font.Black
                    }
                    MouseArea {
                        id: rtlArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: I18n.setRtlEnabled(!I18n.rtlOverride)
                    }
                    Tip {
                        visible: rtlArea.containsMouse
                        text: I18n.t("settings.language.rtlModeHint")
                    }
                }
            }

            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

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
                            horizontalAlignment: Text.AlignLeft
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
                horizontalAlignment: Text.AlignLeft
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
}
