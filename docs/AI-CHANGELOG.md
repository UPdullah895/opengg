# AI Contributor Changelog

This is the **append-only session log** for all AI agents working on OpenGG. Every agent must update this after completing work.

**Format**: Newest entry first. See the template below.

---

### [2026-08-05] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Mixer stuck-duck, theme reset, minimize-to-tray, RTL language toggle)

**What Changed:**
- `core/src/ear_blast.rs`: added `EarBlastState::active_channels()` and
  `release_all()` — force-restores every currently-ducked channel's real
  PipeWire volume, bypassing the normal dB/hold-time gating.
- `qt-shell/src/audio.rs`: `start_vu_stream`'s reader thread now calls
  `ear_blast::release_all()` right after its main loop exits (page
  navigated away or app quitting). Root cause of "Game/Media drop to 60%
  when I open the Mixer": Ear Blast Protection was ducking a channel, then
  the VU stream stopped before its next `check()` pass could release it —
  nothing was left monitoring the level, so PipeWire's real sink volume
  stayed stuck at the duck target indefinitely, surfacing next time as "the
  Mixer opened at 60%."
- `qt-shell/src/theme.rs`: added a real `reset()` qinvokable that discards
  `theme.json`'s saved override and re-derives from built-in defaults.
  `GeneralPanel.qml`'s reset button previously called `reload()`, which
  only re-applies whatever is already on disk — a no-op once a custom
  accent had been saved, since disk had nothing else to fall back to.
- `qt-shell/qml/Main.qml`: built a full minimize-to-tray feature via
  `Qt.labs.platform.SystemTrayIcon` (confirmed present system-wide, no new
  native bindings needed) — a root `s`/`Connections` mirror of
  `SettingsController.settingsJson` reads `runInBackground`; `onClosing`
  intercepts the frameless titlebar's close button and hides instead of
  quitting unless the setting is off or the tray menu's "Quit" was used
  (`reallyQuit` flag). "Start on Boot" was already correct at the backend
  level (verified `~/.config/autostart/opengg.desktop` on disk) — the real
  gap was that Minimize to Tray had no implementation anywhere.
- `qt-shell/src/i18n.rs`: added a `rtlOverride` qproperty (persisted
  independently of the active language) plus `setRtlEnabled`,
  `openLocalesFolder`, and `reloadLocales` qinvokables. Arabic (and any
  future RTL-tagged locale) now defaults to LTR rendering like every other
  language instead of auto-flipping the layout; a separate opt-in toggle
  lets the user switch to RTL, and that choice survives later language
  switches because it's a distinct persisted field, not derived from the
  active language.
- `qt-shell/qml/pages/settings/LanguagePanel.qml`: replaced the plain
  `SettingsCard` title with a bespoke header row (same reasoning as
  MixerRoutingPanel's Ear Blast Protection card — see `SettingsCard.qml`'s
  own header-note) hosting three buttons: open the locales folder
  (`en.json`/`ar.json` side by side, for translating or adding a new
  language pack), reload locales from disk, and an RTL toggle pill that
  only appears when the active language's `_meta.dir` is `"rtl"`.
- `qt-shell/locales/en.json` / `ar.json`: added a `"tray"` block
  (`show`/`quit` strings for the tray menu). The `"language"` block's
  `addLanguage`/`reloadLanguages`/`rtlModeHint` keys already existed in
  both locales (ported earlier, unused until this session).
- `core/src/system.rs`: added `open_path()` — opens any file/dir with the
  desktop's default handler; used by `openLocalesFolder` instead of adding
  a duplicate `open` crate dependency to `qt-shell`.

**Why:**
Direct user report of five issues: Mixer volume unexpectedly dropping,
General's theme reset button doing nothing, Start on Boot/Minimize to Tray
not working, and a request to default Arabic to LTR with an opt-in RTL
toggle plus a way to export the English strings for translation. The
RTL/language design (independent `rtlMode` field, default `false`, header
button layout) was not invented — it mirrors an already-designed-but-never-
ported feature found in the archived `frontend/src/components/settings/
LanguageSettings.vue` and `frontend/src/stores/persistence.ts`.

**Landmines & Discoveries:**
- **cxx-qt qinvokable/qproperty name collisions**: a hand-written
  qinvokable's Rust name or its `#[cxx_name]` cannot reuse the name a
  `#[qproperty(...)]` auto-generates for its setter (`set_<field>` in Rust,
  camelCase `set<Field>` in C++/QML) — collides even if the invokable's own
  QML-facing name differs. `rtl_override`'s qproperty setter is
  `setRtlOverride`; the invokable had to become `apply_rtl_override` /
  `cxx_name = "setRtlEnabled"` to avoid a `defined multiple times` /
  "cannot be overloaded" build error.
- **`self.as_mut()` + reading `self.*` in the same statement** trips
  Rust's borrow checker under cxx-qt's `Pin<&mut Self>` pattern — hoist
  every needed `self.*` read into a `let` binding before any
  `self.as_mut()` mutation call.
- **`use cxx_qt::CxxQtType;`** is required to call `.rust_mut()` on
  `Pin<&mut Self>` for mutating a plain (non-qproperty) struct field after
  construction — not imported by default; found by grepping `eq.rs`/
  `clips.rs` for existing working uses.
- The `open` crate is only a dependency of the `core` crate, not
  `qt-shell` directly — route new "open this path" calls through a
  `core::system` helper rather than adding a second copy of the dependency.

**Verification:**
- `cargo build` clean (qt-shell)
- `cargo test` — 22/22 passing (including a new
  `i18n::tests::language_and_rtl_mode_are_independent`)
- `qt-shell/tools/check-colors.sh` — clean
- `qt-shell/tools/ui-shots.sh` — zero QML warnings

---

### [2026-08-05] Claude Sonnet 5 — qt6-gstreamer-player-b3 (fillWidth-wrapper bug, round 2: ShortcutsPanel + MixerRoutingPanel)

**What Changed:**
User's follow-up screenshot showed ShortcutsPanel's key-recorder boxes still
clustered next to their labels instead of right-aligned like the reference
design — the exact same root cause just fixed in `GeneralPanel.qml` (a
nested Layout marked `Layout.fillWidth: true` doesn't stretch the way a
plain `Item` spacer does), just missed there because the earlier sweep only
grepped for `ToggleSwitch` call sites and this row uses a plain `Rectangle`
key-box instead.
- `ShortcutsPanel.qml`: fixed both the header row ("Reset to Defaults" was
  landing next to the title) and every per-shortcut row (key box was landing
  next to the label) — replaced the nested-RowLayout wrappers with
  `Item { Layout.fillWidth: true }` spacers.
- Audited the rest of `qml/` for the same shape (a wrapper Layout whose sole
  child is another Layout, followed by a trailing sibling control) and found
  two more live instances in `MixerRoutingPanel.qml`'s Danger Zone card
  ("Reset Virtual Audio" / "Remove Virtual Audio & Restore OS Defaults" —
  both `ColumnLayout`-wrapped). Fixed the same way.
- Confirmed via screenshot that `DspControls.qml`'s Noise Reduction/Gate/
  Compressor toggles, `GraphicEQ.qml`'s Enabled toggle, and Extensions'
  Modules list do **not** have this bug — they either use a dedicated `Item`
  spacer already, put `Layout.fillWidth` directly on a `Text`, or wrap
  multiple `Text` children directly in a `ColumnLayout` (no intermediate
  nested Layout) — narrowing the actual defect to "a Layout whose *sole*
  child is itself a Layout doesn't propagate `Layout.fillWidth` to its
  parent's stretch calculation," not "any nested Layout is broken."

**Why:**
Direct user follow-up with a labeled before/after crop pinpointing the
Shortcuts key-box position. Given the same bug had just been found once,
did a systematic sweep rather than fixing only the reported instance.

**Verification:**
- `cargo build` succeeded
- `qt-shell/tools/check-colors.sh` passed
- `qt-shell/tools/ui-shots.sh` — all 16 pages, zero QML warnings
- `cargo test` — 21/21 passing
- Screenshotted Shortcuts, Audio Engine (Danger Zone), and Mixer → Chat tab
  (DSP Controls) to confirm right-alignment matches the reference pattern
  and that the DspControls toggles were never actually affected
- Live app relaunched (`QT_FORCE_STDERR_LOGGING=1`) — clean startup

---

### [2026-08-05] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Toggle centering, real toggle-row bug, Shortcuts overcorrection, icon picker, module→nav gating)

**What Changed:**
- `ToggleSwitch.qml`: knob was `y: 3` against a `parent.height-8` knob in a
  22px track — 3px top / 5px bottom, not centered. Fixed to
  `(parent.height - height) / 2`.
- `GeneralPanel.qml` — the real "toggle attached to the title" bug: "Start
  on Boot" / "Minimize to Tray" wrapped their title+info-icon in a *nested*
  `RowLayout` marked `Layout.fillWidth: true`. Unlike a plain `Item` or
  `ColumnLayout` spacer, that nested RowLayout doesn't actually stretch to
  claim the row's remaining space, so the toggle ended up clustered right
  next to the icon instead of pushed to the card's right edge like every
  other toggle row in the app. Found by systematically screenshotting every
  ToggleSwitch call site (7 files) at both 1280px and a narrow 960px test
  width — this was the only one that didn't match the established
  Item-spacer pattern. Replaced with the same dedicated
  `Item { Layout.fillWidth: true }` spacer used everywhere else.
- `ShortcutsPanel.qml`: last session's fix for "crammed" rows (10px/10px
  margins) overcorrected — user's side-by-side comparison showed the
  reference/old design is markedly more compact than what shipped. Reduced
  to 5px/5px, keeping the divider.
- `TrackManagementPanel.qml`: the per-track icon button just cycled blindly
  through a fixed 6-icon array on every click with no way to see or choose
  a specific one. Replaced with an inline icon-choice strip (plain `Row`
  toggled by a `openIconPickerFor` property) — deliberately *not* a
  Popup/ComboBox, since a separate `IconPicker.qml` built on those was
  already documented in this file as hanging the app at startup for
  unknown reasons.
- `Sidebar.qml`: Settings → Extensions → Modules toggles (audio/device/
  replay) called `ExtensionsController.setModule()` but nothing ever read
  `modulesJson` outside the Extensions panel itself — confirmed via grep
  this was never wired even in the archived Vue reference, so it's a new
  feature rather than a porting gap. Added `visibleNavItems`, filtering
  `navItems` by module state (mixer→audio, devices→device, clips→replay;
  home/settings have no backing module and always show), and an
  `ExtensionsController.refresh()` call in `Component.onCompleted` so the
  sidebar reflects saved module state from the first frame rather than only
  after the user visits the Extensions panel.

**Why:**
Follow-up to the previous design-fidelity pass: user reported the toggle
knob still looked off-center, pointed at a toggle sitting flush against a
title in a screenshot, provided a direct before/after comparison showing
Shortcuts had gone too far the other way, and asked for the Timeline Tracks
icon control and Extensions module toggles to actually do something instead
of being decorative. Also asked for a GPU Screen Recorder install
explanation — `RecorderInstallHelper.qml` (distro-aware install command +
installed/missing states) already covers this and is embedded in Capture &
Sound; screenshot-verified it renders the installed-confirmation state
correctly, so no code change was needed there.

**Deliberately not touched:** Timeline Tracks config still doesn't affect
the actual clip editor (`ClipEditorPage.qml`) — that's audio/video editor
scope, out of bounds per the standing agreement that playback/editor work
is handled elsewhere. Only the settings-panel icon-picker UX itself was
fixed.

**Verification:**
- `cargo build` succeeded
- `qt-shell/tools/check-colors.sh` passed
- `qt-shell/tools/ui-shots.sh` — all 16 pages, zero QML warnings, including
  after the TrackManagementPanel delegate restructure (risk of reintroducing
  the documented Popup/ComboBox hang — confirmed clean)
- `cargo test` — 21/21 passing
- Screenshotted every ToggleSwitch call site at 1280px and 960px to find
  the actual "attached to title" offender before writing a fix
- Live app relaunched (`QT_FORCE_STDERR_LOGGING=1`) — clean startup

---

### [2026-08-04] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Toggle/dropdown/shortcuts/storage fidelity fixes)

**What Changed:**
- `ToggleSwitch.qml`: the knob was hardcoded `"#ffffff"` and the track was a
  solid accent fill with no border — user asked why the toggle's white
  didn't match the theme. Read the original `ToggleSwitch.vue`: it was never
  white. Track is an accent-tinted (not solid) fill with an accent border
  when on; knob is `text-muted` off / `accent` on. Rewrote to match exactly.
- `SelectField.qml`: bumped from `implicitHeight: 26, font.pixelSize: 11,
  color: Theme.textDim`, no focus feedback, to `34px/13px/Theme.text` with an
  `activeFocus` border — matching every other ComboBox in the app (Capture &
  Sound's Quality/FPS fields). This was the "dropdown menus are weak/small"
  complaint; SelectField backs Clip Preferences and Notifications'
  Position/Duration fields, so it read as visibly weaker than the plain
  ComboBoxes used elsewhere despite being the more common variant.
- `ShortcutsPanel.qml`: rows had no divider between them and only a 4px top
  margin, so all 9 actions ran together into one dense block ("crammed
  together"). Restructured the Repeater delegate to a `ColumnLayout` with a
  divider after every row except the last, plus real 10px top/bottom margins.
- `StoragePanel.qml`: Clip/Screenshot directory rows were bare text with no
  background, reading as "just text" rather than a file location. Wrapped
  each row in a bordered `Theme.bg` chip (36px, radius, folder icon + path +
  remove icon), same treatment for both `clipDirRow` and `shotDirRow`.

**Why:**
User sent 6 screenshots (toggle switches, Clip Preferences dropdown,
Shortcuts page, Language panel, Timeline Tracks, Storage panel) with four
explicit text complaints: toggle color mismatch, cramped Shortcuts spacing,
storage rows looking like plain text, and weak/small dropdowns. Addressed
each by reading the original Vue source (`ToggleSwitch.vue`) or comparing
against sibling components already in the QML port (other ComboBoxes, other
card dividers) rather than guessing at new styling.

**Deliberately not touched:** two elements visible in the reference images
but not named in the text complaint were left alone — the Language panel's
folder/refresh header icons (no backend command wired for them) and Timeline
Tracks' "Live Preview" section (documented Phase 5/editor scope, not
implemented in the Qt port yet). Both would be net-new feature work, not
part of this design-fidelity pass.

**Verification:**
- `cargo build` succeeded
- `qt-shell/tools/check-colors.sh` passed
- `qt-shell/tools/ui-shots.sh` — all 16 pages captured, zero QML warnings
- `cargo test` — 21/21 passing
- Individual screenshots of General/Shortcuts/Storage/MixerRouting panels
  visually inspected
- Live app relaunched (`QT_FORCE_STDERR_LOGGING=1`) — clean startup, no QML
  errors in the log

---

### [2026-08-04] Claude Opus 5 — qt6-gstreamer-player-b3 (Settings/Mixer consistency pass)

**What Changed:**
- Fixed a settings-panel right-edge overflow: `settingsContentCol` combined
  an explicit `x: 28` offset with `width: parent.width`, pushing its right
  edge 28px past the viewport — a 28px left gutter with none on the right,
  visible as cards bleeding past the window boundary at wide window sizes.
  Moved the gutter to symmetric `Loader` margins instead.
- Mixer EQ/DSP tabs: `GraphicEQ.qml` had no card wrapper at all, unlike its
  `DspControls.qml` sibling on the same Chat tab (Noise Reduction/Gate/
  Compressor, each in a bordered card) — it floated directly on the bare
  page background. Wrapped it in a matching bordered card, added an
  "Enabled" label next to the previously-unlabeled EQ toggle, and gave the
  "Flat" reset button a hover state matching other outlined buttons.
- Settings → Notifications: the "Position" field was the only field in
  Settings still using a plain, unstyled label (14px, not bold) and a raw
  ComboBox instead of the SelectField + uppercase-label pattern Clip
  Preferences established. Gave it a real "Display" card title and moved
  Position/Duration onto that pattern.
- Fixed "clips"/"Used" stat-label casing mismatch in Storage's Disk Usage
  card (English locale only — Arabic has no case distinction).
- Added `--panel <tab>` support to the Mixer page's screenshot path
  (`MixerPage.qml`), matching Settings' existing convention, so EQ/DSP tabs
  can be captured headlessly for review.

**Why:**
User asked for a design consistency pass (spacing, icons, text) across
Settings and Mixer, after flagging the settings right-edge overflow via a
live screenshot at 1920×1080. Rather than guess app-wide, scoped to
Settings + Mixer via AskUserQuestion, then surveyed every panel with
`ui-shots.sh`-adjacent screenshots to find concrete inconsistencies (missing
cards, unlabeled controls, mismatched patterns) instead of restyling things
that were already fine.

**Deliberately not touched:** Capture & Sound's label-left field rows
(Quality/FPS/Replay Buffer/Monitor Target) use a different, but internally
consistent, "label left of control" pattern rather than Clip Preferences'
"uppercase label above control" pattern. Both are legitimate settings-UI
conventions; converting one to the other across a 4-row/2-toggle card
control risked regressions for a stylistic judgment call, not a bug — left
as-is.

---

### [2026-08-04] Claude Opus 5 — qt6-gstreamer-player-b3 (Language panel redesign)

**What Changed:**
- Settings → Language: replaced the ComboBox dropdown with the original
  Vue design's selectable list of language rows (2-letter code badge, name,
  LTR/RTL tag), matching `LanguageSettings.vue` faithfully. Active language
  gets an accent border + tinted background; clicking any row calls
  `I18n.applyLanguage()` directly.
- Added `I18n.languageDir(code)` (`src/i18n.rs`) — the existing
  `available_languages()`/`rtl` surface only exposed the *current* language's
  direction; the per-row LTR/RTL badge needed direction for every language in
  the list, not just the active one.

**Why:**
User supplied side-by-side screenshots of the old Tauri/Vue Language and
Shortcuts pages vs. the current Qt port and asked to close the gap. Shortcuts
already matched closely (header, divider, reset button, keybind pills);
Language was the real gap — a plain dropdown instead of the original's list.

**Deliberately not ported:** the Vue original's two extra header icon
buttons (open a user-locales folder in the file manager; hot-reload
JSON locale files dropped there at runtime). Neither has a backend
equivalent in the Qt port — `qt-shell` only loads locales once at startup
from `OPENGG_LOCALES_DIR`. Porting the buttons faithfully would mean adding
that runtime-reload feature first, which is out of scope for a visual-parity
pass; flagging here rather than shipping dead buttons.

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
