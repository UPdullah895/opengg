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

## Requirements

### Build Tools

| Tool | Notes |
|------|-------|
| **Rust + Cargo** (stable) | [rustup.rs](https://rustup.rs) |
| **Node.js 18+** | `sudo pacman -S nodejs` |
| **npm** | Bundled with Node.js |
| **Tauri CLI v2** | Installed via `npm install` |

### System Dependencies

| Package | Purpose | Install (Arch / CachyOS) |
|---------|---------|--------------------------|
| **PipeWire** | Audio virtual sinks and routing | `sudo pacman -S pipewire pipewire-pulse` |
| **WirePlumber** | PipeWire session manager | `sudo pacman -S wireplumber` |
| **gpu-screen-recorder** | Low-latency replay (NVENC / VAAPI) | `yay -S gpu-screen-recorder` |
| **FFmpeg** | Clip trimming and export | `sudo pacman -S ffmpeg` |
| **xdg-desktop-portal** | Screen capture portal | `sudo pacman -S xdg-desktop-portal` |
| **libwebkit2gtk** | Tauri WebView | `sudo pacman -S webkit2gtk-4.1` |

> **GPU Recording:** gpu-screen-recorder requires NVIDIA (NVENC) or AMD/Intel (VAAPI). For NVIDIA, also install `cuda`. For AMD, ensure `mesa-vdpau` / `libva-mesa-driver` are installed.

## Quick Start

```bash
# 1. Clone
git clone https://github.com/UPdullah895/opengg.git
cd opengg

# 2. First-time setup (udev rules, groups, D-Bus policy, data dirs)
./dev.sh setup

# 3. Install npm dependencies
cd frontend && npm install && cd ..

# 4. Run everything (daemon + Tauri frontend with unified logs)
./dev.sh
```

## Development Commands

| Command | What it does |
|---------|--------------|
| `./dev.sh` | Full stack — daemon + Tauri frontend |
| `./dev.sh daemon` | Daemon only |
| `./dev.sh ui` | Frontend only (hot-reload) |
| `./dev.sh build` | Release build (deb / rpm) |
| `./dev.sh setup` | First-time: udev rules, groups, D-Bus policy |
| `make dev` | Same as `./dev.sh` |
| `make build` | Release build |
| `make clean` | Remove all build artifacts |
| `make install` | Install daemon binary to `~/.local/bin` |

### Frontend-only (no Tauri shell)

```bash
cd frontend
npm run dev          # Vite at http://localhost:1420
npm run build        # vue-tsc + vite build
npx vue-tsc --noEmit # Fast type-check
```

### Tauri Rust-only check

```bash
cd frontend/src-tauri && cargo check
```

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
├── frontend/
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
├── extension-template/     # Scaffold for third-party extensions
└── packaging/              # udev rules, systemd, D-Bus, polkit
```

## Extensions

Drop an extension folder into `~/.local/share/opengg/extensions/`. Each folder must contain a `manifest.json`. Enable/disable in **Settings → Extensions** — no restart required.

## Data Locations

| Data | Path |
|------|------|
| Daemon config | `~/.config/opengg/daemon.toml` |
| UI settings | `~/.config/opengg/ui-settings.json` |
| Theme | `~/.config/opengg/theme.json` |
| Default clips dir | `~/Videos/OpenGG/` |
| Thumbnails | `~/.local/share/opengg/thumbnails/` |
| Crash log | `~/.local/share/opengg/opengg_crash.log` |

## Troubleshooting

### "Localhost Connection" error on launch

The frontend connects to a local warp server for media files. If connection refused:

1. Check no process occupies the media port: `ss -tlnp | grep 990`
2. Kill stale opengg processes: `pkill -9 opengg`
3. Restart: `./dev.sh ui`

In dev mode (`npm run dev` without Tauri), backend invoke calls fail silently — this is expected; use `./dev.sh ui` for the full Tauri shell.

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
