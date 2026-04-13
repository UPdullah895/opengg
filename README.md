# OpenGG

**Open-source Linux gaming hub** — unified audio mixer, device/RGB manager, and instant replay.

A modular alternative to SteelSeries GG (Sonar + Engine + Moments).

## Architecture

| Component | Tech | Purpose |
|-----------|------|---------|
| **openggd** | Rust + tokio | Background daemon managing audio, devices, replay |
| **Frontend** | Tauri + Vue 3 + TypeScript | Desktop UI with dark gaming aesthetic |
| **IPC** | D-Bus (zbus) + Unix socket | Reliable, standard Linux inter-process communication |
| **Audio** | PipeWire (native bindings) | Virtual sinks, per-app routing, parametric EQ |
| **Devices** | ratbagd (D-Bus) + OpenRGB SDK | Mouse/keyboard config, unified RGB |
| **Replay** | gpu-screen-recorder + FFmpeg | Low-overhead recording, clip trimming |

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full blueprint.

## Modules

### Module 1: Audio Hub
5-channel mixer (Game/Chat/Media/Aux/Mic) with PipeWire virtual sinks, per-app routing, parametric EQ via WirePlumber filter chains, and RNNoise mic denoising.

### Module 2: Device & RGB Manager
Mouse/keyboard configuration via ratbagd, unified RGB control via OpenRGB SDK, and auto-profile switching when games launch.

### Module 3: Clipping & Replay
GPU-accelerated replay buffer via gpu-screen-recorder, global hotkey saves, clip gallery with thumbnails, and FFmpeg-based trim/export.

## Quick Start

```bash
# System dependencies (Arch/CachyOS)
sudo pacman -S pipewire pipewire-pulse wireplumber rust nodejs npm
yay -S gpu-screen-recorder

# Clone
git clone https://github.com/UPdullah895/opengg.git
cd opengg

# First-time setup (installs npm deps, creates data dirs)
./dev.sh setup

# Run everything in one command
./dev.sh
```

### Development Commands

| Command | What it does |
|---------|--------------|
| `./dev.sh` | Full stack — daemon + Tauri frontend with unified logs |
| `./dev.sh daemon` | Daemon only (Rust backend) |
| `./dev.sh ui` | Frontend only (Tauri + Vue) |
| `./dev.sh build` | Release build |
| `./dev.sh setup` | First-time dependency install |
| `make dev` | Same as `./dev.sh` |
| `make build` | Release build |
| `make clean` | Remove all build artifacts |
| `make install` | Install daemon binary to `~/.local/bin` |

## Project Structure

```
opengg/
├── dev.sh                  # ← Single command: ./dev.sh
├── Makefile                # make dev / make build / make clean
├── daemon/                 # Rust background daemon
│   ├── src/
│   │   ├── main.rs         # Entry point, module loading
│   │   ├── audio/          # Module 1: Audio Hub
│   │   │   ├── mod.rs
│   │   │   ├── hub.rs      # Top-level facade
│   │   │   ├── sinks.rs    # PipeWire virtual sink creation
│   │   │   └── routing.rs  # App-to-sink routing
│   │   ├── config/         # TOML config management
│   │   │   └── mod.rs
│   │   └── ipc/            # D-Bus service layer
│   │       ├── mod.rs
│   │       └── audio_iface.rs
│   ├── scripts/
│   │   ├── setup.sh        # First-run system setup
│   │   └── wireplumber/    # WirePlumber Lua scripts
│   └── Cargo.toml
├── frontend/               # Tauri + Vue 3 desktop app (TODO)
├── packaging/              # System integration files
│   ├── 99-opengg.rules     # udev rules
│   ├── openggd.service     # systemd user unit
│   ├── org.opengg.Daemon.service  # D-Bus activation
│   └── org.opengg.setup.policy    # polkit policy
└── docs/
    └── ARCHITECTURE.md     # Full design document
```

## Security Model

- **Zero sudo** at runtime — daemon runs as unprivileged user
- **Group-based access**: `audio` (PipeWire), `input` (hotkeys/devices), `video` (GPU recording)
- **polkit** for one-time privileged setup (udev rules, group membership)
- **systemd hardening**: `NoNewPrivileges`, `ProtectSystem=strict`, `PrivateTmp`
- **D-Bus auto-activation**: daemon starts on demand, no manual launch needed

## Extensions & Plugins

OpenGG supports optional feature extensions and third-party plugins.

### Built-in Extensions

| Extension | Toggle in | Description |
|-----------|-----------|-------------|
| **Overlays System** | Settings → Extensions | Burn text, images, and GIFs into clips at export time |
| **TikTok Vertical Export** | Settings → Extensions | Crop and export clips in 9:16 portrait mode for TikTok / Reels |

### Third-party Plugins

Drop a plugin folder into `~/.local/share/opengg/plugins/`. Each plugin must contain a `manifest.json`:

```json
{
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "description": "What this plugin does"
}
```

OpenGG scans this directory on startup and whenever Settings → Extensions is opened. No restart is required.

### Virtual Microphone Sink (PipeWire)

OpenGG creates a virtual PipeWire sink named `opengg_mic` for the Mic channel so that noise-reduction and EQ filters remain persistent even when the physical hardware source changes. PipeWire and WirePlumber are required:

```bash
sudo pacman -S pipewire wireplumber
```

The virtual sink is created automatically on first launch and appears in the Mixer as **OpenGG Microphone**.

## License

MIT
