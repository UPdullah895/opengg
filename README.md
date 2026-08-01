<p align="center">
  <img src="frontend/src-tauri/icons/128x128.png" alt="OpenGG" width="120" />
</p>

**v0.1.5** · Open-source Linux gaming hub — unified audio mixer, device/RGB manager, and instant replay. A modular alternative to SteelSeries GG (Sonar + Engine + Moments).

---

## Installation

### Arch Linux / CachyOS (recommended)

```bash
yay -S opengg-bin
```

The `opengg-bin` AUR package installs the pre-built binary and sets up all required udev rules, D-Bus policy, and systemd units automatically.

### Build from source

See [Quick Start](#quick-start) below.

## Modules

- **Audio Hub** — 5-channel PipeWire mixer (Game / Chat / Media / Aux / Mic), per-app routing, parametric EQ, and RNNoise mic denoising.
- **Device & RGB Manager** — Mouse/keyboard configuration via ratbagd, unified RGB control via OpenRGB SDK, and auto-profile switching on game launch.
- **Clipping & Replay** — GPU-accelerated replay buffer via gpu-screen-recorder, global hotkey saves, clip gallery with thumbnails, and FFmpeg-based trim/export.

## Architecture

**UI**: Qt6/QML + cxx-qt (`qt-shell/`) — native performance, zero-latency rendering, full themability.

**Shared Logic**: `opengg-core` crate provides all daemon/PipeWire/SQLite/media access.

**Daemon**: `openggd` — background process managing audio routing, device profiles, and replay buffer via D-Bus.

**Legacy UI**: Tauri + Vue UI archived under `frontend/` — preserved for historical reference and reversibility; no longer built by default. See [`frontend/ARCHIVED.md`](frontend/ARCHIVED.md) for details.

**Process Model**: Each crate (`daemon/`, `core/`, `qt-shell/`) is built independently — there is no shared Cargo workspace. The Qt binary links to `opengg-core` and communicates with `openggd` via D-Bus.

## Requirements

### Build Tools

| Tool | Notes |
|------|-------|
| **Rust + Cargo** (stable) | [rustup.rs](https://rustup.rs) |
| **Node.js 18+** | Only needed to build archived frontend; `sudo pacman -S nodejs` |
| **npm** | Only needed to build archived frontend; bundled with Node.js |

### System Dependencies

| Package | Purpose | Install (Arch / CachyOS) |
|---------|---------|--------------------------|
| **Qt 6 Libraries** | Qt6/QML runtime | `sudo pacman -S qt6-base qt6-declarative qt6-multimedia` |
| **PipeWire** | Audio virtual sinks and routing | `sudo pacman -S pipewire pipewire-pulse` |
| **WirePlumber** | PipeWire session manager | `sudo pacman -S wireplumber` |
| **GStreamer + gst-plugin-qml6** | Unified clip playback | `sudo pacman -S gstreamer gst-plugin-qml6` |
| **gpu-screen-recorder** | Low-latency replay (NVENC / VAAPI) | `yay -S gpu-screen-recorder` |
| **FFmpeg** | Clip trimming and export | `sudo pacman -S ffmpeg` |
| **xdg-desktop-portal** | Screen capture portal | `sudo pacman -S xdg-desktop-portal` |

> **GPU Recording:** gpu-screen-recorder requires NVIDIA (NVENC) or AMD/Intel (VAAPI). For NVIDIA, also install `cuda`. For AMD, ensure `mesa-vdpau` / `libva-mesa-driver` are installed.

## Quick Start

```bash
# 1. Clone
git clone https://github.com/UPdullah895/opengg.git
cd opengg

# 2. First-time setup (udev rules, groups, D-Bus policy, data dirs)
./dev.sh setup

# 3. Run everything (daemon + Qt6 frontend with unified logs)
./dev.sh
```

## Development Commands

| Command | What it does |
|---------|--------------|
| `./dev.sh` | Full stack — daemon + Qt6 frontend |
| `./dev.sh daemon` | Daemon only |
| `./dev.sh ui` | Qt6/QML frontend only (debug build + run) |
| `./dev.sh ui-legacy` | Tauri/Vue frontend only (archived, for reference) |
| `./dev.sh build` | Release build (daemon + Qt6 frontend) |
| `./dev.sh setup` | First-time: udev rules, groups, D-Bus policy, data dirs |
| `make dev` | Same as `./dev.sh` |
| `make ui` | Same as `./dev.sh ui` |
| `make ui-legacy` | Same as `./dev.sh ui-legacy` |
| `make build` | Release build |
| `make clean` | Remove build artifacts |
| `make install` | Install daemon to `~/.local/bin` and Qt binary as `~/.local/bin/opengg` |
| `make lint` | cargo clippy (daemon + qt-shell) + vue-tsc + check-colors.sh |

## Project Structure

```
opengg/
├── dev.sh                  # Unified dev orchestration
├── Makefile                # Convenience wrappers
├── daemon/                 # Rust background daemon (openggd)
│   └── src/
│       ├── main.rs         # Entry, D-Bus, process watcher
│       ├── audio/          # PipeWire, routing, EQ, NR
│       ├── device/         # ratbagd, OpenRGB, game profiles
│       ├── replay/         # gpu-screen-recorder, clips, SQLite
│       ├── config/         # TOML config (~/.config/opengg/)
│       └── ipc/            # D-Bus interface definitions
├── qt-shell/               # Qt6/QML native UI (cxx-qt)
│   ├── src/
│   │   ├── main.rs         # App setup, D-Bus, shortcuts
│   │   └── commands.rs     # QML invokable functions
│   ├── qml/
│   │   ├── App.qml         # Root: pages, theme, navigation
│   │   ├── pages/          # MixerPage, ClipsPage, SettingsPage, etc.
│   │   ├── components/     # Reusable QML elements
│   │   └── Theme.qml       # Design tokens
│   └── build.rs            # QML resource registration
├── core/                   # Shared opengg-core crate
│   └── src/
│       ├── lib.rs          # Public API
│       ├── audio/          # Routing, mixing logic
│       ├── replay/         # Clip metadata, FFmpeg integration
│       └── device/         # Device abstraction
├── frontend/               # ARCHIVED: Tauri + Vue UI
│   ├── ARCHIVED.md         # Archive notes
│   ├── src/
│   │   ├── App.vue         # Root: nav, theme, onboarding
│   │   ├── pages/          # Home, Mixer, Clips, Devices, Settings
│   │   ├── components/     # ClipCard, ChannelStrip, GraphicEQ, …
│   │   ├── stores/         # Pinia: audio, replay, persistence, dsp
│   │   └── locales/        # en.json, ar.json (full RTL)
│   └── src-tauri/
│       └── src/
│           ├── main.rs     # Tauri: tray, shortcuts, file watcher
│           ├── commands.rs # All invoke() handlers
│           └── media_server.rs  # Local warp server for assets
├── packaging/              # udev rules, systemd, D-Bus, polkit
└── opengg-launch.sh        # Launcher: finds and runs the release binary
```

## Extensions

Drop an extension folder into `~/.local/share/opengg/extensions/`. Each folder must contain a `manifest.json`. Enable/disable in **Settings → Extensions** — no restart required.

## Data Locations

| Data | Path |
|------|------|
| Daemon config | `~/.config/opengg/daemon.toml` |
| UI settings | `~/.config/opengg/ui-settings.json` |
| Theme | `~/.config/opengg/theme.json` |
| Clip database | `~/.local/share/opengg/clips.db` |
| Default clips dir | `~/Videos/OpenGG/` |
| Thumbnails | `~/.local/share/opengg/thumbnails/` |
| Crash log | `~/.local/share/opengg/opengg_crash.log` |

## Troubleshooting

### "Clip playback stalls"

The unified GStreamer pipeline (qml6glsink) requires `gst-plugin-qml6` to be installed:

```bash
sudo pacman -S gst-plugin-qml6
```

This is a runtime dependency only — not needed at build time.

### PipeWire permission issues

If audio routing fails or virtual sinks don't appear:

1. Confirm your user is in the `audio` group:
   ```bash
   groups | grep audio
   # If missing:
   sudo usermod -aG audio $USER && newgrp audio
   ```
2. Verify PipeWire is running: `systemctl --user status pipewire`
3. If WirePlumber is not managing sessions, start it:
   ```bash
   systemctl --user enable --now wireplumber
   ```
4. If virtual sinks were corrupted, use **Settings → Danger Zone → Remove Virtual Audio** to reset routing, then relaunch OpenGG.

### gpu-screen-recorder not found

```bash
yay -S gpu-screen-recorder
which gpu-screen-recorder   # should print a path
```

For VAAPI (AMD / Intel), also verify: `vainfo`

## Security Model

- **Zero sudo at runtime** — daemon runs as an unprivileged user
- **Group-based access**: `audio` (PipeWire), `input` (hotkeys/devices), `video` (GPU recording)
- **polkit** for one-time privileged setup (udev rules, group membership)
- **D-Bus auto-activation** — daemon starts on demand, no manual launch needed

## License

MIT
