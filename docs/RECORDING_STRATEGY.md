# Recording strategy: software capture, maintenance, Linux → Windows, and what to build on

*Research note, 2026-10-22. It answers four questions: the risks of software screen recording compared with a hardware recorder; the long-term benefits and maintenance cost; whether "Linux first, Windows later" causes problems; and which existing projects to reuse, rebuild or learn from.*

## Short answers

1. **Software capture is the right foundation.** Every clipping product in this category (ShadowPlay, Medal, Outplayed, SteelSeries Moments, OBS's replay buffer) does it this way, because only software capture can deliver:
   - an always-on replay buffer with no extra hardware;
   - one audio track per mixer channel;
   - knowing which game is running.

   A capture card is worth supporting later as an *extra input*, for consoles or two-PC setups. It is not an alternative.
2. **Yes, it needs continuous maintenance.** The components it depends on change under it: GPU drivers, compositor capture protocols, desktop portals, encoder SDKs, Windows capture APIs and anti-cheat. Expect a steady stream of small fixes rather than a finished component. The answer is to make that cheap:
   - one capture interface;
   - several interchangeable backends;
   - an automated "record 5 seconds and check the file" test.
3. **Linux first is fine, but Windows is not a recompile.** The hard part of the Windows port is not video capture. It is the **audio model**. OpenGG's mixer is built on virtual sinks, and Windows has no equivalent without a **signed kernel audio driver**, or a different design based on per-process loopback capture. The other gaps can be planned for: D-Bus IPC, gpu-screen-recorder being Linux-only, ratbagd being Linux-only, code signing, and anti-cheat.
4. **Build on GStreamer; keep gpu-screen-recorder for now.** GStreamer is LGPL and cross-platform, has Rust bindings, and qt-shell already depends on it. It has capture elements for PipeWire on Linux and DXGI/WGC on Windows, per-process audio loopback on Windows, and all the hardware encoders. Keep gpu-screen-recorder (GSR) as the Linux backend until a GStreamer backend matches it. Use GPL projects (OBS, Sunshine, GSR) as references to study, not as code to copy.

---

## 1. Software capture versus a hardware recorder

### What each one is

- **Software capture.** The PC reads its own frames (from the compositor or GPU), encodes them on the GPU's video engine (NVENC, VCN/AMF, Quick Sync, VA-API) and writes them to disk. OpenGG does this today through GSR.
- **Hardware capture.** The GPU's output goes over HDMI into a capture card, either inside the same PC or in a second one. The card hands video to software that still has to encode and store it.

So "hardware" does not remove software. It moves capture from the GPU's memory to a cable.

### Risks of software capture, and how to contain them

| Risk | What actually happens | Mitigation |
|---|---|---|
| **Performance cost** | GPU encoders are separate silicon, but capture and colour conversion still use some GPU time and VRAM, and a CPU (software) encode is expensive. Costs show up as a few FPS or frame-time spikes on weak GPUs. | Keep frames on the GPU from capture to encoder (dmabuf on Linux, D3D11 textures on Windows), and never fall back to x264 silently. Show the chosen encoder in the UI and diagnostics. |
| **Encoder session limits** | NVIDIA caps concurrent NVENC sessions on GeForce cards. The cap was raised from 3 to 5 ([Tom's Hardware](https://tomshardware.com/news/nvidia-increases-concurrent-nvenc-sessions-on-consumer-gpus), [Guru3D](https://guru3d.com/story/nvidia-removes-encoding-limitations-on-consumer-gpus-allowing-up-to-5-simultaneous-streams)). Replay, recording, Discord streaming and OBS together can hit it, and the failure looks like "recording won't start". | Use one encode for replay and recording (save from the same stream) rather than two sessions. Detect the error and say what it means. |
| **Driver and compositor breakage** | A driver or desktop update changes capture behaviour: black frames, colour shifts, crashes. This is the main maintenance cost (section 2). | Keep several backends that can be switched, and run a smoke test after updates. |
| **Wayland permissions** | Capture goes through the xdg-desktop-portal ScreenCast dialog. Restore tokens are **single-use**: an app that doesn't save the new token after each `Start` asks the user again every time ([portal behaviour summary](https://screenconnect.product.connectwise.com/communities/1/topics/5169-bug-linux-client-repeatedly-opens-gnome-remote-desktop-permission-dialogs-and-ignores-restore-token), [KDE bug 517454](https://bugs.kde.org/show_bug.cgi?id=517454)). | Use `persist_mode = 2` and save the newest token every time. |
| **Privileged capture** | GSR's KMS path (monitor capture without the portal) needs `gsr-kms-server` with elevated capabilities ([GSR AppData](https://git.dec05eba.com/gpu-screen-recorder-gtk/tree/com.dec05eba.gpu_screen_recorder.appdata.xml)). That is a setuid-style attack surface, and some distributions dislike it. | Prefer the portal. Use KMS only as an opt-in for users who want no prompt. |
| **Anti-cheat (Windows)** | Capture that *injects into the game* (OBS-style "game capture" hooks) can be blocked or flagged by Vanguard, EasyAntiCheat and others ([OBS forum on XIGNCODE](https://obsproject.com/forum/threads/obs-0-62b-test-1.13284/page-2), [troubleshooting summary](https://blog.eklipse.gg/streaming-tips/fix-game-capture-not-working-obs.html)). | **Never inject.** Use Windows.Graphics.Capture or Desktop Duplication, which read from the compositor. The cost is no exclusive-fullscreen capture on some titles. |
| **Protected content** | DRM video (Netflix and similar) records as black. | Expected and correct behaviour. Document it. |
| **HDR, VRR, multiple and mixed-DPI monitors** | Each one is a source of wrong colours or wrong sizes, and HDR to SDR tone-mapping in particular. | Treat each as a feature with its own test, not an afterthought. |
| **Disk and write load** | Constant writing of a replay buffer wears SSDs and fills disks. | Keep the buffer in RAM (GSR already does) and write only on save. The storage limit and save checks are already in place. |

### Risks of a hardware recorder

- **Cost and setup:** a card, a cable, sometimes a second PC.
- **HDCP:** some sources refuse capture.
- **No per-application audio:** the card gets one mixed stereo signal, so per-channel tracks are impossible.
- **No game knowledge:** no automatic naming, folders or per-game profiles.
- **Limited by the card:** a 4K/HDR/high-refresh card is expensive, and many cheap ones cap at 1080p60.
- **Still needs the same software:** an encoder, a replay buffer and a UI.

Its genuine advantages are no load on the gaming PC (with a second PC) and immunity to anti-cheat and driver capture problems.

### Verdict

Software capture is what the product is for: instant replay with mixer-aware tracks and game knowledge. Add UVC/V4L2 capture-card input later as a *source* option (a GStreamer `v4l2src` / `mfvideosrc` pipeline) for console players and two-PC setups. It costs little once the capture interface below exists.

---

## 2. Long-term benefits and maintenance

### Benefits of owning the capture path

- **Tracks that match the mixer.** OpenGG already records each mixer channel (`OpenGG_<ch>.monitor`) as its own track. That is the product's distinguishing feature, and only software capture can do it.
- **One encode for everything.** The replay buffer, full recording and the share/export pipeline can all come from one stream.
- **Game knowledge.** Naming, folders and future per-game profiles depend on knowing the focused application.
- **Room to grow.** Markers, highlights, smart save, webcam overlay and capture-card input all extend the same pipeline.

### Why it never stops needing work

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

### Recommended foundation: GStreamer (LGPL), from Rust

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

- **Hardware recording as the main path** (section 1).
- **Embedding libobs:** it would force a licence change and bring a very large dependency.
- **Driving a separate OBS through obs-websocket:** it is clumsy for users and gives OpenGG no control over tracks.
- **Writing raw capture plus encoder code from scratch** per platform per GPU vendor. GStreamer and FFmpeg already carry that maintenance.

---

## 5. Suggested plan

1. **Move recording into the daemon** behind `CaptureBackend`, with GSR as the first backend. The replay buffer keeps running with the window closed, and one supervisor owns it. Fix `kill_kms_server` while there.
2. **Add the contract test and a "Test recording" button.** Record, save, check with `ffprobe`, and include the result in Copy Diagnostics.
3. **Write a GStreamer backend for Linux:** portal plus `pipewiresrc`, VA-API/NVENC, per-channel audio tracks, and a ring-buffer replay. Ship it as an alternative and compare it with GSR on real hardware.
4. **Phase 3 IPC work:** a transport-agnostic typed IPC crate, which is a prerequisite for Windows.
5. **Windows backend.** The same GStreamer backend with `d3d11screencapturesrc` and `wasapi2src` process loopback, so per-app tracks need no driver. Mixer v1 on Windows is per-app volume plus tracks.
6. **Decide on a Windows virtual audio driver only after v1 ships.** It would add true per-app routing to devices, but it brings EV signing, attestation and kernel-mode maintenance. Fund it only if users ask for routing, not just tracks.
7. **Capture-card input:** `v4l2src` / `mfvideosrc` as a source option for console and two-PC users.

Steps 1–3 are worth doing even if Windows never happens. They make Linux recording more robust, testable and independent of one external program.
