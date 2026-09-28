// video_bridge — the Qt facts the unified GStreamer player needs that cxx-qt
// cannot express, kept as a deliberately tiny C-linkage surface.
//
// WHY THIS FILE EXISTS: qml6glsink renders by being handed the QQuickItem it
// should draw into. cxx-qt-lib has no QQuickItem binding, and marshalling a
// QQuickItem* through a #[qinvokable] is not something cxx-qt 0.9 supports.
// Rather than fight that, the item is never passed across the boundary from
// QML at all: QML tags it with an objectName, and this looks it up from the
// application's top-level windows and returns the bare pointer.
//
// Note what is deliberately NOT here: the sink's `widget` property is a plain
// G_TYPE_POINTER, so Rust sets it itself via glib. Keeping GStreamer and GLib
// out of this file means it compiles against nothing but Qt, which cxx-qt's
// own cc build already configures.
//
// See qt-shell/spikes/qml6glsink/ for the harness that established the
// constraints encoded here.

#include <QtGui/QGuiApplication>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <QtCore/QString>

extern "C" {

/// Select the OpenGL scene-graph backend. qml6glsink can only take a GL
/// context from an OpenGL scene graph, and Qt latches the choice when the
/// first window is created — so this MUST run before the QML engine loads.
void opengg_use_opengl_scenegraph()
{
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
}

/// Whether a GL window system is actually available. Under
/// QT_QPA_PLATFORM=offscreen it is not — qml6glsink fails the pipeline with
/// "Could not initialize window system" — so callers must fall back to a
/// poster frame instead of building a video branch that cannot start.
bool opengg_has_gl_window_system()
{
    const QString platform = QGuiApplication::platformName();
    return platform != QStringLiteral("offscreen")
        && platform != QStringLiteral("minimal");
}

/// The QQuickItem named `object_name`, searched across the application's
/// top-level Quick windows, as a bare pointer. Null when the item is not in
/// the scene yet — a normal transient state during page construction, not an
/// error. findChild walks the whole QObject tree, so the item may live
/// arbitrarily deep inside pages and layouts.
void *opengg_find_video_item(const char *object_name)
{
    if (object_name == nullptr) {
        return nullptr;
    }
    const QString name = QString::fromUtf8(object_name);
    const auto windows = QGuiApplication::topLevelWindows();
    for (QWindow *w : windows) {
        auto *qw = qobject_cast<QQuickWindow *>(w);
        if (qw == nullptr) {
            continue;
        }
        if (auto *item = qw->findChild<QQuickItem *>(name)) {
            return static_cast<void *>(item);
        }
    }
    return nullptr;
}

} // extern "C"
