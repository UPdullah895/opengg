# Plan: clip audio correctness + design/icon parity

Written 2026-08-01 in response to the two standing problems after the Clips
rebuild: (1) audio misbehaviour when playing a clip in Edit and Preview,
(2) the current design — icons especially — reading as worse than the old
Vue interface.

---

## Problem 1 — Audio in Edit & Preview

### Architecture recap

Multi-track clips split ownership: Qt Multimedia's `MediaPlayer` renders the
**picture** (its own audio output muted), and `ClipAudioMixer` (GStreamer:
`decodebin → per-track volume → audiomixer → autoaudiosink`) renders the
**sound**. The two run on **independent clocks**, reconciled by a QML timer
that polls both positions every 400 ms and re-seeks the audio when they
differ by more than 120 ms.

### Root-cause analysis (from code; to be confirmed by an instrumented run)

The reported symptoms — "audio duplication" and "a track starts when you
click the video without it playing" — both fall out of specific defects:

- **D1 — start gating on intent, not progress.** The mixer starts when
  `mp.playbackState` becomes `PlayingState`, which Qt reports *hundreds of
  milliseconds before the first frame renders* (file open, demux, decoder
  warm-up). Audio therefore leads the picture at every start. This is the
  "starts without the video playing" symptom, verbatim.
- **D2 — drift correction by re-seeking.** When the poll sees >120 ms of
  drift it seeks the audio back to the video position — which **replays the
  audio you just heard**. A syllable heard twice *is* "audio duplication".
  During startup (D1 guarantees early drift) this fires repeatedly:
  stutter + echo.
- **D3 — editor decides ownership too late.** `ClipEditorPage` binds
  `mp.source` declaratively to the clip path, so Qt begins loading before
  `onClipChanged` has run `ClipAudioMixer.load()` and before `mixed` is
  true — a window where Qt's own (single-track) audio output is unmuted.
- **D4 — seeks use `KEY_UNIT`** (snap-to-keyframe) instead of `ACCURATE`,
  adding avoidable error to every correction.
- **D5 — GStreamer state changes are async**; a `seek` issued while a
  `play()`/`pause()` transition is still settling can be swallowed.
- **D6 — sink latency is never compensated.** The PipeWire sink buffer
  (~100–200 ms) is a *constant* offset between "position reported" and
  "sound heard" that no amount of position-matching removes.

> **DECISION (2026-08-01, user):** the Qt Multimedia player may be replaced
> outright — GStreamer owning both picture and sound is the committed
> long-run direction, not a contingency. Phase B below is therefore the
> **main track**; Phase A is retained only as the fallback if the Phase B
> spike fails. Sequencing table at the bottom updated to match.

### Phase A — make the dual-clock design correct (FALLBACK ONLY)

Execute only if the Phase B spike fails. Do these in order; each is
independently verifiable.

1. **Instrumentation first.** Temporary on-screen readout (the established
   text-binding debug technique) showing both clock positions, drift, and
   pipeline states. Define a test matrix and keep it for the whole phase:
   3-track mp4, 1-track clip, the .mov with device-named tracks; actions:
   open, pause, click-video toggle, seek while playing, seek while paused,
   Edit↔Preview switch, EndOfMedia replay, rapid open/close.
2. **One sync authority.** Extract the start/stop/seek/drift logic that is
   currently duplicated across `VideoPlayer.qml` and `ClipEditorPage.qml`
   into a single shared QML object (`AVSync.qml`) that owns the mixer.
   Two copies of subtle clock logic is how the next regression happens.
3. **Gate audio start on actual video progress** (fixes D1): start the mixer
   only when `mp.position` first advances past its start value (not on
   `playbackState`), seeded with an `ACCURATE` seek to that exact position.
4. **Replace seek-correction with rate-skew** (fixes D2): drift ≤ 40 ms →
   do nothing; 40–300 ms → nudge the mixer's playback rate ±2–3 % until
   converged (inaudible, no replaying); > 300 ms → one `ACCURATE` seek.
   No corrections at all until the first post-start convergence.
5. **Constant latency bias** (fixes D6): measure the steady-state residual
   offset with the step-1 instrumentation and subtract it as a named
   constant when comparing clocks.
6. **Editor ownership before load** (fixes D3): stop binding `mp.source`;
   set it imperatively *after* `ClipAudioMixer.load()` returns, and keep
   Qt's output muted while ownership is undecided.
7. **Seek hygiene** (fixes D4/D5): `FLUSH|ACCURATE` flags; never issue a
   seek while a state transition is pending — funnel everything through
   the step-2 authority.
8. **Kill-switch:** if the pipeline errors or drift never converges for a
   clip, drop the mixer for that clip (fall back to Qt single-track audio)
   rather than glitching forever. Log it.

**Verification:** instrumented runs across the full matrix; drift stays flat
within ±40 ms after convergence; no audible echo/stutter on start, seek, or
pause-resume; standard suite (clippy, tests, ui-shots) stays green; live
relaunch for the user's own ears — audio quality is ultimately not
screenshot-verifiable.

### Phase B — one pipeline owns picture AND sound (MAIN TRACK)

The categorical fix: extend the existing `mixer_pipeline` so decodebin's
video pad feeds a Qt-integrated video sink instead of a fakesink — one
pipeline, one clock, **zero sync code to get wrong**. Every defect in the
root-cause list (D1–D6) ceases to exist rather than being compensated.

**Prerequisite (user action):** `gst-plugin-qml6` is packaged
(`extra/gst-plugin-qml6 1.28.5`) but not installed:

    sudo pacman -S gst-plugin-qml6

**Step B1 — spike: DONE 2026-08-01. VERDICT: GO.** Proven with a standalone
C++ harness (`scratchpad/qml6-spike/`) against a real 3-track capture:
video decodes into a QML scene through
`filesrc → decodebin → glupload → glcolorconvert → qml6glsink`, with the
sink's `widget` set to a QML video item. Four findings, all load-bearing:

1. **The QML type is `GstGLQt6VideoItem`, NOT `GstGLVideoItem`.** The
   module is `org.freedesktop.gstreamer.Qt6GLVideoItem`, but the type
   inside it is `GstGLQt6VideoItem`; `GstGLVideoItem` is the *Qt5* name
   that every example online still uses. Symptom of getting it wrong is
   the misleading `"GstGLVideoItem is not a type"` — note that Qt says
   *"is not a type"*, not *"module is not installed"*, which is the tell
   that the module resolved and only the name is wrong.
2. **The type is registered by the GStreamer plugin, not by a qmldir.**
   Arch ships only `/usr/lib/gstreamer-1.0/libgstqml6.so` — no QML module
   directory. `qmlRegisterType` runs inside
   `gst_element_register_qml6glsink`, so the plugin must be loaded
   (`gst_plugin_load_by_name("qml6")` or any `qml6glsink` element
   creation) **before** the QML engine resolves the import.
3. **`QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL)` must be
   called before any window exists**, and the pipeline must not go to
   PLAYING until the scene graph is initialised. Starting it in `main()`
   returns `GST_STATE_CHANGE_FAILURE` (0) because the sink cannot acquire
   a GL context yet; deferring to `QQuickWindow::sceneGraphInitialized`
   yields `ASYNC` (2) and `GST_PAD_LINK_OK`.
4. **`QT_QPA_PLATFORM=offscreen` CANNOT render it** — confirmed
   `SPIKE-BUS-ERROR: Could not initialize window system`. This is the
   ui-shots risk landing: it is the *window system*, not the GL driver,
   so a software-GL override will not rescue it.

**Consequence of (4) — mandatory design constraint for B2/B3:** the player
must degrade deliberately when no GL window system is available. Detect it
(the sink's state change failing, or `QT_QPA_PLATFORM=offscreen`) and fall
back to the existing fakesink video branch plus a poster frame from the
clip's cached thumbnail. That keeps `ui-shots.sh` green and keeps every
control, timeline and dialog verifiable in CI; only the moving picture is
absent there, and it is verified in a real session instead. Silently
shipping a harness that errors on two pages is not acceptable.

**Step B2 — `ClipPlayer` controller.** Promote `ClipAudioMixer` into a
full player: position/duration (polled from the pipeline), playing state,
rate, ACCURATE seeks, per-track and master gain, EndOfMedia→paused-reset.
One QML-facing surface replaces `MediaPlayer + VideoOutput +
ClipAudioMixer` in both views.

**Step B3 — migrate both views.** `VideoPlayer.qml` (Preview) and
`ClipEditorPage.qml` (Edit) swap onto `ClipPlayer`; delete every line of
ownership-token, drift-timer, and mute-arbitration code — that entire
class of bug goes away. Parity checklist to verify per view: play/pause,
click-to-toggle, scrub bar tracking + hover preview, ±5s skip, volume +
mute, playback rate, track mute toggles, keyboard map, EndOfMedia replay,
expanded/theater modes, trim-window clamping (editor), offscreen capture.
Clips with zero audio tracks and broken files fall back gracefully (poster
frame + error text; Qt Multimedia can remain compiled-in as a fallback
until parity is confirmed, then the `Multimedia` qt_module dependency is
dropped).

**Packaging note:** adds a runtime dependency on a system GStreamer
package — consistent with the §5.1/§5.3 never-vendor policy. AUR/Flatpak
manifests gain `gst-plugin-qml6`.

---

## Problem 2 — Design and icons

### 2a. Icon quality (est. ½ session, app-wide payoff)

> **OWNED BY THE USER (2026-08-01) — do not implement.** The user is doing
> the icon work themselves. The diagnosis below stands and is theirs to use;
> another contributor picking up this plan should start at 2b instead.
> Broader interface polish before 0.2.0 is also being handled separately.

The icons are geometrically faithful (mechanically extracted from the Vue
SVGs) but **render badly**, and rendering is what the eye judges:

- **Root cause: the default Shape renderer.** `Icon.qml` uses Qt Quick
  Shapes' default `GeometryRenderer`, which on this Qt produces effectively
  un-antialiased stroke edges — `antialiasing: true` alone does not smooth
  curves. At 13–16 px a 2 px-authored stroke looks jagged and thin. The fix
  is one line: `preferredRendererType: Shape.CurveRenderer` (GPU curve AA,
  Qt 6.6+; we are on 6.11). Verify with the existing `--page icons` contact
  sheet, before/after, zoomed crops.
- **Sizing path:** the current `scale:` transform trick should be re-checked
  under CurveRenderer; if needed, size the Shape directly so tessellation
  happens at final pixel size.
- **Geometry audit:** a minority of glyphs were hand-drawn rather than
  extracted (ear, camera, grid, calendar, rewind/fast-forward, maximize…).
  Write `tools/extract-icons.py` that mechanically converts source SVGs
  (the Vue inline originals / lucide) to path data, regenerate those
  entries, and diff the contact sheet against the Vue UI rendered at the
  same pixel sizes. Fix outliers only.

### 2b. Editor/player design parity (sequenced from docs/CLIP_EDITOR_DESIGN_GAP.md)

The comparison doc's finding stands: the port has the old editor's *nouns*
(regions, labels) but not its *verbs* (manipulability). In order of carried
weight:

1. **Waveforms in audio lanes.** `core::media::generate_waveform` already
   returns peaks — unused. Load per-track peaks off-thread in
   `EditorController` (established Threading pattern), render ~200 peak
   bars per lane (Repeater of Rects or a Canvas), grey when muted. This is
   the single biggest visual + functional gap.
2. **Split video/audio trim.** Second, independent audio range
   (`audioTrimS/E`) clamped inside the video range, with the old unified ⁄
   split behaviour and four handles. This is what "I cannot move the audio
   track" was pointing at — the control was never ported. Check how the
   Vue side persists the audio range (extend `trim_state` schema if
   needed) before building.
3. **Draggable playhead** with a visible grip (reuse the fixed TrimHandle
   mechanics).
4. **Undo/redo** of trim edits — a stack of 4 floats, `Ctrl+Z` /
   `Ctrl+Shift+Z`, matching the old keys. Cheap, and it changes how the
   editor *feels*.
5. **Track naming chain:** embedded title → Settings trackDefs →
   captureTracks override → "Audio N" (port `getCaptureTrackName`; mind
   the top-level-vs-`settings`-envelope rule when reading ui-settings).
   Fixes lanes showing "Device…" instead of the user's own names.
6. **Per-track volume** popovers on the lanes — the mixer already supports
   continuous per-track gain (`setTrackVolume`); this is UI only.
7. **Per-type lane icons/colors** (video/game/chat/mic/media matched by
   name, accent fallback).
8. **Resizable splitter** between preview and timeline + collapsible info
   panel, both persisted.
9. **Export hand-off:** draggable file row in the done state
   (`Drag.dragType: Automatic` with `text/uri-list`). Spike first —
   external drags out of QML on Wayland are known-fiddly; fallback is
   keeping Show-in-folder/Copy-path prominent.
10. **Player polish:** the 2a icon fix applies automatically; then a
    spacing/typography pass against the CustomVideoPlayer reference
    screenshots.

---

## Sequencing

| Order | Slice | Why first |
|---|---|---|
| 1 | 2a icons (CurveRenderer + audit) | Cheapest change, app-wide visible lift |
| 2 | Phase B spike → B2/B3 unified player | Committed direction; erases the whole desync bug class |
| 3 | 2b.1 waveforms + 2b.2 split trim | Carry most of the perceived design gap |
| 4 | 2b.3–2b.7 | Editor manipulability + naming |
| 5 | 2b.8–2b.10 | Layout flexibility + hand-off |
| — | Phase A | Only if the Phase B spike fails |

Every slice ends with: offscreen screenshot or instrumented run as proof,
the standard suite (clippy / tests / check-colors / ui-shots), a commit,
and a live relaunch so the running app matches the claim.
