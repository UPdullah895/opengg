# SteelSeries GG parity review + Montage builder

**Date:** 2026-10-22 · **Status:** Phase 1 implemented (this branch), Phases 2–4 planned

This doc does four things: it summarises how SteelSeries GG's capture and audio
features work, maps them against what OpenGG v0.2.1 already ships, critiques
the "clone GG" idea to find where OpenGG should do better instead of copying,
and gives the build order. Phase 1, the montage builder, is implemented in the
same branch.

---

## 1. How SteelSeries GG works (the parts that matter here)

GG is an Electron shell over three engines: **Engine** (device config and RGB),
**Sonar** (audio) and **Moments** (capture). Only Sonar and Moments are in scope.

### 1.1 Moments: capture → clip → edit → montage → share

| Stage | What GG does |
|---|---|
| **Replay buffer** | Encodes the game in the background (NVENC/AMF/QSV) into a rolling RAM/disk buffer, 15 s to 20 min. A hotkey saves the last *N* seconds as a clip. |
| **Auto-clip** | For supported titles (CS2, Valorant, LoL, Fortnite, Apex …) it reads game events (kills, wins, aces) from game integrations and saves clips with no hotkey press. |
| **Full-session recording** | Optional continuous recording next to the buffer. |
| **Audio tracks** | Records game audio and microphone; with Sonar it can capture per-channel audio, so mic and game can be balanced after the fact. |
| **Library** | Clips are grouped by game, with favourites, search, and thumbnails. |
| **Clip editor** | Trim in/out, per-track audio balance, a frame screenshot, and export at a target size or quality. |
| **Montage** | Pick several clips → arrange them on a timeline → transitions → optional music → render one video. This is the "highlight reel" flow. |
| **Share** | Upload to a SteelSeries share link, or push to Discord / YouTube / social; a *mobile/vertical* export crops the clip to 9:16. |

### 1.2 Sonar: virtual devices + routing + mixing

| Feature | What GG does |
|---|---|
| **Virtual devices** | Five virtual outputs/inputs (*Gaming, Chat, Media, Aux, Microphone*). Apps send audio to one of them, and Sonar mixes them into the real headset. |
| **App routing** | Each app picks its Sonar device, either in Windows' per-app output setting or by dragging it in Sonar. |
| **Per-channel EQ** | A 10-band parametric EQ per channel, with presets (and game-specific presets for footsteps and so on). |
| **ChatMix** | One balance control between Game and Chat, available on the headset dial too. |
| **Spatial audio** | Virtual 7.1 HRTF on the Game channel. |
| **Mic processing** | ClearCast AI noise cancellation, noise gate, compressor, impact-noise reduction, plus mic EQ. |
| **Smart Volume** | Loudness normalisation so a loud media source and quiet chat land at similar levels. |
| **Streamer Mode** | **Two independent mixes**: a *Personal* (monitor) mix for the headset and a *Stream* mix exposed as a separate virtual device for OBS. Each channel has its own volume and mute in each mix. |
| **Per-channel output device** | Any channel can go to a different physical device. |

---

## 2. Gap analysis: OpenGG v0.2.1 vs GG

| GG feature | OpenGG today | Gap |
|---|---|---|
| Replay buffer + hotkey | ✅ gpu-screen-recorder, evdev hotkeys | — |
| Multi-track audio capture | ✅ one stream per source, titled Game/Chat/Mic | — |
| Clip library, favourites, game tags, search | ✅ | — |
| Clip editor: trim, tracks, export to size | ✅ | — |
| **Montage** | ❌ none | **Phase 1, implemented here** |
| **Vertical / mobile export** | ❌ | **Phase 1, folded into the montage formats** |
| Auto-clip on game events | ❌ | Phase 4 (needs per-game integrations) |
| Cloud share links | ❌ | Out of scope on purpose (no first-party service; see §3) |
| 5 virtual channels + app routing | ✅ null-sinks + `move-sink-input`, persisted | — |
| Per-channel EQ, ChatMix | ✅ 10-band EQ, ChatMix | — |
| Mic noise suppression / gate / compressor | ⚠️ `effects.rs` scaffolding (rnnoise / LSP URIs) not wired to the UI | Phase 3 |
| **Streamer Mode (dual mix)** | ❌ one mix only | **Phase 2** |
| Smart Volume (loudness) | ❌ | Phase 3 (an LSP loudness compressor in the same filter chain) |
| Spatial audio | ❌ | Phase 3, optional (needs an HRTF convolver such as `filter-chain` with a SOFA file) |

---

## 3. Critique: where copying GG would be the wrong call

1. **GG's montage throws away your trims.** Clips enter the montage timeline
   untrimmed, so you cut the same moment twice. OpenGG already stores a trim
   window per clip (`clips.trim_state`), so **the montage uses it by
   default.** The editor is where you cut; the montage is where you sequence.
2. **Multi-track audio is a liability in a montage.** Each OpenGG capture has
   N audio streams (Game, Chat, Mic …), and some clips have none.
   `concat`/`acrossfade` need every segment to have the same layout. **Each
   clip's tracks are summed into one stereo bed, and silent clips get
   generated silence**, so a clip without audio never breaks the render.
3. **A flat music bed fights the gameplay.** GG mixes music at a fixed level,
   which either drowns the callouts or vanishes in quiet moments. **OpenGG
   ducks the music with a sidechain compressor keyed on the clip audio**,
   which is what a human editor would do by hand.
4. **Vertical export shouldn't need a second tool.** A 16:9 capture
   letterboxed into 9:16 looks bad. The **"Vertical 9:16" format uses a
   blurred, cropped copy of the frame as the background**, the standard look
   for Shorts and TikTok.
5. **No cloud dependency.** GG's share links route through SteelSeries
   servers. OpenGG writes a local MP4 (`+faststart`, so it streams when
   uploaded anywhere) and offers *Show in folder* / *Copy path*. Uploading
   belongs in an extension, not core.
6. **No timeline UI for v1.** GG's montage has a full drag timeline. For a
   highlight reel the decisions are *which clips*, *what order*, *how they
   join* and *what music*. A reorderable list plus the existing per-clip
   trims covers that without a second NLE to maintain. A timeline can come
   later, but only if users ask for per-clip in/out edits *inside* the
   montage.
7. **Streamer Mode should not double the PipeWire graph blindly.** Sonar
   implements the stream mix as a second set of virtual devices. On PipeWire
   the cheaper equivalent is **one extra `OpenGG_Stream` null-sink fed by a
   per-channel loopback with its own volume**. That adds 5 loopbacks rather
   than 5 more sinks, and it respects the "never restart PipeWire" rule
   because everything is still `pactl load-module` at runtime.

---

## 4. Plan

### Phase 1: Montage builder ✅ (this branch)

- **`core/src/montage.rs`**, split in two halves:
  - `plan(spec, sources) -> MontagePlan` is **pure**: segment resolution
    (explicit window → saved trim → whole clip, clamped), transition clamping
    (≤ half the shortest segment), output sizing (even sides), and the full
    ffmpeg `filter_complex`:
    - per clip: `scale`+`pad` (letterbox) or `split`/`boxblur`/`overlay`
      (blur fill), then `fps`, `format=yuv420p` and `settb=AVTB` so every
      pair is `xfade`-compatible;
    - per clip audio: `amix normalize=0` across tracks / passthrough /
      `anullsrc`, then `apad,atrim` to pin the exact segment length;
    - joins: `concat` (cut) or chained `xfade` + `acrossfade` with
      accumulated offsets (crossfade / dip-to-black);
    - optional reel fade in/out; optional music (`-stream_loop -1`, trimmed
      to length, faded out, optionally `sidechaincompress`-ducked, then
      `alimiter`).
  - `run_plan` runs ffmpeg with `-progress pipe:1` for real progress, a
    cancel flag (kills ffmpeg, removes the partial file), and the stderr tail
    in the error message.
- **`core/src/dialogs.rs`**: `pick_file` (kdialog / zenity) for the music track.
- **`qt-shell/src/montage.rs`**: `MontageController` singleton
  (`probeClips`, `build`, `cancel`, `reset`, `pickMusic`, `defaultOutput`,
  progress qproperties).
- **`qt-shell/qml/components/MontageDialog.qml`**: a reorderable clip list
  with trimmed lengths, transition and length chips, format (Source / 1080p /
  720p / Vertical 9:16), codec, a fade toggle, music path, volume and duck
  toggle, output path, progress, Stop, and a done state.
- **Clips page**: a **Montage** button in the selection bar. Clips open
  oldest-first, since a reel reads in the order things happened.
- **Harness**: a `clips:montage` screenshot target.
- **Tests**: 13 unit tests on the planner and progress parser, plus one end-to-end
  render against real ffmpeg (2-track + silent + differently-sized clips,
  blur fill, crossfade, ducked music, then cancel).

### Phase 2: Streamer Mode (dual mix)

1. Daemon `sinks.rs`: create an `OpenGG_Stream` null-sink. For each channel,
   `module-loopback` from `OpenGG_<ch>.monitor` → `OpenGG_Stream`, with
   `sink_input_properties=media.name=OpenGG stream <ch>`.
2. Track the loopback sink-input IDs per channel. Stream-mix volume and mute
   become `set-sink-input-volume/mute` on those. Personal-mix volume stays on
   the channel sink, exactly as today.
3. D-Bus: `SetStreamVolume(channel, vol)`, `SetStreamMute(channel, bool)`,
   `SetStreamerMode(bool)` (loads or unloads the loopbacks). Persist under
   `mixer.stream` in `ui-settings.json`, read-modify-write like
   `mixer.appRules`.
4. UI: a Personal / Stream toggle in the mixer header. The channel strips
   rebind to the selected mix, and the stream mix shows a "Use
   `OpenGG_Stream.monitor` in OBS" hint.
5. Verification needs a real PipeWire session. It cannot be done headless,
   and AGENTS.md forbids restarting PipeWire, so test with `pw-dump` before
   and after.

### Phase 3: Mic and output processing

Wire the existing `effects.rs` `FilterGraph` to real PipeWire `filter-chain`
modules: rnnoise noise suppression, an LSP gate and compressor on Mic, and an
LSP loudness compressor as "Smart Volume" on Media and Chat. Each is a toggle
in the channel's DSP flyout, and each degrades to hidden when the plugin is
not installed.

### Phase 4: Auto-clip

Event sources per game: CS2 Game State Integration (HTTP POST, which needs
nothing else) first, then log tailers. Each event calls the existing
"save replay" path with a tag, so clips arrive pre-named
(`CS2 – Ace – Mirage`).

---

## 5. Verification done for Phase 1

- `cargo test` in `core/`: 14/14 montage tests, including the real ffmpeg render.
- `cargo clippy -- -W clippy::all` in `core/` and `qt-shell/`: no new warnings.
- `qt-shell/tools/check-colors.sh`: clean.
- `cargo build` in `qt-shell/` against Qt 6.4: succeeds.
- The dialog was rendered headless with stub controllers (form, rendering and
  done states, plus reorder, total-length maths and close/reopen); see
  AI-CHANGELOG for why the full-app sweep could not run in this environment.
