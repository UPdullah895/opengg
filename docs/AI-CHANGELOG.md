# AI Contributor Changelog

This is the **append-only session log** for all AI agents working on OpenGG. Every agent must update this after completing work.

**Format**: Newest entry first. See the template below.

---

### [2026-08-04] Claude Opus 5 — qt6-gstreamer-player-b3

**What Changed:**
- Settings → General: added a native `ColorDialog` color picker (click the
  accent swatch) instead of hex-only entry; dark/light-mode and reload icons
  are now button-shaped (background + border) instead of bare floating icons;
  Clip Preferences' segmented pills became two labeled `SelectField` dropdowns
  (`defaultClickAction`, `dateFormat`) — both were previously write-only
  settings, now wired to actually change clip-card click behavior and enable
  date-based clip search; Diagnostics' "Open Crash Logs Folder" is now an
  outlined button matching the reference design.
- New `SettingsCard.qml` component: title + divider + body, used to give
  every settings-panel card section a consistent underlined title. Migrated
  simple single-title cards across all 11 panels to it, and manually inserted
  the same divider into cards with bespoke header rows (a toggle, a link, a
  reset button) that don't fit the plain-title shape.
- Root-caused and fixed the real settings-card width cap: `SettingsPage.qml`'s
  `Loader` was pinned to `Layout.preferredWidth: 680`; also removed 21
  additional `Layout.preferredWidth: 680/640` caps scattered across individual
  card `Rectangle`s in 10 panel files. Cards now span the available width at
  any window size.
- Reverted a leftover debug-only window-size hook in `Main.qml`.

**Why:**
User feedback on four points: no color picker, non-functional-looking
dark/light toggle buttons, a non-functional Clip Preferences section that
also needed a redesign, and inconsistent/non-underlined section titles across
every settings panel except Ear Blast Protection.

**Landmines & Discoveries:**
- **`Loader` has no `resizeMode` property.** It sizes itself around its
  loaded item, not the other way around — there is no "stretch the loaded
  item to the Loader's width" mode. Assigning a `resizeMode` (a hallucinated
  API) is a hard QML load failure that takes down the whole app; the fix is
  `width: parent.width` on the loaded item's own root, same as
  `settingsContentCol` in `SettingsPage.qml` already does for the identical
  reason (not being a Layout child of anything).
- A QML load failure from a bad property assignment can look exactly like a
  hung headless screenshot run — the process never exits, and truncated
  `2>&1 | tail` output hides the actual `QQmlApplicationEngine failed to load
  component` error. Always re-run with `timeout N` and full untruncated
  output before treating an unexplained hang as a real performance issue.

---

### [2026-08-03] Claude Opus 5 — qt6-gstreamer-player-b3

**What Changed:**
- `21f721e`: `drop_video_pad`'s fakesink now sets `sync=true`; `ChannelStrip`
  clamps its fader height and gains a `compact` mode; `Main.qml` gains minimum
  window dimensions.
- CI now builds and tests the Qt shell (clippy `-D warnings`, `cargo test`,
  `check-colors.sh`, `ui-shots.sh` with screenshot artifacts) in an Arch
  container. The archived Tauri/Vue jobs — the only red checks on the board —
  are removed from `ci.yml` and `security.yml`.
- `SegmentedToggle.qml` replaces the per-corner-radius approach used for the
  Clips grid/list pill.

**Why:**
Two user-reported bugs (playback racing to the end of a clip after a skip;
UI elements overlapping in a quarter-screen window), plus the discovery that
CI gated exclusively on archived code and never built the shipping UI.

**Landmines & Discoveries:**
- **GstBin folds POSITION queries by taking the MAXIMUM across sinks.** A
  `fakesink` defaults to `sync=false` and so consumes decoded video as fast as
  the file reads; that runaway pad, not the audio, then becomes the pipeline's
  reported position. Measured: 5117ms of media per 400ms of wall time, versus
  398-401ms with `sync=true`. Any discard sink in a pipeline whose position is
  ever queried MUST set `sync=true`.
- **A negative height in a QML `Column` does not clamp — it stacks subsequent
  children BACKWARDS.** `ChannelStrip` sized its fader as `strip.height - 168`,
  which inverted below 168px and drew the channel name on top of the volume
  readout. Always `Math.max(0, ...)` a computed height.
- `minimumWidth`/`minimumHeight` on a Window are only hints; tiling
  compositors (Hyprland, sway) size from their own layout and ignore them, so
  components must degrade on their own regardless.
- **Per-corner radii (`topLeftRadius` and friends) require Qt 6.7**, newer than
  Debian stable's Qt, which the distro matrix builds against. A clipping
  rounded parent gets the same result portably.
- The bug was found by measuring, not reading: an earlier plausible-sounding
  theory (that `videoActive` was false, leaving the drift timer running) was
  disproved by checking that `no-more-pads` fires after all pads are added.

**Verification:**
- Regression test `playback_advances_at_wallclock_speed_before_and_after_a_seek`
  asserts ~1x advance; confirmed it FAILS (21516ms) with the fix reverted.
- `cargo test` 16 passed; `cargo clippy --all-targets -- -D warnings` clean
- `./tools/check-colors.sh` clean; `./tools/ui-shots.sh` zero QML warnings
- Before/after offscreen captures of the mixer at 928x492 (the reported
  quarter-screen size) confirm the overlap is gone

---

### [2026-08-02] Claude Sonnet 5 — qt6-gstreamer-player-b3

**What Changed:**
- `728694a`: `ClipAudioMixer.load()` gained a `want_video` param — the editor
  was requesting a GL video branch it had no `ClipVideoSurface` for, stalling
  the pipeline; `VideoPlayer.qml`'s drift-correction timer now skips while
  `videoActive` is true, since the recurring seek was hitting the now-visible
  GStreamer video branch and made playback visibly repeat/jump after a skip;
  the preview volume slider's `value:` binding (severed by the first drag,
  same landmine as the clips-per-row slider) is fixed the same way; list-view
  thumbnails grew 78×44 → 140×79; `IconToggle` gained a `segment` prop so the
  grid/list toggle renders as one fused pill; `RecordingControl`'s dropdown
  now matches the Home dashboard's richer recorder panel (status line,
  Start/Save buttons, Quality/FPS/Buffer/Target grid).

**Why:**
User-reported regressions from B2 (editor audio glitches, video stutter on
seek) plus a batch of Clips-page design-parity feedback, all live-tested
against a reference screenshot of the intended look.

**Landmines & Discoveries:**
- `ClipAudioMixer.load()`'s `want_video` decision cannot be made from
  `gl_video_available()` alone — it has to be per-caller. A video branch with
  no `ClipVideoSurface` to attach to leaves `qml6glsink` with no widget,
  which stalls pipeline state changes and can take the *audio* down with it
  even though the caller only wanted sound.
- The dual-clock drift-correction timer (`ClipAudioMixer.seek()` every 400ms
  on >120ms drift) was written when the mixer only ever owned audio. Once B2
  made it also own the picture, the same timer was periodically re-seeking
  the now-visible video — this is the concrete, user-visible instance of the
  "entire class of bug" B3 is meant to delete outright.

**Verification:**
- `cargo build`, `cargo test` (15 passed)
- `./tools/check-colors.sh` clean
- `./tools/ui-shots.sh` — all pages, zero QML warnings
- Manual offscreen screenshots of Clips grid/list/recording-menu states,
  compared directly against the user's reference screenshots

---

### [2026-08-01] Claude Opus 5 — qt6-migration (Phase B1 spike)

**What Changed:**
- `efe32de`: marked plan slice 2a (icons) as user-owned so contributors start at 2b
- `914861c`: `qt-shell/spikes/qml6glsink/` — standalone C++ harness proving the
  unified GStreamer player, plus the plan's B1 section rewritten with findings

**Why:**
Phase B (one GStreamer pipeline owning both picture and sound) is the committed
direction for clip playback. Its two risks had to be settled before building
anything: the QQuickItem-pointer handoff, and whether it survives the offscreen
screenshot harness.

**Landmines & Discoveries:**
- The QML type is **`GstGLQt6VideoItem`**, not `GstGLVideoItem` — the latter is
  the Qt5 name that essentially every online example still uses. Wrong name
  fails with `"GstGLVideoItem is not a type"`; note it says *is not a type*,
  not *module is not installed*, which is the tell that the module resolved.
- The type registers from inside `gst_element_register_qml6glsink`, not from a
  qmldir (Arch ships only `libgstqml6.so`), so the GStreamer plugin must be
  loaded before the QML engine resolves the import.
- `QQuickWindow::setGraphicsApi(OpenGL)` must precede any window, and the
  pipeline must reach PLAYING only after `sceneGraphInitialized` — otherwise
  `GST_STATE_CHANGE_FAILURE` (0) instead of `ASYNC` (2).
- **`qml6glsink` cannot run under `QT_QPA_PLATFORM=offscreen`** ("Could not
  initialize window system"). It is the window system, not the GL driver, so
  software GL does not rescue it. The real player must degrade to a poster
  frame without GL, or `ui-shots.sh` breaks on the player and editor pages.
- The debugging itself hit this repo's own documented landmine: without
  `QT_FORCE_STDERR_LOGGING=1` the QML error printed *nothing at all*.

**Verification:**
- Spike run against a real 3-track capture on Wayland: `ASYNC` state change,
  `GST_PAD_LINK_OK`, and a screenshot showing actual decoded video frames.
- Same binary under `offscreen`: reproducible bus error, captured verbatim.

---

### [2026-08-01] Claude (main session, Sonnet 5 → Opus 5 → Fable 5) — qt6-migration

**What Changed:**
- `fee433c`: Mixer round 2 — toolbar/borders/ChatMix/Ear-Blast + two real backend bugs
  (stale deployed daemon binary; Mic mute now also mutes @DEFAULT_SOURCE@; Mic
  device list split from the capture-source list)
- `060c54b` / `05cd520` / `99ebf66`: Clips rebuild — multi-select, list/date views,
  stats bar, toolbar, full player transport, dedicated editor page
- `ee94d40` + `3ff1ccd`: GStreamer multi-track audio mixer (all tracks at once,
  per-track gain) + the ownership/sync/trim/z-order regression fixes
- `cfb0368`: Export settings dialog (target size, codec, two-pass encode in core)
- `39ad78e` / `af67442` / `01bb419`: editor design-gap analysis + plan of record —
  **decision: a single GStreamer pipeline (qml6glsink) will replace Qt Multimedia
  for clip playback entirely**; icons fix = Shape.CurveRenderer
- `89c6c71`: review fixes for the switchover-agent's work (fabricated commit
  hashes in this file's first entry, misleading Layout rules in AGENTS.md,
  make install ETXTBSY)

**Why:**
User feedback rounds on the Mixer and Clips pages, then the switchover decision:
qt-shell becomes the launched UI and GStreamer the committed playback direction.

**Landmines & Discoveries:**
- Everything durable was folded into AGENTS.md; the deep narrative lives in the
  session memory and the plan/gap docs under docs/.
- Subagent lesson: an agent that ingests very large files can stall the stream
  watchdog (two 600s stalls); inline the needed facts into the prompt instead.
  And verify agent-written logs against `git log` — the first draft of this
  file's older entry cited four commits that did not exist.

**Verification:**
- Full suite per slice: cargo clippy + test (core/daemon/qt-shell),
  check-colors.sh, ui-shots.sh (zero QML warnings), screenshot review,
  live relaunches; mixer pipeline verified against a real 3-track capture;
  export path verified by a real 1s stream-copy test.

---

### [2026-08-01] Claude Fable 5 — qt6-migration

**What Changed:**
- `a47b6e3`: Archive the Tauri UI in place; qt-shell becomes the default build/dev flows
  - Updated Makefile lint recipe to run clippy on qt-shell + check-colors.sh
  - Fixed dev.sh run_frontend build guard (replaced `& wait $!` pattern with proper `if ! cargo build`)
  - Added ui-legacy flow documentation to dev.sh help text
- `1f38bb7`: Point the launcher at the Qt shell binary
  - Reordered opengg-launch.sh candidate search: qt-shell/target/release/opengg-qt first
  - Reverted packaging/*.desktop StartupWMClass changes (were incorrect; left unchanged)
- `1825dbe`: README: reflect the Qt6/QML architecture and switchover
  - Rewrote architecture section: Qt6/QML + cxx-qt, shared opengg-core, daemon, legacy UI archived
  - Updated Requirements: Qt 6, GStreamer + gst-plugin-qml6, removed Tauri/Node from mandatory build tools
  - Reflected all verified build/run commands from fixed Makefile and dev.sh
  - Kept AUR install, data locations, and troubleshooting sections (still accurate)
- `b31af5f`: Add AI contributor guide and session changelog convention
  - Created AGENTS.md: mandatory read for AI agents working on this codebase
    - Architecture boundaries: crate separation, IPC flow, access control
    - Mandatory verification: clippy/test/lint/ui-shots protocol before commits
    - QML landmines: pixelSize integers, file registration, layout rules, binding lifetime, anchors, mapToItem(), Rust invokables
    - Theme/icon rules: Theme tokens only, no bare hex/emoji
    - Audio/system rules: never restart PipeWire, stale daemon binary check, safe binary replacement
    - Process discipline: dev app on live desktop, no git-worktree, safe process relaunch
    - Documentation convention: append to AI-CHANGELOG after each session
  - Created docs/AI-CHANGELOG.md with this header and first entry

**Why:**
The Qt6/QML migration switchover was left half-done by a previous agent (two stalls). This session completes all remaining fixes and infrastructure:
1. The Makefile lint target didn't actually run qt-shell clippy (just referenced it)
2. dev.sh had a dead-code bug where failed builds wouldn't properly abort
3. opengg-launch.sh prioritized old installed binaries over the repo build, launching stale UI
4. README still described the old Tauri architecture
5. No documentation existed for AI agents working in this codebase (high friction for next contributor)

**Landmines & Discoveries:**
- **Git worktree bug**: Previous agent discovered git-worktree isolation doesn't work in this repo (wrong base branch resolution). Documented as non-negotiable rule in AGENTS.md.
- **Qt stderr logging**: Qt logs go to journald, not stderr. Screenshots with `QT_FORCE_STDERR_LOGGING=1` are required to catch real warnings. Build logs often appear clean but hide warnings.
- **Pipeline error handling under `set -euo pipefail`**: The `cargo build | sed &` followed by `wait $!` pattern is a dead code trap. Under strict shell settings, backgrounded pipelines fail silently. Replaced with guarded pipeline `if ! cargo build | sed`.
- **check-colors.sh integration**: The Makefile lint target now correctly invokes it; previous state was incomplete.
- **Desktop file WM_CLASS**: Extensive investigation showed Qt apps set WM_CLASS from the binary name, not `StartupWMClass` in .desktop files. Reverted unnecessary changes to packaging/*.desktop files.
- **GStreamer unified pipeline**: README updated to clarify gst-plugin-qml6 is a runtime dependency (not build-time), replacing Qt Multimedia for clip playback.

**Verification:**
- `bash -n dev.sh` passed
- `bash -n opengg-launch.sh` passed
- `make -n ui`, `make -n ui-legacy`, `make -n build`, `make -n install`, `make -n lint` all passed
- Manual verification of Makefile recipe order and help text consistency
- README.md architecture section cross-checked against CLAUDE.md and qt-shell/ file structure
- AGENTS.md reviewed for completeness against all landmines and rules mentioned in task context

---

## Entry Template

When you complete work on OpenGG, copy this template and fill it in:

```markdown
### [YYYY-MM-DD] Agent Name (Claude Model) — Branch

**What Changed:**
- Commit hash: description
- Commit hash: description

**Why:**
A 1–2 sentence explanation of the business/technical reason.

**Landmines & Discoveries:**
- Any hard-won insights or constraints uncovered

**Verification:**
- `make lint` passed
- `make build` succeeded
- Manual testing of [specific scenario]
```

**Important**: Keep entries **newest first**. Maintain the exact format so agents can scan this file quickly.
