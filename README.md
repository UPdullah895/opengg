# OpenGG

**v0.1.1 Beta** · Open-source Linux gaming hub — unified audio mixer, device/RGB manager, and instant replay.

A modular alternative to SteelSeries GG (Sonar + Engine + Moments).

## Modules

### Audio Hub
5-channel PipeWire mixer (Game / Chat / Media / Aux / Mic), per-app routing, parametric EQ, and RNNoise mic denoising.

### Device & RGB Manager
Mouse/keyboard configuration via ratbagd, unified RGB control via OpenRGB SDK, and auto-profile switching on game launch.

### Clipping & Replay
GPU-accelerated replay buffer via gpu-screen-recorder, global hotkey saves, clip gallery with thumbnails, and FFmpeg-based trim/export.

---

## Requirements

### Build Tools

| Tool | Notes |
|------|-------|
| **Rust + Cargo** (stable) | Install via [rustup.rs](https://rustup.rs) |
| **Node.js 18+** | `sudo pacman -S nodejs` |
| **npm** | Bundled with Node.js |
| **Tauri CLI v2** | Installed automatically via `npm install` |

### System Dependencies

| Package | Purpose | Install (Arch / CachyOS) |
|---------|---------|--------------------------|
| **PipeWire** | Audio Hub virtual sinks and routing | `sudo pacman -S pipewire pipewire-pulse` |
| **WirePlumber** | PipeWire session manager | `sudo pacman -S wireplumber` |
| **gpu-screen-recorder** | Low-latency replay buffer (NVENC / VAAPI) | `yay -S gpu-screen-recorder` |
| **FFmpeg** | Clip trimming and export | `sudo pacman -S ffmpeg` |
| **xdg-desktop-portal** | Screen capture portal (Wayland / XWayland) | `sudo pacman -S xdg-desktop-portal` |
| **libwebkit2gtk** | Tauri WebView | `sudo pacman -S webkit2gtk-4.1` |

> **GPU Recording:** gpu-screen-recorder requires NVIDIA (NVENC) or AMD/Intel (VAAPI). For NVIDIA, also install the `cuda` package. For AMD, ensure `mesa-vdpau` / `libva-mesa-driver` are installed.

---

## Quick Start

```bash
# 1. Clone
git clone https://github.com/UPdullah895/opengg.git
cd opengg

# 2. First-time system setup (udev rules, groups, D-Bus policy, data dirs)
./dev.sh setup

# 3. Install npm dependencies
cd frontend && npm install && cd ..

# 4. Run everything (daemon + Tauri frontend with unified logs)
./dev.sh
```

---

## Development Commands

| Command | What it does |
|---------|--------------|
| `./dev.sh` | Full stack — daemon + Tauri frontend with unified logs |
| `./dev.sh daemon` | Daemon only (Rust backend) |
| `./dev.sh ui` | Frontend only (Tauri + Vue hot-reload) |
| `./dev.sh build` | Release build (AppImage / deb / rpm) |
| `./dev.sh setup` | First-time: udev rules, groups, D-Bus policy, data dirs |
| `make dev` | Same as `./dev.sh` |
| `make build` | Release build |
| `make clean` | Remove all build artifacts |
| `make install` | Install daemon binary to `~/.local/bin` |

### Frontend-only (no Tauri shell)

```bash
cd frontend
npm run dev          # Vite hot-reload at http://localhost:1420
npm run build        # vue-tsc + vite build
npx vue-tsc --noEmit # Fast TypeScript type-check
```

### Tauri Rust-only check (fast, no link)

```bash
cd frontend/src-tauri && cargo check
```

---

## Project Structure

```
opengg/
├── dev.sh                  # Unified dev orchestration (run this)
├── Makefile                # Convenience wrappers around dev.sh
├── daemon/                 # Rust background daemon (openggd)
│   └── src/
│       ├── main.rs         # Entry point, D-Bus registration, process watcher
│       ├── audio/          # PipeWire virtual sinks, routing, EQ, noise reduction
│       ├── device/         # ratbagd D-Bus, OpenRGB SDK, game profiles
│       ├── replay/         # gpu-screen-recorder, clip scanning, SQLite
│       ├── config/         # TOML config (~/.config/opengg/daemon.toml)
│       └── ipc/            # D-Bus interface definitions
├── frontend/
│   ├── src/
│   │   ├── App.vue         # Root: nav, theme loading, media port, onboarding
│   │   ├── pages/          # HomePage, MixerPage, ClipsPage, DevicesPage, SettingsPage
│   │   ├── components/     # ClipCard, AdvancedEditor, ChannelStrip, GraphicEQ, …
│   │   ├── stores/         # Pinia: audio.ts, replay.ts, persistence.ts, dsp.ts
│   │   └── locales/        # en.json, ar.json (vue-i18n, full RTL support)
│   └── src-tauri/
│       └── src/
│           ├── main.rs     # Tauri setup, tray, file watcher, shortcuts
│           ├── commands.rs # All invoke() handlers
│           └── media_server.rs  # Local warp server for video/thumbnail assets
├── extensions/             # Built-in optional extensions
│   ├── overlays-system/    # Burn text/images into clips at export
│   └── tiktok-export/      # 9:16 portrait crop/export for TikTok/Reels
├── extension-template/     # Scaffold for third-party extension development
└── packaging/              # udev rules, systemd unit, D-Bus service, polkit policy
```

---

## Extensions

Drop an extension folder into `~/.local/share/opengg/extensions/`. Each folder must contain a `manifest.json`. Enable/disable extensions in **Settings → Extensions** — no restart required.

### Built-in Extensions

| Extension | Description |
|-----------|-------------|
| **Overlays System** | Burn text, images, and GIFs into clips at export time |
| **TikTok Vertical Export** | Crop and export clips in 9:16 portrait mode for TikTok / Reels |

---

## Data Locations

| Data | Path |
|------|------|
| Daemon config | `~/.config/opengg/daemon.toml` |
| UI settings | `~/.config/opengg/ui-settings.json` |
| Theme | `~/.config/opengg/theme.json` |
| Default clips dir | `~/Videos/OpenGG/` |
| Thumbnails | `~/.local/share/opengg/thumbnails/` |
| Crash log | `~/.local/share/opengg/opengg_crash.log` |

---

## Troubleshooting

### "Localhost Connection" error on launch

The Tauri frontend connects to a local warp server for media files. If you see a connection refused error:

1. Ensure no other process is occupying the media port: `ss -tlnp | grep 990`
2. Kill any stale opengg processes: `pkill -9 opengg`
3. Restart: `./dev.sh ui`

In dev mode (`npm run dev` without Tauri), the backend invoke calls will fail silently — this is expected; use `./dev.sh ui` to include the Tauri shell.

### PipeWire permission issues

If audio routing fails or virtual sinks don't appear:

1. Confirm your user is in the `audio` group:
   ```bash
   groups | grep audio
   # If missing:
   sudo usermod -aG audio $USER && newgrp audio
   ```
2. Verify PipeWire is running: `systemctl --user status pipewire`
3. If WirePlumber is not managing PipeWire sessions, start it:
   ```bash
   systemctl --user enable --now wireplumber
   ```
4. If virtual sinks were corrupted, use **Settings → Danger Zone → Remove Virtual Audio** to reset routing, then relaunch OpenGG.

### gpu-screen-recorder not found

Install it and ensure it is on your `$PATH`:
```bash
yay -S gpu-screen-recorder
which gpu-screen-recorder   # should print a path
```

For VAAPI (AMD / Intel), also verify:
```bash
vainfo   # should list your GPU's codec support
```

---

## Security Model

- **Zero sudo at runtime** — daemon runs as an unprivileged user
- **Group-based access**: `audio` (PipeWire), `input` (hotkeys/devices), `video` (GPU recording)
- **polkit** for one-time privileged setup (udev rules, group membership)
- **D-Bus auto-activation** — daemon starts on demand, no manual launch needed

---

## License

MIT
