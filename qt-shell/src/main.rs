mod audio;
mod audio_mixer;
mod mixer_pipeline;
mod clips;
mod device;
mod editor;
mod eq;
mod extensions;
mod hotkeys;
mod i18n;
mod recording;
mod screenshot;
mod settings;
mod storage;
mod system;
mod theme;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

use std::ffi::{c_char, c_int, CStr};
use std::process::ExitCode;

extern "C" {
    /// See src/cpp/video_bridge.cpp.
    fn opengg_use_opengl_scenegraph();
    /// See src/cpp/log_bridge.cpp.
    fn opengg_install_qt_message_handler();
    fn opengg_qt_runtime_version() -> *const c_char;
    fn opengg_qt_build_version() -> *const c_char;
    fn opengg_qt_platform(buf: *mut c_char, n: c_int);
}

/// Called by the C++ Qt message handler for every qDebug/qWarning/QML
/// console message; forwards into the shared logger.
#[no_mangle]
pub extern "C" fn opengg_log_qt_message(
    level: c_int,
    category: *const c_char,
    file: *const c_char,
    line: c_int,
    msg: *const c_char,
) {
    let s = |p: *const c_char| {
        if p.is_null() {
            std::borrow::Cow::Borrowed("")
        } else {
            unsafe { CStr::from_ptr(p) }.to_string_lossy()
        }
    };
    opengg_core::logging::qt_message(level, &s(category), &s(file), line, &s(msg));
}

fn c_str(p: *const c_char) -> String {
    if p.is_null() {
        return "?".into();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// One block of facts that explains most "works on my machine" reports.
/// Everything slow (bus query, `pactl info`) runs on a thread so startup is
/// never delayed by it.
fn log_startup_summary() {
    let mut plat = [0 as c_char; 64];
    unsafe { opengg_qt_platform(plat.as_mut_ptr(), plat.len() as c_int) };
    let (rt, build) = unsafe { (c_str(opengg_qt_runtime_version()), c_str(opengg_qt_build_version())) };
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| "-".into());
    let distro = opengg_core::system::get_distro_info();
    log::info!(
        "OpenGG {} | Qt {} (built against {}) | QPA {} | session {} / {} | distro {} (like: {}) | log {}",
        env!("CARGO_PKG_VERSION"),
        rt,
        build,
        c_str(plat.as_ptr()),
        env("XDG_SESSION_TYPE"),
        env("XDG_CURRENT_DESKTOP"),
        distro.id,
        distro.id_like,
        opengg_core::logging::log_file_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(stderr only)".into()),
    );
    let _ = std::thread::Builder::new()
        .name("startup-summary".into())
        .spawn(|| {
            log::info!("{}", opengg_core::system::dependency_summary());
            log::info!("audio server: {}", opengg_core::system::audio_server_summary());
            log::info!("org.opengg.Daemon: {}", opengg_core::system::daemon_status());
        });
}

/// QML failed to load: a window never appears, so without this the process
/// just sits there invisibly. Say so where the user can find it, then exit.
fn fail_startup(url: &str) -> ! {
    let log_path = opengg_core::logging::log_file_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "the terminal output".into());
    log::error!("QML failed to load ({url}); see the preceding Qt messages. Log file: {log_path}");
    // Best effort: a user launching from the app menu has no terminal.
    let _ = std::process::Command::new("notify-send")
        .args(["-a", "OpenGG", "-u", "critical"])
        .arg("OpenGG failed to start")
        .arg(format!("See {log_path}"))
        .status();
    std::process::exit(1);
}

fn main() -> ExitCode {
    // Logging first: everything below, including Qt's own diagnostics and
    // any panic, must be able to reach the log file.
    opengg_core::logging::init();
    opengg_core::logging::install_panic_hook();
    unsafe { opengg_install_qt_message_handler() };

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
        log::error!("gstreamer init failed, clip video disabled: {e}");
    } else if gstreamer::ElementFactory::make("qml6glsink").build().is_err() {
        log::warn!(
            "qml6glsink unavailable — install {} for clip video",
            opengg_core::gsr::qml6_plugin_package()
        );
    }

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    log_startup_summary();

    if let Some(mut engine) = engine.as_mut() {
        // Emitted (Qt 6.4+) when Main.qml or something it imports cannot be
        // created; Qt has already logged the reason through the handler.
        engine
            .as_mut()
            .on_object_creation_failed(|_, url| fail_startup(&url.to_string()))
            .release();
        engine.load(&QUrl::from("qrc:/qt/qml/com/opengg/app/qml/Main.qml"));
    }

    let code = app.as_mut().map(|app| app.exec()).unwrap_or(1);
    if code != 0 {
        log::warn!("event loop exited with status {code}");
    }
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
