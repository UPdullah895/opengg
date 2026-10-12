import QtQuick
import org.freedesktop.gstreamer.Qt6GLVideoItem

// The GL video item ClipVideoSurface loads by URL. It lives in its own file
// so the GStreamer QML import is resolved only when this is loaded: the
// module ships with the qml6glsink plugin, which distros package separately,
// and an unresolved import at the top of ClipVideoSurface took the clip page
// — and with it the whole main window — down with it.
//
// The objectName MUST match mixer_pipeline::VIDEO_ITEM_OBJECT_NAME. The type
// is `GstGLQt6VideoItem`, NOT the Qt5 `GstGLVideoItem` most examples use.
GstGLQt6VideoItem {
    objectName: "clipVideoItem"
}
