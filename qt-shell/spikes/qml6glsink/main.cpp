// Spike: does qml6glsink render video into a Qt6 QML scene on this machine,
// and does it survive QT_QPA_PLATFORM=offscreen (which the ui-shots
// verification harness depends on)?
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QQuickItem>
#include <QTimer>
#include <QImage>
#include <gst/gst.h>
#include <cstdio>

int main(int argc, char *argv[])
{
    // qml6glsink needs the scene graph on OpenGL; must be set before any window.
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);

    gst_init(&argc, &argv);
    QGuiApplication app(argc, argv);

    // Creating the element force-loads libgstqml6.so, which is what registers
    // the org.freedesktop.gstreamer.Qt6GLVideoItem QML type. Doing this AFTER
    // the engine loads QML would make the import fail.
    // qmlRegisterType lives inside gst_element_register_qml6glsink, which runs
    // during the plugin's registration — force the plugin to load explicitly
    // rather than relying on factory_make to have triggered it.
    GstPlugin *qmlplug = gst_plugin_load_by_name("qml6");
    fprintf(stderr, "SPIKE: plugin_load_by_name(qml6) = %p\n", (void*)qmlplug);
    GstElement *sink = gst_element_factory_make("qml6glsink", "sink");
    if (!sink) { fprintf(stderr, "SPIKE-FAIL: no qml6glsink element\n"); return 2; }
    fprintf(stderr, "SPIKE: sink created ok\n");

    QQmlApplicationEngine engine;
    engine.load(QUrl("qrc:/main.qml"));
    if (engine.rootObjects().isEmpty()) { fprintf(stderr, "SPIKE-FAIL: QML load failed\n"); return 3; }

    auto *win = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    if (!win) { fprintf(stderr, "SPIKE-FAIL: root is not a window\n"); return 4; }
    QQuickItem *videoItem = win->findChild<QQuickItem *>("videoItem");
    if (!videoItem) { fprintf(stderr, "SPIKE-FAIL: no videoItem\n"); return 5; }

    // THE risky handoff: a QQuickItem* set on a GObject property.
    g_object_set(sink, "widget", videoItem, NULL);

    const char *clip = argc > 1 ? argv[1] : nullptr;
    if (!clip) { fprintf(stderr, "SPIKE-FAIL: no clip arg\n"); return 6; }

    GstElement *pipeline = gst_pipeline_new("spike");
    GstElement *src   = gst_element_factory_make("filesrc", "src");
    GstElement *dec   = gst_element_factory_make("decodebin", "dec");
    GstElement *upl   = gst_element_factory_make("glupload", "upl");
    GstElement *conv  = gst_element_factory_make("glcolorconvert", "conv");
    if (!src || !dec || !upl || !conv) { fprintf(stderr, "SPIKE-FAIL: element make\n"); return 7; }
    g_object_set(src, "location", clip, NULL);

    gst_bin_add_many(GST_BIN(pipeline), src, dec, upl, conv, sink, NULL);
    gst_element_link(src, dec);
    gst_element_link_many(upl, conv, sink, NULL);

    g_signal_connect(dec, "pad-added", G_CALLBACK(+[](GstElement *, GstPad *pad, gpointer user) {
        GstElement *upl = static_cast<GstElement *>(user);
        GstCaps *caps = gst_pad_get_current_caps(pad);
        if (!caps) return;
        const gchar *name = gst_structure_get_name(gst_caps_get_structure(caps, 0));
        if (g_str_has_prefix(name, "video/")) {
            GstPad *sinkpad = gst_element_get_static_pad(upl, "sink");
            GstPadLinkReturn r = gst_pad_link(pad, sinkpad);
            fprintf(stderr, "SPIKE: video pad link = %d\n", r);
            gst_object_unref(sinkpad);
        }
        gst_caps_unref(caps);
    }), upl);

    // Report why a state change fails instead of guessing.
    GstBus *bus = gst_element_get_bus(pipeline);
    gst_bus_add_watch(bus, +[](GstBus *, GstMessage *msg, gpointer) -> gboolean {
        if (GST_MESSAGE_TYPE(msg) == GST_MESSAGE_ERROR) {
            GError *e = nullptr; gchar *dbg = nullptr;
            gst_message_parse_error(msg, &e, &dbg);
            fprintf(stderr, "SPIKE-BUS-ERROR: %s | %s\n", e->message, dbg ? dbg : "");
            g_error_free(e); g_free(dbg);
        }
        return TRUE;
    }, nullptr);

    // qml6glsink can only obtain a GL context once the Qt scene graph has been
    // initialised, which happens after the window is first rendered. Starting
    // the pipeline in main() therefore fails the state change outright.
    QObject::connect(win, &QQuickWindow::sceneGraphInitialized, win, [pipeline]() {
        GstStateChangeReturn sc = gst_element_set_state(pipeline, GST_STATE_PLAYING);
        fprintf(stderr, "SPIKE: set_state PLAYING (post-SG-init) = %d\n", sc);
    }, Qt::QueuedConnection);

    // Give it time to negotiate + render some frames, then grab the window.
    QTimer::singleShot(3500, [&]() {
        QImage img = win->grabWindow();
        bool ok = !img.isNull() && img.save("/tmp/qml6-spike.png");
        fprintf(stderr, "SPIKE: grab %dx%d saved=%d\n", img.width(), img.height(), ok);
        gst_element_set_state(pipeline, GST_STATE_NULL);
        app.quit();
    });
    return app.exec();
}
