# OpenGG: a critical review

*October 2026, at v0.2.1 plus the Kairos-integration branch. This is a whole-program review: what OpenGG is for, how it goes about it, and what it lacks. Each finding cites the code it is based on. The ranked plan at the end drives the next implementation phases.*

## 1. Purpose, and the verdict

OpenGG promises Linux gamers what SteelSeries GG gives Windows users, in three modules:

- **Sonar:** a per-app audio mixer with EQ, ChatMix and mic processing.
- **Engine:** device settings, RGB and per-game profiles.
- **Moments:** instant replay and a clip editor.

**Verdict:** OpenGG is a well-crafted UI over a fragile and partly unimplemented engine.

- **The UI is good.** The Qt6/QML shell is polished, themed, translated and screenshot-tested. The Moments path is now the strongest part, after the Kairos work.
- **Audio** is the module users judge first. It works on a static desktop but does not survive normal life: unplugging a headset, apps launched later, or a PipeWire restart.
- **Three headline Engine features don't exist yet:** RGB, per-game profiles and the extension store. Mic processing is shown as working controls but does nothing.
- **The only shipped package** (AUR) installs a daemon that cannot start. Its udev rules make whole vendors' USB devices world-writable.

The gap between how the program looks and what it does is the main risk. A user who sees mic noise suppression switches, an RGB page or a profiles list will assume they work.

## 2. Critical findings (fix before anything else)

| # | Finding | Evidence | Real-world effect |
|---|---|---|---|
| C1 | The packaged daemon cannot start | `release.yml` installs `/usr/bin/openggd`, but `packaging/openggd.service` and `org.opengg.Daemon.service` say `Exec=%h/.local/bin/openggd`. dbus-daemon does not expand `%h` | `yay -S opengg-bin` gives a UI with no mixer, no replay and no hotkeys |
| C2 | The systemd unit fails on a fresh account | `ReadWritePaths=%h/.config/opengg %h/.local/share/opengg %h/Videos/OpenGG`: if any path is missing, the unit fails with 226/NAMESPACE | The daemon won't start until `dev.sh setup` has created the directories |
| C3 | The udev rules make whole vendors world-writable | `packaging/99-opengg.rules:14-26` sets `MODE="0666"` on all SteelSeries, Corsair, Razer, Logitech, HyperX, Glorious and Wooting USB devices; raw HID is opened to group `input` | Any local process can read raw keystrokes, detach drivers or flash firmware. OpenGG never opens hidraw or usbfs itself |
| C4 | Extensions run with no consent the daemon enforces | `daemon/src/extensions/mod.rs:362-364` enables an extension unless it is marked `false`. `start_all` spawns every discovered executable unsandboxed. Consent lives only in the UI's settings file | Dropping a folder into `~/.local/share/opengg/extensions/` gives persistent code execution at login, inheriting the daemon's `input` group, which is equivalent to a keylogger |
| C5 | Audio channels are pinned to the startup device | `daemon/src/audio/sinks.rs:788-814` hard-links each channel's monitor to the default sink once, with `pw-link` | Unplug or replug a headset, or change the default output, and every OpenGG channel goes silent until restart |
| C6 | Per-app routing rules apply once, at UI startup | `apply_saved_app_rules` runs only from `AudioController::hydrate` (`qt-shell/src/audio.rs:209-219`); a never-expiring "already routed" cache (`core/src/audio.rs:1426,1647`) | "Discord → Chat" stops working for anything launched after OpenGG, which is most things |
| C7 | A PipeWire restart silently removes all virtual audio | Sinks are `pactl` modules inside `pipewire-pulse`, with no graph watcher; `AudioHub::new` succeeds even if every sink failed (`sinks.rs:163-172`) | After a WirePlumber restart or recovery, the mixer shows channels that no longer exist |
| C8 | Crash diagnostics don't exist | No panic hook anywhere. `crash_log_dir()` is `CARGO_MANIFEST_DIR/../Logs`, a path on the build machine (`core/src/system.rs:15`). The README's `opengg_crash.log` is never written | Bug reports arrive with no logs, and "Open crash logs folder" opens nothing |

(The branch already fixed several first-order bugs: trims that dropped tracks, game names cut short, UI-thread saves, settings loss, dead hotkeys. See `docs/KAIROS_INTEGRATION.md`.)

## 3. Module by module

### 3.1 Sonar (audio)

**How it works.** It is built from command-line tools: `pactl load-module module-null-sink` for each channel, then `pw-link` to wire each one to an output, `pactl move-sink-input` to route apps, and `pw-dump`/`pactl` text parsing to read state back.

- **Ownership is split** in a way the code calls a fallback: sinks, volume and mute belong to the daemon, while rules, devices, EQ, VU and ear-blast protection run in the UI process.
- **There are two implementations of the same features**, one in `core/src/audio.rs` and one in the daemon. They diverge, for example in the fallback's sinks having no Mic channel.

**What goes wrong:**

- **Brittle device handling (C5, C7).** Nothing listens to the PipeWire graph. There is no `pactl subscribe` and no libpipewire registry, so every hot-plug or restart leaves stale state.
- **The virtual mic is pinned to the original microphone** with `source_dont_move=true` (`sinks.rs:617-620`). Changing or unplugging the mic silences it permanently.
- **Lost fader values.** The volume debounce drops the trailing value (`sinks.rs:262-272`), so the last position of a drag is often lost. One OS thread per mouse move races the others. The 2 s poll then snaps the fader back.
- **Locale-dependent parsing.** Volume and mute are read by parsing *localised* `pactl` text (`sinks.rs:343-378`, `core/src/audio.rs:88-118,1359`). Under a German or Arabic locale the read-back is wrong.
- **Mic processing is fake.** `applyNoiseGate/Compressor/NoiseReduction` are empty (`qt-shell/src/eq.rs:468-470`), and `daemon/src/audio/effects.rs` asks PipeWire for an `rnnoise` echo-cancel method that does not exist.
- **The EQ DSP itself is fine** (PipeWire builtin biquads, so no extra latency), but its plumbing is not:
  - It runs on the UI thread: a `pw-dump` plus ten `pw-cli` calls per slider move.
  - It races its own node creation, so presets often don't apply.
  - It is not persisted.
  - It is orphaned if the UI crashes, and can double the audio.
- **Ear-blast protection only runs while the Mixer page is visible**, which is exactly when you are *not* in a game. Its threshold practically never triggers on real material.
- **VU meters** spawn six `pw-cat --latency 256` streams. That can drag the whole graph to a 256-sample quantum, raising CPU use and xrun risk.

**Direction.** The daemon should own the audio graph through libpipewire (pipewire-rs) and reconcile a declared state continuously:

- per-channel loopback or filter-chain nodes, with links WirePlumber manages to follow the default device or a chosen one;
- rules applied to streams as they appear;
- EQ, limiter and mic chain always in the graph (flat means bypass);
- a typed, versioned D-Bus API (`org.opengg.Daemon.Audio1`) with change signals instead of 2 s polling.

The UI then becomes presentation only, and the core pactl fallback is deleted.

### 3.2 Engine (devices, RGB, profiles)

- **Devices are a thin, decent client over ratbagd (mice) and `headsetcontrol` (headsets).** ratbagd's D-Bus quirks are handled correctly. Wired, wireless and Bluetooth detection reads real sysfs topology. Absent services degrade to empty lists rather than crashing.
  - **No guidance:** the UI never tells the user to install them, and the AUR package doesn't list them.
  - **Blocking calls:** headset calls run synchronous `Command::output()` with no timeout inside async D-Bus handlers (`device/dbus.rs:200-288`). A hung `headsetcontrol` stalls the daemon's other interfaces.
  - **Polling:** the Devices page polls every 3 s. Every poll forks `headsetcontrol` on machines without a headset (negative results are never cached) and re-reads overrides from disk.
- **RGB does not exist.** `SetRgb` returns NotSupported (`device/dbus.rs:299-306`). The OpenRGB client is dead code with an invalid `UPDATEMODE` payload and no controller-data parser. Keyboards are not even a device type.
- **Per-game profiles do not exist.** The process watcher logs and does nothing (`daemon/src/main.rs:92-102`), and `SetProfile` returns NotSupported. Its exe-substring matching would also miss most Proton games (the exe is `wine64-preloader`). A better detector already exists in `core/src/foreground.rs` (Steam app id, focused window).
- **Battery levels are polled but never become a low-battery notice.**

### 3.3 Moments (replay, clips, editor, share)

**Now the strongest module.** It gained:
- reliable saves, with a save gate, finished-file detection and disk checks;
- correct game names on X11 and Wayland, per-game folders and a size limit;
- a supervised recorder and screenshots;
- a non-destructive edit engine and platform-fitted sharing.

**Still missing:**
- **Multi-part timeline.** The editor UI exposes one trim window and export effects, but none of the engine's split, delete or per-part speed.
- **Full recording with pause.** "toggleRecording" actually toggles the replay buffer.
- **Screenshots in the library.**
- **Live status:** buffered seconds, memory and encoder.
- **"Only while gaming".**
- **A real end-to-end test against gpu-screen-recorder.**

### 3.4 Extensions and security

**Extensions:**
- **Permissions are cosmetic.** The permission tiers map to a whitelist that existed only in the removed Tauri/Vue shell, and the Qt shell cannot load extension UI.
- **Can't turn one off.** A running extension without a consent record can only be disabled by granting consent first (`ExtensionsPanel.qml:39-46`).
- **No integrity checks.** The registry has no hashes or signatures, and the PKGBUILD uses `sha256sums=('SKIP' ...)`.

**System access:**
- **Hotkeys need the `input` group**, which is keylogger-equivalent for the whole process tree, and the extensions inherit it. The XDG GlobalShortcuts portal (KDE and GNOME 48+) is never tried, and keyboards plugged in after start are never watched.
- **The Flatpak manifest** allows `flatpak-spawn --host`, a full sandbox escape, yet still can't reach any devices.

### 3.5 Delivery: build, ship, install, test

- **Arch only.** The AUR is the only channel. The Flatpak is an unbuildable stub with 8 TODOs. There are no deb, rpm or AppImage packages, and nothing for Steam Deck, Bazzite or gamescope, which is where much of the Linux gaming audience is.
- **Manual root steps at install:**
  - `usermod` for four groups (`audio` is discouraged on logind systems), a udev reload and a relogin.
  - A polkit policy that points at a helper which doesn't exist.
- **Dependency guidance is wrong or inert:**
  - The "Recheck" button can never succeed, because `is_available()` caches forever (`core/src/subprocess.rs:22-43`).
  - Debian and Fedora users are told to install gpu-screen-recorder as a Flatpak, which the `$PATH` check can never find.
- **First run** is a UI tour. It never checks that the daemon is reachable or the groups and encoder are in place.
- **CI doesn't cover core.** Its 197 tests never run in CI (no core job; `make test` doesn't exist), and the UI is only built on Arch. Releases are not gated on tests and are unsigned.
- **Three independent crates with three lockfiles** have drifted: 65 packages resolve to different versions (tokio, serde, libc, toml…). `subprocess.rs` is duplicated, and every dependency is built three times. A Cargo workspace would fix all three.
- **The IPC contract is strings.** The UI calls `call_method("SomeName", …)` and the daemon doesn't share types with core, so a renamed method compiles cleanly and fails at runtime.
- **Docs describe a program that no longer exists.** `ARCHITECTURE.md` and `EXTENSION_DEV.md` still describe Tauri and Vue. `AGENTS.md` cites a `CLAUDE.md` that isn't in the repo. A stray root `package-lock.json` is left over.
- **Accessibility:** zero `Accessible.*` properties in 68 QML files, and 178 `Rectangle`+`MouseArea` "buttons" that keyboards and screen readers cannot reach.
- **i18n:** about 43 hard-coded English strings remain, and nothing checks for them.
- **No update notice** and no daemon-vs-UI version check.

## 4. What it lacks to fulfil its purpose (ranked by user value)

1. **Audio that survives real life:** following the default or chosen device across hot-plug, rules for apps launched later, recovery after PipeWire restarts.
2. **An install that works anywhere:** a correct AUR package, a Flatpak on Flathub, an AppImage or rpm/deb, and a Steam Deck / gamescope story.
3. **Mic processing:** RNNoise or DeepFilterNet noise suppression, gate, compressor, mic EQ and sidetone, run by the daemon.
4. **Per-game profiles** that switch audio, EQ, DPI and RGB, then restore them on exit.
5. **RGB through OpenRGB** (sync all, per zone), and keyboards as devices.
6. **A first-run system check** with one-click fixes, and diagnostics you can copy into a bug report.
7. **Persistent EQ presets** per channel and per game; ChatMix as a real balance stage, including hardware ChatMix dials.
8. **A multi-part editor timeline** and full recording on the new engine.
9. **Battery and low-battery notifications;** DPI-stage editing; macros.
10. **Spatial audio** (an HRTF convolver), a streamer mix, and a limiter instead of ear-blast ducking.
11. **Accessibility basics and a translation pipeline.**
12. **A signed extension store** with a real sandbox, or no store until there is one.

## 5. The plan

Ordered by risk removed per unit of work. Phase 1 is safety and honesty, Phase 2 is the audio foundation, Phase 3 is reach, Phase 4 is the missing headline features.

### Status (2026-10-22)

- **Phase 1: done.** Packaging, device permissions, extension consent, diagnostics, Recheck, the disabled mic cards, and the core CI job with `make test`.
- **Phase 2: mostly done.**
  - Done:
    - the event-driven reconciler (`daemon/src/audio/reconcile.rs`);
    - JSON parsing and the C locale everywhere;
    - timeouts on every helper command;
    - non-blocking D-Bus handlers;
    - the trailing volume write;
    - the EQ in the daemon (in the graph, saved);
    - mic processing (RNNoise, gate, compressor) where the plugins exist.
  - Open: replacing Ear Blast's volume ducking with a real limiter (swh `fast_lookahead_limiter` is the obvious engine), and a move from `pactl subscribe` to pipewire-rs.
- **Phases 3–4: not started.**

**Phase 1: make what ships correct and safe** (days)
- Packaging: the packaged unit and D-Bus service run `/usr/bin/openggd`; `ReadWritePaths` uses `-` prefixes; release CI verifies the units.
- Remove the world-writable udev rules and the dead polkit policy; cut the group list to `input`.
- Extensions: disabled by default, the daemon enforces consent, disabling always works, the legacy bare-executable path is removed.
- Crash diagnostics: a panic hook and log file under `$XDG_STATE_HOME/opengg`; `crash_log_dir` fixed; a "copy diagnostics" action.
- Make "Recheck" work; correct the install advice.
- Honesty: hide controls that do nothing (mic processing) until they work.
- CI: a core job and `make test`.

**Phase 2: the audio foundation** (weeks)
- Have the daemon reconcile the graph from events (`pactl subscribe` as a first step, then pipewire-rs): re-link channels on device changes, recreate sinks after restarts, apply routing rules to new streams.
- Use a locale-independent `pactl -f json` with timeouts.
- Coalesce volume changes with a trailing flush.
- Move EQ into the daemon, always in-graph and persisted.
- Mic chain with RNNoise; a limiter instead of ear-blast ducking.

**Phase 3: reach**
- A Cargo workspace, then a shared typed IPC crate with a D-Bus contract test.
- Flatpak (with the GlobalShortcuts portal), then AppImage or rpm/deb.
- First-run system check; update notice.

**Phase 4: headline features**
- Per-game profiles on `foreground.rs`.
- RGB through OpenRGB.
- Multi-part timeline UI; full recording.
- Battery notices.
- Accessibility pass.

**Done:** the zbus 6 migration (PR #64). The `deny.toml` ignores for the zbus helper crates are gone.
