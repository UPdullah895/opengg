# Spike: qml6glsink video into a Qt6 QML scene

Standalone C++ harness that proved the unified-player approach before any of
it was built into the app (plan step B1, 2026-08-01). Kept because it is the
smallest thing that reproduces the setup, and because every non-obvious
constraint below cost time to find.

## Run

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build -j4
./build/qml6spike ~/Videos/OpenGG/<some-clip>.mp4     # writes /tmp/qml6-spike.png
```

Always run it with `QT_FORCE_STDERR_LOGGING=1` — Qt sends QML errors to
journald, so failures otherwise print *nothing at all*.

## What it establishes

* **The QML type is `GstGLQt6VideoItem`**, in module
  `org.freedesktop.gstreamer.Qt6GLVideoItem`. `GstGLVideoItem` is the Qt5
  name and is what nearly every example online shows; using it fails with
  `"GstGLVideoItem is not a type"` — which reads like a missing module but
  actually means the module resolved and only the type name is wrong.
* **The type is registered by the GStreamer plugin**, not by a qmldir —
  Arch ships only `libgstqml6.so`. `qmlRegisterType` runs inside
  `gst_element_register_qml6glsink`, so the plugin has to be loaded before
  the QML engine resolves the import.
* **`QQuickWindow::setGraphicsApi(OpenGL)` before any window**, and the
  pipeline must reach PLAYING only after `sceneGraphInitialized` — earlier
  and the sink has no GL context, giving `GST_STATE_CHANGE_FAILURE` (0)
  rather than `ASYNC` (2).
* **It cannot run under `QT_QPA_PLATFORM=offscreen`** —
  `Could not initialize window system`. That is the *window system*, not
  the GL driver, so forcing software GL does not help. The real player
  must therefore degrade to a poster frame when there is no GL window
  system, or `tools/ui-shots.sh` breaks on the player and editor pages.

## Requires

`gst-plugin-qml6` (`sudo pacman -S gst-plugin-qml6`).
