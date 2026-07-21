use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("com.opengg.app").qml_files([
            "qml/Main.qml",
            "qml/Titlebar.qml",
            "qml/Sidebar.qml",
            "qml/Theme.qml",
            "qml/pages/HomePage.qml",
            "qml/pages/MixerPage.qml",
            "qml/pages/ClipsPage.qml",
            "qml/pages/DevicesPage.qml",
            "qml/pages/SettingsPage.qml",
        ]),
    )
    .build();
}
