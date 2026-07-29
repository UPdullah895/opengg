mod audio;
mod clips;
mod device;
mod eq;
mod extensions;
mod i18n;
mod recording;
mod settings;
mod storage;
mod system;
mod theme;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/com/opengg/app/qml/Main.qml"));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
