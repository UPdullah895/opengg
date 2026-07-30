use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    // Theme.qml uses `pragma Singleton`; it MUST be registered with `.singleton(true)`
    // so the generated qmldir emits `singleton Theme ...`. Without this the `Theme`
    // reference (e.g. `Theme.bg`) resolves to undefined at runtime and colors fall
    // back to Qt defaults (white bg / black text).
    let qml_files: Vec<QmlFile> = vec![
        QmlFile::from("qml/Main.qml"),
        QmlFile::from("qml/Titlebar.qml"),
        QmlFile::from("qml/Sidebar.qml"),
        QmlFile::from("qml/Theme.qml").singleton(true),
        QmlFile::from("qml/Icons.qml").singleton(true),
        QmlFile::from("qml/TourController.qml").singleton(true),
        QmlFile::from("qml/TourOverlay.qml"),
        QmlFile::from("qml/pages/HomePage.qml"),
        QmlFile::from("qml/pages/MixerPage.qml"),
        QmlFile::from("qml/pages/ClipsPage.qml"),
        QmlFile::from("qml/pages/DevicesPage.qml"),
        QmlFile::from("qml/pages/SettingsPage.qml"),
        QmlFile::from("qml/pages/settings/GeneralPanel.qml"),
        QmlFile::from("qml/pages/settings/LanguagePanel.qml"),
        QmlFile::from("qml/pages/settings/NotificationsPanel.qml"),
        QmlFile::from("qml/pages/settings/ShortcutsPanel.qml"),
        QmlFile::from("qml/pages/settings/StoragePanel.qml"),
        QmlFile::from("qml/pages/settings/TrackManagementPanel.qml"),
        QmlFile::from("qml/pages/settings/AboutPanel.qml"),
        QmlFile::from("qml/pages/settings/CaptureSoundPanel.qml"),
        QmlFile::from("qml/pages/settings/MixerRoutingPanel.qml"),
        QmlFile::from("qml/pages/settings/ExtensionsPanel.qml"),
        QmlFile::from("qml/pages/settings/StorePanel.qml"),
        QmlFile::from("qml/pages/settings/ComingSoonPanel.qml"),
        QmlFile::from("qml/components/VideoPlayer.qml"),
        QmlFile::from("qml/components/TrimEditor.qml"),
        QmlFile::from("qml/components/ChannelStrip.qml"),
        QmlFile::from("qml/components/ClipCard.qml"),
        QmlFile::from("qml/components/ClipContextMenu.qml"),
        QmlFile::from("qml/components/Icon.qml"),
        QmlFile::from("qml/components/SettingsHeading.qml"),
        QmlFile::from("qml/components/IconGallery.qml"),
        QmlFile::from("qml/components/InfoIcon.qml"),
        QmlFile::from("qml/components/ToggleSwitch.qml"),
        QmlFile::from("qml/components/RecorderInstallHelper.qml"),
        QmlFile::from("qml/components/HSlider.qml"),
        QmlFile::from("qml/components/GraphicEQ.qml"),
        QmlFile::from("qml/components/DspControls.qml"),
    ];

    CxxQtBuilder::new_qml_module(QmlModule::new("com.opengg.app").qml_files(qml_files))
        // Rust cxx-qt QObjects (registered into the com.opengg.app module).
        .files([
            "src/i18n.rs",
            "src/audio.rs",
            "src/device.rs",
            "src/clips.rs",
            "src/editor.rs",
            "src/extensions.rs",
            "src/eq.rs",
            "src/recording.rs",
            "src/screenshot.rs",
            "src/settings.rs",
            "src/storage.rs",
            "src/system.rs",
            "src/theme.rs",
        ])
        // Qt Multimedia (MediaPlayer/VideoOutput) for the clip player — the
        // GStreamer backend is the PoC-validated Wayland-native path (plan §7).
        .qt_module("Multimedia")
        .build();
}
