mod audio;
mod audio_mixer;
mod mixer_pipeline;
mod clips;
mod device;
mod editor;
mod eq;
mod extensions;
mod i18n;
mod recording;
mod screenshot;
mod settings;
mod storage;
mod system;
mod theme;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

extern "C" {
    /// See src/cpp/video_bridge.cpp.
    fn opengg_use_opengl_scenegraph();
}

fn main() {
    // ── Clip video prerequisites, in this order ───────────────────────────
    // Both were established by the qml6glsink spike (qt-shell/spikes/), and
    // both are silent failures if skipped or reordered.
    //
    // 1. qml6glsink can only take a GL context from an OpenGL scene graph,
    //    and Qt latches the graphics API when the first window is created.
    unsafe { opengg_use_opengl_scenegraph() };
    // 2. The QML type GstGLQt6VideoItem is registered by the GStreamer
    //    plugin (there is no qmldir — Arch ships only libgstqml6.so), so the
    //    plugin must be loaded BEFORE the engine resolves the import, or the
    //    video surface fails to instantiate with "is not a type".
    if let Err(e) = gstreamer::init() {
        eprintln!("gstreamer init failed, clip video disabled: {e}");
    } else if gstreamer::ElementFactory::make("qml6glsink").build().is_err() {
        eprintln!("qml6glsink unavailable — install gst-plugin-qml6 for clip video");
    }

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/com/opengg/app/qml/Main.qml"));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
