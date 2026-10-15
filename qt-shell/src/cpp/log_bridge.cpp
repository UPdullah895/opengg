// log_bridge — routes Qt's own diagnostics (QML warnings, "X is not a type",
// binding loops, plugin load errors) into the Rust logger, so they land in
// ~/.local/state/opengg/logs/opengg.log with category and file:line.
//
// WHY: with journald support Qt's default handler sends everything to the
// journal, which a user launching OpenGG from the app menu never looks at.
// Replacing the handler also means QT_FORCE_STDERR_LOGGING is moot: the Rust
// logger always writes to stderr as well, so tools/ui-shots.sh still sees
// every QML warning.

#include <QtCore/QByteArray>
#include <QtCore/QtGlobal>
#include <QtCore/QLoggingCategory>
#include <QtCore/QString>
#include <QtGui/QGuiApplication>

#include <cstring>

extern "C" {
// Implemented in Rust (qt-shell/src/main.rs).
void opengg_log_qt_message(int level, const char *category, const char *file,
                           int line, const char *msg);
}

static void handler(QtMsgType type, const QMessageLogContext &ctx, const QString &msg)
{
    // Values mirror core::logging::qt_message's documented mapping.
    int level = 0;
    switch (type) {
    case QtDebugMsg: level = 0; break;
    case QtWarningMsg: level = 1; break;
    case QtCriticalMsg: level = 2; break;
    case QtFatalMsg: level = 3; break;
    case QtInfoMsg: level = 4; break;
    }
    const QByteArray text = msg.toUtf8();
    opengg_log_qt_message(level, ctx.category ? ctx.category : "",
                          ctx.file ? ctx.file : "", ctx.line, text.constData());
    // Qt aborts after a fatal message returns from the handler; the Rust
    // logger writes unbuffered, so the line is already on disk.
}

extern "C" {

void opengg_install_qt_message_handler()
{
    qInstallMessageHandler(handler);
}

/// Runtime Qt version (what is actually loaded) — may differ from the build.
const char *opengg_qt_runtime_version()
{
    return qVersion();
}

/// Qt version this binary was compiled against.
const char *opengg_qt_build_version()
{
    return QT_VERSION_STR;
}

/// QPA platform plugin ("wayland", "xcb", "offscreen"...). Needs a
/// QGuiApplication; writes a NUL-terminated string into `buf`.
void opengg_qt_platform(char *buf, int n)
{
    if (!buf || n <= 0)
        return;
    const QByteArray name = QGuiApplication::platformName().toUtf8();
    std::strncpy(buf, name.constData(), static_cast<size_t>(n) - 1);
    buf[n - 1] = '\0';
}

}
