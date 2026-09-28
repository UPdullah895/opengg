import QtQuick
import org.freedesktop.gstreamer.Qt6GLVideoItem
import com.opengg.app

// The surface ClipAudioMixer's GStreamer pipeline renders the picture into,
// with a poster-frame fallback for when it cannot.
//
// The video item is found by C++ via its objectName rather than being passed
// across the cxx-qt boundary — see src/cpp/video_bridge.cpp for why. The name
// below MUST match mixer_pipeline::VIDEO_ITEM_OBJECT_NAME.
//
// NOTE the type is `GstGLQt6VideoItem`, NOT `GstGLVideoItem` — the latter is
// the Qt5 name used by nearly every example online, and getting it wrong
// fails with the misleading "GstGLVideoItem is not a type".
Item {
    id: surface

    /// Cached thumbnail shown when the picture cannot be rendered.
    property string posterSource: ""

    /// True while real video frames are being drawn.
    readonly property bool showingVideo: ClipAudioMixer.videoActive

    // Only instantiated where a GL window system exists. Under
    // QT_QPA_PLATFORM=offscreen (the ui-shots harness) qml6glsink cannot
    // initialise at all, so creating the item there is pointless — the poster
    // frame carries those captures instead.
    Loader {
        anchors.fill: parent
        active: ClipAudioMixer.glAvailable
        // Must be a Component, not a bare instance — assigning an Item
        // directly to sourceComponent is a type error, and it takes the whole
        // surrounding component down with it.
        sourceComponent: Component {
            GstGLQt6VideoItem {
                objectName: "clipVideoItem"
            }
        }
    }

    // Poster frame: the clip's cached thumbnail, shown until (or instead of)
    // live frames. Sits above the video item and fades out once the pipeline
    // reports it is actually rendering, which also covers the gap between
    // opening a clip and the first decoded frame.
    Image {
        anchors.fill: parent
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        cache: true
        visible: opacity > 0.01
        opacity: surface.showingVideo ? 0 : 1
        source: surface.posterSource ? "file://" + surface.posterSource : ""
        Behavior on opacity { NumberAnimation { duration: 140 } }
    }
}
