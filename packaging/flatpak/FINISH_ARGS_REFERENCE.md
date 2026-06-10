# Finish Args Reference

This document details every `finish-args` permission in the Flatpak manifest with justification.

## Display & Graphics

### `--socket=wayland`
**Justification:** Tauri's WebKitGTK 4.1 webview requires the Wayland protocol for rendering on modern Linux desktops.

### `--socket=x11`
**Justification:** X11 socket for legacy X11-only desktops and Xwayland (X11 apps on Wayland compositors).

### `--share=ipc`
**Justification:** IPC namespace sharing required by both Wayland and X11 protocols for efficient communication. Essential for display server integration.

### `--device=dri`
**Justification:** GPU device access for:
- Tauri webview rendering (GPU-accelerated rasterization)
- FFmpeg hardware video codec support (H.264/H.265 encoding/decoding)
- Vulkan/OpenGL if used by future GPU-screen-recorder portal integration

## Audio

### `--socket=pulseaudio`
**Justification:** PipeWire/PulseAudio socket access for:
- `pactl` subprocess calls (device listing, virtual sink creation, app routing)
- `pw-link` for PipeWire connection management
- Audio device enumeration (required for the mixer UI)
- VU meter updates (polling audio levels)

## File System

### `--filesystem=xdg-videos`
**Justification:** Read/write access to `~/Videos` for the default clip storage directory (`~/Videos/OpenGG`).

### `--filesystem=xdg-config/opengg:create`
**Justification:** User configuration directory `~/.config/opengg/` for:
- `daemon.toml` — daemon settings (audio, device, replay config)
- `ui-settings.json` — frontend preferences (mixer routing, DSP presets, theme)
- `theme.json` — custom color scheme
- `:create` flag ensures the directory is created if missing

### `--filesystem=xdg-data/opengg:create`
**Justification:** User data directory `~/.local/share/opengg/` for:
- `clips.db` — SQLite clip metadata database
- `thumbnails/` — video thumbnail cache
- `opengg_crash.log` — diagnostic log
- `:create` flag ensures the directory is created on first run

## D-Bus (Session Bus)

### `--own-name=org.opengg.Daemon`
**Justification:** The unprivileged daemon process owns the `org.opengg.Daemon` service name on the session bus. This allows:
- The Tauri frontend to invoke daemon methods via D-Bus
- External tools to control OpenGG audio/device settings
- Auto-activation of the daemon on first D-Bus method call (defined in `org.opengg.Daemon.service`)

## D-Bus (System Bus)

### `--system-talk-name=org.freedesktop.ratbag1`
**Justification:** System D-Bus access to the `ratbagd` service for mouse and keyboard configuration:
- DPI settings, button mapping, polling rate
- RGB profile management (if hardware supports it via ratbagd)

### `--system-talk-name=org.freedesktop.systemd1`
**Justification:** (Optional, currently unused) System D-Bus access for future daemon lifecycle management:
- Starting/stopping the openggd systemd user service
- Querying service status

## Desktop Portals

### `--talk-name=org.freedesktop.portal.Flatpak`
**Justification:** Portal metadata and sandbox introspection:
- Detecting if the app is running under Flatpak (`/.flatpak-info` may not be accessible)
- Portal filter activation (implicit for portal methods)

### `--talk-name=org.freedesktop.portal.Desktop`
**Justification:** Access to multiple standard portal interfaces:
- `FileChooser` — file open/save dialogs (clip export, config import)
- `ScreenCast` — screen capture for gpu-screen-recorder (with `-w portal` flag)
- `Screenshot` — future screenshot portal support
- `OpenURI` — opening external links

## Networking

### `--share=network`
**Justification:** Network socket access for:
- **OpenRGB** — TCP connection to `localhost:6742` for RGB device control
- **Internal warp media server** — `localhost:<ephemeral port>` HTTP server for video file serving (bypasses WebKitGTK asset:// bugs)
- **Future updates** — if auto-update checking is enabled

**Security note:** Sandboxed, only accessible from the localhost address; no outbound internet access to arbitrary hosts without explicit user action.

## Notifications

### `--talk-name=org.freedesktop.Notifications`
**Justification:** Desktop notification integration for clip save notifications:
- Delegates to the system notification daemon (D-Bus service)
- Supports both X11 and Wayland notification styles

## Implicit Portal Permissions

The following do **not** require explicit `--talk-name` declarations because they are activated via portal filter entries in the sandbox:

- `org.freedesktop.portal.OpenURI` (implied by `--talk-name=org.freedesktop.portal.Desktop`)
- `org.freedesktop.portal.ScreenCast` (implied by `--talk-name=org.freedesktop.portal.Desktop`)
- `org.freedesktop.portal.Screenshot` (implied by `--talk-name=org.freedesktop.portal.Desktop`)

---

## Permission Summary Table

| Argument | Type | Scope | Rationale |
|----------|------|-------|-----------|
| wayland, x11, ipc | Display | Mandatory | Tauri webview rendering |
| pulseaudio | Audio | Mandatory | Audio device access, PipeWire integration |
| xdg-videos, xdg-config/opengg, xdg-data/opengg | FS | Mandatory | Config, clips, cache, database |
| dri | Device | Mandatory | GPU rendering & encoding |
| own-name=org.opengg.Daemon | D-Bus (session) | Mandatory | Daemon registration |
| system-talk-name=ratbag1 | D-Bus (system) | Optional | Mouse/keyboard config |
| system-talk-name=systemd1 | D-Bus (system) | Optional | Future daemon control |
| portal.Flatpak, portal.Desktop | Portal | Mandatory | File dialogs, screen capture, notifications |
| network | Socket | Optional | OpenRGB, media server (localhost only) |
| Notifications | D-Bus | Optional | Clip save notifications |

---

## Testing Permission Scope

To verify which permissions are actually in use:

```bash
# Monitor D-Bus calls during app runtime
dbus-monitor --session path=/org/opengg/Daemon

# Monitor file system access
fatrace -o /tmp/opengg-fat.log
flatpak run org.opengg.OpenGG
# Review /tmp/opengg-fat.log for file access patterns

# Monitor network connections
netstat -tupan | grep opengg
# Should only show localhost:6742 and ephemeral media server ports
```

## Potential Future Permissions

- `--session-talk-name=org.freedesktop.impl.portal.*` — if custom portal backends are used
- `--device=all` — for raw HID device access (headsetcontrol); currently not recommended due to security risk
- `--talk-name=org.freedesktop.Accounts` — if user preference integration is added
