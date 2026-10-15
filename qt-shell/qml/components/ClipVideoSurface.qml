import QtQuick
import com.opengg.app

// The surface ClipAudioMixer's GStreamer pipeline renders the picture into,
// with a poster-frame fallback for when it cannot.
//
// The video item is found by C++ via its objectName rather than being passed
// across the cxx-qt boundary — see src/cpp/video_bridge.cpp for why. The item
// itself (and its objectName) lives in ClipGlVideoItem.qml.
Item {
    id: surface

    /// Cached thumbnail shown when the picture cannot be rendered.
    property string posterSource: ""

    /// True while real video frames are being drawn.
    readonly property bool showingVideo: ClipAudioMixer.videoActive

    // Only instantiated where a GL window system and the qml6glsink plugin
    // exist. Under QT_QPA_PLATFORM=offscreen (the ui-shots harness)
    // qml6glsink cannot initialise at all, so creating the item there is
    // pointless — the poster frame carries those captures instead. Loaded by
    // URL so the GStreamer QML import is only resolved when it is usable —
    // see ClipGlVideoItem.qml.
    Loader {
        anchors.fill: parent
        active: ClipAudioMixer.glAvailable
        source: "ClipGlVideoItem.qml"
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
