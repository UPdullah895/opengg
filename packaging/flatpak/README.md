# OpenGG Flatpak Packaging

This directory contains the Flatpak manifest and metadata for OpenGG.

## Overview

OpenGG is a Linux gaming hub built with a Qt6/QML frontend and a Rust background daemon. The Flatpak package bundles both components and integrates with the host system's D-Bus, PipeWire/PulseAudio, and other services.

### Architecture
- **Daemon**: `openggd` — unprivileged Rust service owning `org.opengg.Daemon` on the session D-Bus
- **Frontend**: `opengg` — Qt6/QML app (built via cxx-qt, binary name `opengg-qt`), video playback via GStreamer's `qml6glsink` into the Qt Quick OpenGL scene graph
- **IPC**: cxx-qt controllers call `opengg-core` directly in-process; `opengg-core` talks to the daemon over the D-Bus session bus
- **Audio**: PipeWire/PulseAudio subprocess calls (`pactl`, `pw-link`)
- **Screen Capture**: `gpu-screen-recorder` subprocess with optional sandbox portal integration

## Build Prerequisites

### System Requirements
- `flatpak-builder` ≥ 1.2.0
- KDE Platform and SDK 6.7+ (see **Runtime Version Choice** below)
- Rust support via `org.freedesktop.Sdk.Extension.rust-stable`
- `python3` (for generating vendored source manifests)

### Installation (Fedora example)
```bash
sudo dnf install flatpak flatpak-builder
flatpak install flathub org.kde.Platform//6.7 org.kde.Sdk//6.7
```

### Installation (Ubuntu/Debian example)
```bash
sudo apt install flatpak flatpak-builder
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install flathub org.kde.Platform/x86_64/6.7 org.kde.Sdk/x86_64/6.7
```

## Runtime Version Choice: KDE 6.7

**Why KDE 6.7?**
- Ships **Qt 6.7+**, which qt-shell's QML requires (per-corner `radius` properties and other 6.7+ APIs — see the note in `.github/workflows/ci.yml` about why CI builds qt-shell on Arch rather than Ubuntu, whose Qt packages lag behind)
- The KDE runtime is the natural fit for a Qt Quick app, the same way the old manifest used the GNOME runtime for its WebKitGTK dependency
- Contains updated PipeWire/PulseAudio bindings

If a newer KDE runtime ships a Qt version qt-shell needs, bump `runtime-version: '6.7'` in `org.opengg.OpenGG.yml` accordingly.

**Not yet resolved (see Stub Items below):** the KDE runtime is not guaranteed to bundle GStreamer's `gst-plugin-qml6`, which qt-shell's video playback depends on — this may need its own module.

## Generating Vendored Sources

Flatpak uses offline builds for security and reproducibility. Cargo dependencies must be vendored before building.

### Prerequisites for Source Generation
```bash
pip install requests  # Required by flatpak-builder-tools scripts
git clone https://github.com/flatpak/flatpak-builder-tools.git ~/flatpak-builder-tools
```

### Generate Cargo Sources

For the daemon:
```bash
cd ~/flatpak-builder-tools/cargo
python3 flatpak-cargo-generator.py \
  ../../../daemon/Cargo.lock \
  -o ../../../packaging/flatpak/cargo-sources.json
```

For the Qt6/QML shell (this also vendors `opengg-core`, since qt-shell depends on it via a path dependency and both are locked in the same `Cargo.lock`):
```bash
cd ~/flatpak-builder-tools/cargo
python3 flatpak-cargo-generator.py \
  ../../../qt-shell/Cargo.lock \
  -o ../../../packaging/flatpak/qt-shell-cargo-sources.json
```

The generated JSON files will be sourced by the manifest's module entries. Store these in version control or regenerate as part of your release pipeline.

## Building the Flatpak

### Quick Test Build
```bash
cd /path/to/opengg
flatpak-builder --user --install build-dir packaging/flatpak/org.opengg.OpenGG.yml
```

- `--user` — installs to user flatpak directory (`~/.local/share/flatpak`)
- `--install` — registers the app after building
- `build-dir` — temporary build directory (can be discarded after)

### Full Build with Cleanup
```bash
rm -rf build-dir
flatpak-builder --user --install --force-clean build-dir packaging/flatpak/org.opengg.OpenGG.yml
```

### Build Output
- Binary: `~/.local/share/flatpak/app/org.opengg.OpenGG/current/active/bin/opengg`
- App metadata: `~/.local/share/flatpak/app/org.opengg.OpenGG/current/active/`

### Running the Flatpak
```bash
flatpak run org.opengg.OpenGG
```

## Finish Arguments (Sandbox Permissions)

Each permission in `finish-args` is justified below:

| Argument | Justification |
|----------|---|
| `--socket=wayland` | Qt Quick scene graph (and qml6glsink video rendering) needs the Wayland protocol |
| `--socket=x11` | X11 socket for legacy/hybrid sessions and Xwayland |
| `--share=ipc` | IPC namespace sharing for X11 and Wayland protocols |
| `--socket=pulseaudio` | PipeWire/PulseAudio socket for audio device enumeration and `pactl` subprocess calls |
| `--filesystem=xdg-videos` | Read/write access to ~/Videos for default clip storage directory |
| `--filesystem=xdg-config/opengg:create` | User config directory (~/.config/opengg/daemon.toml, ui-settings.json, theme.json) |
| `--filesystem=xdg-data/opengg:create` | User data directory (~/.local/share/opengg) for clip database, thumbnails, crash logs |
| `--device=dri` | GPU device access for the Qt Quick OpenGL scene graph and FFmpeg hardware acceleration |
| `--own-name=org.opengg.Daemon` | Own the D-Bus service name for the background daemon |
| `--system-talk-name=org.freedesktop.ratbag1` | Mouse/keyboard configuration via ratbagd (system D-Bus) |
| `--talk-name=org.freedesktop.portal.Flatpak` | Desktop portal integration (XDG portals) |
| `--talk-name=org.freedesktop.portal.Desktop` | Portal access for file dialogs, screen casting, etc. |
| `--system-talk-name=org.freedesktop.systemd1` | Optional: future daemon/service control (currently unused) |
| `--share=network` | Localhost TCP socket for OpenRGB (6742) and internal media server |
| `--talk-name=org.freedesktop.Notifications` | Desktop notifications via notification service |

### Note on Portals
Desktop portals (xdg-desktop-portal, org.freedesktop.portal.*) do **not** require explicit `--talk-name` declarations when using the standard filter names. They are auto-discovered by the portal activation system.

## gpu-screen-recorder Strategy

OpenGG uses `gpu-screen-recorder` for clip recording. Inside the Flatpak sandbox, there are two approaches:

### Option A: Portal-Based Screen Capture (Recommended Long-Term)

Use `gpu-screen-recorder -w portal` to leverage xdg-desktop-portal's `ScreenCast` interface:

**Pros:**
- Integrated sandbox, no privileged access
- Secure (user grants permission once via portal dialog)
- Future-proof (works with future display servers)

**Cons:**
- Requires `gpu-screen-recorder` ≥ 0.5.0 with portal support
- May have lower performance or compatibility issues in early versions

**Setup:**
```bash
# Inside the Flatpak manifest, add an optional module for gpu-screen-recorder if not bundling:
# (Stub example — implementation pending flatpak-builder-tools analysis)
```

### Option B: flatpak-spawn --host (Current Interim Solution)

Run gpu-screen-recorder on the host system using `flatpak-spawn`:

**Pros:**
- Works with any gpu-screen-recorder version today
- Full native performance

**Cons:**
- Requires `--talk-name=org.freedesktop.Flatpak` (escalated D-Bus permission)
- Less secure (app can invoke arbitrary host binaries)
- Tightly couples sandbox app to host package availability

**Implementation:**
`opengg-core` (`core/src/gsr.rs`) already spawns `gpu-screen-recorder` as a plain subprocess; under Flatpak this call needs to detect the sandbox and re-dispatch through `flatpak-spawn --host`:

```rust
let is_flatpak = std::fs::metadata("/.flatpak-info").is_ok();

if is_flatpak {
    // Use flatpak-spawn to invoke gpu-screen-recorder on the host
    let output = std::process::Command::new("flatpak-spawn")
        .args(&["--host", "gpu-screen-recorder", ...])
        .output()?;
} else {
    // Direct invocation (non-Flatpak) — the current, only implemented path
    let output = std::process::Command::new("gpu-screen-recorder")
        .args(&[...])
        .output()?;
}
```

**Decision:** This scaffolding documents both paths; neither Flatpak dispatch branch is implemented in `core/src/gsr.rs` yet. **For production, prioritize Option A** (portal) and file an issue to track gpu-screen-recorder portal support. As interim, Option B works today if users have `gpu-screen-recorder` installed on the host.

### Not Bundling gpu-screen-recorder (Recommended)

The current manifest **does not bundle** gpu-screen-recorder. Instead:
1. Users install `gpu-screen-recorder` on the host system
2. OpenGG app delegates to it via subprocess or flatpak-spawn
3. Reduces Flatpak size and maintenance burden

If bundling becomes necessary, add a `gpu-screen-recorder` module to the manifest's modules list (requires flatpak-builder-tools analysis of its build system).

## Known Limitations in Sandbox

### headsetcontrol / Raw HID

OpenGG supports headset control via `headsetcontrol` for battery and mode indicators. This requires **raw HID device access**, which is not granted by Flatpak by default.

**Current status:** Disabled in sandbox. Workaround: install `headsetcontrol` on the host and use `--device=all` (risky, not recommended).

**Future:** May require a D-Bus service abstraction (e.g., `org.freedesktop.Headsets` or a custom service).

### Device udev Rules

The daemon uses `pactl` (subprocess) to manage PipeWire virtual sinks. This works in the sandbox because `pactl` is a simple D-Bus client.

Raw udev device management (reading `/dev/input/*`, `/sys/class/hidraw/*` directly) requires either:
- `--device=all` (overly permissive)
- Future: a D-Bus portals abstraction for device enumeration

**Ratbagd** (mouse/keyboard) works via system D-Bus (`--system-talk-name=org.freedesktop.ratbag1`) — no raw device access needed.

**OpenRGB** requires TCP 6742 to the host. This works with `--share=network` (localhost only, safe).

### Summary of Limitations
| Feature | Status | Notes |
|---------|--------|-------|
| Audio mixer | ✓ Full | PipeWire/PulseAudio via `pactl` (D-Bus client) |
| Clip recording | ⚠ Portal/Host | gpu-screen-recorder via portal or flatpak-spawn |
| Clip video playback | ⚠ Untested in sandbox | `qml6glsink`'s GL scene-graph requirement (see `qt-shell/src/mixer_pipeline.rs`) has not been verified inside a Flatpak sandbox |
| Mouse/keyboard | ✓ Full | ratbagd over system D-Bus |
| RGB control | ✓ Full | OpenRGB over localhost TCP |
| Headset control | ✗ Limited | Raw HID not available; requires host workaround |
| Clip file access | ✓ Full | ~/Videos and ~/.local/share/opengg sandboxed |

## Testing the Build

### Manifest YAML Validation
```bash
python3 << 'EOF'
import yaml
with open('packaging/flatpak/org.opengg.OpenGG.yml') as f:
    manifest = yaml.safe_load(f)
    print("✓ Manifest YAML is valid")
    print(f"  App ID: {manifest['app-id']}")
    print(f"  Runtime: {manifest['runtime']} ({manifest['runtime-version']})")
    print(f"  Modules: {len(manifest['modules'])}")
EOF
```

### Metainfo XML Validation
```bash
python3 << 'EOF'
import xml.etree.ElementTree as ET
tree = ET.parse('packaging/flatpak/org.opengg.OpenGG.metainfo.xml')
root = tree.getroot()
print("✓ Metainfo XML is valid")
print(f"  ID: {root.find('id').text}")
print(f"  Name: {root.find('name').text}")
print(f"  License: {root.find('project_license').text}")
EOF
```

### Manifest Show (Dry-Run)
```bash
flatpak-builder --show-manifest build-dir packaging/flatpak/org.opengg.OpenGG.yml 2>/dev/null | head -50
```

This displays the resolved manifest without building (requires `flatpak-builder` to be installed).

## Stub Items (TODO)

The following items are **stubbed** and require completion:

1. **`cargo-sources.json`** (daemon) — Generate via flatpak-cargo-generator from `daemon/Cargo.lock`
2. **`qt-shell-cargo-sources.json`** (qt-shell + opengg-core) — Generate via flatpak-cargo-generator from `qt-shell/Cargo.lock`
3. **GStreamer Qt6 sink availability** — Confirm whether the `org.kde.Platform` runtime ships `gst-plugin-qml6`; if not, add a module for it (qt-shell's clip video playback depends on it — see `qt-shell/src/main.rs`)
4. **D-Bus service file** — Uncomment and verify the commented D-Bus service installation in the daemon module
5. **gpu-screen-recorder module** — Decide on bundling vs. host-delegated approach and document final decision
6. **Test on real Flatpak environment** — Requires a machine with `flatpak-builder` and the KDE SDK
7. **Verify qml6glsink inside the sandbox** — `qml6glsink` needs a live GL scene graph (see `AGENTS.md`'s QML landmines and `qt-shell/src/mixer_pipeline.rs`); this has never been confirmed to work under Flatpak's DRI/GL passthrough
8. **Flathub submission** — Screenshots, release notes, and review checklist

## Integration with Release Pipeline

### Recommended Workflow
1. Generate vendored sources as part of release CI:
   ```bash
   ./packaging/flatpak/generate-sources.sh  # (to be created)
   ```
2. Commit generated JSON files to the repo (or store as CI artifacts)
3. Build Flatpak in CI:
   ```bash
   flatpak-builder --user --install ./build build/org.opengg.OpenGG.yml
   ```
4. Package and push to Flathub (manual or CI-driven)

### Permissions Model
- This manifest is **user-installable** (not requiring root or system installation)
- Suitable for Flathub (community distribution) or self-hosted flatpak repositories

## References

- [Flatpak Manifest Format](https://docs.flatpak.org/en/latest/flatpak-manifest.html)
- [Flatpak Portal Documentation](https://flatpak.readthedocs.io/en/latest/desktop-integration.html)
- [flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools)
- [AppStream Metainfo Standard](https://www.freedesktop.org/wiki/Specifications/AppStream/Metadata/)
- [gpu-screen-recorder Portal Support](https://github.com/dec05eba/gpu-screen-recorder/issues)
- [KDE Runtime Releases](https://community.kde.org/Guidelines_and_HOWTOs/Flatpak)

## Contributing

When updating this Flatpak package:
1. Regenerate vendored source manifests for any Cargo.lock changes
2. Test builds locally with `flatpak-builder --show-manifest` before committing
3. Update this README with any new permissions, modules, or limitations
4. Keep the manifest version-synced with OpenGG releases (see `daemon/Cargo.toml` and `qt-shell/Cargo.toml`)
