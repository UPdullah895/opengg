# OpenGG — System Architecture Blueprint
## Open-Source Linux Gaming Hub (SteelSeries GG Alternative)

---

## 1. System Architecture

### High-Level Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                           USER SPACE                                │
│                                                                     │
│  ┌───────────────────────┐   D-Bus IPC        ┌──────────────────┐ │
│  │    Tauri 2 + Vue 3    │◄──────────────────►│   openggd (Rust) │ │
│  │    Frontend UI         │  org.opengg.Daemon │   Background     │ │
│  │                       │                     │   Daemon         │ │
│  │  Pages:               │   Tauri Events      │                  │ │
│  │  ├─ Audio Mixer       │◄─(VU levels)───────│   Modules:       │ │
│  │  ├─ Clips Gallery     │                     │   ├─ audio/      │ │
│  │  ├─ Device Manager    │   warp HTTP         │   ├─ device/     │ │
│  │  └─ Settings          │◄─(media files)──┐  │   ├─ replay/     │ │
│  │                       │                  │  │   └─ config/     │ │
│  │  Tauri Sidecar:       │                  │  └────────┬─────────┘ │
│  │  ├─ commands.rs       │                  │           │           │
│  │  ├─ media_server.rs ──┘                  │           │           │
│  │  └─ SQLite (clips.db)                    │           │           │
│  └───────────────────────┘                  │           │           │
│                                              │           │           │
│  ┌────────────┐  ┌───────────┐  ┌───────────▼──┐  ┌────▼────────┐ │
│  │  ratbagd   │  │ OpenRGB   │  │ gpu-screen-  │  │  PipeWire   │ │
│  │  (D-Bus)   │  │ (TCP SDK) │  │ recorder     │  │  (pactl /   │ │
│  └─────┬──────┘  └─────┬─────┘  │ (subprocess) │  │   pw-link)  │ │
│        │               │        └──────────────┘  └──────┬──────┘ │
│        │               │                                  │        │
├────────┼───────────────┼──────────────────────────────────┼────────┤
│        │   KERNEL       │                                  │        │
│  ┌─────▼─────┐   ┌─────▼─────┐   ┌───────────┐   ┌──────▼──────┐ │
│  │ libinput  │   │  HID/USB  │   │   ALSA    │   │  DRM/KMS   │ │
│  │ udev      │   │  udev     │   │  V4L2     │   │  (display)  │ │
│  └───────────┘   └───────────┘   └───────────┘   └─────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### IPC Strategy

| Communication Path          | Protocol         | Why                                      |
|-----------------------------|------------------|------------------------------------------|
| Frontend → Daemon           | D-Bus (session)  | Standard Linux IPC, auto-activation      |
| Frontend → PipeWire         | pactl / pw-link  | pipewire-rs is !Send/!Sync (Rc types)    |
| Frontend ← VU Meters        | Tauri Events     | 50ms streaming, low overhead             |
| Frontend ← Media Files      | warp HTTP server | Bypasses WebKitGTK asset:// bugs         |
| Daemon → ratbagd            | D-Bus            | ratbagd's native protocol                |
| Daemon → OpenRGB            | TCP socket       | OpenRGB SDK protocol (port 6742)         |
| Daemon → gpu-screen-recorder| subprocess       | CLI tool, spawned/killed via signals     |
| Settings persistence        | JSON file        | ~/.config/opengg/ui-settings.json        |
| Clip metadata               | SQLite           | ~/.local/share/opengg/clips.db           |
| Theme customization         | JSON file        | ~/.config/opengg/theme.json              |

### Technology Stack

| Component         | Technology               | Reasoning                                    |
|-------------------|--------------------------|----------------------------------------------|
| Daemon            | Rust + tokio + zbus      | Memory-safe, async D-Bus, zero GC pauses     |
| Frontend          | Tauri 2 + Vue 3 + Pinia | Full CSS control for gaming aesthetic         |
| Audio             | pactl + pw-link (CLI)    | pipewire-rs has Send/Sync issues with zbus   |
| Media serving     | warp (Rust HTTP)         | Solves WebKitGTK local file loading bugs     |
| Clip metadata     | rusqlite (bundled)       | Fast queries, no external DB dependency      |
| Recording         | gpu-screen-recorder      | GPU-accelerated, minimal CPU overhead        |
| Peripherals       | ratbagctl + OpenRGB TCP  | Well-tested Linux gaming device tools        |

---

## 2. Permissions and Security

### Strategy: udev Rules + User Groups + polkit

Zero sudo during normal operation. The only privileged action is the one-time setup.sh which installs udev rules and adds group memberships.

#### udev Rules (packaging/99-opengg.rules)

```
SUBSYSTEM=="usb", ATTRS{idVendor}=="1038", MODE="0666"   # SteelSeries
SUBSYSTEM=="usb", ATTRS{idVendor}=="046d", MODE="0666"   # Logitech
SUBSYSTEM=="usb", ATTRS{idVendor}=="1532", MODE="0666"   # Razer
KERNEL=="event[0-9]*", SUBSYSTEM=="input", MODE="0660", GROUP="input"
KERNEL=="renderD*", SUBSYSTEM=="drm", MODE="0666"
```

#### User Groups (setup.sh)

```bash
sudo usermod -aG input,video,render $USER
```

- input: Read /dev/input/event* for global hotkeys
- video: Access /dev/video* for V4L2
- render: Access /dev/dri/renderD* for GPU recording

#### D-Bus Auto-Activation (no manual daemon start)

```ini
[D-BUS Service]
Name=org.opengg.Daemon
Exec=/usr/bin/openggd
User=%U
```

---

## 3. Module 1: Audio Hub Implementation

### Critical Rule: Never Restart PipeWire

The daemon creates virtual sinks at runtime via pactl load-module module-null-sink.
This preserves all active audio streams (Discord, browsers, games).

### Sink Creation (Rust)

```rust
fn create_null_sink(sink_name: &str, description: &str) -> Result<u32> {
    let output = Command::new("pactl")
        .args([
            "load-module", "module-null-sink",
            &format!("sink_name={sink_name}"),
            &format!("sink_properties=device.description=\"OpenGG {description}\""),
            "channels=2", "channel_map=front-left,front-right",
        ])
        .output()
        .context("pactl not found")?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().parse().unwrap_or(0))
    } else {
        anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr))
    }
}
```

### Audio Flow

```
App (Discord) → OpenGG_Chat virtual sink → pw-link → Physical headphones
                     ↑                         ↑
              pactl move-sink-input     per-channel device routing
```

### App Routing

```rust
pub fn route_app_to_channel(stream_id: u32, channel: &str) -> Result<()> {
    let sink = if channel == "default" {
        // pactl get-default-sink
    } else {
        format!("OpenGG_{channel}")
    };
    Command::new("pactl")
        .args(["move-sink-input", &stream_id.to_string(), &sink])
        .output()?;
    Ok(())
}
```

---

## Status: What is Built vs. Planned

### Implemented (v1.0.0)

- Virtual sink creation (graceful, no PipeWire restart)
- Per-channel volume/mute and device routing
- App routing with drag-and-drop UI
- VU meter streaming via Tauri events
- Clip scanning with SQLite metadata
- Lazy thumbnail generation (ffmpeg + IntersectionObserver)
- Video trim (stream copy) and sized export
- Favorites, search, sort, filter, multi-select
- Settings persistence (auto-save JSON)
- Theme system (JSON → CSS variables)
- Local media HTTP server (warp)
- Screen recording via gpu-screen-recorder
- Custom titlebar and responsive layout

### Planned (Phase 2)

- Parametric EQ (WirePlumber Lua + LADSPA/LV2)
- AI Noise Cancellation (RNNoise LADSPA)
- Real VU from PCM peaks (pw-mon)
- Mouse DPI/polling (ratbagctl)
- RGB sync (OpenRGB TCP SDK)
- Auto-profile switching (/proc watcher)
- Global hotkey listener (evdev)
- Multi-clip timeline editor
- AppImage/deb packaging

---

65 files, ~5,400 lines — Rust + Tauri 2 + Vue 3 + Pinia + SQLite + warp + PipeWire
