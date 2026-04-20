#!/usr/bin/env bash
# Build a self-contained AppImage for OpenGG
#
# Usage:
#   ./build-appimage.sh              # full build (compiles + packages)
#   ./build-appimage.sh --skip-build # skip cargo/npm compilation (re-package only)
#
# Output: OpenGG-<VERSION>-x86_64.AppImage in the repo root.
# Requires: wget (for appimagetool if not cached), FUSE or kernel ≥ 5.13
set -euo pipefail

VERSION="0.1.1"
ARCH="x86_64"
OUTPUT="OpenGG-${VERSION}-${ARCH}.AppImage"
ROOT="$(cd "$(dirname "$0")" && pwd)"
APPDIR="$ROOT/AppDir"
APPIMAGETOOL="$ROOT/appimagetool-${ARCH}.AppImage"

# ── CLI flags ─────────────────────────────────────────────────────
SKIP_BUILD=false
for arg in "$@"; do
  case "$arg" in
    --skip-build) SKIP_BUILD=true ;;
    --help|-h)
      echo "Usage: $0 [--skip-build]"
      exit 0 ;;
  esac
done

# ── Helpers ───────────────────────────────────────────────────────
banner() { echo ""; echo "==> $*"; }
die()    { echo "ERROR: $*" >&2; exit 1; }

# ── Step 1: Build ─────────────────────────────────────────────────
if [[ "$SKIP_BUILD" == false ]]; then
  banner "[1/6] Building OpenGG ${VERSION} (daemon + frontend)..."
  cd "$ROOT"
  make build
else
  banner "[1/6] Skipping compilation (--skip-build)"
fi

# ── Step 2: Locate Tauri's AppDir ────────────────────────────────
# Tauri builds a complete AppDir (with linuxdeploy) but may fail at the
# final appimagetool step. We use the pre-built AppDir directly.
banner "[2/6] Locating Tauri AppDir..."
BUNDLE_DIR="$ROOT/frontend/src-tauri/target/release/bundle/appimage"
TAURI_APPDIR="$BUNDLE_DIR/OpenGG.AppDir"

# Prefer a finished Tauri AppImage if one exists, otherwise use the AppDir.
TAURI_APPIMAGE=$(find "$BUNDLE_DIR" -maxdepth 1 -name "*.AppImage" 2>/dev/null | head -1)

DAEMON_BIN="$ROOT/daemon/target/release/openggd"
[[ -f "$DAEMON_BIN" ]] || die "openggd not found at $DAEMON_BIN — run 'make daemon-release'"

# ── Step 3: Assemble working AppDir ──────────────────────────────
banner "[3/6] Assembling AppDir and bundling daemon sidecar..."
rm -rf "$APPDIR" "$ROOT/squashfs-root"
cd "$ROOT"

if [[ -n "$TAURI_APPIMAGE" ]]; then
  echo "    Source: $TAURI_APPIMAGE ($(du -h "$TAURI_APPIMAGE" | cut -f1)) — extracting..."
  APPIMAGE_EXTRACT_AND_RUN=1 "$TAURI_APPIMAGE" --appimage-extract
  mv "$ROOT/squashfs-root" "$APPDIR"
elif [[ -d "$TAURI_APPDIR" ]]; then
  echo "    Source: $TAURI_APPDIR — copying (Tauri AppImage step failed, using pre-built AppDir)..."
  cp -a "$TAURI_APPDIR" "$APPDIR"
else
  die "Neither AppImage nor AppDir found in $BUNDLE_DIR — did the build succeed?"
fi

echo "    Daemon binary  : $DAEMON_BIN ($(du -h "$DAEMON_BIN" | cut -f1))"

# Daemon goes alongside the main binary
cp "$DAEMON_BIN" "$APPDIR/usr/bin/openggd"
chmod +x "$APPDIR/usr/bin/openggd"
echo "    Copied openggd → AppDir/usr/bin/openggd"

# appimagetool requires the root-level icon to match Icon= in the .desktop file
# (case-sensitive). Desktop file says Icon=opengg; Tauri names it OpenGG.png.
if [[ -f "$APPDIR/OpenGG.png" && ! -f "$APPDIR/opengg.png" ]]; then
  ln -sf OpenGG.png "$APPDIR/opengg.png"
  echo "    Symlinked opengg.png → OpenGG.png (appimagetool icon fix)"
fi

# ── Step 4: Bundle packaging assets ──────────────────────────────
banner "[4/6] Bundling packaging assets..."
PKG_DEST="$APPDIR/usr/share/opengg/packaging"
mkdir -p "$PKG_DEST"
cp "$ROOT/packaging/99-opengg.rules"           "$PKG_DEST/"
cp "$ROOT/packaging/openggd.service"           "$PKG_DEST/"
cp "$ROOT/packaging/org.opengg.Daemon.service" "$PKG_DEST/"
cp "$ROOT/packaging/org.opengg.setup.policy"   "$PKG_DEST/"

# Write an AppImage-aware setup script.
# It installs openggd to ~/.local/bin (required by the service files which
# reference %h/.local/bin/openggd) and configures udev/D-Bus/systemd.
cat > "$PKG_DEST/setup.sh" << 'SETUP_EOF'
#!/usr/bin/env bash
# OpenGG AppImage — first-run privileged setup
# Run with: sudo APPDIR=<extracted-path> ./setup.sh
set -euo pipefail

# APPDIR is set by the AppImage runtime, or passed explicitly via env.
# Fall back: four levels up from this script's location.
APPDIR="${APPDIR:-$(cd "$(dirname "$0")/../../../.." && pwd)}"
PKG_DIR="$APPDIR/usr/share/opengg/packaging"

echo "╔══════════════════════════════════════════╗"
echo "║       OpenGG AppImage — First-Run Setup  ║"
echo "╚══════════════════════════════════════════╝"
echo ""

USER_NAME="${SUDO_USER:-$USER}"
USER_HOME=$(eval echo "~$USER_NAME")

# 1. Install udev rules
echo "→ Installing udev rules..."
cp "$PKG_DIR/99-opengg.rules" /etc/udev/rules.d/
udevadm control --reload-rules && udevadm trigger
echo "  ✓ udev rules installed"

# 2. Install openggd daemon to standard user path (required by service files)
echo "→ Installing openggd daemon to $USER_HOME/.local/bin/..."
mkdir -p "$USER_HOME/.local/bin"
install -m 755 "$APPDIR/usr/bin/openggd" "$USER_HOME/.local/bin/openggd"
chown "$USER_NAME:" "$USER_HOME/.local/bin/openggd"
echo "  ✓ openggd installed"

# 3. Add user to required groups
echo "→ Configuring user groups for $USER_NAME..."
for group in audio input video; do
  if getent group "$group" > /dev/null 2>&1; then
    usermod -aG "$group" "$USER_NAME" 2>/dev/null || true
    echo "  ✓ $USER_NAME → $group"
  fi
done
if ! getent group "i2c" > /dev/null 2>&1; then groupadd i2c; fi
usermod -aG i2c "$USER_NAME" 2>/dev/null || true
echo "  ✓ $USER_NAME → i2c"

# 4. Install systemd user service
echo "→ Installing systemd user service..."
SYSTEMD_DIR="$USER_HOME/.config/systemd/user"
mkdir -p "$SYSTEMD_DIR"
cp "$PKG_DIR/openggd.service" "$SYSTEMD_DIR/"
chown "$USER_NAME:" "$SYSTEMD_DIR/openggd.service"
echo "  ✓ Service installed"

# 5. Install D-Bus activation
echo "→ Installing D-Bus activation..."
DBUS_DIR="$USER_HOME/.local/share/dbus-1/services"
mkdir -p "$DBUS_DIR"
sed "s|%h|$USER_HOME|g" "$PKG_DIR/org.opengg.Daemon.service" \
    > "$DBUS_DIR/org.opengg.Daemon.service"
chown "$USER_NAME:" "$DBUS_DIR/org.opengg.Daemon.service"
echo "  ✓ D-Bus activation installed"

# 6. Install polkit policy
echo "→ Installing polkit policy..."
cp "$PKG_DIR/org.opengg.setup.policy" /usr/share/polkit-1/actions/ 2>/dev/null || true
echo "  ✓ polkit policy installed"

# 7. Enable service
echo "→ Enabling systemd service..."
su - "$USER_NAME" -c "systemctl --user daemon-reload && systemctl --user enable openggd.service" \
    2>/dev/null || true
echo "  ✓ Service enabled"

echo ""
echo "╔════════════════════════════════════════════════╗"
echo "║  Setup complete!                               ║"
echo "║                                                ║"
echo "║  ⚠  Log out and back in for group changes.    ║"
echo "║                                                ║"
echo "║  The daemon starts automatically when the UI  ║"
echo "║  makes its first D-Bus call.                  ║"
echo "╚════════════════════════════════════════════════╝"
SETUP_EOF
chmod +x "$PKG_DEST/setup.sh"
echo "    Packaged: udev rules, service files, setup.sh"

# ── Step 4b: Bundle GStreamer video plugins ───────────────────────
# Tauri's linuxdeploy bundles the GStreamer framework libraries (libgstreamer-1.0.so.0, etc.)
# but leaves the plugin directory empty. WebKitGTK needs codec plugins to play video clips.
# AppRun sets GST_PLUGIN_PATH to the bundled dir and clears GST_PLUGIN_SYSTEM_PATH_1_0,
# so without these plugins the AppImage cannot decode any video — it freezes/crashes.
banner "[4b/6] Bundling GStreamer video plugins for WebKit playback..."
GST_PLUGIN_DEST="$APPDIR/usr/lib/gstreamer-1.0"
mkdir -p "$GST_PLUGIN_DEST"
SYS_GST_DIR="/usr/lib/gstreamer-1.0"

GST_ESSENTIAL_PLUGINS=(
  libgstcoreelements.so      # filesrc, queue, sink (fundamentals)
  libgstplayback.so          # playbin / uridecodebin (main playback pipeline)
  libgsttypefindfunctions.so # container format auto-detection
  libgstmatroska.so          # MKV demuxer (primary GSR output format)
  libgstisomp4.so            # MP4 demuxer
  libgstlibav.so             # H264/H265/AAC via libavcodec bridge
  libgstvpx.so               # VP8/VP9 (alternate GSR codec)
  libgstx264.so              # H264 encoder
  libgstvideoparsersbad.so   # H264/H265 elementary stream parsers
  libgstaudioparsers.so      # AAC/MP3/FLAC elementary parsers
  libgstaudioconvert.so      # audio format conversion
  libgstaudioresample.so     # sample rate conversion
  libgstvideoconvertscale.so # pixel format conversion + scaling
  libgstvolume.so            # volume control
  libgstopengl.so            # GL video sink (used by WebKitGTK)
  libgstpulseaudio.so        # PulseAudio/PipeWire audio output
  libgstautodetect.so        # autoaudiosink / autovideosink
  libgstapp.so               # appsrc / appsink
  libgstgio.so               # GIO file source (local file playback)
  libgstopus.so              # Opus audio codec
  libgstogg.so               # Ogg container
  libgstvorbis.so            # Vorbis audio
  libgstpbtypes.so           # GStreamer-PbUtils type registration
  libgstflv.so               # FLV container
  libgstavi.so               # AVI container
  libgstpng.so               # PNG image support (thumbnails)
  libgstjpeg.so              # JPEG image support (thumbnails)
)

COPIED=0
for plugin in "${GST_ESSENTIAL_PLUGINS[@]}"; do
  src="$SYS_GST_DIR/${plugin%% *}"   # strip inline comment if any
  if [[ -f "$src" ]]; then
    cp "$src" "$GST_PLUGIN_DEST/"
    COPIED=$((COPIED + 1))
  else
    echo "    WARNING: ${plugin%% *} not found on system — skipping"
  fi
done

# gst-plugin-scanner: needed to build the per-user plugin registry cache
if [[ -f "$SYS_GST_DIR/gst-plugin-scanner" ]]; then
  cp "$SYS_GST_DIR/gst-plugin-scanner" "$GST_PLUGIN_DEST/"
  chmod +x "$GST_PLUGIN_DEST/gst-plugin-scanner"
  echo "    Copied gst-plugin-scanner"
else
  echo "    WARNING: gst-plugin-scanner not found — plugin registry may fail"
fi

echo "    Bundled $COPIED / ${#GST_ESSENTIAL_PLUGINS[@]} GStreamer plugins"

# ── Step 5: Write AppRun ──────────────────────────────────────────
banner "[5/6] Writing AppRun..."
cat > "$APPDIR/AppRun" << 'APPRUN_EOF'
#!/bin/bash
HERE="$(dirname "$(readlink -f "$0")")"

# Bundled libraries take precedence over system — prevents ABI mismatches
# with WebKit and GStreamer shipped inside the AppImage.
export LD_LIBRARY_PATH="${HERE}/usr/lib:${HERE}/usr/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}"

# Use only the bundled GStreamer plugins; don't mix with system plugins.
export GST_PLUGIN_PATH="${HERE}/usr/lib/gstreamer-1.0"
export GST_PLUGIN_SYSTEM_PATH_1_0=""
export GST_PLUGIN_SCANNER="${HERE}/usr/lib/gstreamer-1.0/gst-plugin-scanner"
export GST_REGISTRY="${HOME}/.cache/opengg-gst-registry.bin"

# WebKit shared memory path (Tauri/WebKitGTK requirement).
if [ -d "${HERE}/usr/lib/webkit2gtk-4.1" ]; then
  export WEBKIT_SHARED_MEMORY_DIR="${HERE}/usr/lib/webkit2gtk-4.1"
fi

# Expose bundled share data (icons, mimetypes, etc.)
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"

# Add our bin/ to PATH so opengg can locate the openggd sidecar.
export PATH="${HERE}/usr/bin:${PATH}"

# Ensure data dirs exist — SQLite DB / thumbnail creation silently fails without these,
# causing a blank Clips page on first launch inside the AppImage container.
mkdir -p "${HOME}/.local/share/opengg/thumbnails"
mkdir -p "${HOME}/.config/opengg"

exec "${HERE}/usr/bin/opengg" "$@"
APPRUN_EOF
chmod +x "$APPDIR/AppRun"

# ── Step 6: Fetch appimagetool + repack ───────────────────────────
banner "[6/6] Packaging ${OUTPUT}..."
if [[ ! -f "$APPIMAGETOOL" ]]; then
  echo "    appimagetool not found — downloading..."
  wget -q --show-progress \
    -O "$APPIMAGETOOL" \
    "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
  chmod +x "$APPIMAGETOOL"
fi

cd "$ROOT"
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 \
  "$APPIMAGETOOL" AppDir "$OUTPUT"

# Cleanup extracted dir
rm -rf "$APPDIR"

SIZE=$(du -h "$OUTPUT" | cut -f1)
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Built  : $OUTPUT"
echo "  Size   : $SIZE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "  Run it:"
echo "    chmod +x $OUTPUT && ./$OUTPUT"
echo ""
echo "  First-time setup (one-time, requires sudo):"
echo "    ./$OUTPUT --appimage-extract"
echo "    sudo APPDIR=squashfs-root \\"
echo "      squashfs-root/usr/share/opengg/packaging/setup.sh"
echo "    rm -rf squashfs-root   # cleanup — AppImage is self-contained"
echo ""
