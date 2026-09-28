import QtQuick
import QtQuick.Window
import org.freedesktop.gstreamer.Qt6GLVideoItem 1.0

Window {
    visible: true
    width: 640; height: 360
    color: "#101014"
    GstGLQt6VideoItem {
        objectName: "videoItem"
        anchors.fill: parent
    }
}
