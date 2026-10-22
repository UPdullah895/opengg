# Recording strategy: our own engine or gpu-screen-recorder, maintenance, Linux → Windows, and what to build on

*Research note, 2026-10-22. "External recorder" here means **gpu-screen-recorder (GSR)**, the separate program OpenGG launches today. The note answers four questions:*

- *the risks of building our own recording engine instead of relying on GSR;*
- *the long-term benefits, and whether our own engine needs constant maintenance;*
- *whether "Linux first, Windows later" causes problems;*
- *which existing projects to reuse, rebuild or learn from.*

## Short answers

1. **Don't replace GSR in one step. Put it behind an interface and build our own engine beside it.** GSR is very good at Linux capture today, and replacing it costs a lot of engineering. But relying on it alone has real limits:
   - it is Linux-only, so a Windows version needs a second recorder regardless;
   - its command-line interface changes between releases;
   - it is mostly one person's project;
   - it is a separate program, so OpenGG can only steer it through command-line flags and signals.

   The right move is to wrap GSR, then add our own engine as a second backend and make it the default only once it matches GSR.
2. **Our own engine needs continuous maintenance, and so does depending on GSR, just of a different kind.** With GSR, we track its releases and flag changes. With our own engine, we track GPU drivers, Wayland capture protocols, desktop portals and encoder kits ourselves. Building on GStreamer, rather than writing raw capture code, moves most of that work onto a large, well-funded project.
3. **Linux first is fine, but Windows is not a recompile.** GSR has no Windows version, so a Windows build needs its own recorder anyway. The hardest part of the port is not video at all. It is the mixer's virtual audio channels, which on Windows need a **signed kernel audio driver** or a different design.
4. **Build our engine on GStreamer.** It is LGPL, has Rust bindings, is already a qt-shell dependency, and covers capture, audio and every hardware encoder on both Linux and Windows. Use GSR, OBS and Sunshine (all GPL) as references to study, not code to copy.

---

## 1. Our own engine versus gpu-screen-recorder

### How OpenGG uses GSR today

- **Who launches it.** The UI process (`core::gsr::start_gsr_replay`) runs `gpu-screen-recorder`. The daemon's `Recorder` is a stub.
- **Interface.** OpenGG drives it with command-line flags (`-w -f -r -c -o -a -bm -ffmpeg-opts …`) and Unix signals: SIGUSR1 saves the replay, SIGINT stops it.
- **Audio.** Each mixer channel is recorded as its own track from `OpenGG_<ch>.monitor`.
- **Finished files.** OpenGG detects a finished save by watching the output file, because GSR has no "saved" reply.
- **Cleanup.** OpenGG runs `pkill -9 gsr-kms-server` on cleanup. That also kills a GSR the user runs on their own.

### What GSR gives us

- **Excellent Linux capture.** It keeps frames on the GPU from capture to encoder (dmabuf), supports VA-API and NVENC, works on X11 and Wayland, and can use either the portal or KMS for capture ([Flathub](https://flathub.org/en-GB/apps/com.dec05eba.gpu_screen_recorder)).
- **The hard features already done:** an in-memory replay buffer, per-application audio through PipeWire (`-a app:…`, [man page](https://man.archlinux.org/man/gpu-screen-recorder.1.en)), HDR, and multiple codecs.
- **Packaged widely:** Arch ([package](https://archlinux.org/packages/extra/x86_64/gpu-screen-recorder/)), Debian ([control file](https://tracker.debian.org/media/packages/g/gpu-screen-recorder/control-5.13.8-2)), Flathub and Fedora COPRs.
- **Free maintenance.** Driver and compositor quirks are fixed upstream at no cost to us.

### The risks of relying on GSR

| Risk | Evidence and impact |
|---|---|
| **Linux-only** | There is no Windows version, so the Windows port needs its own recorder whatever happens. Anything OpenGG builds only *around* GSR does not carry over. |
| **The command-line interface moves** | Flags are renamed between releases. In 5.0.x, `-overlay-replay` became `-restart-replay-on-save` and `-overlapping-replay` became `-overlap-replay`, and `-aa`/`-aai` were deprecated in favour of `-a` ([commit log](https://git.dec05eba.com/gpu-screen-recorder/log/?h=5.0.3), [README history](https://git.dec05eba.com/gpu-screen-recorder/log/README.md?h=5.0.0)). A distribution update can break OpenGG's recording with no change on our side. |
| **Version spread** | Users run whatever their distribution ships: Arch has gone through 5.12–5.15 in recent months, and Debian and Flatpak lag. OpenGG has to support several versions at once. |
| **Bus factor** | GSR is essentially one developer's project. That is fine today but a long-term risk for a core feature. |
| **Little control** | OpenGG talks to GSR through flags and signals. It has no API to ask what is happening, get a "save finished" event, change audio tracks while recording, add markers, or recover partially. Every new recording feature waits on upstream or works around the process boundary. |
| **Privileged helper** | KMS capture relies on `gsr-kms-server` with elevated capabilities ([AppData](https://git.dec05eba.com/gpu-screen-recorder-gtk/tree/com.dec05eba.gpu_screen_recorder.appdata.xml)). OpenGG cannot make that safer itself. |
| **Licence** | GSR is GPL-3.0. Running it as a separate program is fine, but OpenGG (MIT) cannot copy its code, so moving its logic in-house means a clean-room rewrite. |
| **A second program to supervise** | Crashes, restarts after suspend, leftover helper processes. OpenGG already has supervision code for these. |

### The risks of building our own engine

| Risk | Impact | Mitigation |
|---|---|---|
| **Large up-front work** | Capture, encoders, colour conversion, replay buffer, muxing, per-track audio, and HDR on several GPU vendors. GSR has years of fixes. | Build on GStreamer instead of raw APIs. Ship it beside GSR, not instead of it. |
| **We inherit driver and compositor breakage** | Today upstream absorbs it. | Interchangeable backends, a recording contract test, a Test recording button. |
| **Performance regressions** | A naive pipeline copies frames through the CPU and costs FPS. | Insist on keeping frames on the GPU (dmabuf, D3D11 textures). Benchmark against GSR before switching the default. |
| **Encoder limits and quirks** | GeForce cards limit simultaneous NVENC sessions (raised from 3 to 5: [Tom's Hardware](https://tomshardware.com/news/nvidia-increases-concurrent-nvenc-sessions-on-consumer-gpus), [Guru3D](https://guru3d.com/story/nvidia-removes-encoding-limitations-on-consumer-gpus-allowing-up-to-5-simultaneous-streams)). | One encoding for both replay and recording, and a clear error message. |
| **Wayland permission prompts** | Our engine must handle the ScreenCast portal itself. Restore tokens are single-use, and a client that doesn't save the new one asks the user every time ([portal behaviour](https://screenconnect.product.connectwise.com/communities/1/topics/5169-bug-linux-client-repeatedly-opens-gnome-remote-desktop-permission-dialogs-and-ignores-restore-token), [KDE bug 517454](https://bugs.kde.org/show_bug.cgi?id=517454)). | `persist_mode = 2`, and save the newest token every time (`ashpd` makes this simple). |

### What our own engine would gain

- **One recorder for Linux and Windows,** behind one interface.
- **Full control:** a real "saved" event with the file path, markers, changing tracks while recording, partial saves, and webcam or capture-card input. The exact audio tracks the mixer defines, with no translation into another program's flags.
- **No command-line interface churn** and no version spread to support.
- **Recording in the daemon** with the replay buffer in-process, so it keeps running with the window closed and has one supervisor.

### Verdict

Keep GSR for now; it is the best Linux recorder available. Stop depending on its interface directly:

1. **Put it behind an interface.** Move recording into the daemon behind `CaptureBackend`, with GSR as the first backend.
2. **Detect versions.** Read `gpu-screen-recorder --version` (OpenGG already does, for diagnostics) and keep one place that maps our options to that version's flags.
3. **Build our own backend beside it.** A GStreamer backend first on Linux; make it the default only when it matches GSR on quality, CPU/GPU cost and reliability; then the same backend on Windows.

This order means the Linux product never gets worse during the switch, and the work done for Linux is the work Windows needs.

## 2. Long-term benefits and maintenance of our own engine

### Benefits of owning the capture path

- **Tracks that match the mixer, directly.** Today each mixer channel (`OpenGG_<ch>.monitor`) is passed to GSR as an `-a` flag. Our own engine would build the tracks from the mixer model itself, on Linux and Windows alike.
- **One encode for everything.** The replay buffer, full recording and the share/export pipeline can all come from one stream.
- **Game knowledge.** Naming, folders and future per-game profiles depend on knowing the focused application.
- **Room to grow without waiting on upstream.** Markers, highlights, smart save, webcam overlay and capture-card input all extend the same pipeline.
- **One less moving dependency.** No GSR flag churn, no spread of GSR versions across distributions.

### Why it never stops needing work

GSR absorbs these today; an engine of our own has to absorb them itself, which is why it should sit on GStreamer rather than raw APIs.

| Moving part | How often it bites | Example |
|---|---|---|
| GPU drivers (NVIDIA proprietary especially) | Several times a year | NVIDIA changes to KMS and dmabuf. VA-API driver regressions on new AMD and Intel hardware. |
| Wayland capture protocols | Ongoing transition | `ext-image-copy-capture-v1` replaces wlroots screencopy ([Phoronix](https://phoronix.com/news/Wayland-Merges-Screen-Capture)). It is in wlroots ≥ 0.19 / Sway ≥ 1.11 today, and support elsewhere is uneven. |
| Portal implementations (GNOME, KDE, wlroots) | Each desktop release | Restore-token bugs, cursor modes, window-capture support. |
| Encoder SDKs | Yearly | New codecs (AV1), dropped GPU generations, changed presets. |
| Windows capture APIs | With Windows feature updates | Graphics Capture features (borderless capture, cursor control) are Windows 11-only. |
| Anti-cheat | Unpredictable | New titles block some capture methods. |
| FFmpeg / GStreamer | Twice a year | API deprecations, new elements. GStreamer's `d3d12screencapturesrc` still lacked Graphics Capture mode at 1.23 ([forum](https://discourse.gstreamer.org/t/d3d12screencapturesrc-missing-properties/990)). |

### How to keep maintenance cheap

1. **One capture interface, several backends.** The daemon already has `platform::CaptureBackend` (`daemon/src/platform/mod.rs`). Make it real: `GsrBackend` (today), `GstBackend` (Linux portal, then Windows), later `CaptureCardBackend`. When one backend breaks on a given driver or desktop, users switch backends instead of waiting for a release.
2. **Capture belongs in the daemon.** Today the UI process (`core::gsr::start_gsr_replay`) spawns GSR, and the daemon's `Recorder` is a stub. The same reasoning that moved the EQ into the daemon applies:
   - the replay buffer should run with the window closed;
   - one owner should supervise and restart the recorder;
   - one owner should report state over D-Bus.
3. **A contract test per backend.** "Record 5 seconds, save, `ffprobe`: video plus N audio tracks, the right duration, a keyframe at the start." Run it in CI where possible (the isolated PipeWire recipe in `AGENTS.md` covers audio; video can use a virtual output or test source) and on demand in diagnostics ("Test recording").
4. **Ask what the system supports instead of assuming.** List encoders and capture methods at startup, show them in Settings and in Copy Diagnostics, and fall back clearly.
5. **Avoid hacks:** no injection, no patched drivers, no setuid unless opted in.

---

## 3. Linux first, then Windows: where the real problems are

The QML UI, Rust, cxx-qt and GStreamer all port well. The difficulties sit in the parts most tied to Linux.

| Area | Linux today | Windows reality | Size of the problem |
|---|---|---|---|
| **Virtual mixer channels** | `pactl load-module module-null-sink`; apps moved between sinks; monitors recorded as tracks | No user-space virtual audio devices. Creating "Game/Chat/Media" outputs needs a **kernel audio driver**, signed through Microsoft attestation with an **EV certificate**: a hardware token, several hundred dollars a year, and fiddly automation ([Microsoft attestation](https://learn.microsoft.com/en-sg/WINDOWS-HARDWARE/DRIVERS/dashboard/code-signing-attestation), [Oracle's experience](https://blogs.oracle.com/linux/?p=2168), [Virtual Display Driver's signing via SignPath](https://www.mintlify.com/VirtualDrivers/Virtual-Display-Driver/development/driver-signing)). SteelSeries Sonar ships its own driver for this reason. | **Largest** |
| **Per-app audio tracks** | Monitors of the virtual sinks | **WASAPI process loopback** captures one process tree's audio *without a driver* (Windows 10 build 20348+ / Windows 11). GStreamer exposes it as `wasapi2src loopback-mode=include-process-tree loopback-target-pid=…` ([GStreamer docs](https://gstreamer.freedesktop.org/documentation/wasapi2/wasapi2src.html), [MR 2311](https://gitlab.freedesktop.org/gstreamer/gst-plugins-bad/-/merge_requests/2311)). | Solvable without a driver |
| **Moving an app to a channel** | `pactl move-sink-input` | Per-app output selection only through the undocumented `IAudioPolicyConfig` interface (used by EarTrumpet and the Windows settings page), and only to *real* devices unless a driver exists. | Medium to large |
| **Screen capture** | GSR (Linux-only) | DXGI Desktop Duplication or Windows.Graphics.Capture, through GStreamer (`d3d11screencapturesrc`), FFmpeg (`ddagrab`, [FFmpeg wiki](https://trac.ffmpeg.org/wiki/Capture/Desktop)) or the `windows-capture` crate (MIT). | Medium (a new backend) |
| **IPC** | D-Bus (zbus) | No session bus. Needs named pipes or a local socket. | Medium. Phase 3's typed IPC crate should be **transport-agnostic** from the start. |
| **Global hotkeys** | evdev (input group) or portal | `RegisterHotKey` / raw input. Easier than on Linux. | Small |
| **Mice** | ratbagd (Linux-only) | Vendor protocols over HID directly. | Medium (per vendor) |
| **Headsets** | headsetcontrol | headsetcontrol is cross-platform (hidapi). | Small |
| **RGB** | OpenRGB | OpenRGB is cross-platform. | Small |
| **Distribution** | AUR / packages | Code signing for the app (SmartScreen), installer, updater. Recording apps are often flagged by antivirus. | Small to medium, mostly cost and process |
| **Anti-cheat** | Rarely relevant | Must avoid injection (section 1). | Small if designed in |

**Conclusion.** Linux first is a sound order, provided the Linux work does not bake Linux concepts into the shared layers. Concretely:

- **Model channels by intent.** The mixer's model should be "a channel is a set of apps plus a volume plus a track", with `null-sink` as one *implementation*. On Windows the first version can be **track capture through process loopback, plus per-app volume** through `ISimpleAudioVolume`, with no driver. Add a signed driver later only if the routing feature is worth the cost.
- **Do the IPC change before the port.** Phase 3's typed IPC crate should hide the transport (D-Bus on Linux, named pipe on Windows) before any Windows work starts.
- **Port the recorder through `CaptureBackend`.** The Windows recorder is a new backend behind the same interface, not a fork of `core::gsr`.

---

## 4. What to reuse, rebuild or learn from

OpenGG is MIT-licensed, so licence decides *how* each project can be used:

- **Permissive** (MIT, Apache, BSD): copy and adapt freely.
- **LGPL:** link dynamically or use as a separate library. Changes to the library itself stay LGPL.
- **GPL / AGPL:** no copying or translating code into OpenGG. Running it as a **separate program** is fine (OpenGG already does this with GSR). Reading it to understand the problem is fine. Clean-room reimplementation from behaviour is fine, and is how the Kairos work was done.

### Recommended foundation for our own engine: GStreamer (LGPL), from Rust

Why it fits:

- **Already a dependency.** qt-shell uses GStreamer for clip playback.
- **Rust bindings.** `gstreamer-rs` is MIT/Apache.
- **One pipeline model on both platforms:**
  - Linux capture: `pipewiresrc`, fed by an xdg-desktop-portal session from **`ashpd`** (MIT). `ximagesrc` for X11.
  - Windows capture: `d3d11screencapturesrc` (DXGI, with a Graphics Capture mode on 11). `d3d12screencapturesrc` is maturing.
  - Audio: `pulsesrc` / `pipewiresrc` on Linux; `wasapi2src` with **process loopback** on Windows.
  - Encoders: `va*` (VA-API), `nv*` (NVENC/NVDEC), `qsv*`, `amf*`, `mf*` (Media Foundation).
  - Output: `mp4mux` / `splitmuxsink`.
- **The replay buffer is ordinary code**, not a fork:
  - keep *encoded* packets in a time-bounded ring, so RAM use is bitrate × seconds;
  - on save, copy everything from the last keyframe before the window start into a muxer;
  - write to a part file and rename (the existing FFmpeg rules apply).

  This is the same design as GSR and OBS, reimplemented, not copied.
- **It can grow:** webcam, capture card (`v4l2src`, `mfvideosrc`), markers and overlays are just more elements.

Main risks: GStreamer's Windows capture elements are newer than OBS's, and some features are Windows 11-only. The plugin set must be bundled with the Windows build, which is normal for GStreamer apps.

### Keep: gpu-screen-recorder (GPL-3.0, separate process)

It is excellent on Linux (dmabuf zero-copy, KMS and portal modes, replay buffer) and already integrated. Keep it as `GsrBackend` and keep using it **as a program**; do not fork it into OpenGG. Its limits:

- it is Linux-only;
- its monitor capture via KMS needs a privileged helper ([GSR AppData](https://git.dec05eba.com/gpu-screen-recorder-gtk/tree/com.dec05eba.gpu_screen_recorder.appdata.xml));
- it is a moving external dependency (Arch ships 5.12–5.15: [Arch package](https://archlinux.org/packages/extra/x86_64/gpu-screen-recorder/)).

A small fix found while reviewing: `core::gsr::kill_kms_server` runs `pkill -9 gsr-kms-server`, which also kills a GSR the user is running on their own. It should kill only the helper that OpenGG's own GSR started.

### Reuse: permissive building blocks

| Project | Licence | Use for |
|---|---|---|
| [`gstreamer-rs`](https://gitlab.freedesktop.org/gstreamer/gstreamer-rs) | MIT/Apache (GStreamer is LGPL) | The pipeline itself |
| `ashpd` | MIT | Portal ScreenCast, restore tokens, GlobalShortcuts |
| `pipewire-rs` | MIT | Direct PipeWire access. Also the planned replacement for `pactl subscribe`. |
| [`wl-screenrec`](https://linuxlinks.com/wl-screenrec-wlroots-screen-recording/) | Apache-2.0 | Reference for zero-copy dmabuf to VA-API on wlroots, in Rust |
| [`windows-capture`](https://docs.rs/crate/windows-capture/^1.0.59) | MIT | Windows.Graphics.Capture from Rust, if not using GStreamer's element |
| [`scap`](https://lib.rs/crates/scap) | MIT | Cross-platform capture crate (ScreenCaptureKit / WGC / PipeWire). Still 0.0.x, so treat as a reference, not a dependency. |
| `wasapi` crate | MIT | WASAPI and process loopback, if not using `wasapi2src` |
| NVIDIA Video Codec SDK headers, AMD AMF, Intel VPL | Permissive SDK licences | Direct encoder access if GStreamer/FFmpeg ever falls short |
| FFmpeg (`ddagrab`, `kmsgrab`) | LGPL build | Alternative capture path. Already used for exports. |

### Learn from, don't copy (GPL / AGPL)

- **OBS Studio / libobs (GPL-2.0+).** The reference for replay buffers, encoder handling and Windows capture. Linking libobs would make OpenGG GPL.
- **Sunshine (GPL-3.0, [legal page](https://docs.lizardbyte.dev/projects/sunshine/latest/md_docs_2legal.html)).** The best real-world reference for low-latency capture plus hardware encode on *both* Linux (KMS, wlroots, portal, X11) and Windows (DXGI), with NVENC, AMF, QSV and VA-API.
- **gpu-screen-recorder (GPL-3.0).** Linux zero-copy and replay details.
- **Cap (AGPL) and Kooha (GPL).** UI and flow ideas only.

### Not recommended

- **Embedding libobs:** it would force a licence change and bring a very large dependency.
- **Driving a separate OBS through obs-websocket:** it is clumsy for users and gives OpenGG no control over tracks.
- **Writing raw capture plus encoder code from scratch** per platform per GPU vendor. GStreamer and FFmpeg already carry that maintenance.

---

## 5. Suggested plan

1. **Move recording into the daemon** behind `CaptureBackend`, with GSR as the first backend. The replay buffer keeps running with the window closed, and one supervisor owns it. Put all GSR flags in one version-aware place, and fix `kill_kms_server` while there.
2. **Add the contract test and a "Test recording" button.** Record, save, check with `ffprobe`, and include the result in Copy Diagnostics.
3. **Write a GStreamer backend for Linux:** portal plus `pipewiresrc`, VA-API/NVENC, per-channel audio tracks, and a ring-buffer replay. Ship it as an alternative and compare it with GSR on real hardware.
4. **Phase 3 IPC work:** a transport-agnostic typed IPC crate, which is a prerequisite for Windows.
5. **Windows backend.** The same GStreamer backend with `d3d11screencapturesrc` and `wasapi2src` process loopback, so per-app tracks need no driver. Mixer v1 on Windows is per-app volume plus tracks.
6. **Decide on a Windows virtual audio driver only after v1 ships.** It would add true per-app routing to devices, but it brings EV signing, attestation and kernel-mode maintenance. Fund it only if users ask for routing, not just tracks.
7. **Capture-card input** (optional, later): `v4l2src` / `mfvideosrc` as a source option for console and two-PC users.

Steps 1–3 are worth doing even if Windows never happens. They make Linux recording more robust and testable, and they end the dependence on one external program's command-line interface, while GSR stays available as a fallback.
